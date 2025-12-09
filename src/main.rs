use clap::{Parser, Subcommand};
use flate2::read::GzDecoder;
use postgresql_embedded::blocking::PostgreSQL;
use postgresql_embedded::{Settings, VersionReq};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process;
use tar::Archive;
use thiserror::Error;
use tracing_subscriber::EnvFilter;

/// Whether PostgreSQL is bundled in this binary
fn is_postgresql_bundled() -> bool {
    env!("POSTGRESQL_BUNDLED") == "true"
}

/// The embedded PostgreSQL bundle (empty if not bundled)
static POSTGRESQL_BUNDLE: &[u8] = include_bytes!(env!("POSTGRESQL_BUNDLE_PATH"));

#[derive(Error, Debug)]
enum CliError {
    #[error("PostgreSQL error: {0}")]
    PostgreSQL(#[from] postgresql_embedded::Error),
    #[error("Extension error: {0}")]
    Extension(#[from] postgresql_extensions::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("No running instance found")]
    NoInstance,
    #[error("Instance already running (pid: {0})")]
    AlreadyRunning(u32),
    #[error("Could not determine data directory")]
    NoDataDir,
    #[error("Failed to parse PID from postmaster.pid")]
    PidParse,
    #[error("Extension '{0}' not found")]
    ExtensionNotFound(String),
    #[error("{0}")]
    Other(String),
}

#[derive(Parser)]
#[command(name = "pg0")]
#[command(about = "Zero-dependency CLI to run embedded PostgreSQL locally", long_about = None)]
#[command(version)]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

const DEFAULT_INSTANCE_NAME: &str = "default";

#[derive(Subcommand)]
enum Commands {
    /// Start PostgreSQL server
    Start {
        /// Instance name (allows running multiple instances)
        #[arg(long, default_value = DEFAULT_INSTANCE_NAME)]
        name: String,

        /// Port to listen on (auto-allocates if not specified and default port is in use)
        #[arg(short, long)]
        port: Option<u16>,

        /// PostgreSQL version (must match bundled version)
        #[arg(short = 'V', long, default_value = env!("PG_VERSION"))]
        version: String,

        /// Data directory (defaults to ~/.pg0/instances/<name>/data)
        #[arg(short, long)]
        data_dir: Option<String>,

        /// Username for the database
        #[arg(short, long, default_value = "postgres")]
        username: String,

        /// Password for the database
        #[arg(short = 'P', long, default_value = "postgres")]
        password: String,

        /// Database name to create
        #[arg(short = 'n', long, default_value = "postgres")]
        database: String,

        /// PostgreSQL configuration options (can be used multiple times)
        /// Example: -c shared_buffers=512MB -c work_mem=128MB
        #[arg(short = 'c', long = "config", value_name = "KEY=VALUE")]
        config: Vec<String>,
    },
    /// Stop PostgreSQL server
    Stop {
        /// Instance name
        #[arg(long, default_value = DEFAULT_INSTANCE_NAME)]
        name: String,
    },
    /// Drop an instance (stop if running, delete all data)
    Drop {
        /// Instance name
        #[arg(long, default_value = DEFAULT_INSTANCE_NAME)]
        name: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
    /// Show PostgreSQL server info (status, connection URI, etc.)
    Info {
        /// Instance name
        #[arg(long, default_value = DEFAULT_INSTANCE_NAME)]
        name: String,

        /// Output format
        #[arg(short, long, default_value = "text")]
        output: OutputFormat,
    },
    /// List all instances
    List {
        /// Output format
        #[arg(short, long, default_value = "text")]
        output: OutputFormat,
    },
    /// Open psql shell connected to the running instance
    Psql {
        /// Instance name
        #[arg(long, default_value = DEFAULT_INSTANCE_NAME)]
        name: String,

        /// Additional arguments to pass to psql
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Show PostgreSQL logs
    Logs {
        /// Instance name
        #[arg(long, default_value = DEFAULT_INSTANCE_NAME)]
        name: String,

        /// Number of lines to show (default: all)
        #[arg(short = 'n', long)]
        lines: Option<usize>,

        /// Follow log output (like tail -f)
        #[arg(short, long)]
        follow: bool,
    },
    /// Install a PostgreSQL extension (e.g., pgvector)
    InstallExtension {
        /// Instance name
        #[arg(long, default_value = DEFAULT_INSTANCE_NAME)]
        name: String,

        /// Extension name (e.g., "vector", "postgis")
        extension: String,
    },
    /// List available extensions
    ListExtensions,
}

#[derive(Clone, Debug, Default, clap::ValueEnum)]
enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Serialize, Deserialize)]
struct InstanceInfo {
    pid: u32,
    port: u16,
    data_dir: PathBuf,
    installation_dir: PathBuf,
    username: String,
    password: String,
    database: String,
    version: String,
}

#[derive(Serialize)]
struct InfoOutput {
    name: String,
    running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    uri: Option<String>,
}

fn get_base_dir() -> Result<PathBuf, CliError> {
    dirs::home_dir()
        .map(|h| h.join(".pg0"))
        .ok_or(CliError::NoDataDir)
}

fn get_instances_dir() -> Result<PathBuf, CliError> {
    Ok(get_base_dir()?.join("instances"))
}

fn get_instance_dir(name: &str) -> Result<PathBuf, CliError> {
    Ok(get_instances_dir()?.join(name))
}

fn get_state_file(name: &str) -> Result<PathBuf, CliError> {
    Ok(get_instance_dir(name)?.join("instance.json"))
}

fn load_instance(name: &str) -> Result<Option<InstanceInfo>, CliError> {
    let state_file = get_state_file(name)?;
    if state_file.exists() {
        let content = fs::read_to_string(&state_file)?;
        Ok(Some(serde_json::from_str(&content)?))
    } else {
        Ok(None)
    }
}

fn save_instance(name: &str, info: &InstanceInfo) -> Result<(), CliError> {
    let instance_dir = get_instance_dir(name)?;
    fs::create_dir_all(&instance_dir)?;
    let state_file = get_state_file(name)?;
    fs::write(&state_file, serde_json::to_string_pretty(info)?)?;
    Ok(())
}

fn remove_instance(name: &str) -> Result<(), CliError> {
    let state_file = get_state_file(name)?;
    if state_file.exists() {
        fs::remove_file(&state_file)?;
    }
    Ok(())
}

fn list_instances() -> Result<Vec<String>, CliError> {
    let instances_dir = get_instances_dir()?;
    if !instances_dir.exists() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in fs::read_dir(&instances_dir)? {
        let entry = entry?;
        if entry.path().is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                // Check if it has an instance.json file
                if entry.path().join("instance.json").exists() {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    Ok(names)
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use std::process::Command;
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        use std::process::Command;
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

/// Read the PID from PostgreSQL's postmaster.pid file
fn read_postmaster_pid(data_dir: &PathBuf) -> Result<u32, CliError> {
    let pid_file = data_dir.join("postmaster.pid");
    let content = fs::read_to_string(&pid_file)?;
    // First line of postmaster.pid is the PID
    content
        .lines()
        .next()
        .and_then(|line| line.trim().parse().ok())
        .ok_or(CliError::PidParse)
}

/// Expand ~ to home directory
fn expand_path(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

/// Check if a port is available for binding
fn is_port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Find an available port, starting from the given port
fn find_available_port(start_port: u16) -> u16 {
    let mut port = start_port;
    while !is_port_available(port) {
        port += 1;
        if port > 65535 - 100 {
            // Wrap around to a random high port if we've gone too far
            port = 49152; // Start of dynamic/private port range
        }
    }
    port
}

/// Extract the bundled PostgreSQL to the installation directory
/// Returns the path to the version-specific directory (e.g., ~/.pg0/installation/18.1.0)
fn extract_bundled_postgresql(installation_dir: &PathBuf, pg_version: &str) -> Result<PathBuf, CliError> {
    let version_dir = installation_dir.join(pg_version);

    // Check if already extracted
    let bin_dir = version_dir.join("bin");
    if bin_dir.exists() && bin_dir.join("postgres").exists() {
        tracing::debug!("PostgreSQL already extracted at {}", version_dir.display());
        return Ok(version_dir);
    }

    if POSTGRESQL_BUNDLE.is_empty() {
        return Err(CliError::Other(
            "PostgreSQL bundle is empty - this binary was not built with BUNDLE_POSTGRESQL=true".to_string()
        ));
    }

    println!("Extracting bundled PostgreSQL {}...", pg_version);
    fs::create_dir_all(&version_dir)?;

    // Extract the tar.gz bundle
    // The archive contains paths like "postgresql-18.1.0-aarch64-apple-darwin/bin/postgres"
    // We need to extract to version_dir, stripping the first path component
    let decoder = GzDecoder::new(POSTGRESQL_BUNDLE);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        // Strip the first component (e.g., "postgresql-18.1.0-aarch64-apple-darwin")
        let stripped_path: PathBuf = path.components().skip(1).collect();
        if stripped_path.as_os_str().is_empty() {
            continue; // Skip the root directory entry
        }

        let dest_path = version_dir.join(&stripped_path);

        // Create parent directories if needed
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Extract the entry
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&dest_path)?;
        } else {
            entry.unpack(&dest_path)?;
        }
    }

    // Verify extraction
    if !bin_dir.join("postgres").exists() {
        return Err(CliError::Other(format!(
            "PostgreSQL extraction failed - postgres binary not found at {}",
            bin_dir.display()
        )));
    }

    // Make binaries executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(entries) = fs::read_dir(&bin_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(metadata) = path.metadata() {
                        let mut perms = metadata.permissions();
                        perms.set_mode(0o755);
                        let _ = fs::set_permissions(&path, perms);
                    }
                }
            }
        }
        // Also make lib files executable/accessible
        let lib_dir = version_dir.join("lib");
        if let Ok(entries) = fs::read_dir(&lib_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(metadata) = path.metadata() {
                        let mut perms = metadata.permissions();
                        perms.set_mode(0o755);
                        let _ = fs::set_permissions(&path, perms);
                    }
                }
            }
        }
    }

    println!("PostgreSQL {} extracted successfully.", pg_version);
    Ok(version_dir)
}

