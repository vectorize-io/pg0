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

    bundle_postgresql(&pg_version, &versions, &out_dir);
    bundle_pgvector(&pg_version, &pgvector_tag, &pgvector_repo, &out_dir);
}

fn bundle_postgresql(pg_version: &str, versions: &HashMap<String, String>, out_dir: &PathBuf) {
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

    let bundle_path = if ext == "tar.gz" {
        slim_bundle(&bundle_path, out_dir)
    } else {
        bundle_path
    };

    let bundle_path = match target.as_str() {
        "aarch64-apple-darwin" | "x86_64-apple-darwin" => {
            bundle_macos_openssl(&bundle_path, versions, out_dir)
        }
        _ => bundle_path,
    };

    println!(
        "cargo:rustc-env=POSTGRESQL_BUNDLE_PATH={}",
        bundle_path.display()
    );
}

/// Since 18.7 the theseus-rs tar.gz bundles also ship LLVM for JIT and Python
/// for PL/Python, which grew pg0 from ~32 MB to ~100 MB. pg0 needs neither:
/// without llvmjit PostgreSQL silently runs without JIT.
fn is_unneeded(path: &str) -> bool {
    let p = path.split_once('/').map_or("", |(_, rest)| rest); // strip top-level dir
    let name = p.rsplit('/').next().unwrap_or("");
    p.starts_with("lib/bitcode/")
        || p.starts_with("lib/python3")
        || p == "lib/Python"
        || name.starts_with("libLLVM")
        || name.starts_with("llvmjit")
        || name.starts_with("libpython")
        || name.contains("plpython3")
        || p.starts_with("dependency-notices/llvm")
        || (p.starts_with("dependency-notices/") && p.contains("python"))
}

fn slim_bundle(upstream: &Path, out_dir: &Path) -> PathBuf {
    let slim = out_dir.join(format!(
        "{}-slim.tar.gz",
        upstream.file_name().unwrap().to_str().unwrap().trim_end_matches(".tar.gz")
    ));
    if slim.exists() {
        return slim;
    }
    let tmp = slim.with_extension("partial");
    let mut dropped = 0u64;
    {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(
            File::open(upstream).expect("open PostgreSQL bundle"),
        ));
        let gz = flate2::write::GzEncoder::new(File::create(&tmp).expect("create slim bundle"), flate2::Compression::default());
        let mut builder = tar::Builder::new(gz);
        for entry in archive.entries().expect("read PostgreSQL bundle") {
            let mut entry = entry.expect("read PostgreSQL bundle entry");
            let path = entry.path().expect("entry path").into_owned();
            if is_unneeded(&path.to_string_lossy()) {
                dropped += entry.header().size().unwrap_or(0);
                continue;
            }
            let mut header = entry.header().clone();
            match entry.link_name().expect("entry link name") {
                Some(link) => {
                    let link = link.into_owned();
                    builder.append_link(&mut header, &path, &link)
                }
                None => builder.append_data(&mut header, &path, &mut entry),
            }
            .expect("write slim bundle entry");
        }
        builder.into_inner().and_then(|gz| gz.finish()).expect("finish slim bundle");
    }
    fs::rename(&tmp, &slim).expect("move slim bundle into place");
    eprintln!("Dropped {} MB of JIT/PL/Python files from the PostgreSQL bundle", dropped >> 20);
    slim
}

