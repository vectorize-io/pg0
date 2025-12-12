#!/usr/bin/env python3
"""
Build script to download the pg0 binary for the current platform.
This is run during wheel building to bundle the binary into the package.
"""

import hashlib
import os
import platform
import stat
import subprocess
import sys
import urllib.request
from pathlib import Path

# These are updated with each release
PG0_VERSION = "v0.9.0"
PG0_REPO = "vectorize-io/pg0"

# SHA256 checksums for each binary (updated with each release)
# To generate: sha256sum pg0-<platform>
CHECKSUMS = {
    "darwin-aarch64": "",  # Will be populated by CI
    "linux-x86_64-gnu": "",
    "linux-x86_64-musl": "",
    "linux-aarch64-gnu": "",
    "linux-aarch64-musl": "",
    "windows-x86_64": "",
}


def get_platform() -> str:
    """Detect the current platform."""
    system = platform.system().lower()
    machine = platform.machine().lower()

    if system == "darwin":
        return "darwin-aarch64"
    elif system == "linux":
        if machine in ("x86_64", "amd64"):
            arch = "x86_64"
        elif machine in ("aarch64", "arm64"):
            arch = "aarch64"
        else:
            raise RuntimeError(f"Unsupported architecture: {machine}")

        # Detect musl vs glibc
        try:
            result = subprocess.run(
                ["ldd", "--version"],
                capture_output=True,
                text=True,
            )
            if "musl" in (result.stdout + result.stderr).lower():
                return f"linux-{arch}-musl"
        except FileNotFoundError:
            pass

        # Check for musl loader
        if Path(f"/lib/ld-musl-{arch}.so.1").exists():
            return f"linux-{arch}-musl"

        return f"linux-{arch}-gnu"
    elif system == "windows":
        return "windows-x86_64"
    else:
        raise RuntimeError(f"Unsupported platform: {system}")


def download_binary(target_dir: Path, plat: str | None = None) -> Path:
    """Download the pg0 binary for the specified platform."""
    if plat is None:
        plat = get_platform()

    ext = ".exe" if plat.startswith("windows") else ""
    filename = f"pg0-{plat}{ext}"
    url = f"https://github.com/{PG0_REPO}/releases/download/{PG0_VERSION}/{filename}"

    target_dir.mkdir(parents=True, exist_ok=True)
    binary_path = target_dir / f"pg0{ext}"

    print(f"Downloading pg0 {PG0_VERSION} for {plat}...")
    print(f"  URL: {url}")

    # Download to temp file first
    tmp_path = binary_path.with_suffix(".tmp")
    urllib.request.urlretrieve(url, tmp_path)

    # Verify checksum if available
    expected_checksum = CHECKSUMS.get(plat, "")
    if expected_checksum:
        with open(tmp_path, "rb") as f:
            actual_checksum = hashlib.sha256(f.read()).hexdigest()
        if actual_checksum != expected_checksum:
            tmp_path.unlink()
            raise RuntimeError(
                f"Checksum mismatch for {plat}!\n"
                f"  Expected: {expected_checksum}\n"
                f"  Actual:   {actual_checksum}"
            )
        print(f"  Checksum verified: {actual_checksum[:16]}...")
    else:
        print("  Warning: No checksum available for verification")

    # Move to final location
    tmp_path.rename(binary_path)

    # Make executable on Unix
    if not plat.startswith("windows"):
        binary_path.chmod(binary_path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

    print(f"  Saved to: {binary_path}")
    return binary_path


def main():
    """Download binary for current platform into pg0/bin/."""
    script_dir = Path(__file__).parent
    bin_dir = script_dir / "pg0" / "bin"

    # Allow overriding platform via environment variable (for CI cross-builds)
    plat = os.environ.get("PG0_TARGET_PLATFORM")

    download_binary(bin_dir, plat)
    print("Done!")


if __name__ == "__main__":
    main()