/// Get the current platform string for downloads
fn get_platform() -> Option<&'static str> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    { Some("aarch64-apple-darwin") }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    { Some("x86_64-apple-darwin") }

    // Linux x86_64 - distinguish between musl and gnu
    #[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "musl"))]
    { Some("x86_64-unknown-linux-musl") }
    #[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    { Some("x86_64-unknown-linux-gnu") }

    // Linux aarch64 - distinguish between musl and gnu
    #[cfg(all(target_os = "linux", target_arch = "aarch64", target_env = "musl"))]
    { Some("aarch64-unknown-linux-musl") }
    #[cfg(all(target_os = "linux", target_arch = "aarch64", target_env = "gnu"))]
    { Some("aarch64-unknown-linux-gnu") }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    { Some("x86_64-pc-windows-msvc") }

    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64", target_env = "musl"),
        all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"),
        all(target_os = "linux", target_arch = "aarch64", target_env = "musl"),
        all(target_os = "linux", target_arch = "aarch64", target_env = "gnu"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    { None }
}

/// Install pgvector extension files into the PostgreSQL installation
fn install_pgvector(installation_dir: &PathBuf, pg_version: &str) -> Result<(), CliError> {
    let platform = get_platform().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Unsupported, "Unsupported platform for pgvector")
    })?;

    let pg_major = pg_version.split('.').next().unwrap_or("16");
    let pgvector_version = env!("PGVECTOR_VERSION");
    let pgvector_tag = env!("PGVECTOR_COMPILED_TAG");
    let pgvector_repo = env!("PGVECTOR_COMPILED_REPO");

    let url = format!(
        "https://github.com/{}/releases/download/{}/pgvector-{}-pg{}.tar.gz",
        pgvector_repo, pgvector_tag, platform, pg_major
    );

    println!("Installing pgvector {}...", pgvector_version);
    tracing::debug!("Downloading pgvector from {}", url);

    // Find the version-specific installation directory
    let version_dir = fs::read_dir(installation_dir)?
        .filter_map(|e| e.ok())
        .find(|e| e.path().is_dir() && e.file_name().to_string_lossy().starts_with(pg_major))
        .map(|e| e.path())
        .ok_or_else(|| std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "PostgreSQL installation directory not found"
        ))?;

    let lib_dir = version_dir.join("lib");
    let extension_dir = version_dir.join("share").join("extension");

    // Check if pgvector is already installed
    if extension_dir.join("vector.control").exists() {
        tracing::debug!("pgvector already installed");
        return Ok(());
    }

    // Download using curl
    let temp_dir = std::env::temp_dir().join("pgvector_download");
    fs::create_dir_all(&temp_dir)?;
    let archive_path = temp_dir.join("pgvector.tar.gz");

    let status = std::process::Command::new("curl")
        .args(["-fsSL", &url, "-o"])
        .arg(&archive_path)
        .status()?;

    if !status.success() {
        fs::remove_dir_all(&temp_dir).ok();
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to download pgvector from {}", url)
        ).into());
    }

    // Extract using tar
    let extract_dir = temp_dir.join("extracted");
    fs::create_dir_all(&extract_dir)?;

    let status = std::process::Command::new("tar")
        .args(["-xzf"])
        .arg(&archive_path)
        .arg("-C")
        .arg(&extract_dir)
        .status()?;

    if !status.success() {
        fs::remove_dir_all(&temp_dir).ok();
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to extract pgvector archive"
        ).into());
    }

    // Copy files to PostgreSQL installation
    fn copy_files_recursive(src: &PathBuf, lib_dir: &PathBuf, ext_dir: &PathBuf) -> std::io::Result<()> {
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                copy_files_recursive(&path, lib_dir, ext_dir)?;
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".so") || name.ends_with(".dylib") || name.ends_with(".dll") {
                    fs::copy(&path, lib_dir.join(name))?;
                } else if name == "vector.control" || name.starts_with("vector--") {
                    fs::copy(&path, ext_dir.join(name))?;
                }
            }
        }
        Ok(())
    }

    copy_files_recursive(&extract_dir, &lib_dir, &extension_dir)?;

    // Cleanup
    fs::remove_dir_all(&temp_dir).ok();

    println!("pgvector {} installed successfully!", pgvector_version);
    Ok(())
}