fn bundle_pgvector(pg_version: &str, pgvector_tag: &str, pgvector_repo: &str, out_dir: &PathBuf) {
    let target = env::var("TARGET").unwrap();

    // Map Rust target to pgvector platform name
    let pgvector_platform = match target.as_str() {
        "aarch64-apple-darwin" => "aarch64-apple-darwin",
        "x86_64-apple-darwin" => "x86_64-apple-darwin",
        "x86_64-unknown-linux-gnu" => "x86_64-unknown-linux-gnu",
        "x86_64-unknown-linux-musl" => "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-gnu" => "aarch64-unknown-linux-gnu",
        "aarch64-unknown-linux-musl" => "aarch64-unknown-linux-musl",
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
        .args(["-fsSL", "--retry", "3", url, "-o"])
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

/// The OpenSSL libraries the theseus-rs macOS build links against.
const MACOS_OPENSSL_LIBS: [&str; 2] = ["libssl.3.dylib", "libcrypto.3.dylib"];

/// Make the theseus-rs macOS PostgreSQL bundle self-contained.
///
/// The theseus-rs macOS binaries (postgres, pg_dump, libpq, pgcrypto, ...)
/// load OpenSSL from the Homebrew prefix of the machine that built them
/// (`/opt/homebrew/opt/openssl@3/lib/...`), so PostgreSQL cannot start on a
/// Mac without Homebrew's openssl@3 (theseus-rs/postgresql-binaries#30).
///
/// We do what Postgres.app does: ship libssl/libcrypto inside the bundle's
/// `lib/` directory and point every Mach-O at them with an
/// `@loader_path`-relative install name, then re-sign ad-hoc because
/// install_name_tool invalidates the signature. OpenSSL comes from
/// conda-forge's pinned, checksummed package (versions.env), which has
/// builds for both macOS architectures with a macOS 11 deployment target.
///
/// Returns the path of the repacked bundle; the build fails if any Mach-O in
/// it still references a library under /opt/homebrew or /usr/local.
fn bundle_macos_openssl(
    upstream_bundle: &Path,
    versions: &HashMap<String, String>,
    out_dir: &Path,
) -> PathBuf {
    let target = env::var("TARGET").unwrap();
    let arch = if target.starts_with("aarch64") {
        "ARM64"
    } else {
        "X86_64"
    };
    let get = |k: String| -> String {
        versions
            .get(&k)
            .unwrap_or_else(|| panic!("Missing {} in versions.env", k))
            .clone()
    };
    let openssl_version = get("OPENSSL_CONDA_VERSION".to_string());
    let url = get(format!("OPENSSL_CONDA_URL_{}", arch));
    let sha256 = get(format!("OPENSSL_CONDA_SHA256_{}", arch));

    let stem = upstream_bundle
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_suffix(".tar.gz"))
        .expect("PostgreSQL bundle name must end in .tar.gz");
    let portable = out_dir.join(format!("{}-openssl-{}.tar.gz", stem, openssl_version));
    if portable.exists() {
        eprintln!(
            "Using cached portable PostgreSQL bundle: {}",
            portable.display()
        );
        return portable;
    }

    let conda_path = out_dir.join(format!("{}.conda", sha256_short(&url)));
    if !conda_path.exists() {
        eprintln!("Downloading {}...", url);
        download_file(&url, &conda_path.to_path_buf()).expect("Failed to download OpenSSL package");
    }
    verify_sha256(&conda_path, &sha256);

    let staging = out_dir.join("macos-portable");
    if staging.exists() {
        fs::remove_dir_all(&staging).expect("Failed to clear macOS staging directory");
    }
    fs::create_dir_all(&staging).expect("Failed to create macOS staging directory");
    let upstream = File::open(upstream_bundle).expect("open PostgreSQL bundle");
    tar::Archive::new(flate2::read::GzDecoder::new(upstream))
        .unpack(&staging)
        .expect("Failed to unpack PostgreSQL bundle");
    let root_name = fs::read_dir(&staging)
        .expect("read staging directory")
        .next()
        .expect("PostgreSQL bundle is empty")
        .expect("read staging entry")
        .file_name();
    let root = staging.join(&root_name);

    for lib in MACOS_OPENSSL_LIBS {
        let bytes = extract_from_conda(&conda_path, "pkg-", &format!("lib/{}", lib))
            .unwrap_or_else(|e| panic!("Failed to extract {} from {}: {}", lib, url, e));
        fs::write(root.join("lib").join(lib), bytes).expect("Failed to write OpenSSL library");
    }
    let license = extract_from_conda(&conda_path, "info-", "info/licenses/LICENSE.txt")
        .unwrap_or_else(|e| panic!("Failed to extract OpenSSL licence from {}: {}", url, e));
    let license_dir = root.join("share").join("openssl");
    fs::create_dir_all(&license_dir).expect("Failed to create OpenSSL licence directory");
    fs::write(license_dir.join("LICENSE.txt"), license).expect("Failed to write OpenSSL licence");

    for path in mach_o_files(&root) {
        // `@loader_path/<up to root>/lib/<lib>` from wherever this file sits.
        let depth = path
            .parent()
            .unwrap()
            .strip_prefix(&root)
            .unwrap()
            .components()
            .count();
        let lib_ref = |lib: &str| format!("@loader_path/{}lib/{}", "../".repeat(depth), lib);

        let mut args: Vec<String> = Vec::new();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if MACOS_OPENSSL_LIBS.contains(&name) {
            args.extend(["-id".to_string(), lib_ref(name)]);
        }
        for dep in mach_o_dependencies(&path) {
            let dep_name = dep.rsplit('/').next().unwrap_or("");
            if MACOS_OPENSSL_LIBS.contains(&dep_name) && dep != lib_ref(dep_name) {
                args.extend(["-change".to_string(), dep.clone(), lib_ref(dep_name)]);
            }
        }
        if args.is_empty() {
            continue;
        }
        run_tool("install_name_tool", &args, &path);
        run_tool(
            "codesign",
            &["-f".to_string(), "-s".to_string(), "-".to_string()],
            &path,
        );
    }

    for path in mach_o_files(&root) {
        for dep in mach_o_dependencies(&path) {
            assert!(
                !dep.starts_with("/opt/homebrew/") && !dep.starts_with("/usr/local/"),
                "{} still references {} after bundling OpenSSL",
                path.display(),
                dep
            );
        }
    }

    let tmp = out_dir.join(format!(
        "{}.partial",
        portable.file_name().unwrap().to_str().unwrap()
    ));
    {
        let file = File::create(&tmp).expect("create portable bundle");
        let gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(gz);
        builder.follow_symlinks(false);
        builder
            .append_dir_all(&root_name, &root)
            .expect("Failed to repack PostgreSQL bundle");
        builder
            .into_inner()
            .and_then(|gz| gz.finish())
            .and_then(|mut f| f.flush())
            .expect("Failed to write portable bundle");
    }
    fs::rename(&tmp, &portable).expect("Failed to move portable bundle into place");
    eprintln!(
        "Bundled OpenSSL {} into {}",
        openssl_version,
        portable.display()
    );
    portable
}

/// Read `wanted` from the zstd tar member whose name starts with `member`
/// (`pkg-` or `info-`) of a conda `.conda` package, which is a zip archive.
fn extract_from_conda(conda_path: &Path, member: &str, wanted: &str) -> io::Result<Vec<u8>> {
    let mut zip = zip::ZipArchive::new(File::open(conda_path)?)?;
    let index = (0..zip.len())
        .find(|&i| {
            zip.by_index(i)
                .map(|e| e.name().starts_with(member) && e.name().ends_with(".tar.zst"))
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, format!("no {}*.tar.zst", member))
        })?;
    let entry = zip.by_index(index)?;
    let mut tar = tar::Archive::new(zstd::Decoder::new(entry)?);
    for tentry in tar.entries()? {
        let mut tentry = tentry?;
        if tentry.path()?.to_str() == Some(wanted) {
            let mut buf = Vec::new();
            tentry.read_to_end(&mut buf)?;
            return Ok(buf);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("{} not found", wanted),
    ))
}

