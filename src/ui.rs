//! `pg0 ui`: a read-only web UI for every pg0 instance, served over HTTP.
//!
//! Read-only is enforced by PostgreSQL itself, never by inspecting SQL text.
//! Each layer below is enough on its own:
//! 1. Queries run as a throwaway role (`pg0_ui_<pid>`) created per UI run. It
//!    is not a superuser and only holds `pg_read_all_data` and
//!    `pg_read_all_stats`: no write grants,
//!    no file or program access. (It can read every table, system catalogs
//!    like pg_authid included: reading is the point, writing is not.)
//! 2. Every query runs on a fresh connection inside `BEGIN READ ONLY`. The
//!    connection is dropped afterwards, which rolls the transaction back.
//! 3. User SQL must parse as one statement (checked with a protocol-level
//!    prepare), so `COMMIT; DROP ...` can't escape the read-only transaction.
//!
//! Custom SQL is refused in databases with `dblink`: it can open a second
//! connection as another user, outside these rules.
//!
//! The HTTP side only accepts Host headers that are an IP address or
//! `localhost` (blocks DNS rebinding), and requires a random token in a custom header on every API
//! call (other websites can't send custom headers without CORS, which we
//! never answer).

use crate::{is_process_named, list_instances, load_instance, running_pid, CliError, InstanceInfo};
use bytes::Bytes;
use futures_util::TryStreamExt;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::header::{HeaderValue, CONTENT_TYPE, HOST, ORIGIN};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::postgres::{PgConnectOptions, PgConnection, PgSslMode};
use sqlx::{Column, ConnectOptions, Connection, Either, Executor, Row, Statement, TypeInfo, ValueRef};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const PAGE: &str = include_str!("ui.html");
/// Bundled so the page looks the same offline (SIL Open Font License, see ui-fonts/).
const FONTS: [(&str, &[u8]); 3] = [
    ("/fonts/inter.woff2", include_bytes!("ui-fonts/inter.woff2")),
    ("/fonts/space-grotesk.woff2", include_bytes!("ui-fonts/space-grotesk.woff2")),
    ("/fonts/jetbrains-mono.woff2", include_bytes!("ui-fonts/jetbrains-mono.woff2")),
];
const QUERY_MAX_ROWS: usize = 1000;
const PAGE_ROWS: u64 = 100;
const MAX_CELL_CHARS: usize = 10_000;
const MAX_BODY_BYTES: usize = 1 << 20;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