fn start(
    name: String,
    port: u16,
    port_was_specified: bool,
    version: String,
    data_dir: Option<String>,
    username: String,
    password: String,
    database: String,
    config: Vec<String>,
) -> Result<(), CliError> {
    // Check if already running
    if let Some(info) = load_instance(&name)? {
        if is_process_running(info.pid) {
            return Err(CliError::AlreadyRunning(info.pid));
        }
        // Stale instance, clean up
        remove_instance(&name)?;
    }

    // Auto-allocate port if the requested port is in use (only if port wasn't explicitly specified)
    let port = if !port_was_specified && !is_port_available(port) {
        let new_port = find_available_port(port);
        println!("Port {} is in use, using port {} instead.", port, new_port);
        new_port
    } else {
        port
    };

    let base_dir = get_base_dir()?;
    let instance_dir = get_instance_dir(&name)?;

    // Use provided data_dir or default to instance-specific directory
    let data_dir = match data_dir {
        Some(dir) => expand_path(&dir),
        None => instance_dir.join("data"),
    };

    let installation_dir = base_dir.join("installation");

    fs::create_dir_all(&data_dir)?;
    fs::create_dir_all(&installation_dir)?;

    println!("Setting up PostgreSQL {}...", version);

    let version_req: VersionReq = version.parse().map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid version: {}", e),
        )
    })?;

    // Build configuration HashMap with sensible defaults
    let mut configuration: HashMap<String, String> = HashMap::new();

    // Apply opinionated defaults optimized for vector/AI workloads
    configuration.insert("shared_buffers".to_string(), "256MB".to_string());
    configuration.insert("maintenance_work_mem".to_string(), "512MB".to_string());
    configuration.insert("effective_cache_size".to_string(), "1GB".to_string());
    configuration.insert("max_parallel_maintenance_workers".to_string(), "4".to_string());
    configuration.insert("work_mem".to_string(), "64MB".to_string());

    // Parse and apply custom config options (these override defaults)
    for cfg in &config {
        if let Some((key, value)) = cfg.split_once('=') {
            configuration.insert(key.trim().to_string(), value.trim().to_string());
        } else {
            eprintln!("Warning: Invalid config format '{}', expected KEY=VALUE", cfg);
        }
    }

    // If PostgreSQL is bundled, extract it and use trust_installation_dir
    // Otherwise, fall back to downloading via postgresql_embedded
    let (settings, use_bundled) = if is_postgresql_bundled() {
        // Extract bundled PostgreSQL
        let version_install_dir = extract_bundled_postgresql(&installation_dir, &version)?;

        let settings = Settings {
            version: version_req,
            port,
            username: username.clone(),
            password: password.clone(),
            data_dir: data_dir.clone(),
            installation_dir: version_install_dir,
            configuration,
            trust_installation_dir: true, // Skip download, use our extracted files
            ..Default::default()
        };
        (settings, true)
    } else {
        let settings = Settings {
            version: version_req,
            port,
            username: username.clone(),
            password: password.clone(),
            data_dir: data_dir.clone(),
            installation_dir: installation_dir.clone(),
            configuration,
            ..Default::default()
        };
        (settings, false)
    };

    let mut postgresql = PostgreSQL::new(settings);

    if !use_bundled {
        println!("Downloading and installing PostgreSQL (this may take a moment on first run)...");
    }
    postgresql.setup()?;

    // Install pgvector extension
    if let Err(e) = install_pgvector(&installation_dir, &version) {
        eprintln!("Warning: Failed to install pgvector: {}", e);
        eprintln!("You can try installing it manually with: pg0 install-extension vector");
    }

    println!("Starting PostgreSQL on port {}...", port);
    postgresql.start()?;

    // Create the user if it's not the default 'postgres'
    // Note: postgresql_embedded always creates 'postgres' as the superuser
    if username != "postgres" {
        println!("Creating user '{}'...", username);
        let psql_path = find_psql_binary(&installation_dir)?;
        let create_user_sql = format!(
            "DO $$ BEGIN IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '{}') THEN CREATE USER \"{}\" WITH SUPERUSER PASSWORD '{}'; END IF; END $$;",
            username, username, password.replace('\'', "''")
        );
        let status = std::process::Command::new(&psql_path)
            .arg(&format!("postgresql://postgres:{}@localhost:{}/postgres", password, port))
            .arg("-c")
            .arg(&create_user_sql)
            .status()?;
        if !status.success() {
            eprintln!("Warning: Failed to create user '{}'", username);
        }
    }

    // Create the database if it doesn't exist and it's not the default 'postgres'
    if database != "postgres" {
        println!("Creating database '{}'...", database);
        if let Err(e) = postgresql.create_database(&database) {
            // Ignore error if database already exists
            let err_str = e.to_string();
            if !err_str.contains("already exists") {
                return Err(e.into());
            }
        }
        // Grant privileges to the user on the database
        if username != "postgres" {
            let psql_path = find_psql_binary(&installation_dir)?;
            let grant_sql = format!("GRANT ALL PRIVILEGES ON DATABASE \"{}\" TO \"{}\";", database, username);
            let _ = std::process::Command::new(&psql_path)
                .arg(&format!("postgresql://postgres:{}@localhost:{}/postgres", password, port))
                .arg("-c")
                .arg(&grant_sql)
                .status();
        }
    }

    // Read PID from postmaster.pid file
    let pid = read_postmaster_pid(&data_dir)?;

    let info = InstanceInfo {
        pid,
        port,
        data_dir: data_dir.clone(),
        installation_dir,
        username: username.clone(),
        password: password.clone(),
        database: database.clone(),
        version: version.clone(),
    };

    save_instance(&name, &info)?;

    println!();
    println!("PostgreSQL is running!");
    println!("  Instance: {}", name);
    println!("  PID:      {}", pid);
    println!("  Port:     {}", port);
    println!("  Username: {}", username);
    println!("  Password: {}", password);
    println!("  Database: {}", database);
    println!("  Data dir: {}", data_dir.display());
    println!();
    println!(
        "Connection URI: postgresql://{}:{}@localhost:{}/{}",
        username, password, port, database
    );
    println!();
    if name == DEFAULT_INSTANCE_NAME {
        println!("Use 'pg0 stop' to stop the server.");
    } else {
        println!("Use 'pg0 stop --name {}' to stop the server.", name);
    }

    // Detach - let the process continue running
    std::mem::forget(postgresql);

    Ok(())
}

