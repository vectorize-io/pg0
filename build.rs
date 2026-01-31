use std::env;
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::PathBuf;

use flate2::write::GzEncoder;
use flate2::Compression;

fn main() {
    println!("cargo:rerun-if-changed=versions.env");

    // Load versions from versions.env
    let versions_env = fs::read_to_string("versions.env").expect("Failed to read versions.env");
    let mut pg_version = String::new();
    let mut pgvector_version = String::new();
    let mut pgvector_tag = String::new();
    let mut pgvector_repo = String::new();
    let mut pgbouncer_version = String::new();
    let mut pgbouncer_tag = String::new();
    let mut pgbouncer_repo = String::new();

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
                "PGBOUNCER_VERSION" => pgbouncer_version = value.trim().to_string(),
                "PGBOUNCER_COMPILED_TAG" => pgbouncer_tag = value.trim().to_string(),
                "PGBOUNCER_COMPILED_REPO" => pgbouncer_repo = value.trim().to_string(),
                _ => {}
            }
        }
    }

    println!("cargo:rustc-env=PG_VERSION={}", pg_version);
    println!("cargo:rustc-env=PGVECTOR_VERSION={}", pgvector_version);
    println!("cargo:rustc-env=PGVECTOR_COMPILED_TAG={}", pgvector_tag);
    println!("cargo:rustc-env=PGVECTOR_COMPILED_REPO={}", pgvector_repo);
    println!("cargo:rustc-env=PGBOUNCER_VERSION={}", pgbouncer_version);
    println!("cargo:rustc-env=PGBOUNCER_COMPILED_TAG={}", pgbouncer_tag);
    println!("cargo:rustc-env=PGBOUNCER_COMPILED_REPO={}", pgbouncer_repo);

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Bundle PostgreSQL, pgvector, and pgbouncer
    bundle_postgresql(&pg_version, &out_dir);
    bundle_pgvector(&pg_version, &pgvector_tag, &pgvector_repo, &out_dir);
    bundle_pgbouncer(&pgbouncer_tag, &pgbouncer_repo, &out_dir);
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

    let is_windows = target.contains("windows");
    let download_ext = if is_windows { "zip" } else { "tar.gz" };
    let download_filename = format!("postgresql-{}-{}.{}", pg_version, pg_target, download_ext);
    let url = format!(
        "https://github.com/theseus-rs/postgresql-binaries/releases/download/{}/{}",
        pg_version, download_filename
    );

    let download_path = out_dir.join(&download_filename);

    // Download if not already cached
    if !download_path.exists() {
        eprintln!(
            "Downloading PostgreSQL {} for {}...",
            pg_version, pg_target
        );
        download_file(&url, &download_path).expect("Failed to download PostgreSQL bundle");
        eprintln!("Downloaded to {}", download_path.display());
    } else {
        eprintln!("Using cached PostgreSQL bundle: {}", download_path.display());
    }

    // For Windows, convert zip to tar.gz so the runtime code can use the same extraction logic
    let final_bundle_path = if is_windows {
        let targz_filename = format!("postgresql-{}-{}.tar.gz", pg_version, pg_target);
        let targz_path = out_dir.join(&targz_filename);

        if !targz_path.exists() {
            eprintln!("Converting zip to tar.gz for unified extraction...");
            convert_zip_to_targz(&download_path, &targz_path)
                .expect("Failed to convert zip to tar.gz");
            eprintln!("Converted to {}", targz_path.display());
        } else {
            eprintln!("Using cached converted bundle: {}", targz_path.display());
        }
        targz_path
    } else {
        download_path
    };

    println!(
        "cargo:rustc-env=POSTGRESQL_BUNDLE_PATH={}",
        final_bundle_path.display()
    );
}

