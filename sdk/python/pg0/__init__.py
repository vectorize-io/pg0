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
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Optional


__version__ = "0.1.0"


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


def _get_bundled_binary() -> Optional[Path]:
    """Get the path to the bundled pg0 binary, if it exists."""
    package_dir = Path(__file__).parent
    binary_name = "pg0.exe" if sys.platform == "win32" else "pg0"
    bundled_path = package_dir / "bin" / binary_name
    if bundled_path.exists():
        return bundled_path
    return None


def _get_install_dir() -> Path:
    """Get the directory where pg0 binary should be installed."""
    # Use ~/.local/bin on Unix, or a pg0-specific dir
    if sys.platform == "win32":
        base = Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData" / "Local"))
        return base / "pg0" / "bin"
    else:
        return Path.home() / ".local" / "bin"


def _find_pg0() -> str:
    """Find the pg0 binary or raise an error if not found."""
    # Check for bundled binary first (from platform-specific wheel)
    bundled = _get_bundled_binary()
    if bundled:
        return str(bundled)

    # Check PATH
    path = shutil.which("pg0")
    if path:
        return path

    # Check common install location
    install_dir = _get_install_dir()
    binary_name = "pg0.exe" if sys.platform == "win32" else "pg0"
    binary_path = install_dir / binary_name

    if binary_path.exists():
        return str(binary_path)

    # No binary found
    raise Pg0NotFoundError(
        "pg0 binary not found. Install it with:\n"
        "  curl -fsSL https://raw.githubusercontent.com/vectorize-io/pg0/main/install.sh | bash\n"
        "Or download from: https://github.com/vectorize-io/pg0/releases"
    )


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
        port: Port to listen on (None = auto-select available port)
        username: Database username
        password: Database password
        database: Database name
        data_dir: Custom data directory
        config: Dict of PostgreSQL configuration options

    Example:
        # Simple usage (auto-selects available port)
        pg = Pg0()
        pg.start()
        print(pg.uri)
        pg.stop()

        # Context manager with specific port
        with Pg0(port=5433, database="myapp") as pg:
            print(pg.uri)

        # Custom config
        pg = Pg0(config={"shared_buffers": "512MB"})
    """

    def __init__(
        self,
        name: str = "default",
        port: Optional[int] = None,
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
            "--username", self.username,
            "--password", self.password,
            "--database", self.database,
        ]

        if self.port is not None:
            args.extend(["--port", str(self.port)])

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

    def logs(self, lines: Optional[int] = None) -> str:
        """
        Get PostgreSQL logs for this instance.

        Args:
            lines: Number of lines to return (None = all logs)

        Returns:
            Log content as string

        Example:
            print(pg.logs(50))  # Last 50 lines
        """
        args = ["logs", "--name", self.name]
        if lines is not None:
            args.extend(["-n", str(lines)])
        result = _run_pg0(*args, check=False)
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


def list_extensions() -> list[str]:
    """
    List available PostgreSQL extensions.

    Returns:
        List of available extension names

    Example:
        extensions = pg0.list_extensions()
        print(extensions)  # ['vector', 'postgis', ...]
    """
    result = _run_pg0("list-extensions", check=False)
    lines = result.stdout.strip().split("\n")
    return [line.strip() for line in lines if line.strip()]


def start(
    name: str = "default",
    port: Optional[int] = None,
    username: str = "postgres",
    password: str = "postgres",
    database: str = "postgres",
    **config: str,
) -> InstanceInfo:
    """
    Start a PostgreSQL instance (convenience function).

    Args:
        name: Instance name
        port: Port to listen on (None = auto-select available port)
        username: Database username
        password: Database password
        database: Database name
        **config: PostgreSQL configuration options

    Returns:
        InstanceInfo with connection details

    Example:
        info = pg0.start(shared_buffers="512MB")  # auto-selects port
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


def logs(name: str = "default", lines: Optional[int] = None) -> str:
    """
    Get PostgreSQL logs for an instance (convenience function).

    Args:
        name: Instance name
        lines: Number of lines to return (None = all logs)

    Returns:
        Log content as string
    """
    args = ["logs", "--name", name]
    if lines is not None:
        args.extend(["-n", str(lines)])
    result = _run_pg0(*args, check=False)
    return result.stdout


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
    "list_instances",
    "list_extensions",
    "start",
    "stop",
    "drop",
    "info",
    "logs",
    "_get_bundled_binary",  # for testing
]