fn stop(name: String) -> Result<(), CliError> {
    let info = load_instance(&name)?.ok_or(CliError::NoInstance)?;

    if !is_process_running(info.pid) {
        println!("PostgreSQL instance '{}' is not running.", name);
        return Ok(());
    }

    println!("Stopping PostgreSQL instance '{}' (pid: {})...", name, info.pid);

    // Send SIGTERM to gracefully stop
    #[cfg(unix)]
    {
        use std::process::Command;
        let _ = Command::new("kill")
            .args(["-TERM", &info.pid.to_string()])
            .output();
    }
    #[cfg(windows)]
    {
        use std::process::Command;
        let _ = Command::new("taskkill")
            .args(["/PID", &info.pid.to_string()])
            .output();
    }

    // Wait a bit for graceful shutdown
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Force kill if still running
    if is_process_running(info.pid) {
        #[cfg(unix)]
        {
            use std::process::Command;
            let _ = Command::new("kill")
                .args(["-9", &info.pid.to_string()])
                .output();
        }
        #[cfg(windows)]
        {
            use std::process::Command;
            let _ = Command::new("taskkill")
                .args(["/F", "/PID", &info.pid.to_string()])
                .output();
        }
    }

    println!("PostgreSQL instance '{}' stopped.", name);

    Ok(())
}