struct State {
    token: String,
    nonce: String,
    role: String,
    /// Instance name -> password of our read-only role in that instance.
    roles: Mutex<HashMap<String, String>>,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ApiRequest {
    Instances,
    Databases { instance: String },
    Tables { instance: String, db: String },
    Structure { instance: String, db: String, schema: String, table: String },
    Rows {
        instance: String,
        db: String,
        schema: String,
        table: String,
        #[serde(default)]
        offset: u64,
        order: Option<String>,
        #[serde(default)]
        desc: bool,
    },
    Query { instance: String, db: String, sql: String },
    Overview { instance: String, db: String },
}

pub fn run(host: String, port: u16) -> Result<(), CliError> {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    rt.block_on(serve(host, port))
}

async fn serve(host: String, port: u16) -> Result<(), CliError> {
    let listener = tokio::net::TcpListener::bind((host.as_str(), port)).await.map_err(|e| {
        CliError::Other(format!("could not listen on {host}:{port} ({e}). Pick another port with --port"))
    })?;
    let port = listener.local_addr()?.port();
    let state = Arc::new(State {
        token: random_hex(32),
        nonce: random_hex(16),
        role: format!("pg0_ui_{}", std::process::id()),
        roles: Mutex::new(HashMap::new()),
    });

    // 0.0.0.0 / :: listen everywhere, so loopback works; otherwise use the given address.
    let local = match host.as_str() {
        "0.0.0.0" | "::" => "127.0.0.1".to_string(),
        h if h.contains(':') => format!("[{h}]"),
        h => h.to_string(),
    };
    let url = format!("http://{local}:{port}/#token={}", state.token);
    println!("pg0 ui is running (read-only) at:");
    println!();
    println!("  {url}");
    if let Some(ip) = lan_ip().filter(|_| host == "0.0.0.0" || host == "::") {
        println!("  http://{ip}:{port}/#token={}", state.token);
    }
    println!();
    println!("Anyone with this link can read all your data. Press Ctrl+C to stop.");
    open_browser(&url);

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else {
                    // e.g. out of file handles: don't spin, give connections time to close.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                };
                let state = state.clone();
                tokio::spawn(async move {
                    let service = hyper::service::service_fn(move |req| handle(state.clone(), req));
                    let _ = hyper::server::conn::http1::Builder::new()
                        .timer(TokioTimer::new())
                        .header_read_timeout(Duration::from_secs(10))
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
            _ = &mut shutdown => break,
        }
    }

    println!();
    println!("Stopping pg0 ui and removing its read-only users...");
    let names: Vec<String> = state.roles.lock().await.keys().cloned().collect();
    for name in names {
        if let Ok(Some(info)) = load_instance(&name) {
            let drop = async {
                let mut conn = admin_conn(&info).await?;
                conn.execute(format!("DROP ROLE IF EXISTS {}", quote_ident(&state.role)).as_str()).await?;
                Ok::<_, sqlx::Error>(())
            };
            let error = match tokio::time::timeout(Duration::from_secs(5), drop).await {
                Ok(Ok(())) => continue,
                Ok(Err(e)) => db_message(&e),
                Err(_) => "timed out".to_string(),
            };
            eprintln!(
                "Warning: could not remove read-only user {} from '{name}' ({error}). \
                 The next `pg0 ui` run removes it.",
                state.role
            );
        }
    }
    Ok(())
}

/// This machine's address on the local network: the source address the OS
/// would pick to reach a public IP. Connecting a UDP socket sends nothing.
fn lan_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip())
}

/// Accept `localhost` or an IP literal, never a domain name: a DNS rebinding
/// attack needs the browser to send the attacker's domain as the Host.
fn host_allowed(host: &str) -> bool {
    let name = match host.rsplit_once(':') {
        Some((name, port)) if !name.ends_with(':') && port.parse::<u16>().is_ok() => name,
        _ => host,
    };
    let name = name.trim_start_matches('[').trim_end_matches(']');
    name == "localhost" || name.parse::<std::net::IpAddr>().is_ok()
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = std::process::Command::new("xdg-open");
    // Best effort: on a headless box the printed link is enough.
    let _ = cmd
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::getrandom(&mut buf).expect("OS random number generator failed");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn quote_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

async fn handle(state: Arc<State>, req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let response = match route(&state, req).await {
        Ok(r) => r,
        Err((status, message)) => json_response(status, json!({ "error": message })),
    };
    Ok(response)
}

type ApiError = (StatusCode, String);

fn bad(message: impl Into<String>) -> ApiError {
    (StatusCode::BAD_REQUEST, message.into())
}

async fn route(state: &State, req: Request<Incoming>) -> Result<Response<Full<Bytes>>, ApiError> {
    let header = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok());
    let host = header(HOST.as_str()).filter(|h| host_allowed(h));
    // Same-origin only: the page's own fetches send Origin = http://<Host>.
    let origin_ok = header(ORIGIN.as_str()).is_none_or(|o| host.is_some_and(|h| o == format!("http://{h}")));
    let host_ok = host.is_some();
    if !host_ok || !origin_ok {
        return Err((StatusCode::FORBIDDEN, "forbidden".into()));
    }

    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => Ok(page_response(state)),
        (&Method::GET, path) if path.starts_with("/fonts/") => {
            let (_, font) = FONTS
                .iter()
                .find(|(p, _)| *p == path)
                .ok_or((StatusCode::NOT_FOUND, "not found".to_string()))?;
            let mut r = base_response(StatusCode::OK, "font/woff2", font.to_vec());
            r.headers_mut().insert("cache-control", HeaderValue::from_static("public, max-age=86400"));
            Ok(r)
        }
        (&Method::POST, "/api") => {
            let token_ok = header("x-pg0-token")
                .is_some_and(|t| constant_time_eq(t.as_bytes(), state.token.as_bytes()));
            if !token_ok {
                return Err((StatusCode::UNAUTHORIZED, "missing or wrong token: open the link printed by `pg0 ui`".into()));
            }
            let body = Limited::new(req.into_body(), MAX_BODY_BYTES)
                .collect()
                .await
                .map_err(|_| bad("request body too large"))?
                .to_bytes();
            let request: ApiRequest = serde_json::from_slice(&body).map_err(|e| bad(e.to_string()))?;
            let value = api(state, request).await.map_err(bad)?;
            Ok(json_response(StatusCode::OK, value))
        }
        _ => Err((StatusCode::NOT_FOUND, "not found".into())),
    }
}

