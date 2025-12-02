"""
pg0 - Embedded PostgreSQL for Python

Usage:
    from pg0 import Pg0

    # Start PostgreSQL
    pg = Pg0()
    pg.start()
    print(pg.uri)
    pg.stop()

    # Or use context manager
    with Pg0() as pg:
        print(pg.uri)
"""

from __future__ import annotations

import json
import os
import platform
import shutil
import stat
import subprocess
import sys
import tempfile
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Optional


__version__ = "0.1.0"

# GitHub repo for pg0 releases
PG0_REPO = "vectorize-io/pg0"


class Pg0Error(Exception):
    """Base exception for pg0 errors."""
    pass


class Pg0NotFoundError(Pg0Error):
    """pg0 binary not found and could not be installed."""
    pass


class Pg0NotRunningError(Pg0Error):
    """PostgreSQL instance is not running."""
    pass


class Pg0AlreadyRunningError(Pg0Error):
    """PostgreSQL instance is already running."""
    pass


@dataclass
class InstanceInfo:
    """Information about a PostgreSQL instance."""
    name: str
    running: bool
    pid: Optional[int] = None
    port: Optional[int] = None
    version: Optional[str] = None
    username: Optional[str] = None
    database: Optional[str] = None
    data_dir: Optional[str] = None
    uri: Optional[str] = None

    @classmethod
    def from_dict(cls, data: dict) -> "InstanceInfo":
        return cls(
            name=data.get("name", "default"),
            running=data.get("running", False),
            pid=data.get("pid"),
            port=data.get("port"),
            version=data.get("version"),
            username=data.get("username"),
            database=data.get("database"),
            data_dir=data.get("data_dir"),
            uri=data.get("uri"),
        )


def _get_install_dir() -> Path:
    """Get the directory where pg0 binary should be installed."""
    # Use ~/.local/bin on Unix, or a pg0-specific dir
    if sys.platform == "win32":
        base = Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData" / "Local"))
        return base / "pg0" / "bin"
    else:
        return Path.home() / ".local" / "bin"


def _get_platform() -> str:
    """Get the platform string for downloading the correct binary."""
    system = platform.system().lower()
    machine = platform.machine().lower()

    if system == "darwin":
        # macOS - only Apple Silicon supported, Intel uses Rosetta
        return "darwin-aarch64"
    elif system == "linux":
        # Detect architecture
        if machine in ("x86_64", "amd64"):
            arch_str = "x86_64"
        elif machine in ("aarch64", "arm64"):
            arch_str = "aarch64"
        else:
            raise Pg0NotFoundError(f"Unsupported Linux architecture: {machine}")

        # Detect libc (musl vs glibc)
        # Check for musl by looking for the musl loader
        import subprocess
        try:
            result = subprocess.run(
                ["ldd", "--version"],
                capture_output=True,
                text=True,
                timeout=5,
            )
            output = result.stdout + result.stderr
            if "musl" in output.lower():
                return f"linux-{arch_str}-musl"
        except (FileNotFoundError, subprocess.TimeoutExpired):
            pass

        # Check for musl loader file
        musl_loaders = [
            f"/lib/ld-musl-{arch_str}.so.1",
            "/lib/ld-musl-x86_64.so.1",
            "/lib/ld-musl-aarch64.so.1",
        ]
        for loader in musl_loaders:
            if Path(loader).exists():
                return f"linux-{arch_str}-musl"

        # Default to glibc
        return f"linux-{arch_str}-gnu"
    elif system == "windows":
        return "windows-x86_64"
    else:
        raise Pg0NotFoundError(f"Unsupported platform: {system}")


def _get_latest_version() -> str:
    """Get the latest pg0 version from GitHub."""
    url = f"https://api.github.com/repos/{PG0_REPO}/releases/latest"
    try:
        with urllib.request.urlopen(url, timeout=30) as response:
            data = json.loads(response.read().decode())
            return data["tag_name"]
    except Exception as e:
        raise Pg0NotFoundError(f"Failed to fetch latest version: {e}")