fn drop_instance(name: String, force: bool) -> Result<(), CliError> {
    let instance = load_instance(&name)?;

    if instance.is_none() {
        println!("Instance '{}' does not exist.", name);
        return Ok(());
    }

    let info = instance.unwrap();

    // Confirmation prompt unless --force
    if !force {
        println!("This will permanently delete instance '{}' and all its data:", name);
        println!("  Data dir: {}", info.data_dir.display());
        println!();
        print!("Are you sure? [y/N] ");
        std::io::Write::flush(&mut std::io::stdout())?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    // Stop if running
    if is_process_running(info.pid) {
        println!("Stopping PostgreSQL instance '{}' (pid: {})...", name, info.pid);
        #[cfg(unix)]
        {
            use std::process::Command;
            let _ = Command::new("kill")
                .args(["-TERM", &info.pid.to_string()])
                .output();
        }
        #[cfg(windows)]
        {
            use std::process::Command;
            let _ = Command::new("taskkill")
                .args(["/PID", &info.pid.to_string()])
                .output();
        }
        std::thread::sleep(std::time::Duration::from_secs(2));

        if is_process_running(info.pid) {
            #[cfg(unix)]
            {
                use std::process::Command;
                let _ = Command::new("kill")
                    .args(["-9", &info.pid.to_string()])
                    .output();
            }
            #[cfg(windows)]
            {
                use std::process::Command;
                let _ = Command::new("taskkill")
                    .args(["/F", "/PID", &info.pid.to_string()])
                    .output();
            }
        }
    }

    // Delete data directory
    if info.data_dir.exists() {
        println!("Deleting data directory: {}", info.data_dir.display());
        fs::remove_dir_all(&info.data_dir)?;
    }

    // Delete instance directory (contains instance.json)
    let instance_dir = get_instance_dir(&name)?;
    if instance_dir.exists() {
        fs::remove_dir_all(&instance_dir)?;
    }

    println!("Instance '{}' dropped.", name);

    Ok(())
}

fn info(name: String, output_format: OutputFormat) -> Result<(), CliError> {
    let instance = load_instance(&name)?;

    let output = match instance {
        Some(info) => {
            let running = is_process_running(info.pid);
            if running {
                let uri = format!(
                    "postgresql://{}:{}@localhost:{}/{}",
                    info.username, info.password, info.port, info.database
                );
                InfoOutput {
                    name: name.clone(),
                    running: true,
                    pid: Some(info.pid),
                    port: Some(info.port),
                    version: Some(info.version),
                    username: Some(info.username),
                    database: Some(info.database),
                    data_dir: Some(info.data_dir.display().to_string()),
                    uri: Some(uri),
                }
            } else {
                // Stopped but instance exists - show data_dir
                InfoOutput {
                    name: name.clone(),
                    running: false,
                    pid: None,
                    port: Some(info.port),
                    version: Some(info.version),
                    username: Some(info.username),
                    database: Some(info.database),
                    data_dir: Some(info.data_dir.display().to_string()),
                    uri: None,
                }
            }
        }
        None => {
            // Instance doesn't exist
            InfoOutput {
                name: name.clone(),
                running: false,
                pid: None,
                port: None,
                version: None,
                username: None,
                database: None,
                data_dir: None,
                uri: None,
            }
        }
    };

    match output_format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        OutputFormat::Text => {
            if output.running {
                println!("PostgreSQL instance '{}' is running", name);
                println!("  PID:      {}", output.pid.unwrap());
                println!("  Port:     {}", output.port.unwrap());
                println!("  Version:  {}", output.version.as_ref().unwrap());
                println!("  Username: {}", output.username.as_ref().unwrap());
                println!("  Database: {}", output.database.as_ref().unwrap());
                println!("  Data dir: {}", output.data_dir.as_ref().unwrap());
                println!();
                println!("URI: {}", output.uri.as_ref().unwrap());
            } else if output.data_dir.is_some() {
                println!("PostgreSQL instance '{}' is stopped", name);
                println!("  Port:     {}", output.port.unwrap());
                println!("  Version:  {}", output.version.as_ref().unwrap());
                println!("  Username: {}", output.username.as_ref().unwrap());
                println!("  Database: {}", output.database.as_ref().unwrap());
                println!("  Data dir: {}", output.data_dir.as_ref().unwrap());
                println!();
                println!("Use 'pg0 start --name {}' to start it.", name);
            } else {
                println!("PostgreSQL instance '{}' does not exist", name);
            }
        }
    }

    Ok(())
}