fn base_response(status: StatusCode, content_type: &str, body: Vec<u8>) -> Response<Full<Bytes>> {
    let mut r = Response::new(Full::new(Bytes::from(body)));
    *r.status_mut() = status;
    let h = r.headers_mut();
    h.insert(CONTENT_TYPE, HeaderValue::from_str(content_type).unwrap());
    h.insert("cache-control", HeaderValue::from_static("no-store"));
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    h.insert("x-frame-options", HeaderValue::from_static("DENY"));
    h.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    r
}

fn json_response(status: StatusCode, value: Value) -> Response<Full<Bytes>> {
    base_response(status, "application/json", value.to_string().into_bytes())
}

fn page_response(state: &State) -> Response<Full<Bytes>> {
    let mut r = base_response(
        StatusCode::OK,
        "text/html; charset=utf-8",
        PAGE.replace("{{NONCE}}", &state.nonce).into_bytes(),
    );
    let csp = format!(
        "default-src 'none'; script-src 'nonce-{n}'; style-src 'nonce-{n}'; connect-src 'self'; font-src 'self'; \
         img-src data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        n = state.nonce
    );
    r.headers_mut().insert("content-security-policy", HeaderValue::from_str(&csp).unwrap());
    r
}

async fn api(state: &State, request: ApiRequest) -> Result<Value, String> {
    match request {
        ApiRequest::Instances => tokio::task::spawn_blocking(|| {
            let mut out = Vec::new();
            for name in list_instances().map_err(|e| e.to_string())? {
                if let Ok(Some(info)) = load_instance(&name) {
                    out.push(json!({
                        "name": name,
                        "running": running_pid(&info).is_some(),
                        "port": info.port,
                        "version": info.version,
                        "database": info.database,
                    }));
                }
            }
            Ok(json!(out))
        })
        .await
        .map_err(|e| e.to_string())?,
        ApiRequest::Databases { instance } => {
            let info = instance_info(&instance).await?;
            let mut conn = ro_conn(state, &instance, &info, &info.database).await?;
            let rows: Vec<(String,)> = sqlx::query_as(
                "SELECT datname::text FROM pg_database WHERE datallowconn AND NOT datistemplate ORDER BY 1",
            )
            .fetch_all(&mut conn)
            .await
            .map_err(|e| db_message(&e))?;
            Ok(json!(rows.into_iter().map(|r| r.0).collect::<Vec<_>>()))
        }
        ApiRequest::Tables { instance, db } => {
            let info = instance_info(&instance).await?;
            let mut conn = ro_conn(state, &instance, &info, &db).await?;
            let rows: Vec<(String, String, String, f64)> = sqlx::query_as(
                "SELECT n.nspname::text, c.relname::text, c.relkind::text, c.reltuples::float8 \
                 FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f') \
                   AND n.nspname NOT IN ('pg_catalog', 'information_schema') \
                   AND n.nspname NOT LIKE 'pg\\_toast%' AND n.nspname NOT LIKE 'pg\\_temp%' \
                 ORDER BY 1, 2",
            )
            .fetch_all(&mut conn)
            .await
            .map_err(|e| db_message(&e))?;
            Ok(json!(rows
                .into_iter()
                .map(|(schema, name, kind, rows)| json!({
                    "schema": schema, "name": name, "kind": kind,
                    "rows": if rows < 0.0 { Value::Null } else { json!(rows as i64) },
                }))
                .collect::<Vec<_>>()))
        }
        ApiRequest::Structure { instance, db, schema, table } => {
            let info = instance_info(&instance).await?;
            let mut conn = ro_conn(state, &instance, &info, &db).await?;
            let rel = format!("{}.{}", quote_ident(&schema), quote_ident(&table));
            let columns: Vec<(String, String, bool, Option<String>, bool)> = sqlx::query_as(
                "SELECT a.attname::text, format_type(a.atttypid, a.atttypmod), NOT a.attnotnull, \
                        pg_get_expr(d.adbin, d.adrelid), COALESCE(a.attnum = ANY(i.indkey), false) \
                 FROM pg_attribute a \
                 LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
                 LEFT JOIN pg_index i ON i.indrelid = a.attrelid AND i.indisprimary \
                 WHERE a.attrelid = $1::regclass AND a.attnum > 0 AND NOT a.attisdropped \
                 ORDER BY a.attnum",
            )
            .bind(&rel)
            .fetch_all(&mut conn)
            .await
            .map_err(|e| db_message(&e))?;
            let indexes: Vec<(String, String)> = sqlx::query_as(
                "SELECT indexname::text, indexdef FROM pg_indexes \
                 WHERE schemaname = $1 AND tablename = $2 ORDER BY 1",
            )
            .bind(&schema)
            .bind(&table)
            .fetch_all(&mut conn)
            .await
            .map_err(|e| db_message(&e))?;
            Ok(json!({
                "columns": columns.into_iter().map(|(name, ty, nullable, default, pk)| json!({
                    "name": name, "type": ty, "nullable": nullable, "default": default, "pk": pk,
                })).collect::<Vec<_>>(),
                "indexes": indexes.into_iter().map(|(name, def)| json!({ "name": name, "definition": def }))
                    .collect::<Vec<_>>(),
            }))
        }
        ApiRequest::Rows { instance, db, schema, table, offset, order, desc } => {
            let info = instance_info(&instance).await?;
            let mut conn = ro_conn(state, &instance, &info, &db).await?;
            let order_by = order
                .map(|c| format!(" ORDER BY {} {}", quote_ident(&c), if desc { "DESC" } else { "ASC" }))
                .unwrap_or_default();
            let sql = format!(
                "SELECT * FROM {}.{}{order_by} LIMIT {} OFFSET {offset}",
                quote_ident(&schema),
                quote_ident(&table),
                PAGE_ROWS + 1
            );
            let mut result = run_sql(&mut conn, &sql, PAGE_ROWS as usize).await?;
            result["offset"] = json!(offset);
            Ok(result)
        }
        ApiRequest::Overview { instance, db } => {
            let info = instance_info(&instance).await?;
            let mut conn = ro_conn(state, &instance, &info, &db).await?;
            let mut out = serde_json::Map::new();
            for (key, sql) in OVERVIEW_QUERIES {
                out.insert(key.to_string(), json_query(&mut conn, sql).await.map_err(|e| format!("{key}: {e}"))?);
            }
            // Only works when the extension is installed and preloaded. Runs
            // last: a failure aborts the transaction.
            let has_pgss = out["summary"]["extensions"]
                .as_array()
                .is_some_and(|exts| exts.iter().any(|e| e["name"] == "pg_stat_statements"));
            if has_pgss {
                out.insert("top_queries".into(), json_query(&mut conn, TOP_QUERIES).await.unwrap_or(Value::Null));
            }
            Ok(Value::Object(out))
        }
        ApiRequest::Query { instance, db, sql } => {
            let info = instance_info(&instance).await?;
            let mut conn = ro_conn(state, &instance, &info, &db).await?;
            let (has_dblink,): (bool,) =
                sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'dblink')")
                    .fetch_one(&mut conn)
                    .await
                    .map_err(|e| db_message(&e))?;
            if has_dblink {
                return Err("Custom SQL is turned off in this database because it has the dblink \
                            extension, which can open connections outside the UI's read-only rules. \
                            Browsing tables still works."
                    .into());
            }
            run_sql(&mut conn, &sql, QUERY_MAX_ROWS).await
        }
    }
}