def install(version: Optional[str] = None, force: bool = False) -> Path:
    """
    Install the pg0 binary.

    Args:
        version: Version to install (default: latest)
        force: Force reinstall even if already installed

    Returns:
        Path to the installed binary
    """
    install_dir = _get_install_dir()
    binary_name = "pg0.exe" if sys.platform == "win32" else "pg0"
    binary_path = install_dir / binary_name

    # Check if already installed
    if binary_path.exists() and not force:
        return binary_path

    # Get version
    if version is None:
        version = _get_latest_version()

    # Get platform
    plat = _get_platform()

    # Build download URL
    ext = ".exe" if sys.platform == "win32" else ""
    filename = f"pg0-{plat}{ext}"
    url = f"https://github.com/{PG0_REPO}/releases/download/{version}/{filename}"

    print(f"Installing pg0 {version}...")

    # Create install directory
    install_dir.mkdir(parents=True, exist_ok=True)

    # Download binary
    try:
        with tempfile.NamedTemporaryFile(delete=False) as tmp:
            tmp_path = Path(tmp.name)

        with urllib.request.urlopen(url, timeout=120) as response:
            tmp_path.write_bytes(response.read())

        # Move to install location
        shutil.move(str(tmp_path), str(binary_path))

        # Make executable on Unix
        if sys.platform != "win32":
            binary_path.chmod(binary_path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

        print(f"Installed pg0 to {binary_path}")
        return binary_path

    except Exception as e:
        # Cleanup
        if tmp_path.exists():
            tmp_path.unlink()
        raise Pg0NotFoundError(f"Failed to install pg0: {e}")


def _find_pg0() -> str:
    """Find the pg0 binary, installing if necessary."""
    # Check PATH first
    path = shutil.which("pg0")
    if path:
        return path

    # Check our install location
    install_dir = _get_install_dir()
    binary_name = "pg0.exe" if sys.platform == "win32" else "pg0"
    binary_path = install_dir / binary_name

    if binary_path.exists():
        return str(binary_path)

    # Auto-install
    installed_path = install(version=None)
    return str(installed_path)


def _run_pg0(*args: str, check: bool = True) -> subprocess.CompletedProcess:
    """Run a pg0 command."""
    pg0_path = _find_pg0()
    try:
        result = subprocess.run(
            [pg0_path, *args],
            capture_output=True,
            text=True,
        )
        if check and result.returncode != 0:
            stderr = result.stderr.strip()
            if "already running" in stderr.lower():
                raise Pg0AlreadyRunningError(stderr)
            elif "no running instance" in stderr.lower() or "not running" in stderr.lower():
                raise Pg0NotRunningError(stderr)
            else:
                raise Pg0Error(stderr or f"pg0 command failed with code {result.returncode}")
        return result
    except FileNotFoundError:
        raise Pg0NotFoundError("pg0 binary not found")


class Pg0:
    """
    Embedded PostgreSQL instance.

    Args:
        name: Instance name (allows multiple instances)
        port: Port to listen on
        username: Database username
        password: Database password
        database: Database name
        data_dir: Custom data directory
        config: Dict of PostgreSQL configuration options

    Example:
        # Simple usage
        pg = Pg0()
        pg.start()
        print(pg.uri)
        pg.stop()

        # Context manager
        with Pg0(port=5433, database="myapp") as pg:
            print(pg.uri)

        # Custom config
        pg = Pg0(config={"shared_buffers": "512MB"})
    """

    def __init__(
        self,
        name: str = "default",
        port: int = 5432,
        username: str = "postgres",
        password: str = "postgres",
        database: str = "postgres",
        data_dir: Optional[str] = None,
        config: Optional[dict[str, str]] = None,
    ):
        self.name = name
        self.port = port
        self.username = username
        self.password = password
        self.database = database
        self.data_dir = data_dir
        self.config = config or {}

    def start(self) -> InstanceInfo:
        """
        Start the PostgreSQL instance.

        Returns:
            InstanceInfo with connection details

        Raises:
            Pg0AlreadyRunningError: If instance is already running
            Pg0Error: If start fails
        """
        args = [
            "start",
            "--name", self.name,
            "--port", str(self.port),
            "--username", self.username,
            "--password", self.password,
            "--database", self.database,
        ]

        if self.data_dir:
            args.extend(["--data-dir", self.data_dir])

        for key, value in self.config.items():
            args.extend(["-c", f"{key}={value}"])

        _run_pg0(*args)
        return self.info()

    def stop(self) -> None:
        """
        Stop the PostgreSQL instance.

        Note: Does not raise an error if the instance is not running.
        """
        _run_pg0("stop", "--name", self.name, check=False)

    def drop(self, force: bool = True) -> None:
        """
        Drop the PostgreSQL instance (stop if running, delete all data).

        Args:
            force: Skip confirmation prompt (default True for programmatic use)

        Warning:
            This permanently deletes all data for this instance!
        """
        args = ["drop", "--name", self.name]
        if force:
            args.append("--force")
        _run_pg0(*args, check=False)

    def info(self) -> InstanceInfo:
        """
        Get information about the PostgreSQL instance.

        Returns:
            InstanceInfo with current status and connection details
        """
        result = _run_pg0("info", "--name", self.name, "-o", "json", check=False)
        data = json.loads(result.stdout)
        return InstanceInfo.from_dict(data)

    @property
    def uri(self) -> Optional[str]:
        """Get the connection URI if running."""
        return self.info().uri

    @property
    def running(self) -> bool:
        """Check if the instance is running."""
        return self.info().running

    def psql(self, *args: str) -> subprocess.CompletedProcess:
        """
        Run psql with the given arguments.

        Args:
            *args: Arguments to pass to psql (e.g., "-c", "SELECT 1")

        Returns:
            CompletedProcess with stdout/stderr

        Example:
            result = pg.psql("-c", "SELECT version();")
            print(result.stdout)
        """
        return _run_pg0("psql", "--name", self.name, *args)

    def execute(self, sql: str) -> str:
        """
        Execute a SQL command and return the output.

        Args:
            sql: SQL command to execute

        Returns:
            Command output as string

        Example:
            output = pg.execute("SELECT version();")
        """
        result = self.psql("-c", sql)
        return result.stdout

    def __enter__(self) -> "Pg0":
        """Context manager entry - starts PostgreSQL."""
        self.start()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        """Context manager exit - stops PostgreSQL."""
        try:
            self.stop()
        except Pg0NotRunningError:
            pass


def list_instances() -> list[InstanceInfo]:
    """
    List all pg0 instances.

    Returns:
        List of InstanceInfo for all known instances
    """
    result = _run_pg0("list", "-o", "json", check=False)
    data = json.loads(result.stdout)
    return [InstanceInfo.from_dict(item) for item in data]


def start(
    name: str = "default",
    port: int = 5432,
    username: str = "postgres",
    password: str = "postgres",
    database: str = "postgres",
    **config: str,
) -> InstanceInfo:
    """
    Start a PostgreSQL instance (convenience function).

    Args:
        name: Instance name
        port: Port to listen on
        username: Database username
        password: Database password
        database: Database name
        **config: PostgreSQL configuration options

    Returns:
        InstanceInfo with connection details

    Example:
        info = pg0.start(port=5433, shared_buffers="512MB")
        print(info.uri)
    """
    pg = Pg0(
        name=name,
        port=port,
        username=username,
        password=password,
        database=database,
        config=config,
    )
    return pg.start()


def stop(name: str = "default") -> None:
    """
    Stop a PostgreSQL instance (convenience function).

    Args:
        name: Instance name to stop
    """
    _run_pg0("stop", "--name", name, check=False)


def drop(name: str = "default", force: bool = True) -> None:
    """
    Drop a PostgreSQL instance (convenience function).

    Stops the instance if running and deletes all data.

    Args:
        name: Instance name to drop
        force: Skip confirmation prompt (default True for programmatic use)

    Warning:
        This permanently deletes all data for this instance!
    """
    args = ["drop", "--name", name]
    if force:
        args.append("--force")
    _run_pg0(*args, check=False)


def info(name: str = "default") -> InstanceInfo:
    """
    Get information about a PostgreSQL instance (convenience function).

    Args:
        name: Instance name

    Returns:
        InstanceInfo with current status
    """
    result = _run_pg0("info", "--name", name, "-o", "json", check=False)
    data = json.loads(result.stdout)
    return InstanceInfo.from_dict(data)


# Keep PostgreSQL as alias for backwards compatibility
PostgreSQL = Pg0


__all__ = [
    "Pg0",
    "PostgreSQL",  # alias
    "InstanceInfo",
    "Pg0Error",
    "Pg0NotFoundError",
    "Pg0NotRunningError",
    "Pg0AlreadyRunningError",
    "install",
    "list_instances",
    "start",
    "stop",
    "drop",
    "info",
]