fn find_psql_binary(installation_dir: &PathBuf) -> Result<PathBuf, CliError> {
    // Look for psql in installation_dir/*/bin/psql (version subdirectory)
    if let Ok(entries) = fs::read_dir(installation_dir) {
        for entry in entries.flatten() {
            let psql_path = entry.path().join("bin").join("psql");
            if psql_path.exists() {
                return Ok(psql_path);
            }
        }
    }

    // Fallback: try direct path (in case structure changes)
    let direct_path = installation_dir.join("bin").join("psql");
    if direct_path.exists() {
        return Ok(direct_path);
    }

    Err(CliError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!(
            "psql not found in {}",
            installation_dir.display()
        ),
    )))
}

fn psql(name: String, args: Vec<String>) -> Result<(), CliError> {
    let info = load_instance(&name)?.ok_or(CliError::NoInstance)?;

    if !is_process_running(info.pid) {
        remove_instance(&name)?;
        return Err(CliError::NoInstance);
    }

    let psql_path = find_psql_binary(&info.installation_dir)?;

    // Build connection URI
    let uri = format!(
        "postgresql://{}:{}@localhost:{}/{}",
        info.username, info.password, info.port, info.database
    );

    // Execute psql with the connection URI and any additional args
    let status = std::process::Command::new(&psql_path)
        .arg(&uri)
        .args(&args)
        .status()?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

fn logs(name: String, lines: Option<usize>, follow: bool) -> Result<(), CliError> {
    let instance_dir = get_instance_dir(&name)?;
    let log_dir = instance_dir.join("data").join("log");

    if !log_dir.exists() {
        return Err(CliError::Other(format!(
            "Log directory not found for instance '{}'. Has PostgreSQL been started?",
            name
        )));
    }

    // Find the most recent log file
    let mut log_files: Vec<_> = fs::read_dir(&log_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .collect();

    if log_files.is_empty() {
        return Err(CliError::Other(format!(
            "No log files found for instance '{}'",
            name
        )));
    }

    // Sort by modification time, most recent first
    log_files.sort_by_key(|e| std::cmp::Reverse(
        e.metadata().and_then(|m| m.modified()).ok()
    ));

    let log_file = &log_files[0].path();

    if follow {
        // Follow mode - use tail -f equivalent
        println!("Following logs for instance '{}' (Ctrl+C to exit):", name);
        println!("Log file: {}", log_file.display());
        println!();

        let mut file = fs::File::open(log_file)?;
        let mut pos = file.metadata()?.len();

        // Print existing content first
        use std::io::{BufRead, BufReader, Seek, SeekFrom};
        file.seek(SeekFrom::Start(0))?;
        let reader = BufReader::new(&file);
        for line in reader.lines() {
            println!("{}", line?);
        }

        // Now follow new content
        loop {
            file.seek(SeekFrom::Start(pos))?;
            let reader = BufReader::new(&file);
            for line in reader.lines() {
                println!("{}", line?);
            }
            pos = file.metadata()?.len();
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    } else {
        // Show logs (optionally limited to N lines)
        use std::io::{BufRead, BufReader};
        let file = fs::File::open(log_file)?;
        let reader = BufReader::new(file);
        let all_lines: Vec<_> = reader.lines().collect::<Result<_, _>>()?;

        let lines_to_show = if let Some(n) = lines {
            &all_lines[all_lines.len().saturating_sub(n)..]
        } else {
            &all_lines[..]
        };

        println!("Logs for instance '{}' ({})", name, log_file.display());
        println!();
        for line in lines_to_show {
            println!("{}", line);
        }
    }

    Ok(())
}

fn find_installed_version(installation_dir: &PathBuf) -> Result<String, CliError> {
    if let Ok(entries) = fs::read_dir(installation_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    // Check if it looks like a version directory
                    if name.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                        return Ok(name.to_string());
                    }
                }
            }
        }
    }
    Err(CliError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "No PostgreSQL version found in installation directory",
    )))
}