/// Convert a zip archive to tar.gz format
fn convert_zip_to_targz(zip_path: &PathBuf, targz_path: &PathBuf) -> io::Result<()> {
    let zip_file = File::open(zip_path)?;
    let reader = BufReader::new(zip_file);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let targz_file = File::create(targz_path)?;
    let encoder = GzEncoder::new(targz_file, Compression::default());
    let mut tar_builder = tar::Builder::new(encoder);

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let name = file.name().to_string();

        if file.is_dir() {
            // Add directory entry
            let mut header = tar::Header::new_gnu();
            header.set_path(&name)?;
            header.set_size(0);
            header.set_mode(0o755);
            header.set_entry_type(tar::EntryType::Directory);
            header.set_cksum();
            tar_builder.append(&header, io::empty())?;
        } else {
            // Add file entry
            let mut header = tar::Header::new_gnu();
            header.set_path(&name)?;
            header.set_size(file.size());
            // Preserve executable permissions for binaries
            if name.ends_with(".exe") || name.ends_with(".dll") || name.contains("/bin/") {
                header.set_mode(0o755);
            } else {
                header.set_mode(0o644);
            }
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();

            let mut contents = Vec::new();
            io::copy(&mut file, &mut contents)?;
            tar_builder.append(&header, contents.as_slice())?;
        }
    }

    tar_builder.finish()?;
    Ok(())
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
        match download_file(&url, &bundle_path) {
            Ok(_) => eprintln!("Downloaded to {}", bundle_path.display()),
            Err(e) => {
                eprintln!("Warning: Failed to download pgvector: {}. Vector extension will not be available.", e);
                let marker = out_dir.join("pgvector_bundle.tar.gz");
                fs::write(&marker, b"").expect("Failed to create empty pgvector marker");
                println!(
                    "cargo:rustc-env=PGVECTOR_BUNDLE_PATH={}",
                    marker.display()
                );
                return;
            }
        }
    } else {
        eprintln!("Using cached pgvector bundle: {}", bundle_path.display());
    }

    println!(
        "cargo:rustc-env=PGVECTOR_BUNDLE_PATH={}",
        bundle_path.display()
    );
}

fn bundle_pgbouncer(pgbouncer_tag: &str, pgbouncer_repo: &str, out_dir: &PathBuf) {
    let target = env::var("TARGET").unwrap();

    // Map Rust target to pgbouncer platform name
    // Expected format from pgbouncer_compiled: pgbouncer-<platform>.tar.gz
    let pgbouncer_platform = match target.as_str() {
        "aarch64-apple-darwin" => "aarch64-apple-darwin",
        "x86_64-apple-darwin" => "x86_64-apple-darwin",
        "x86_64-unknown-linux-gnu" => "x86_64-unknown-linux-gnu",
        "x86_64-unknown-linux-musl" => "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-gnu" => "aarch64-unknown-linux-gnu",
        "aarch64-unknown-linux-musl" => "aarch64-unknown-linux-musl",
        "x86_64-pc-windows-msvc" => {
            eprintln!("Warning: pgbouncer not available for Windows, skipping bundle");
            let marker = out_dir.join("pgbouncer_bundle.tar.gz");
            fs::write(&marker, b"").expect("Failed to create empty pgbouncer marker");
            println!(
                "cargo:rustc-env=PGBOUNCER_BUNDLE_PATH={}",
                marker.display()
            );
            return;
        }
        _ => {
            eprintln!(
                "Warning: Unknown target {}, pgbouncer will not be bundled",
                target
            );
            let marker = out_dir.join("pgbouncer_bundle.tar.gz");
            fs::write(&marker, b"").expect("Failed to create empty pgbouncer marker");
            println!(
                "cargo:rustc-env=PGBOUNCER_BUNDLE_PATH={}",
                marker.display()
            );
            return;
        }
    };

    let filename = format!("pgbouncer-{}.tar.gz", pgbouncer_platform);
    let url = format!(
        "https://github.com/{}/releases/download/{}/{}",
        pgbouncer_repo, pgbouncer_tag, filename
    );

    let bundle_path = out_dir.join(&filename);

    // Download if not already cached
    if !bundle_path.exists() {
        eprintln!(
            "Downloading pgbouncer for {}...",
            pgbouncer_platform
        );
        match download_file(&url, &bundle_path) {
            Ok(_) => eprintln!("Downloaded to {}", bundle_path.display()),
            Err(e) => {
                eprintln!("Warning: Failed to download pgbouncer: {}. Pooling will not be available.", e);
                let marker = out_dir.join("pgbouncer_bundle.tar.gz");
                fs::write(&marker, b"").expect("Failed to create empty pgbouncer marker");
                println!(
                    "cargo:rustc-env=PGBOUNCER_BUNDLE_PATH={}",
                    marker.display()
                );
                return;
            }
        }
    } else {
        eprintln!("Using cached pgbouncer bundle: {}", bundle_path.display());
    }

    println!(
        "cargo:rustc-env=PGBOUNCER_BUNDLE_PATH={}",
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
