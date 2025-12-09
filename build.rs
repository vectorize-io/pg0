use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=versions.env");
    println!("cargo:rerun-if-env-changed=BUNDLE_POSTGRESQL");

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

    // Check if we should bundle PostgreSQL
    let bundle = env::var("BUNDLE_POSTGRESQL")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    if bundle {
        bundle_postgresql(&pg_version, &out_dir);
    } else {
        // Create an empty marker file so include_bytes! doesn't fail
        let marker = out_dir.join("postgresql_bundle.tar.gz");
        if !marker.exists() {
            fs::write(&marker, b"").expect("Failed to create empty bundle marker");
        }
        println!(
            "cargo:rustc-env=POSTGRESQL_BUNDLE_PATH={}",
            marker.display()
        );
        println!("cargo:rustc-env=POSTGRESQL_BUNDLED=false");
    }
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
    println!("cargo:rustc-env=POSTGRESQL_BUNDLED=true");
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