fn install_extension(instance_name: String, extension_name: String) -> Result<(), CliError> {
    let info = load_instance(&instance_name)?.ok_or(CliError::NoInstance)?;

    if !is_process_running(info.pid) {
        remove_instance(&instance_name)?;
        return Err(CliError::NoInstance);
    }

    println!("Fetching available extensions...");

    let available = postgresql_extensions::blocking::get_available_extensions()?;

    // Find the extension (case-insensitive search)
    let ext = available
        .iter()
        .find(|e| e.name().to_lowercase() == extension_name.to_lowercase())
        .ok_or_else(|| CliError::ExtensionNotFound(extension_name.clone()))?;

    let ext_name = ext.name().to_string();
    let ext_namespace = ext.namespace().to_string();
    println!("Installing extension '{}'...", ext_name);

    // Get installed PostgreSQL version
    let pg_version = find_installed_version(&info.installation_dir)?;
    let version_req: VersionReq = pg_version.parse().map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid version: {}", e),
        )
    })?;

    // Build Settings for the extension installer
    // The installation_dir needs to point to the version-specific directory
    let version_install_dir = info.installation_dir.join(&pg_version);
    let settings = Settings {
        version: version_req.clone(),
        port: info.port,
        username: info.username.clone(),
        password: info.password.clone(),
        data_dir: info.data_dir.clone(),
        installation_dir: version_install_dir,
        ..Default::default()
    };

    postgresql_extensions::blocking::install(
        &settings,
        &ext_namespace,
        &ext_name,
        &version_req,
    )?;

    println!("Extension '{}' installed successfully!", ext_name);
    println!();
    println!("To enable it in your database, run:");
    println!("  pg0 psql -c \"CREATE EXTENSION IF NOT EXISTS {};\"", ext_name);

    Ok(())
}

