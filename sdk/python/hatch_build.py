"""
Hatch build hook to include the pg0 binary in the wheel.

The binary can come from:
1. PG0_BINARY_PATH env var - path to a pre-built binary (for CI/release)
2. Local cargo build - builds from source using cargo (default for local dev)
3. GitHub releases - downloads from releases (fallback, requires PG0_VERSION)
"""

import hashlib
import os
import platform
import shutil
import stat
import subprocess
import urllib.request
from pathlib import Path
from typing import Any

from hatchling.builders.hooks.plugin.interface import BuildHookInterface

# GitHub repo for downloading releases (fallback only)
PG0_REPO = "vectorize-io/pg0"


def get_platform() -> str:
    """Detect the current platform."""
    system = platform.system().lower()
    machine = platform.machine().lower()

    if system == "darwin":
        return "darwin-aarch64" if machine == "arm64" else "darwin-x86_64"
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


def build_binary_locally(target_dir: Path) -> Path:
    """Build pg0 binary from source using cargo."""
    # Find the repo root (sdk/python -> repo root)
    repo_root = Path(__file__).parent.parent.parent

    cargo_toml = repo_root / "Cargo.toml"
    if not cargo_toml.exists():
        raise RuntimeError(f"Cargo.toml not found at {cargo_toml}")

    print("Building pg0 binary from source...")
    print(f"  Repo root: {repo_root}")

    # Build with cargo
    env = os.environ.copy()
    env["BUNDLE_POSTGRESQL"] = "true"

    result = subprocess.run(
        ["cargo", "build", "--release"],
        cwd=repo_root,
        env=env,
        capture_output=True,
        text=True,
    )

    if result.returncode != 0:
        print(f"  stdout: {result.stdout}")
        print(f"  stderr: {result.stderr}")
        raise RuntimeError(f"Cargo build failed: {result.stderr}")

    # Find the built binary
    system = platform.system().lower()
    binary_name = "pg0.exe" if system == "windows" else "pg0"
    built_binary = repo_root / "target" / "release" / binary_name

    if not built_binary.exists():
        raise RuntimeError(f"Built binary not found at {built_binary}")

    # Copy to target directory
    target_dir.mkdir(parents=True, exist_ok=True)
    target_path = target_dir / binary_name
    shutil.copy2(built_binary, target_path)

    # Make executable on Unix
    if system != "windows":
        target_path.chmod(target_path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

    print(f"  Built binary copied to: {target_path}")
    return target_path


def download_binary(target_dir: Path, plat: str, version: str) -> Path:
    """Download the pg0 binary from GitHub releases."""
    ext = ".exe" if plat.startswith("windows") else ""
    filename = f"pg0-{plat}{ext}"
    url = f"https://github.com/{PG0_REPO}/releases/download/{version}/{filename}"

    target_dir.mkdir(parents=True, exist_ok=True)
    binary_path = target_dir / f"pg0{ext}"

    print(f"Downloading pg0 {version} for {plat}...")
    print(f"  URL: {url}")

    # Download to temp file first
    tmp_path = binary_path.with_suffix(".tmp")
    urllib.request.urlretrieve(url, tmp_path)

    # Move to final location
    tmp_path.rename(binary_path)

    # Make executable on Unix
    if not plat.startswith("windows"):
        binary_path.chmod(binary_path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

    print(f"  Saved to: {binary_path}")
    return binary_path


class CustomBuildHook(BuildHookInterface):
    """Build hook to include pg0 binary in wheel build."""

    PLUGIN_NAME = "custom"

    def initialize(self, version: str, build_data: dict[str, Any]) -> None:
        """Called before the build starts."""
        if self.target_name != "wheel":
            # Only include binary for wheel builds, not sdist
            return

        root = Path(self.root)
        bin_dir = root / "pg0" / "bin"
        system = platform.system().lower()
        ext = ".exe" if system == "windows" else ""
        binary_path = bin_dir / f"pg0{ext}"

        # Check if binary already exists
        if binary_path.exists():
            print(f"Binary already exists: {binary_path}")
        # Option 1: Use pre-built binary from env var (for CI)
        elif os.environ.get("PG0_BINARY_PATH"):
            src_path = Path(os.environ["PG0_BINARY_PATH"])
            if not src_path.exists():
                raise RuntimeError(f"PG0_BINARY_PATH does not exist: {src_path}")
            print(f"Using pre-built binary from PG0_BINARY_PATH: {src_path}")
            bin_dir.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src_path, binary_path)
            if system != "windows":
                binary_path.chmod(binary_path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
        # Option 2: Download from GitHub releases (requires PG0_VERSION)
        elif os.environ.get("PG0_VERSION"):
            plat = os.environ.get("PG0_TARGET_PLATFORM") or get_platform()
            download_binary(bin_dir, plat, os.environ["PG0_VERSION"])
        # Option 3: Build locally from source (default for local dev)
        else:
            build_binary_locally(bin_dir)

        # Note: The binary is included via artifacts = ["pg0/bin/*"] in pyproject.toml
        # No need to use force_include here
