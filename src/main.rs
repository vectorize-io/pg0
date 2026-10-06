use clap::{Parser, Subcommand};
use flate2::read::GzDecoder;
use postgresql_embedded::blocking::PostgreSQL;
use postgresql_embedded::{Settings, VersionReq};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use tar::Archive;
use thiserror::Error;
use tracing_subscriber::EnvFilter;

/// The embedded PostgreSQL bundle
static POSTGRESQL_BUNDLE: &[u8] = include_bytes!(env!("POSTGRESQL_BUNDLE_PATH"));

/// The embedded pgvector bundle
static PGVECTOR_BUNDLE: &[u8] = include_bytes!(env!("PGVECTOR_BUNDLE_PATH"));

/// Extra runtime libraries (libxml2.so.2 + the libicu major it transitively
/// loads) that the bundled PostgreSQL binary dynamic-links against. Empty on
/// platforms where the host reliably provides them (macOS, Windows,
/// Alpine/musl). See build.rs.
static RUNTIME_LIBS_BUNDLE: &[u8] = include_bytes!(env!("RUNTIME_LIBS_BUNDLE_PATH"));

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

        /// Maximum seconds to wait for graceful shutdown before sending SIGKILL.
        /// Matches `pg_ctl -w -t <timeout>` semantics; the command does not return
        /// until the postmaster has exited and postmaster.pid is gone.
        #[arg(long, default_value_t = 60)]
        timeout: u64,
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
    // Optional: state written by other tools may lack it or hold null (#36).
    pid: Option<u32>,
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

/// Whether `pid` is a live PostgreSQL server process. Liveness alone is not
/// enough: after a reboot the OS may hand a saved pid to an unrelated process,
/// which we must neither wait on nor signal (#37).
fn is_postgres_process(pid: u32) -> bool {
    if pid == 0 {
        // kill(0, ...) targets our own process group and always "succeeds".
        return false;
    }
    let is_postgres_path = |path: &str| {
        Path::new(path.trim())
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("postgres"))
    };
    #[cfg(unix)]
    {
        process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .map(|o| o.status.success() && is_postgres_path(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW,
            PROCESS_QUERY_LIMITED_INFORMATION,
        };

        // `tasklist` is an external command and can block before `start` emits
        // any output. Query the process handle directly instead.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return false;
            }

            let mut exit_code = 0;
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let running = GetExitCodeProcess(handle, &mut exit_code) != 0
                && exit_code == STILL_ACTIVE as u32
                && QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len) != 0
                && is_postgres_path(&String::from_utf16_lossy(&buf[..len as usize]));
            let _ = CloseHandle(handle);
            running
        }
    }
}

/// The pid of this instance's live postmaster, if any: the saved pid must
/// match the data dir's postmaster.pid and be a live postgres process.
fn running_pid(info: &InstanceInfo) -> Option<u32> {
    let pid = info.pid?;
    (read_postmaster_pid(&info.data_dir).ok()? == pid && is_postgres_process(pid)).then_some(pid)
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

/// Read the latest PostgreSQL log file content (last 20 lines)
fn read_latest_pg_log(data_dir: &PathBuf) -> Option<String> {
    let log_dir = data_dir.join("log");
    let entries = fs::read_dir(&log_dir).ok()?;

    // Find the most recent log file
    let mut log_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "log").unwrap_or(false))
        .collect();
    log_files.sort_by_key(|e| {
        std::cmp::Reverse(
            e.metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
        )
    });

    let log_file = log_files.first()?;
    let content = fs::read_to_string(log_file.path()).ok()?;

    // Get last 20 lines
    let lines: Vec<&str> = content.lines().collect();
    let last_lines: Vec<&str> = lines.iter().rev().take(20).rev().cloned().collect();

    if last_lines.is_empty() {
        None
    } else {
        Some(last_lines.join("\n"))
    }
}

/// Name of the main PostgreSQL server binary for the current target platform.
/// theseus-rs bundles `postgres.exe` on Windows and `postgres` everywhere else.
#[cfg(windows)]
const POSTGRES_BINARY: &str = "postgres.exe";
#[cfg(not(windows))]
const POSTGRES_BINARY: &str = "postgres";

