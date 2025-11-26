use clap::{Parser, Subcommand};
use postgresql_embedded::blocking::PostgreSQL;
use postgresql_embedded::{Settings, VersionReq};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process;
use thiserror::Error;
use tracing_subscriber::EnvFilter;

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

        /// Port to listen on
        #[arg(short, long, default_value = "5432")]
        port: u16,

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
    },
    /// Stop PostgreSQL server
    Stop {
        /// Instance name
        #[arg(long, default_value = DEFAULT_INSTANCE_NAME)]
        name: String,
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

/// Get the current platform string for downloads
fn get_platform() -> Option<&'static str> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    { Some("aarch64-apple-darwin") }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    { Some("x86_64-apple-darwin") }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    { Some("x86_64-unknown-linux-gnu") }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    { Some("aarch64-unknown-linux-gnu") }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    { Some("x86_64-pc-windows-msvc") }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
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
    version: String,
    data_dir: Option<String>,
    username: String,
    password: String,
    database: String,
) -> Result<(), CliError> {
    // Check if already running
    if let Some(info) = load_instance(&name)? {
        if is_process_running(info.pid) {
            return Err(CliError::AlreadyRunning(info.pid));
        }
        // Stale instance, clean up
        remove_instance(&name)?;
    }

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

    let settings = Settings {
        version: version_req,
        port,
        username: username.clone(),
        password: password.clone(),
        data_dir: data_dir.clone(),
        installation_dir: installation_dir.clone(),
        ..Default::default()
    };

    let mut postgresql = PostgreSQL::new(settings);

    println!("Downloading and installing PostgreSQL (this may take a moment on first run)...");
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
        println!("PostgreSQL instance '{}' is not running (stale state detected).", name);
        remove_instance(&name)?;
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

    remove_instance(&name)?;
    println!("PostgreSQL instance '{}' stopped.", name);

    Ok(())
}

fn info(name: String, output_format: OutputFormat) -> Result<(), CliError> {
    let instance = match load_instance(&name)? {
        Some(info) => {
            if is_process_running(info.pid) {
                Some(info)
            } else {
                remove_instance(&name)?;
                None
            }
        }
        None => None,
    };

    let output = if let Some(info) = instance {
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
            port: None,
            version: None,
            username: None,
            database: None,
            data_dir: None,
            uri: None,
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
            } else {
                println!("PostgreSQL instance '{}' is not running", name);
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
        let info = match load_instance(name)? {
            Some(info) => {
                if is_process_running(info.pid) {
                    Some(info)
                } else {
                    remove_instance(name)?;
                    None
                }
            }
            None => None,
        };

        let output = if let Some(info) = info {
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
                port: None,
                version: None,
                username: None,
                database: None,
                data_dir: None,
                uri: None,
            }
        };
        instances.push(output);
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
                        println!("  {} ({})", instance.name, status);
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
        } => start(name, port, version, data_dir, username, password, database),
        Commands::Stop { name } => stop(name),
        Commands::Info { name, output } => info(name, output),
        Commands::List { output } => list(output),
        Commands::Psql { name, args } => psql(name, args),
        Commands::InstallExtension { name, extension } => install_extension(name, extension),
        Commands::ListExtensions => list_extensions(),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}
