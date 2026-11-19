"""`pg0 ui` reads the instance password from the file that PGPASSFILE names."""

import sys

import pytest

from .pgpass_support import (
    FILE_STATES,
    UiServer,
    apply_file_state,
    assert_absent,
    assert_files_absent,
    start_from_file,
)

INSTANCES = '{"action":"instances"}'
DATABASES = '{"action":"databases","instance":"t"}'
TABLES_APP = '{"action":"tables","instance":"t","db":"app"}'
ROWS = '{"action":"rows","instance":"t","db":"app","schema":"public","table":"notes"}'


@pytest.fixture
def started(pg0_home):
    instance = start_from_file(pg0_home, "appuser", "app")
    seed = pg0_home.pg0(
        "psql", "--name", "t", "--",
        "-c", "create table notes(id int, body text); insert into notes values (1, 'hello');",
        pgpassfile=instance.file,
    )
    assert seed.ok, f"{seed.output}\nstart output: {instance.started.stdout}\nserver log:\n{pg0_home.server_log()}"
    return instance


def test_ui_lists_instances_databases_tables_and_rows(started):
    """Given an instance started with PGPASSFILE, when pg0 ui runs with the same PGPASSFILE,
    expected the instance, its databases, its tables, and the rows of a table are listed through
    the temporary read-only role."""
    ui = UiServer(started.home, started.file)
    (c1, instances), (c2, databases), (c3, tables), (c4, rows) = (
        ui.api(body) for body in (INSTANCES, DATABASES, TABLES_APP, ROWS)
    )
    assert (c1, c2, c3, c4) == (200, 200, 200, 200), f"{instances} {databases} {tables} {rows}"
    assert '"t"' in instances
    assert '"app"' in databases
    assert "notes" in tables
    assert "hello" in rows
    ui.stop()


def test_ui_browses_other_database_with_single_entry(started):
    """Given a database other than the saved one, when its tables are browsed, expected success
    without another entry in the file."""
    home = started.home
    created = home.pg0("psql", "--name", "t", "--", "-c", "create database other", pgpassfile=started.file)
    assert created.ok, created.output
    ui = UiServer(home, started.file)
    code, body = ui.api('{"action":"tables","instance":"t","db":"other"}')
    assert code == 200, body
    ui.stop()


def test_ui_session_leaks_no_password(started):
    """Given a browsing session, when every response, every file, and the ui process are scanned,
    expected no form of the instance password in output, argument list, environment, or file."""
    home = started.home
    ui = UiServer(home, started.file)
    markers = {INSTANCES: '"t"', DATABASES: '"app"', TABLES_APP: "notes", ROWS: "hello"}
    for body, marker in markers.items():
        code, response = ui.api(body)
        assert code == 200, response
        assert marker in response, f"the scan read the response to {body}: {response}"
        assert_absent("response", response, started.pw)
    args = home.process_args(ui.pid)
    environment = home.process_environment(ui.pid)
    assert " ui " in f" {args} ", f"the scan read the arguments of pg0 ui: {args}"
    assert "PG0_HOME" in environment, "the scan read the environment of pg0 ui"
    assert_absent("args", args, started.pw)
    assert_absent("env", environment, started.pw)
    stdout, stderr = ui.stop()
    assert "#token=" in stdout, "the scan read the output of pg0 ui"
    assert_absent("stdout", stdout, started.pw)
    assert_absent("stderr", stderr, started.pw)
    scanned = assert_files_absent([home.state, home.tmp], started.pw)
    assert home.state / "instances" / "t" / "instance.json" in scanned


@pytest.mark.parametrize("state", FILE_STATES)
def test_ui_reports_unusable_file_and_keeps_serving(started, state):
    """Given a PGPASSFILE that is unset, empty, missing, unreadable, open, or without a matching
    entry, when the page lists the instance's databases, expected a credential error for that
    instance that names PGPASSFILE (and the file when named, and the entry), that is not an
    authentication error, and that pg0 ui keeps serving (a second request still answers). On Windows
    an open file is accepted (pg0 checks no access rights there), so the databases are listed."""
    value = apply_file_state(started, state)
    try:
        ui = UiServer(started.home, value)
        code, first = ui.api(DATABASES)
        if sys.platform == "win32" and state == "open":
            assert code == 200, f"{state}: {first}"
            assert '"app"' in first, f"{state}: {first}"
            ui.stop()
            return
        assert code != 200, f"{state}: {first}"
        assert "PGPASSFILE" in first, f"{state}: {first}"
        assert "authentication failed" not in first.lower(), f"{state}: {first}"
        if state not in ("unset", "empty"):
            assert str(value) in first, f"{state} names the file: {first}"
        code, second = ui.api(INSTANCES)
        assert code == 200, f"{state}: the ui keeps serving"
        assert '"t"' in second, f"{state}: {second}"
        ui.stop()
    finally:
        if started.file.exists():
            started.home.make_readable(started.file)


def test_ui_shutdown_removes_temporary_role(started):
    """Given pg0 ui shutdown, when the roles are listed, expected the temporary role is gone (the
    shutdown removal read the password from the file)."""
    home = started.home
    ui = UiServer(home, started.file)
    code, body = ui.api(DATABASES)
    assert code == 200, body
    ui.stop()
    roles = home.pg0(
        "psql", "--name", "t", "--",
        "-Atc", "select count(*) from pg_roles where rolname like 'pg0_ui_%'",
        pgpassfile=started.file,
    )
    assert roles.stdout.strip() == "0", roles.output