/// Extract the embedded bundle into `version_dir`, stripping the top-level
/// directory entry (e.g. "postgresql-18.1.0-<target>/"). theseus-rs publishes
/// the Windows bundle as a ZIP and every other platform as tar.gz.
#[cfg(not(windows))]
fn extract_postgresql_archive(bundle: &[u8], version_dir: &std::path::Path) -> Result<(), CliError> {
    let decoder = GzDecoder::new(bundle);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        let stripped_path: PathBuf = path.components().skip(1).collect();
        if stripped_path.as_os_str().is_empty() {
            continue;
        }

        let dest_path = version_dir.join(&stripped_path);
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&dest_path)?;
        } else {
            entry.unpack(&dest_path)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn extract_postgresql_archive(bundle: &[u8], version_dir: &std::path::Path) -> Result<(), CliError> {
    use std::io::Cursor;
    let reader = Cursor::new(bundle);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| CliError::Other(format!("Failed to read PostgreSQL ZIP archive: {}", e)))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| CliError::Other(format!("Failed to read ZIP entry {}: {}", i, e)))?;

        let entry_path = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => continue, // Skip unsafe / absolute / traversal-containing names
        };

        let stripped_path: PathBuf = entry_path.components().skip(1).collect();
        if stripped_path.as_os_str().is_empty() {
            continue;
        }

        let dest_path = version_dir.join(&stripped_path);
        if entry.is_dir() {
            fs::create_dir_all(&dest_path)?;
        } else {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = fs::File::create(&dest_path)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    Ok(())
}

/// Extract the bundled PostgreSQL to the installation directory
/// Returns the path to the version-specific directory (e.g., ~/.pg0/installation/18.1.0)
fn extract_bundled_postgresql(installation_dir: &PathBuf, pg_version: &str) -> Result<PathBuf, CliError> {
    let version_dir = installation_dir.join(pg_version);

    // Check if already extracted
    let bin_dir = version_dir.join("bin");
    let already_extracted = bin_dir.exists() && bin_dir.join(POSTGRES_BINARY).exists();
    // Earlier macOS builds shipped a bundle that loads OpenSSL from Homebrew;
    // re-extract over it so the bundled OpenSSL replaces those binaries.
    #[cfg(target_os = "macos")]
    let already_extracted = already_extracted && version_dir.join("lib/libcrypto.3.dylib").exists();

    if !already_extracted {
        if POSTGRESQL_BUNDLE.is_empty() {
            return Err(CliError::Other(
                "PostgreSQL bundle is empty - this binary was not built with BUNDLE_POSTGRESQL=true".to_string()
            ));
        }

        println!("Extracting bundled PostgreSQL {}...", pg_version);
        fs::create_dir_all(&version_dir)?;

        extract_postgresql_archive(POSTGRESQL_BUNDLE, &version_dir)?;

        if !bin_dir.join(POSTGRES_BINARY).exists() {
            return Err(CliError::Other(format!(
                "PostgreSQL extraction failed - {} not found at {}",
                POSTGRES_BINARY,
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
    } else {
        tracing::debug!("PostgreSQL already extracted at {}", version_dir.display());
    }

    // Always ensure the runtime libs are unpacked and LD_LIBRARY_PATH points
    // at them. Runs even when the postgres install is cached so users
    // upgrading from a pg0 version that didn't ship the bundle pick up the
    // new libs without manually wiping ~/.pg0/installation/.
    ensure_runtime_libs(&version_dir)?;
    #[cfg(target_os = "linux")]
    prepend_lib_dir_to_ld_library_path(&version_dir.join("lib"));

    // Final guard: if any required .so is still unresolved, surface a clear
    // error instead of letting initdb / postgres start with a confusing
    // dlopen failure.
    #[cfg(target_os = "linux")]
    check_shared_libraries(&bin_dir)?;

    if !already_extracted {
        println!("PostgreSQL {} extracted successfully.", pg_version);
    }
    Ok(version_dir)
}

/// Unpack RUNTIME_LIBS_BUNDLE into `<version_dir>/lib/` and create the SONAME
/// symlinks the dynamic linker looks up (e.g. libxml2.so.2 ->
/// libxml2.so.2.9.14). No-op when the bundle is empty (non-Linux-GNU targets)
/// or when the libs are already present.
fn ensure_runtime_libs(version_dir: &Path) -> Result<(), CliError> {
    if RUNTIME_LIBS_BUNDLE.is_empty() {
        return Ok(());
    }

    let lib_dir = version_dir.join("lib");
    fs::create_dir_all(&lib_dir)?;

    let decoder = GzDecoder::new(RUNTIME_LIBS_BUNDLE);
    let mut archive = Archive::new(decoder);
    let mut extracted_names: Vec<String> = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let basename = match path.file_name().and_then(|s| s.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };
        let dest = lib_dir.join(&basename);
        if !dest.exists() {
            entry.unpack(&dest)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&dest)?.permissions();
                perms.set_mode(0o755);
                let _ = fs::set_permissions(&dest, perms);
            }
        }
        extracted_names.push(basename);
    }

    // Create SONAME symlinks: libxml2.so.2 -> libxml2.so.2.9.14, etc.
    // The dynamic linker only looks up files by SONAME (".so.<major>"), not
    // the fully-versioned filename, so the symlinks are what actually makes
    // the bundled libs reachable.
    #[cfg(unix)]
    for name in &extracted_names {
        if let Some(soname) = soname_for(name) {
            let link = lib_dir.join(&soname);
            if link.exists() {
                continue;
            }
            // Symlink relative to lib_dir so it stays valid if the install
            // directory is moved.
            if let Err(e) = std::os::unix::fs::symlink(name, &link) {
                if e.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(e.into());
                }
            }
        }
    }

    Ok(())
}

