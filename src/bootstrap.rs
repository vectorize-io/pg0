//! The role and grant steps of `pg0 start`. They run in-process through sqlx with the password in
//! memory, so no password reaches the argument list, environment, or a file of a child process.

use sqlx::postgres::{PgConnectOptions, PgConnection, PgSslMode};
use sqlx::{ConnectOptions, Connection};

/// The program names of the child processes that `pg0 start` spawns itself.
pub fn bootstrap_child_commands() -> Vec<String> {
    vec!["initdb".to_string()]
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn quote_literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// The statements that create a superuser role. `SET log_min_error_statement` comes first and keeps
/// the password out of the server log when `CREATE USER` fails. `CREATE USER` is a direct statement
/// because a failure inside a DO block logs the password in its CONTEXT line.
pub fn role_statements(username: &str, password: &str) -> Vec<String> {
    vec![
        "SET log_min_error_statement = 'panic'".to_string(),
        format!(
            "CREATE USER {} WITH SUPERUSER PASSWORD {}",
            quote_ident(username),
            quote_literal(password)
        ),
    ]
}

pub fn grant_statement(database: &str, username: &str) -> String {
    format!(
        "GRANT ALL PRIVILEGES ON DATABASE {} TO {}",
        quote_ident(database),
        quote_ident(username)
    )
}

async fn connect(port: u16, password: &str) -> Result<PgConnection, sqlx::Error> {
    PgConnectOptions::new()
        .host("127.0.0.1")
        .port(port)
        .username("postgres")
        .password(password)
        .database("postgres")
        .ssl_mode(PgSslMode::Disable)
        .application_name("pg0 start")
        .disable_statement_logging()
        .connect()
        .await
}

fn run<F: std::future::Future<Output = Result<(), sqlx::Error>>>(
    future: F,
) -> Result<(), crate::CliError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime
        .block_on(future)
        .map_err(|e| crate::CliError::Other(e.to_string()))
}

/// Create the role unless it exists.
pub fn create_role(port: u16, password: &str, username: &str) -> Result<(), crate::CliError> {
    run(async {
        let mut conn = connect(port, password).await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT FROM pg_roles WHERE rolname = $1)")
                .bind(username)
                .fetch_one(&mut conn)
                .await?;
        if !exists {
            for statement in role_statements(username, password) {
                sqlx::raw_sql(&statement).execute(&mut conn).await?;
            }
        }
        conn.close().await
    })
}

/// Grant the role all privileges on the database.
pub fn grant_database(
    port: u16,
    password: &str,
    database: &str,
    username: &str,
) -> Result<(), crate::CliError> {
    run(async {
        let mut conn = connect(port, password).await?;
        sqlx::raw_sql(&grant_statement(database, username))
            .execute(&mut conn)
            .await?;
        conn.close().await
    })
}
