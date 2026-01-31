use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=versions.env");

    // Load versions from versions.env
    let versions_env = fs::read_to_string("versions.env").expect("Failed to read versions.env");
    let mut pg_version = String::new();
    let mut pgvector_version = String::new();
    let mut pgvector_tag = String::new();
    let mut pgvector_repo = String::new();

    for line in versions_env.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "PG_VERSION" => pg_version = value.trim().to_string(),
                "PGVECTOR_VERSION" => pgvector_version = value.trim().to_string(),
                "PGVECTOR_COMPILED_TAG" => pgvector_tag = value.trim().to_string(),
                "PGVECTOR_COMPILED_REPO" => pgvector_repo = value.trim().to_string(),
                _ => {}
            }
        }
    }

    println!("cargo:rustc-env=PG_VERSION={}", pg_version);
    println!("cargo:rustc-env=PGVECTOR_VERSION={}", pgvector_version);
    println!("cargo:rustc-env=PGVECTOR_COMPILED_TAG={}", pgvector_tag);
    println!("cargo:rustc-env=PGVECTOR_COMPILED_REPO={}", pgvector_repo);

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Bundle PostgreSQL and pgvector
    bundle_postgresql(&pg_version, &out_dir);
    bundle_pgvector(&pg_version, &pgvector_tag, &pgvector_repo, &out_dir);
}

fn bundle_postgresql(pg_version: &str, out_dir: &PathBuf) {
    let target = env::var("TARGET").unwrap();

    // Map Rust target to theseus-rs binary name
    let pg_target = match target.as_str() {
        "aarch64-apple-darwin" => "aarch64-apple-darwin",
        "x86_64-apple-darwin" => "x86_64-apple-darwin",
        "x86_64-unknown-linux-gnu" => "x86_64-unknown-linux-gnu",
        "x86_64-unknown-linux-musl" => "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-gnu" => "aarch64-unknown-linux-gnu",
        "aarch64-unknown-linux-musl" => "aarch64-unknown-linux-musl",
        "x86_64-pc-windows-msvc" => "x86_64-pc-windows-msvc",
        _ => {
            eprintln!(
                "Warning: Unknown target {}, PostgreSQL will not be bundled",
                target
            );
            let marker = out_dir.join("postgresql_bundle.tar.gz");
            fs::write(&marker, b"").expect("Failed to create empty bundle marker");
            println!(
                "cargo:rustc-env=POSTGRESQL_BUNDLE_PATH={}",
                marker.display()
            );
            println!("cargo:rustc-env=POSTGRESQL_BUNDLED=false");
            return;
        }
    };

    let ext = if target.contains("windows") {
        "zip"
    } else {
        "tar.gz"
    };
    let filename = format!("postgresql-{}-{}.{}", pg_version, pg_target, ext);
    let url = format!(
        "https://github.com/theseus-rs/postgresql-binaries/releases/download/{}/{}",
        pg_version, filename
    );

    let bundle_path = out_dir.join(&filename);

    // Download if not already cached
    if !bundle_path.exists() {
        eprintln!(
            "Downloading PostgreSQL {} for {}...",
            pg_version, pg_target
        );
        download_file(&url, &bundle_path).expect("Failed to download PostgreSQL bundle");
        eprintln!("Downloaded to {}", bundle_path.display());
    } else {
        eprintln!("Using cached PostgreSQL bundle: {}", bundle_path.display());
    }

    println!(
        "cargo:rustc-env=POSTGRESQL_BUNDLE_PATH={}",
        bundle_path.display()
    );
}

fn bundle_pgvector(pg_version: &str, pgvector_tag: &str, pgvector_repo: &str, out_dir: &PathBuf) {
    let target = env::var("TARGET").unwrap();

    // Map Rust target to pgvector platform name
    let pgvector_platform = match target.as_str() {
        "aarch64-apple-darwin" => "aarch64-apple-darwin",
        "x86_64-apple-darwin" => "x86_64-apple-darwin",
        "x86_64-unknown-linux-gnu" => "x86_64-unknown-linux-gnu",
        "x86_64-unknown-linux-musl" => "x86_64-unknown-linux-gnu", // musl uses gnu pgvector
        "aarch64-unknown-linux-gnu" => "aarch64-unknown-linux-gnu",
        "aarch64-unknown-linux-musl" => "aarch64-unknown-linux-gnu", // musl uses gnu pgvector
        "x86_64-pc-windows-msvc" => {
            eprintln!("Warning: pgvector not available for Windows, skipping bundle");
            let marker = out_dir.join("pgvector_bundle.tar.gz");
            fs::write(&marker, b"").expect("Failed to create empty pgvector marker");
            println!(
                "cargo:rustc-env=PGVECTOR_BUNDLE_PATH={}",
                marker.display()
            );
            return;
        }
        _ => {
            eprintln!(
                "Warning: Unknown target {}, pgvector will not be bundled",
                target
            );
            let marker = out_dir.join("pgvector_bundle.tar.gz");
            fs::write(&marker, b"").expect("Failed to create empty pgvector marker");
            println!(
                "cargo:rustc-env=PGVECTOR_BUNDLE_PATH={}",
                marker.display()
            );
            return;
        }
    };

    // Get PG major version (e.g., "18" from "18.1.0")
    let pg_major = pg_version.split('.').next().unwrap_or("18");

    let filename = format!("pgvector-{}-pg{}.tar.gz", pgvector_platform, pg_major);
    let url = format!(
        "https://github.com/{}/releases/download/{}/{}",
        pgvector_repo, pgvector_tag, filename
    );

    let bundle_path = out_dir.join(&filename);

    // Download if not already cached
    if !bundle_path.exists() {
        eprintln!(
            "Downloading pgvector for {} (PG {})...",
            pgvector_platform, pg_major
        );
        download_file(&url, &bundle_path).expect("Failed to download pgvector bundle");
        eprintln!("Downloaded to {}", bundle_path.display());
    } else {
        eprintln!("Using cached pgvector bundle: {}", bundle_path.display());
    }

    println!(
        "cargo:rustc-env=PGVECTOR_BUNDLE_PATH={}",
        bundle_path.display()
    );
}

fn download_file(url: &str, dest: &PathBuf) -> io::Result<()> {
    // Use curl for downloading (available on all CI platforms)
    let status = std::process::Command::new("curl")
        .args(["-fsSL", url, "-o"])
        .arg(dest)
        .status()?;

    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to download {}", url),
        ));
    }

    Ok(())
}
