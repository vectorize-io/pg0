use std::collections::HashMap;
use std::env;
use std::fs;
use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn main() {
    println!("cargo:rerun-if-changed=versions.env");

    let versions_env = fs::read_to_string("versions.env").expect("Failed to read versions.env");
    let mut versions: HashMap<String, String> = HashMap::new();
    for line in versions_env.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            versions.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    let get = |k: &str| -> String {
        versions
            .get(k)
            .unwrap_or_else(|| panic!("Missing {} in versions.env", k))
            .clone()
    };

    let pg_version = get("PG_VERSION");
    let pgvector_version = get("PGVECTOR_VERSION");
    let pgvector_tag = get("PGVECTOR_COMPILED_TAG");
    let pgvector_repo = get("PGVECTOR_COMPILED_REPO");

    println!("cargo:rustc-env=PG_VERSION={}", pg_version);
    println!("cargo:rustc-env=PGVECTOR_VERSION={}", pgvector_version);
    println!("cargo:rustc-env=PGVECTOR_COMPILED_TAG={}", pgvector_tag);
    println!("cargo:rustc-env=PGVECTOR_COMPILED_REPO={}", pgvector_repo);

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    bundle_postgresql(&pg_version, &out_dir);
    bundle_pgvector(&pg_version, &pgvector_tag, &pgvector_repo, &out_dir);
    bundle_runtime_libs(&versions, &out_dir);
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
        "x86_64-pc-windows-msvc" => "x86_64-pc-windows-msvc",
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

/// Bundle libxml2.so.2 and its transitive ICU dependency alongside PostgreSQL.
///
/// The theseus-rs PostgreSQL build links against libxml2.so.2 (DT_NEEDED);
/// libxml2 then pulls ICU in transitively. Both have been replaced upstream:
///   - libxml2 2.14 (Ubuntu 25.10+) bumped the SONAME to .so.16; there is no
///     .so.2 compat package.
///   - ICU has moved to .so.74 in 24.04, .so.76 in 25.10+, and continues to
///     drift forward.
///
/// We download libxml2 + the matching libicu .deb files from Ubuntu 22.04's
/// archive at build time, repack them into a single tar.gz, and embed that
/// into the pg0 binary. Ubuntu 22.04 is chosen because its libs require at
/// most GLIBC 2.34, keeping us inside the manylinux_2_35 wheel baseline -
/// Ubuntu 24.04's libs would require GLIBC 2.38 and break users on 22.04 /
/// Debian 12.
///
/// At runtime, main.rs extracts the libs next to the bundled postgres binary
/// and prepends that directory to LD_LIBRARY_PATH.
///
/// Only Linux GNU targets get a non-empty bundle. Other targets get an empty
/// bundle file so the include_bytes! macro in main.rs has something to point
/// at on every platform.
fn bundle_runtime_libs(versions: &HashMap<String, String>, out_dir: &PathBuf) {
    let target = env::var("TARGET").unwrap();
    let bundle_path = out_dir.join("runtime_libs.tar.gz");

    let arch = match target.as_str() {
        "x86_64-unknown-linux-gnu" => "AMD64",
        "aarch64-unknown-linux-gnu" => "ARM64",
        _ => {
            // Empty bundle for everything else.
            fs::write(&bundle_path, b"").expect("Failed to write empty runtime libs bundle");
            println!(
                "cargo:rustc-env=RUNTIME_LIBS_BUNDLE_PATH={}",
                bundle_path.display()
            );
            return;
        }
    };

    let entries: Vec<DebSpec> = vec![
        DebSpec {
            url: versions
                .get(&format!("LIBXML2_DEB_URL_{}", arch))
                .expect("missing LIBXML2_DEB_URL")
                .clone(),
            sha256: versions
                .get(&format!("LIBXML2_DEB_SHA256_{}", arch))
                .expect("missing LIBXML2_DEB_SHA256")
                .clone(),
            // Ubuntu 22.04 ships libxml2.so.2 -> libxml2.so.2.9.13.
            wanted: vec!["libxml2.so.2.9.13".to_string()],
        },
        DebSpec {
            url: versions
                .get(&format!("LIBICU_DEB_URL_{}", arch))
                .expect("missing LIBICU_DEB_URL")
                .clone(),
            sha256: versions
                .get(&format!("LIBICU_DEB_SHA256_{}", arch))
                .expect("missing LIBICU_DEB_SHA256")
                .clone(),
            // Ubuntu 22.04 ships libicu70.1 (matching the .so.70 SONAME).
            // libxml2.so.2 only directly needs libicuuc, but libicuuc itself
            // pulls in libicudata; libicui18n is included for completeness.
            wanted: vec![
                "libicudata.so.70.1".to_string(),
                "libicui18n.so.70.1".to_string(),
                "libicuuc.so.70.1".to_string(),
            ],
        },
    ];

    let mut staged: Vec<(String, Vec<u8>)> = Vec::new();
    for spec in &entries {
        let deb_path = out_dir.join(format!(
            "{}.deb",
            sha256_short(&spec.url)
        ));
        if !deb_path.exists() {
            eprintln!("Downloading {}...", spec.url);
            download_file(&spec.url, &deb_path).expect("Failed to download .deb");
        }
        verify_sha256(&deb_path, &spec.sha256);
        for filename in &spec.wanted {
            let bytes = extract_lib_from_deb(&deb_path, filename)
                .unwrap_or_else(|e| panic!("Failed to extract {} from {}: {}", filename, spec.url, e));
            staged.push((filename.clone(), bytes));
        }
    }

    write_tar_gz(&bundle_path, &staged).expect("Failed to write runtime libs bundle");
    eprintln!(
        "Bundled runtime libs ({} files) at {}",
        staged.len(),
        bundle_path.display()
    );
    println!(
        "cargo:rustc-env=RUNTIME_LIBS_BUNDLE_PATH={}",
        bundle_path.display()
    );
}

struct DebSpec {
    url: String,
    sha256: String,
    /// Filenames (basename only) we want to pull out of `data.tar.zst`.
    wanted: Vec<String>,
}

fn sha256_short(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(&hasher.finalize()[..8])
}

fn verify_sha256(path: &Path, expected: &str) {
    let mut file = File::open(path).expect("open deb for hashing");
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).expect("read deb");
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual = hex::encode(hasher.finalize());
    assert_eq!(
        actual,
        expected,
        "SHA256 mismatch for {} — refusing to ship a deb that doesn't match versions.env",
        path.display()
    );
}