/// Every regular Mach-O file under `dir`, found by its magic number.
fn mach_o_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).expect("read bundle directory") {
        let path = entry.expect("read bundle entry").path();
        let meta = fs::symlink_metadata(&path).expect("stat bundle entry");
        if meta.is_dir() {
            found.extend(mach_o_files(&path));
        } else if meta.is_file() {
            let mut magic = [0u8; 4];
            let is_mach_o = File::open(&path)
                .and_then(|mut f| f.read_exact(&mut magic))
                .is_ok()
                && matches!(
                    u32::from_be_bytes(magic),
                    0xfeedface | 0xfeedfacf | 0xcefaedfe | 0xcffaedfe | 0xcafebabe
                );
            if is_mach_o {
                found.push(path);
            }
        }
    }
    found
}

/// The install names a Mach-O file loads, as `otool -L` lists them (for a
/// dylib this includes its own install name).
fn mach_o_dependencies(path: &Path) -> Vec<String> {
    let output = std::process::Command::new("otool")
        .arg("-L")
        .arg(path)
        .output()
        .expect("Failed to run otool (install the Xcode Command Line Tools)");
    assert!(
        output.status.success(),
        "otool -L {} failed",
        path.display()
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .skip(1)
        .filter_map(|line| line.trim().split(" (compatibility").next())
        .filter(|dep| !dep.is_empty())
        .map(str::to_string)
        .collect()
}

fn run_tool(tool: &str, args: &[String], path: &Path) {
    let output = std::process::Command::new(tool)
        .args(args)
        .arg(path)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run {}: {}", tool, e));
    assert!(
        output.status.success(),
        "{} {:?} {} failed: {}",
        tool,
        args,
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}