/// Given a fully-versioned shared-library filename like
/// `libxml2.so.2.9.13` or `libicudata.so.70.1`, return the SONAME the dynamic
/// linker actually resolves: `libxml2.so.2`, `libicudata.so.70`. Returns None
/// for filenames we don't recognise (so we don't accidentally create bogus
/// symlinks).
fn soname_for(filename: &str) -> Option<String> {
    let so_idx = filename.find(".so.")?;
    let (stem, rest) = filename.split_at(so_idx + ".so.".len());
    // `stem` ends with ".so."; `rest` is the version tail e.g. "2.9.14".
    let major = rest.split('.').next()?;
    if major.is_empty() {
        return None;
    }
    Some(format!("{}{}", stem, major))
}

/// Walk up from a `bin/psql` path to the version-specific install dir and
/// make sure the runtime libs are present + LD_LIBRARY_PATH points at them.
/// Used by `pg0 psql`, which spawns psql against an instance whose install
/// directory was set up by an earlier `pg0 start` (possibly from a previous
/// pg0 release that didn't ship the libs bundle).
fn ensure_runtime_libs_for_psql(psql_path: &Path) -> Result<(), CliError> {
    let version_dir = match psql_path.parent().and_then(|p| p.parent()) {
        Some(p) => p.to_path_buf(),
        None => return Ok(()),
    };
    ensure_runtime_libs(&version_dir)?;
    #[cfg(target_os = "linux")]
    prepend_lib_dir_to_ld_library_path(&version_dir.join("lib"));
    Ok(())
}

/// Prepend `lib_dir` to the process LD_LIBRARY_PATH so that subprocesses
/// (initdb, postgres, pg_ctl, psql) find the bundled libs first. Existing
/// entries are preserved.
#[cfg(target_os = "linux")]
fn prepend_lib_dir_to_ld_library_path(lib_dir: &Path) {
    let lib_dir_s = lib_dir.to_string_lossy().to_string();
    let new = match std::env::var("LD_LIBRARY_PATH") {
        Ok(existing) if !existing.is_empty() => {
            // Avoid duplicating ourselves on repeat calls.
            if existing
                .split(':')
                .any(|p| p == lib_dir_s)
            {
                return;
            }
            format!("{}:{}", lib_dir_s, existing)
        }
        _ => lib_dir_s,
    };
    std::env::set_var("LD_LIBRARY_PATH", new);
}

/// Check that the postgres binary can find all required shared libraries.
/// Only called on Linux. If ldd is unavailable, silently skips the check.
#[cfg(target_os = "linux")]
fn check_shared_libraries(bin_dir: &std::path::Path) -> Result<(), CliError> {
    let postgres_path = bin_dir.join("postgres");
    let output = match std::process::Command::new("ldd")
        .arg(&postgres_path)
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            tracing::debug!("Could not run ldd to check shared libraries: {}", e);
            return Ok(());
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let missing: Vec<&str> = stdout
        .lines()
        .filter(|line| line.contains("not found"))
        .map(|line| line.trim())
        .collect();

    if missing.is_empty() {
        return Ok(());
    }

    let missing_list = missing.join("\n  ");
    Err(CliError::Other(format!(
        "The bundled PostgreSQL binary is missing required system libraries:\n  \
         {}\n\n\
         Install the missing libraries using your system package manager. For example:\n  \
         Arch Linux:    sudo pacman -S <package>\n  \
         Ubuntu/Debian: sudo apt install <package>\n  \
         Fedora/RHEL:   sudo dnf install <package>",
        missing_list
    )))
}