/// Extract `<wanted>` (matched by basename) from the `data.tar.zst` member of
/// a .deb archive. Returns the raw file contents.
fn extract_lib_from_deb(deb_path: &Path, wanted: &str) -> io::Result<Vec<u8>> {
    let file = File::open(deb_path)?;
    let mut ar = ar::Archive::new(file);
    while let Some(entry) = ar.next_entry() {
        let mut entry = entry?;
        let id = std::str::from_utf8(entry.header().identifier()).unwrap_or("");
        if !id.starts_with("data.tar") {
            continue;
        }
        // The stream is one of: data.tar (uncompressed), data.tar.gz, data.tar.xz,
        // data.tar.zst. Modern Ubuntu uses .zst; we only support that path.
        if !id.ends_with(".zst") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported data archive compression: {}", id),
            ));
        }
        let decoder = zstd::Decoder::new(&mut entry)?;
        let mut tar = tar::Archive::new(decoder);
        for tentry in tar.entries()? {
            let mut tentry = tentry?;
            let path = tentry.path()?.to_path_buf();
            let basename = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if basename == wanted {
                let mut buf = Vec::new();
                tentry.read_to_end(&mut buf)?;
                return Ok(buf);
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("{} not found in {}", wanted, deb_path.display()),
    ))
}

/// Write `entries` as a flat tar.gz: each entry becomes a top-level file with
/// 0o755 permissions. The runtime extractor in src/main.rs creates the SONAME
/// symlinks (e.g. libxml2.so.2 -> libxml2.so.2.9.14) after extraction.
fn write_tar_gz(out_path: &Path, entries: &[(String, Vec<u8>)]) -> io::Result<()> {
    let file = File::create(out_path)?;
    let gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(gz);
    for (name, bytes) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_path(name)?;
        header.set_size(bytes.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append(&header, &mut bytes.as_slice())?;
    }
    let gz = builder.into_inner()?;
    gz.finish()?.flush()?;
    Ok(())
}