fn list(output_format: OutputFormat) -> Result<(), CliError> {
    let instance_names = list_instances()?;

    let mut instances: Vec<InfoOutput> = Vec::new();
    for name in &instance_names {
        if let Some(info) = load_instance(name)? {
            let running = is_process_running(info.pid);
            let output = if running {
                let uri = format!(
                    "postgresql://{}:{}@localhost:{}/{}",
                    info.username, info.password, info.port, info.database
                );
                InfoOutput {
                    name: name.clone(),
                    running: true,
                    pid: Some(info.pid),
                    port: Some(info.port),
                    version: Some(info.version),
                    username: Some(info.username),
                    database: Some(info.database),
                    data_dir: Some(info.data_dir.display().to_string()),
                    uri: Some(uri),
                }
            } else {
                InfoOutput {
                    name: name.clone(),
                    running: false,
                    pid: None,
                    port: Some(info.port),
                    version: Some(info.version),
                    username: Some(info.username),
                    database: Some(info.database),
                    data_dir: Some(info.data_dir.display().to_string()),
                    uri: None,
                }
            };
            instances.push(output);
        }
    }

    match output_format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&instances)?);
        }
        OutputFormat::Text => {
            if instances.is_empty() {
                println!("No instances found.");
            } else {
                println!("Instances:");
                println!();
                for instance in &instances {
                    let status = if instance.running { "running" } else { "stopped" };
                    if instance.running {
                        println!(
                            "  {} ({}) - port {} - {}",
                            instance.name,
                            status,
                            instance.port.unwrap(),
                            instance.uri.as_ref().unwrap()
                        );
                    } else {
                        println!(
                            "  {} ({}) - port {} - {}",
                            instance.name,
                            status,
                            instance.port.unwrap(),
                            instance.data_dir.as_ref().unwrap()
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

fn list_extensions() -> Result<(), CliError> {
    println!("Fetching available extensions...");

    let extensions = postgresql_extensions::blocking::get_available_extensions()?;

    println!();
    println!("Available extensions:");
    println!();

    for ext in extensions {
        println!("  {} - {}", ext.name(), ext.description());
    }

    Ok(())
}

fn init_logging(verbose: bool) {
    let filter = if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("warn")
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}

fn main() {
    let cli = Cli::parse();

    init_logging(cli.verbose);

    let result = match cli.command {
        Commands::Start {
            name,
            port,
            version,
            data_dir,
            username,
            password,
            database,
            config,
        } => {
            let port_was_specified = port.is_some();
            let port = port.unwrap_or(5432);
            start(name, port, port_was_specified, version, data_dir, username, password, database, config)
        }
        Commands::Stop { name } => stop(name),
        Commands::Drop { name, force } => drop_instance(name, force),
        Commands::Info { name, output } => info(name, output),
        Commands::List { output } => list(output),
        Commands::Psql { name, args } => psql(name, args),
        Commands::Logs { name, lines, follow } => logs(name, lines, follow),
        Commands::InstallExtension { name, extension } => install_extension(name, extension),
        Commands::ListExtensions => list_extensions(),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}