/// Install pgvector extension files into the PostgreSQL installation
fn install_pgvector(installation_dir: &PathBuf, pg_version: &str) -> Result<(), CliError> {
    let pg_major = pg_version.split('.').next().unwrap_or("16");
    let pgvector_version = env!("PGVECTOR_VERSION");

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

    if PGVECTOR_BUNDLE.is_empty() {
        return Err(CliError::Other(
            "pgvector bundle is empty - this binary was not built with BUNDLE_POSTGRESQL=true".to_string()
        ));
    }

    println!("Installing pgvector {}...", pgvector_version);

    // Extract bundled pgvector
    let decoder = GzDecoder::new(PGVECTOR_BUNDLE);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.ends_with(".so") || name.ends_with(".dylib") || name.ends_with(".dll") {
                let dest = lib_dir.join(name);
                entry.unpack(&dest)?;
                // Make library executable on Unix
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(metadata) = dest.metadata() {
                        let mut perms = metadata.permissions();
                        perms.set_mode(0o755);
                        let _ = fs::set_permissions(&dest, perms);
                    }
                }
            } else if name == "vector.control" || name.starts_with("vector--") {
                let dest = extension_dir.join(name);
                entry.unpack(&dest)?;
            }
        }
    }

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
        check_not_running(&info.data_dir)?;
        // Stale instance: clean up instance metadata but preserve data directory.
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

    // Enable logging to files (required for `pg0 logs` command)
    configuration.insert("logging_collector".to_string(), "on".to_string());
    configuration.insert("log_directory".to_string(), "log".to_string());
    configuration.insert("log_filename".to_string(), "postgresql-%Y-%m-%d.log".to_string());
    configuration.insert("log_rotation_age".to_string(), "1d".to_string());
    configuration.insert("log_rotation_size".to_string(), "100MB".to_string());

    // Pin timezone to UTC so PostgreSQL never reads the tzdata directory at startup.
    // The theseus-rs binaries are compiled with --with-system-tzdata=/usr/share/zoneinfo,
    // which doesn't exist on NixOS (tzdata lives at /etc/zoneinfo) and causes a FATAL
    // "could not find a suitable time zone abbreviations file" on server start.
    configuration.insert("timezone".to_string(), "UTC".to_string());
    configuration.insert("log_timezone".to_string(), "UTC".to_string());

    // Parse and apply custom config options (these override defaults)
    for cfg in &config {
        if let Some((key, value)) = cfg.split_once('=') {
            configuration.insert(key.trim().to_string(), value.trim().to_string());
        } else {
            eprintln!("Warning: Invalid config format '{}', expected KEY=VALUE", cfg);
        }
    }

    // Extract bundled PostgreSQL
    let version_install_dir = extract_bundled_postgresql(&installation_dir, &version)?;

    // Windows initdb takes its locale from the OS (LC_ALL is ignored) and
    // rejects localized names like "Turkish_Türkiye.1252" (#35).
    // postgresql_embedded can't pass --locale, so initialize the cluster
    // ourselves; setup() then skips its own initdb.
    #[cfg(windows)]
    if !data_dir.join("postgresql.conf").exists() {
        init_data_dir(&version_install_dir, &data_dir, &password)?;
    }

    let settings = Settings {
        version: version_req,
        port,
        username: username.clone(),
        password: password.clone(),
        data_dir: data_dir.clone(),
        installation_dir: version_install_dir,
        configuration,
        trust_installation_dir: true, // Use our extracted files
        temporary: false, // Never delete data directory on drop - pg0 manages data lifecycle explicitly
        timeout: Some(std::time::Duration::from_secs(600)), // 10 minute timeout for slow systems (ARM64 emulation under QEMU)
        ..Default::default()
    };

    let mut postgresql = PostgreSQL::new(settings);
    postgresql.setup()?;

    // Install pgvector extension
    if let Err(e) = install_pgvector(&installation_dir, &version) {
        eprintln!("Warning: Failed to install pgvector: {}", e);
        eprintln!("You can try installing it manually with: pg0 install-extension vector");
    }

    println!("Starting PostgreSQL on port {}...", port);
    if let Err(e) = postgresql.start() {
        // Try to read the PostgreSQL log for more context
        let log_context = read_latest_pg_log(&data_dir);
        let error_msg = if let Some(log) = log_context {
            format!("Failed to start PostgreSQL: {}\n\nPostgreSQL log:\n{}", e, log)
        } else {
            format!("Failed to start PostgreSQL: {}", e)
        };
        return Err(CliError::Other(error_msg));
    }

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
            .arg(&format!("postgresql://postgres:{}@127.0.0.1:{}/postgres", password, port))
            .arg("-c")
            .arg(&create_user_sql)
            .status()?;
        if !status.success() {
            eprintln!("Warning: Failed to create user '{}'", username);
        }
    }

    // Create the database if it doesn't exist and it's not the default 'postgres'
    if database != "postgres" {
        // Pre-check existence rather than relying on the duplicate-database error
        // string, which is localized by PostgreSQL's lc_messages (e.g. on Windows
        // with a Chinese locale: `数据库 "x" 已经存在`). See vectorize-io/pg0#13.
        if !postgresql.database_exists(&database)? {
            println!("Creating database '{}'...", database);
            postgresql.create_database(&database)?;
        }
        // Grant privileges to the user on the database
        if username != "postgres" {
            let psql_path = find_psql_binary(&installation_dir)?;
            let grant_sql = format!("GRANT ALL PRIVILEGES ON DATABASE \"{}\" TO \"{}\";", database, username);
            let _ = std::process::Command::new(&psql_path)
                .arg(&format!("postgresql://postgres:{}@127.0.0.1:{}/postgres", password, port))
                .arg("-c")
                .arg(&grant_sql)
                .status();
        }
    }

    // Read PID from postmaster.pid file
    let pid = read_postmaster_pid(&data_dir)?;

    let info = InstanceInfo {
        pid: Some(pid),
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
        "Connection URI: postgresql://{}:{}@127.0.0.1:{}/{}",
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

/// Refuse if a live postgres owns this data dir's postmaster.pid; otherwise
/// remove a stale postmaster.pid so PostgreSQL can start with existing data.
/// The pidfile is trusted over instance.json's pid, which can be stale or
/// reused after a reboot (#37).
fn check_not_running(data_dir: &Path) -> Result<(), CliError> {
    let data_dir = data_dir.to_path_buf();
    if let Some(pid) = read_postmaster_pid(&data_dir)
        .ok()
        .filter(|&pid| is_postgres_process(pid))
    {
        return Err(CliError::AlreadyRunning(pid));
    }
    let pid_file = data_dir.join("postmaster.pid");
    if pid_file.exists() {
        println!("Removing stale postmaster.pid (server no longer running)...");
        fs::remove_file(&pid_file)?;
    }
    Ok(())
}

/// SIGKILL fallback that re-checks the pid first, so a pid the OS has
/// already handed to another process is never killed (#37).
fn kill_if_postgres(pid: u32) {
    if is_postgres_process(pid) {
        send_kill_signal(pid);
    }
}

/// Run initdb exactly as postgresql_embedded does, plus `--locale=C`.
/// Only called on Windows; kept cross-platform so it is tested everywhere.
#[cfg_attr(not(windows), allow(dead_code))]
fn init_data_dir(version_dir: &Path, data_dir: &Path, password: &str) -> Result<(), CliError> {
    let pwfile = std::env::temp_dir().join(format!("pg0-initdb-{}.pw", process::id()));
    fs::write(&pwfile, password)?;
    let initdb = if cfg!(windows) { "initdb.exe" } else { "initdb" };
    let output = process::Command::new(version_dir.join("bin").join(initdb))
        .arg("-D")
        .arg(data_dir)
        .args(["-U", "postgres", "--auth=password", "--encoding=UTF8", "--locale=C"])
        .arg(format!("--pwfile={}", pwfile.display()))
        .output();
    let _ = fs::remove_file(&pwfile);
    let output = output?;
    if !output.status.success() {
        return Err(CliError::Other(format!(
            "initdb failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

fn send_kill_signal(pid: u32) {
    #[cfg(unix)]
    {
        use std::process::Command;
        let _ = Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }
    #[cfg(windows)]
    {
        use std::process::Command;
        let _ = Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .output();
    }
}

/// Poll until the postmaster process has exited AND its postmaster.pid file is
/// gone from the data directory, or `timeout` elapses. Returns true if shutdown
/// completed within the deadline.
///
/// Matches `pg_ctl -w` semantics. Without this, a `stop` → `start` sequence on
/// a busy instance races with the still-draining postmaster: the next start
/// sees a live postmaster.pid and fails. See vectorize-io/pg0#17.
fn wait_for_shutdown(pid: u32, data_dir: &PathBuf, timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    let pid_file = data_dir.join("postmaster.pid");
    loop {
        if !is_postgres_process(pid) && !pid_file.exists() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn find_pg_ctl_binary(installation_dir: &PathBuf) -> Result<PathBuf, CliError> {
    let pg_ctl_name = if cfg!(windows) { "pg_ctl.exe" } else { "pg_ctl" };

    // Same layout as find_psql_binary: installation_dir/<version>/bin/pg_ctl
    if let Ok(entries) = fs::read_dir(installation_dir) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("bin").join(pg_ctl_name);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    let direct = installation_dir.join("bin").join(pg_ctl_name);
    if direct.exists() {
        return Ok(direct);
    }

    Err(CliError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!(
            "{} not found in {}",
            pg_ctl_name,
            installation_dir.display()
        ),
    )))
}

/// Stop the postmaster cleanly. Delegates to `pg_ctl stop -m fast -w` so the
/// shutdown uses the correct per-platform signal (especially on Windows, where
/// plain `taskkill` does not trigger graceful PostgreSQL shutdown) and the
/// command does not return until the postmaster has exited and postmaster.pid
/// is gone. On timeout, falls back to SIGKILL and returns an error so callers
/// know the shutdown wasn't clean. See vectorize-io/pg0#17.
fn stop(name: String, timeout_secs: u64) -> Result<(), CliError> {
    let info = load_instance(&name)?.ok_or(CliError::NoInstance)?;

    let Some(pid) = running_pid(&info) else {
        println!("PostgreSQL instance '{}' is not running.", name);
        return Ok(());
    };

    println!("Stopping PostgreSQL instance '{}' (pid: {})...", name, pid);

    let pg_ctl = find_pg_ctl_binary(&info.installation_dir)?;
    let pg_ctl_status = std::process::Command::new(&pg_ctl)
        .arg("stop")
        .arg("-D")
        .arg(&info.data_dir)
        .arg("-m")
        .arg("fast")
        .arg("-w")
        .arg("-t")
        .arg(timeout_secs.to_string())
        .status()?;

    // Belt-and-suspenders: even after pg_ctl reports success, give the OS a
    // brief moment to reap the process and remove postmaster.pid before any
    // subsequent start runs.
    if pg_ctl_status.success()
        && wait_for_shutdown(
            pid,
            &info.data_dir,
            std::time::Duration::from_secs(5),
        )
    {
        println!("PostgreSQL instance '{}' stopped.", name);
        return Ok(());
    }

    eprintln!(
        "PostgreSQL did not shut down within {}s, sending SIGKILL...",
        timeout_secs
    );
    kill_if_postgres(pid);
    Err(CliError::Other(format!(
        "PostgreSQL instance '{}' did not shut down within {}s; sent SIGKILL",
        name, timeout_secs
    )))
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

    // Stop if running — wait for the postmaster to fully exit before
    // deleting the data directory so we don't yank files out from under
    // an in-progress shutdown.
    if let Some(pid) = running_pid(&info) {
        println!("Stopping PostgreSQL instance '{}' (pid: {})...", name, pid);
        let stopped = match find_pg_ctl_binary(&info.installation_dir) {
            Ok(pg_ctl) => std::process::Command::new(&pg_ctl)
                .arg("stop")
                .arg("-D")
                .arg(&info.data_dir)
                .arg("-m")
                .arg("fast")
                .arg("-w")
                .arg("-t")
                .arg("60")
                .status()
                .map(|s| s.success())
                .unwrap_or(false),
            Err(_) => false,
        };
        if !stopped
            || !wait_for_shutdown(pid, &info.data_dir, std::time::Duration::from_secs(5))
        {
            kill_if_postgres(pid);
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
            let running = is_database_healthy(&info);
            if running {
                let uri = format!(
                    "postgresql://{}:{}@127.0.0.1:{}/{}",
                    info.username, info.password, info.port, info.database
                );
                InfoOutput {
                    name: name.clone(),
                    running: true,
                    pid: info.pid,
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
    let psql_name = if cfg!(windows) { "psql.exe" } else { "psql" };

    // Look for psql in installation_dir/*/bin/psql (version subdirectory)
    if let Ok(entries) = fs::read_dir(installation_dir) {
        for entry in entries.flatten() {
            let psql_path = entry.path().join("bin").join(psql_name);
            if psql_path.exists() {
                return Ok(psql_path);
            }
        }
    }

    // Fallback: try direct path (in case structure changes)
    let direct_path = installation_dir.join("bin").join(psql_name);
    if direct_path.exists() {
        return Ok(direct_path);
    }

    Err(CliError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!(
            "{} not found in {}",
            psql_name,
            installation_dir.display()
        ),
    )))
}

/// Return whether PostgreSQL is alive *and* can serve a query.
///
/// A live postmaster can still be unusable (for example, after its shared
/// memory has been removed). Do not report such an instance as running:
/// callers use this status to decide whether it is safe to reuse a database.
fn is_database_healthy(info: &InstanceInfo) -> bool {
    if running_pid(info).is_none() {
        return false;
    }

    let psql_path = match find_psql_binary(&info.installation_dir) {
        Ok(path) => path,
        Err(_) => return false,
    };
    if ensure_runtime_libs_for_psql(&psql_path).is_err() {
        return false;
    }

    process::Command::new(psql_path)
        // Avoid user psql configuration and fail instead of prompting if the
        // saved instance credentials no longer work.
        .args(["-X", "-w", "-q", "-v", "ON_ERROR_STOP=1"])
        .args(["-h", "127.0.0.1", "-p", &info.port.to_string()])
        .args(["-U", &info.username, "-d", &info.database])
        .args(["-c", "SELECT 1"])
        // Keep health checks bounded when a broken backend accepts a TCP
        // connection but never completes startup or the query.
        .env("PGCONNECT_TIMEOUT", "1")
        .env("PGOPTIONS", "-c statement_timeout=1000")
        .env("PGPASSWORD", &info.password)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn psql(name: String, args: Vec<String>) -> Result<(), CliError> {
    let info = load_instance(&name)?.ok_or(CliError::NoInstance)?;

    if running_pid(&info).is_none() {
        remove_instance(&name)?;
        return Err(CliError::NoInstance);
    }

    let psql_path = find_psql_binary(&info.installation_dir)?;
    // psql is dynamic-linked against the same libxml2/libicu as postgres, so
    // make sure subprocess can find the bundled libs even when this command is
    // invoked against an instance that another `pg0 start` already extracted.
    ensure_runtime_libs_for_psql(&psql_path)?;

    // Build connection URI
    let uri = format!(
        "postgresql://{}:{}@127.0.0.1:{}/{}",
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

    if running_pid(&info).is_none() {
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
    // Make sure the bundled libxml2/libicu are present and on the loader path
    // before pg_config / pg_ctl are spawned on a host where the system libs
    // are missing or have a different SONAME.
    ensure_runtime_libs(&version_install_dir)?;
    #[cfg(target_os = "linux")]
    prepend_lib_dir_to_ld_library_path(&version_install_dir.join("lib"));
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
            let running = is_database_healthy(&info);
            let output = if running {
                let uri = format!(
                    "postgresql://{}:{}@127.0.0.1:{}/{}",
                    info.username, info.password, info.port, info.database
                );
                InfoOutput {
                    name: name.clone(),
                    running: true,
                    pid: info.pid,
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{}-{}",
            prefix,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    /// Spawn `sleep` renamed to `postgres` and record it in postmaster.pid.
    fn spawn_fake_postmaster(test_dir: &Path) -> process::Child {
        let fake = test_dir.join("postgres");
        fs::copy("/bin/sleep", &fake).unwrap();
        let child = process::Command::new(&fake).arg("30").spawn().unwrap();
        let data_dir = test_dir.join("data");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("postmaster.pid"), format!("{}\n", child.id())).unwrap();
        child
    }

    fn instance(test_dir: &Path, pid: Option<u32>) -> InstanceInfo {
        InstanceInfo {
            pid,
            port: 5432,
            data_dir: test_dir.join("data"),
            installation_dir: test_dir.to_path_buf(),
            username: "postgres".to_string(),
            password: "postgres".to_string(),
            database: "postgres".to_string(),
            version: "18.1.0".to_string(),
        }
    }

    #[test]
    fn running_pid_rejects_stale_or_reused_pids() {
        let test_dir = unique_dir("pg0-running-pid");
        fs::create_dir_all(&test_dir).unwrap();
        let mut child = spawn_fake_postmaster(&test_dir);
        let pid = child.id();

        assert_eq!(running_pid(&instance(&test_dir, Some(pid))), Some(pid));
        // #36: missing / zero pid.
        assert_eq!(running_pid(&instance(&test_dir, None)), None);
        assert_eq!(running_pid(&instance(&test_dir, Some(0))), None);
        assert!(!is_postgres_process(0));
        // #37: saved pid is alive but not postgres (reused after reboot).
        let me = process::id();
        let data_dir = test_dir.join("data");
        fs::write(data_dir.join("postmaster.pid"), format!("{}\n", me)).unwrap();
        assert_eq!(running_pid(&instance(&test_dir, Some(me))), None);
        // Saved pid doesn't match postmaster.pid.
        fs::write(data_dir.join("postmaster.pid"), format!("{}\n", pid)).unwrap();
        assert_eq!(running_pid(&instance(&test_dir, Some(me))), None);
        // No postmaster.pid at all.
        fs::remove_file(data_dir.join("postmaster.pid")).unwrap();
        assert_eq!(running_pid(&instance(&test_dir, Some(pid))), None);

        child.kill().unwrap();
        fs::remove_dir_all(test_dir).unwrap();
    }

    const STATE: &str = r#""port":5432,"data_dir":"/d","installation_dir":"/i","username":"u","password":"p","database":"db","version":"18""#;

    #[test]
    fn instance_json_without_pid_loads() {
        let info: InstanceInfo = serde_json::from_str(&format!("{{{}}}", STATE)).unwrap();
        assert_eq!(info.pid, None);
    }

    #[test]
    fn instance_json_with_null_pid_loads() {
        let info: InstanceInfo =
            serde_json::from_str(&format!(r#"{{"pid":null,{}}}"#, STATE)).unwrap();
        assert_eq!(info.pid, None);
    }

    #[test]
    fn instance_json_round_trips_pid() {
        let json = serde_json::to_string(&instance(Path::new("/t"), Some(42))).unwrap();
        let info: InstanceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info.pid, Some(42));
    }

    #[test]
    fn is_postgres_process_checks_liveness_and_name() {
        let test_dir = unique_dir("pg0-is-postgres");
        fs::create_dir_all(&test_dir).unwrap();
        let mut child = spawn_fake_postmaster(&test_dir);
        let pid = child.id();

        assert!(is_postgres_process(pid));
        // Alive but not postgres, e.g. a pid reused after reboot.
        assert!(!is_postgres_process(process::id()));
        // kill(0) would target our own process group.
        assert!(!is_postgres_process(0));

        child.kill().unwrap();
        child.wait().unwrap();
        assert!(!is_postgres_process(pid));
        fs::remove_dir_all(test_dir).unwrap();
    }

    #[test]
    fn check_not_running_refuses_live_postmaster() {
        let test_dir = unique_dir("pg0-check-live");
        fs::create_dir_all(&test_dir).unwrap();
        let mut child = spawn_fake_postmaster(&test_dir);
        let data_dir = test_dir.join("data");

        let result = check_not_running(&data_dir);
        assert!(matches!(result, Err(CliError::AlreadyRunning(p)) if p == child.id()));
        // Never remove a live server's pidfile.
        assert!(data_dir.join("postmaster.pid").exists());

        child.kill().unwrap();
        fs::remove_dir_all(test_dir).unwrap();
    }

    #[test]
    fn check_not_running_clears_stale_pidfiles() {
        let data_dir = unique_dir("pg0-check-stale");
        fs::create_dir_all(&data_dir).unwrap();
        let pid_file = data_dir.join("postmaster.pid");

        // #37: pidfile names a pid the OS reused for a non-postgres process.
        fs::write(&pid_file, format!("{}\n", process::id())).unwrap();
        check_not_running(&data_dir).unwrap();
        assert!(!pid_file.exists());

        // Unparseable pidfile.
        fs::write(&pid_file, "garbage\n").unwrap();
        check_not_running(&data_dir).unwrap();
        assert!(!pid_file.exists());

        // No pidfile at all.
        check_not_running(&data_dir).unwrap();
        fs::remove_dir_all(data_dir).unwrap();
    }

    #[test]
    fn kill_if_postgres_spares_other_processes() {
        let test_dir = unique_dir("pg0-kill");
        fs::create_dir_all(&test_dir).unwrap();

        let mut other = process::Command::new("sleep").arg("30").spawn().unwrap();
        kill_if_postgres(other.id());
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(other.try_wait().unwrap().is_none(), "non-postgres process was killed");
        other.kill().unwrap();

        let mut postgres = spawn_fake_postmaster(&test_dir);
        kill_if_postgres(postgres.id());
        assert!(!postgres.wait().unwrap().success());

        fs::remove_dir_all(test_dir).unwrap();
    }

    #[test]
    fn wait_for_shutdown_ignores_reused_pid() {
        let test_dir = unique_dir("pg0-wait");
        fs::create_dir_all(&test_dir).unwrap();
        let timeout = std::time::Duration::from_millis(300);

        // Postgres alive with its pidfile: not shut down.
        let mut postgres = spawn_fake_postmaster(&test_dir);
        let data_dir = test_dir.join("data");
        assert!(!wait_for_shutdown(postgres.id(), &data_dir, timeout));
        postgres.kill().unwrap();

        // Pid now held by a non-postgres process and no pidfile: shut down.
        fs::remove_file(data_dir.join("postmaster.pid")).unwrap();
        assert!(wait_for_shutdown(process::id(), &data_dir, timeout));

        fs::remove_dir_all(test_dir).unwrap();
    }

    #[test]
    fn init_data_dir_uses_c_locale() {
        let test_dir = unique_dir("pg0-initdb");
        let version_dir =
            extract_bundled_postgresql(&test_dir.join("installation"), env!("PG_VERSION")).unwrap();
        let data_dir = test_dir.join("data");
        fs::create_dir_all(&data_dir).unwrap();

        init_data_dir(&version_dir, &data_dir, "secret").unwrap();

        let conf = fs::read_to_string(data_dir.join("postgresql.conf")).unwrap();
        assert!(
            conf.lines().any(|l| l.starts_with("lc_messages = C")),
            "cluster not initialized with --locale=C"
        );
        let hba = fs::read_to_string(data_dir.join("pg_hba.conf")).unwrap();
        assert!(hba.lines().any(|l| !l.starts_with('#') && l.trim_end().ends_with("password")));
        // The temporary password file must not be left behind.
        assert!(!std::env::temp_dir()
            .join(format!("pg0-initdb-{}.pw", process::id()))
            .exists());
        // setup() skips its own initdb when this file exists.
        assert!(data_dir.join("postgresql.conf").exists());

        fs::remove_dir_all(test_dir).unwrap();
    }

    #[test]
    fn health_check_fails_without_live_postmaster() {
        let test_dir = unique_dir("pg0-health-stale");
        fs::create_dir_all(test_dir.join("data")).unwrap();
        // Reused pid: alive, matches the pidfile, but not postgres.
        let me = process::id();
        fs::write(test_dir.join("data").join("postmaster.pid"), format!("{}\n", me)).unwrap();
        assert!(!is_database_healthy(&instance(&test_dir, Some(me))));
        assert!(!is_database_healthy(&instance(&test_dir, None)));
        fs::remove_dir_all(test_dir).unwrap();
    }

    #[test]
    fn health_check_requires_a_successful_query() {
        let test_dir = unique_dir("pg0-health-check");
        let bin_dir = test_dir.join("18.1.0").join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let psql_path = bin_dir.join("psql");
        fs::write(
            &psql_path,
            "#!/bin/sh\nfor arg do\n  [ \"$arg\" = \"SELECT 1\" ] && exit 0\ndone\nexit 1\n",
        )
        .unwrap();
        fs::set_permissions(&psql_path, fs::Permissions::from_mode(0o755)).unwrap();

        let mut child = spawn_fake_postmaster(&test_dir);
        let info = instance(&test_dir, Some(child.id()));

        assert!(is_database_healthy(&info));
        child.kill().unwrap();
        fs::remove_dir_all(test_dir).unwrap();
    }
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
        Commands::Stop { name, timeout } => stop(name, timeout),
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
