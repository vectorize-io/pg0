"""Fixtures for the PGPASSFILE tests (``test_pgpass_*.py``)."""

import hashlib
import os
import sys
from pathlib import Path

import pytest
import pg0

from .pgpass_support import Pg0Home, grant_user_full_control


def pytest_report_header(config):
    """Name the executable under test and its SHA-256, so a run records what it exercised.

    The header never raises and never changes a test result.
    """
    if sys.platform == "win32":
        return None
    try:
        executable = pg0._find_pg0()
        digest = hashlib.sha256(Path(executable).read_bytes()).hexdigest()
    except Exception as e:  # discovery or read failure: report it, keep collecting
        return f"pg0 executable: not found ({type(e).__name__}: {e})"
    return [f"pg0 executable: {executable}", f"pg0 executable sha256: {digest}"]


@pytest.fixture(scope="session")
def pg0_installation(tmp_path_factory):
    """One extracted native installation per session, beneath a pytest-managed folder."""
    installation = tmp_path_factory.mktemp("pg0-installation")
    capture = tmp_path_factory.mktemp("pg0-installation-capture")
    grant_user_full_control(installation, env=dict(os.environ), scratch=capture)
    return installation


@pytest.fixture
def pg0_home(tmp_path, pg0_installation):
    """An isolated PG0_HOME, TMPDIR, HOME and working directory for one test.

    Fails before the test when the test port is in use. After the test, even
    after a failure, it stops every server and ``pg0 ui`` process, drops the
    instance state, and fails when a server, a process, or state remains.
    """
    home = Pg0Home(tmp_path, pg0_installation)
    home.check_port_free()
    try:
        yield home
    finally:
        home.cleanup()
