"""Commands that run after `pg0 start` read the password from the file that PGPASSFILE names."""

import json
import sys

import pytest

from .pgpass_support import (
    FILE_STATES,
    TEST_PORT,
    apply_file_state,
    assert_absent,
    assert_files_absent,
    start_from_file,
)


@pytest.fixture
def started(pg0_home):
    return start_from_file(pg0_home, "appuser", "app")


def test_start_from_file_saves_empty_password_and_no_path(started):
    """Given a start with the password from PGPASSFILE, when instance.json is read, expected an
    empty password and no password-file path."""
    text = started.home.instance_json()
    assert json.loads(text)["password"] == ""
    assert str(started.file) not in text, "no path"
    assert_absent("instance.json", text, started.pw)


def test_later_commands_read_file_from_another_folder(started):
    """Given a started instance, when info, list, and psql run from another working directory
    with an absolute PGPASSFILE, expected they succeed, the URLs hold no password, and psql runs
    the query."""
    home = started.home
    elsewhere = home.root / "elsewhere"
    elsewhere.mkdir()
    info = home.pg0("info", "--name", "t", pgpassfile=started.file, cwd=elsewhere)
    listing = home.pg0("list", pgpassfile=started.file, cwd=elsewhere)
    psql = home.pg0("psql", "--name", "t", "--", "-Atc", "select 41 + 1", pgpassfile=started.file, cwd=elsewhere)
    for run in (info, listing, psql):
        assert run.ok, run.output
        assert_absent("stdout", run.stdout, started.pw)
        assert_absent("stderr", run.stderr, started.pw)
    assert f"127.0.0.1:{TEST_PORT}/app" in info.stdout, info.output
    assert psql.stdout.strip() == "42", psql.output


def test_relative_path_resolves_against_each_working_folder(started):
    """Given a relative PGPASSFILE path, when a command runs in the folder that holds the file and
    again in another folder, expected it succeeds in the first folder and names the file in an
    error in the second."""
    home = started.home
    query = ("psql", "--name", "t", "--", "-Atc", "select 1")
    here = home.pg0(*query, pgpassfile="pgpass", cwd=home.cwd)
    assert here.ok, here.output
    elsewhere = home.root / "elsewhere"
    elsewhere.mkdir()
    there = home.pg0(*query, pgpassfile="pgpass", cwd=elsewhere)
    assert not there.ok, there.output
    assert "PGPASSFILE" in there.stderr, there.output


def test_stop_and_logs_need_no_file(started):
    """Given a started instance and the password file removed and PGPASSFILE unset, when stop and
    logs run, expected both succeed."""
    started.file.unlink()
    logs = started.home.pg0("logs", "--name", "t")
    assert logs.ok, logs.output
    stopped = started.home.stop()
    assert stopped.ok, stopped.output


@pytest.mark.parametrize("state", FILE_STATES)
def test_later_commands_report_unusable_file_as_credential_error(started, state):
    """Given an empty saved password and a PGPASSFILE that is unset, empty, missing, unreadable,
    open, or without a matching entry, when info, list, psql, and install-extension run, expected a
    non-zero exit with an error that names PGPASSFILE, the file when named, and the entry, and that
    is not reported as a stopped instance or an authentication error. On Windows an open file is
    accepted (pg0 checks no access rights there), so the commands succeed."""
    home = started.home
    value = apply_file_state(started, state)
    commands = [
        ["info", "--name", "t"],
        ["list"],
        ["psql", "--name", "t", "--", "-c", "select 1"],
        ["install-extension", "--name", "t", "pgvector_compiled"],
    ]
    # pg0 checks no access rights on Windows, so an open file is a usable file there.
    usable = sys.platform == "win32" and state == "open"
    try:
        for command in commands:
            run = home.pg0(*command, pgpassfile=value)
            label = f"{state} {command}"
            if usable:
                if command[0] == "install-extension":
                    # It passes the credential step and then ends with an extension error.
                    assert "PGPASSFILE" not in run.stderr, f"{label}\n{run.output}"
                else:
                    assert run.ok, f"{label}\n{run.output}"
                continue
            assert not run.ok, f"{label}\n{run.output}"
            assert "PGPASSFILE" in run.stderr, f"{label}\n{run.output}"
            if state in ("unmatched", "missing"):
                assert f"127.0.0.1:{TEST_PORT}:app:appuser" in run.stderr, f"{label}\n{run.output}"
            if state not in ("unset", "empty"):
                assert str(value) in run.stderr, f"{label} names the file\n{run.output}"
            assert "authentication failed" not in run.stderr.lower(), f"{label}\n{run.output}"
            assert "stopped" not in run.stdout.lower(), f"{label}: reported as stopped\n{run.output}"
    finally:
        if started.file.exists():
            home.make_readable(started.file)


def test_install_extension_passes_credential_step_without_leak(started):
    """Given a started instance and a usable PGPASSFILE, when install-extension runs, expected it
    passes the credential step (no PGPASSFILE error) and its output and files hold no password.
    No extension is installable for the bundled PostgreSQL here (the registry has no build for
    it), so the command ends with an extension error after the credential step."""
    home = started.home
    run = home.pg0("install-extension", "--name", "t", "pgvector_compiled", pgpassfile=started.file, timeout=120)
    assert "PGPASSFILE" not in run.stderr, run.output
    assert_absent("stdout", run.stdout, started.pw)
    assert_absent("stderr", run.stderr, started.pw)
    scanned = assert_files_absent([home.state, home.tmp], started.pw)
    assert home.state / "instances" / "t" / "instance.json" in scanned


def test_info_prints_password_free_uri_for_empty_saved_password(started):
    """Given an instance whose saved password is empty, when info runs with a usable PGPASSFILE,
    expected the URL holds no password and no password reaches the output."""
    run = started.home.pg0("info", "--name", "t", "--output", "json", pgpassfile=started.file)
    assert run.ok, run.output
    assert_absent("info --json", run.stdout, started.pw)
    assert f"postgresql://appuser@127.0.0.1:{TEST_PORT}/app" in run.stdout, run.output