/// Run a query that returns one JSON text value and parse it.
async fn json_query(conn: &mut PgConnection, sql: &str) -> Result<Value, String> {
    let text: Option<String> = sqlx::query_scalar(sql).fetch_one(conn).await.map_err(|e| db_message(&e))?;
    Ok(text.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null))
}

const OVERVIEW_QUERIES: [(&str, &str); 6] = [
    ("summary", r#"
SELECT json_build_object(
  'size', pg_database_size(current_database()),
  'version', current_setting('server_version'),
  'started', pg_postmaster_start_time(),
  'xid_age', (SELECT age(datfrozenxid) FROM pg_database WHERE datname = current_database()),
  'freeze_max_age', current_setting('autovacuum_freeze_max_age')::bigint,
  'connections', (SELECT count(*) FROM pg_stat_activity WHERE datname = current_database() AND application_name <> 'pg0 ui'),
  'max_connections', current_setting('max_connections')::int,
  'waiting_locks', (SELECT count(*) FROM pg_locks l JOIN pg_database d ON d.oid = l.database WHERE NOT l.granted AND d.datname = current_database()),
  'stats', (SELECT row_to_json(s) FROM (
      SELECT xact_commit, xact_rollback, blks_read, blks_hit, tup_returned, tup_fetched, tup_inserted,
             tup_updated, tup_deleted, temp_files, temp_bytes, deadlocks, stats_reset
      FROM pg_stat_database WHERE datname = current_database()) s),
  'settings', (SELECT json_agg(json_build_object('name', n, 'value', current_setting(n)))
      FROM unnest(ARRAY['shared_buffers', 'work_mem', 'maintenance_work_mem', 'effective_cache_size',
                        'max_connections', 'autovacuum', 'autovacuum_vacuum_scale_factor',
                        'random_page_cost', 'max_wal_size']) n),
  'extensions', (SELECT json_agg(json_build_object('name', extname, 'version', extversion) ORDER BY extname) FROM pg_extension)
)::text"#),
    ("tables", r#"
SELECT coalesce(json_agg(t ORDER BY t.total DESC), '[]')::text FROM (
  SELECT n.nspname AS schema, c.relname AS name, c.relkind::text AS kind,
    pg_total_relation_size(c.oid) AS total, pg_relation_size(c.oid) AS heap, pg_indexes_size(c.oid) AS indexes,
    CASE WHEN c.reltoastrelid <> 0 THEN pg_total_relation_size(c.reltoastrelid) ELSE 0 END AS toast,
    s.n_live_tup AS live, s.n_dead_tup AS dead, s.n_mod_since_analyze AS mods,
    s.seq_scan, s.idx_scan, s.n_tup_ins AS ins, s.n_tup_upd AS upd, s.n_tup_del AS del,
    greatest(s.last_vacuum, s.last_autovacuum) AS last_vacuum,
    greatest(s.last_analyze, s.last_autoanalyze) AS last_analyze,
    CASE WHEN c.relkind <> 'p' THEN age(c.relfrozenxid) END AS xid_age
  FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
  LEFT JOIN pg_stat_user_tables s ON s.relid = c.oid
  WHERE c.relkind IN ('r', 'p', 'm') AND n.nspname NOT IN ('pg_catalog', 'information_schema') AND n.nspname NOT LIKE 'pg\_toast%' AND n.nspname NOT LIKE 'pg\_temp%'
) t"#),
    ("indexes", r#"
SELECT coalesce(json_agg(t ORDER BY t.size DESC), '[]')::text FROM (
  SELECT n.nspname AS schema, tc.relname AS "table", ic.relname AS name, pg_relation_size(ic.oid) AS size,
    s.idx_scan AS scans, x.indisunique AS uniq, x.indisprimary AS pk, x.indisvalid AS valid, am.amname AS method
  FROM pg_index x
  JOIN pg_class ic ON ic.oid = x.indexrelid
  JOIN pg_class tc ON tc.oid = x.indrelid
  JOIN pg_namespace n ON n.oid = tc.relnamespace
  JOIN pg_am am ON am.oid = ic.relam
  LEFT JOIN pg_stat_user_indexes s ON s.indexrelid = x.indexrelid
  WHERE n.nspname NOT IN ('pg_catalog', 'information_schema') AND n.nspname NOT LIKE 'pg\_toast%' AND n.nspname NOT LIKE 'pg\_temp%'
) t"#),
    ("diagram", r#"
SELECT coalesce(json_agg(t), '[]')::text FROM (
  SELECT n.nspname AS schema, c.relname AS name,
    json_agg(json_build_object('name', a.attname, 'type', format_type(a.atttypid, a.atttypmod),
             'pk', coalesce(a.attnum = ANY(pk.conkey), false), 'nn', a.attnotnull) ORDER BY a.attnum) AS columns
  FROM pg_class c
  JOIN pg_namespace n ON n.oid = c.relnamespace
  JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped
  LEFT JOIN pg_constraint pk ON pk.conrelid = c.oid AND pk.contype = 'p'
  WHERE c.relkind IN ('r', 'p') AND NOT c.relispartition AND n.nspname NOT IN ('pg_catalog', 'information_schema') AND n.nspname NOT LIKE 'pg\_toast%' AND n.nspname NOT LIKE 'pg\_temp%'
  GROUP BY n.nspname, c.relname
) t"#),
    ("foreign_keys", r#"
SELECT coalesce(json_agg(t), '[]')::text FROM (
  SELECT con.conname AS name, sn.nspname AS schema, sc.relname AS "table",
    (SELECT array_agg(a.attname ORDER BY k.i) FROM unnest(con.conkey) WITH ORDINALITY k(n, i)
       JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.n) AS columns,
    tn.nspname AS ref_schema, tc.relname AS ref_table,
    (SELECT array_agg(a.attname ORDER BY k.i) FROM unnest(con.confkey) WITH ORDINALITY k(n, i)
       JOIN pg_attribute a ON a.attrelid = con.confrelid AND a.attnum = k.n) AS ref_columns
  FROM pg_constraint con
  JOIN pg_class sc ON sc.oid = con.conrelid JOIN pg_namespace sn ON sn.oid = sc.relnamespace
  JOIN pg_class tc ON tc.oid = con.confrelid JOIN pg_namespace tn ON tn.oid = tc.relnamespace
  WHERE con.contype = 'f' AND NOT sc.relispartition
) t"#),
    ("activity", r#"
SELECT json_build_object(
  'states', (SELECT coalesce(json_agg(s), '[]') FROM (
      SELECT coalesce(state, 'background') AS state, count(*) AS n FROM pg_stat_activity
      WHERE datname = current_database() AND application_name <> 'pg0 ui' GROUP BY 1 ORDER BY 2 DESC) s),
  'running', (SELECT coalesce(json_agg(q), '[]') FROM (
      SELECT pid, usename AS "user", application_name AS app, state, wait_event_type AS wait,
             extract(epoch FROM now() - coalesce(xact_start, query_start))::float8 AS seconds, query
      FROM pg_stat_activity
      WHERE datname = current_database() AND application_name <> 'pg0 ui' AND state IS DISTINCT FROM 'idle'
      ORDER BY coalesce(xact_start, query_start) LIMIT 10) q)
)::text"#),
];

const TOP_QUERIES: &str = r#"
SELECT coalesce(json_agg(t), '[]')::text FROM (
  SELECT query, calls, total_exec_time AS total_ms, mean_exec_time AS mean_ms, rows
  FROM pg_stat_statements WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
  ORDER BY total_exec_time DESC LIMIT 10
) t"#;

async fn instance_info(name: &str) -> Result<InstanceInfo, String> {
    let name = name.to_string();
    // running_pid runs `ps`: keep it off the async worker threads.
    tokio::task::spawn_blocking(move || {
        let info = load_instance(&name)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("instance '{name}' does not exist"))?;
        if running_pid(&info).is_none() {
            return Err(format!("instance '{name}' is not running. Start it with: pg0 start --name {name}"));
        }
        Ok(info)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn db_message(e: &sqlx::Error) -> String {
    match e.as_database_error() {
        Some(db) => db.message().to_string(),
        None => e.to_string(),
    }
}

fn base_options(info: &InstanceInfo) -> PgConnectOptions {
    PgConnectOptions::new()
        .host("127.0.0.1")
        .port(info.port)
        .ssl_mode(PgSslMode::Disable)
        .application_name("pg0 ui")
        .statement_cache_capacity(0)
        .disable_statement_logging()
}

async fn connect(options: PgConnectOptions) -> Result<PgConnection, sqlx::Error> {
    tokio::time::timeout(CONNECT_TIMEOUT, PgConnection::connect_with(&options))
        .await
        .map_err(|_| sqlx::Error::PoolTimedOut)?
}

/// A superuser connection, used only to create and drop the read-only role.
async fn admin_conn(info: &InstanceInfo) -> Result<PgConnection, sqlx::Error> {
    connect(
        base_options(info)
            .username(&info.username)
            .password(&info.password)
            .database(&info.database),
    )
    .await
}

/// (Re)create this run's read-only role in the instance and return its password.
async fn create_role(state: &State, info: &InstanceInfo) -> Result<String, String> {
    let mut conn = admin_conn(info).await.map_err(|e| db_message(&e))?;

    // Remove roles left behind by `pg0 ui` runs that are no longer alive.
    let stale: Vec<(String,)> = sqlx::query_as("SELECT rolname::text FROM pg_roles WHERE rolname ~ '^pg0_ui_[0-9]+$'")
        .fetch_all(&mut conn)
        .await
        .map_err(|e| db_message(&e))?;
    for (role,) in stale {
        let pid: u32 = role["pg0_ui_".len()..].parse().unwrap_or(0);
        if role != state.role && !is_process_named(pid, "pg0") {
            let _ = conn.execute(format!("DROP ROLE IF EXISTS {}", quote_ident(&role)).as_str()).await;
        }
    }

    // Always start from a fresh role, so nothing granted to an old one carries over.
    let role = quote_ident(&state.role);
    let password = random_hex(24);
    for sql in [
        format!("DROP ROLE IF EXISTS {role}"),
        // The password is hex, so it is safe inside the literal.
        format!(
            "CREATE ROLE {role} LOGIN PASSWORD '{password}' NOSUPERUSER NOCREATEDB NOCREATEROLE \
             NOREPLICATION NOBYPASSRLS INHERIT"
        ),
        // pg_read_all_stats: see every session's query in pg_stat_activity.
        format!("GRANT pg_read_all_data, pg_read_all_stats TO {role}"),
    ] {
        conn.execute(sql.as_str()).await.map_err(|e| db_message(&e))?;
    }
    Ok(password)
}

/// A fresh read-only connection, already inside `BEGIN READ ONLY`. Dropping it
/// closes the socket, and PostgreSQL rolls the transaction back.
async fn ro_conn(state: &State, name: &str, info: &InstanceInfo, db: &str) -> Result<PgConnection, String> {
    // The lock only covers role setup (concurrent first requests must not race
    // on CREATE ROLE). Connecting happens outside it, so one slow instance
    // doesn't hold up requests to the others.
    let password = {
        let mut roles = state.roles.lock().await;
        if !roles.contains_key(name) {
            roles.insert(name.to_string(), create_role(state, info).await?);
        }
        roles[name].clone()
    };
    let options = |password: &str| {
        base_options(info)
            .username(&state.role)
            .password(password)
            .database(db)
            .options([
                ("default_transaction_read_only", "on"),
                ("statement_timeout", "30s"),
                ("lock_timeout", "5s"),
                ("idle_in_transaction_session_timeout", "60s"),
            ])
    };
    let mut conn = match connect(options(&password)).await {
        Ok(conn) => conn,
        // Auth failed: the instance was recreated or someone dropped the role.
        Err(e) if e.as_database_error().and_then(|d| d.code()).is_some_and(|c| c.starts_with("28")) => {
            let password = {
                let mut roles = state.roles.lock().await;
                // Another request may have recreated it already.
                if roles.get(name) == Some(&password) {
                    roles.insert(name.to_string(), create_role(state, info).await?);
                }
                roles[name].clone()
            };
            connect(options(&password)).await.map_err(|e| db_message(&e))?
        }
        Err(e) => return Err(db_message(&e)),
    };
    conn.execute("BEGIN READ ONLY").await.map_err(|e| db_message(&e))?;
    Ok(conn)
}

/// Run one statement and return its columns and up to `max_rows` rows as text.
async fn run_sql(conn: &mut PgConnection, sql: &str, max_rows: usize) -> Result<Value, String> {
    let started = Instant::now();
    // Protocol-level prepare rejects more than one statement, and gives us the
    // column names and types even when the result has no rows.
    let statement = (&mut *conn).prepare(sql).await.map_err(|e| db_message(&e))?;
    let columns: Vec<Value> = statement
        .columns()
        .iter()
        .map(|c| json!({ "name": c.name(), "type": c.type_info().name() }))
        .collect();

    // Run it unprepared (simple protocol): every value comes back as text, so
    // any type, extension types included, displays as PostgreSQL prints it.
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut truncated = false;
    let mut rows_affected = None;
    let mut stream = sqlx::raw_sql(sql).fetch_many(&mut *conn);
    while let Some(item) = stream.try_next().await.map_err(|e| db_message(&e))? {
        match item {
            Either::Left(done) => rows_affected = Some(done.rows_affected()),
            Either::Right(row) => {
                if rows.len() == max_rows {
                    truncated = true;
                    break;
                }
                rows.push(
                    (0..row.len())
                        .map(|i| match row.try_get_raw(i) {
                            Ok(v) if v.is_null() => Value::Null,
                            Ok(v) => json!(cap(String::from_utf8_lossy(v.as_bytes().unwrap_or_default()))),
                            Err(_) => Value::Null,
                        })
                        .collect(),
                );
            }
        }
    }
    Ok(json!({
        "columns": columns,
        "rows": rows,
        "truncated": truncated,
        "rows_affected": rows_affected,
        "elapsed_ms": started.elapsed().as_millis() as u64,
    }))
}

fn cap(s: std::borrow::Cow<str>) -> String {
    match s.char_indices().nth(MAX_CELL_CHARS) {
        Some((at, _)) => format!("{}…", &s[..at]),
        None => s.into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_ident_escapes_double_quotes() {
        assert_eq!(quote_ident("users"), "\"users\"");
        assert_eq!(quote_ident("a\"; DROP TABLE x; --"), "\"a\"\"; DROP TABLE x; --\"");
    }

    #[test]
    fn host_check_blocks_domain_names() {
        for ok in ["127.0.0.1:5050", "localhost:5050", "192.168.1.20:5050", "[::1]:5050", "10.0.0.2"] {
            assert!(host_allowed(ok), "{ok}");
        }
        for bad in ["evil.com:5050", "evil.com", "127.0.0.1.evil.com:5050", "localhost.evil.com"] {
            assert!(!host_allowed(bad), "{bad}");
        }
    }

    #[test]
    fn token_compare() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }

    #[test]
    fn cap_truncates_long_cells() {
        assert_eq!(cap("short".into()), "short");
        let long = "é".repeat(MAX_CELL_CHARS + 5);
        assert_eq!(cap(long.into()).chars().count(), MAX_CELL_CHARS + 1);
    }
}
