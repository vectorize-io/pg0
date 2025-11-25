use std::fs;

fn main() {
    // Read versions from versions.env
    let versions = fs::read_to_string("versions.env").expect("Failed to read versions.env");

    for line in versions.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            println!("cargo:rustc-env={}={}", key.trim(), value.trim());
        }
    }

    // Re-run if versions.env changes
    println!("cargo:rerun-if-changed=versions.env");
}
