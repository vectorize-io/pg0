"""Runtime executable selection and separate native/state directories."""

import json
import subprocess
import shutil
import sys
from pathlib import Path

import pytest
import pg0


def test_explicit_executable_precedes_bundled_binary(tmp_path, monkeypatch):
    executable = tmp_path / "approved directory" / ("pg0.exe" if sys.platform == "win32" else "pg0")
    executable.parent.mkdir()
    executable.write_bytes(b"test executable")
    executable.chmod(0o755)
    monkeypatch.setenv("PG0_EXECUTABLE", str(executable))
    monkeypatch.setattr(pg0, "_get_bundled_binary", lambda: pytest.fail("unexpected fallback"))
    assert pg0._find_pg0() == str(executable)


@pytest.mark.parametrize("value", ["", "relative/pg0", "missing"])
def test_invalid_executable_does_not_fall_back(tmp_path, monkeypatch, value):
    if value == "missing":
        value = str(tmp_path / "missing")
    monkeypatch.setenv("PG0_EXECUTABLE", value)
    monkeypatch.setattr(pg0, "_get_bundled_binary", lambda: pytest.fail("unexpected fallback"))
    with pytest.raises(pg0.Pg0NotFoundError, match="PG0_EXECUTABLE"):
        pg0._find_pg0()


def test_directory_is_not_an_executable(tmp_path, monkeypatch):
    monkeypatch.setenv("PG0_EXECUTABLE", str(tmp_path))
    with pytest.raises(pg0.Pg0NotFoundError, match="PG0_EXECUTABLE"):
        pg0._find_pg0()


@pytest.mark.skipif(sys.platform == "win32", reason="POSIX executable permission")
def test_non_executable_file_is_rejected(tmp_path, monkeypatch):
    executable = tmp_path / "pg0"
    executable.touch(mode=0o600)
    monkeypatch.setenv("PG0_EXECUTABLE", str(executable))
    with pytest.raises(pg0.Pg0NotFoundError, match="not executable"):
        pg0._find_pg0()


def test_unset_override_keeps_bundled_discovery(tmp_path, monkeypatch):
    monkeypatch.delenv("PG0_EXECUTABLE", raising=False)
    bundled = tmp_path / "bundled-pg0"
    monkeypatch.setattr(pg0, "_get_bundled_binary", lambda: bundled)
    assert pg0._find_pg0() == str(bundled)


def test_native_installation_and_state_can_be_separated(tmp_path, monkeypatch):
    """Exercise the built CLI through SDK discovery, SQL, stop, and restart."""
    if sys.platform == "win32":
        # Elevated Windows sessions can make pytest's mode=0700 directory
        # owned by Administrators. initdb removes that group from its token.
        sid = subprocess.check_output(
            ["powershell.exe", "-NoProfile", "-Command",
             "[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value"],
            text=True,
        ).strip()
        subprocess.run(
            ["icacls", str(tmp_path), "/grant:r", f"*{sid}:(OI)(CI)(F)"],
            check=True, capture_output=True,
        )
    original = Path(pg0._find_pg0())
    approved = tmp_path / "approved executables"
    approved.mkdir()
    executable = approved / original.name
    shutil.copy2(original, executable)
    state = tmp_path / "writable state"
    installation = approved / "installation"
    monkeypatch.setenv("PG0_EXECUTABLE", str(executable))
    monkeypatch.setenv("PG0_HOME", str(state))
    monkeypatch.setenv("PG0_INSTALLATION_DIR", str(installation))
    instance = pg0.Pg0(name="runtime-paths")
    try:
        instance.start()
        metadata = json.loads(
            (state / "instances/runtime-paths/instance.json").read_text()
        )
        assert Path(metadata["installation_dir"]) == installation
        assert Path(instance.info().data_dir) == state / "instances/runtime-paths/data"
        instance.execute("CREATE EXTENSION IF NOT EXISTS vector")
        result = instance.execute("SELECT '[1,2]'::vector <-> '[1,2]'::vector")
        assert "0" in result
        instance.execute("CREATE TABLE path_test (value integer)")
        instance.execute("INSERT INTO path_test VALUES (42)")
        instance.stop()
        instance.start()
        assert "42" in instance.execute("SELECT value FROM path_test")
        assert any(installation.glob("*/bin/postgres*"))
        assert not (state / "installation").exists()
    finally:
        instance.drop(force=True)
