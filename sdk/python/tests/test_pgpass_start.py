"""`pg0 start` reads the password from the file that PGPASSFILE names."""

import re
import socket
import sys

import pytest

from .pgpass_support import (
    TEST_PORT,
    entry,
    generated_password,
    pgpass_escape,
    port_accepts_connections,
    write_pgpass,
)


def start_as_app(home, file):
    return home.start("--username", "app", "--database", "app", pgpassfile=file)


def assert_no_state(home):
    assert not (home.state / "instances" / "t").exists(), "no data folder"
    assert not port_accepts_connections(TEST_PORT), "no server"


def test_matching_entry_sets_password_and_rejects_default(pg0_home):
    """Given PGPASSFILE names a file with a matching entry, when pg0 start runs, expected the
    server accepts the password of that entry and rejects the default password postgres."""
    pw = generated_password()
    file = write_pgpass(pg0_home.cwd, "pgpass", [entry("127.0.0.1", TEST_PORT, "app", "app", pw)], 0o600)
    started = start_as_app(pg0_home, file)
    assert started.ok, started.output
    assert pg0_home.can_login(TEST_PORT, "app", "app", pw)
    assert not pg0_home.can_login(TEST_PORT, "app", "app", "postgres")


@pytest.mark.parametrize(
    "pattern",
    [
        pytest.param("*:{p}:app:app", id="host"),
        pytest.param("127.0.0.1:*:app:app", id="port"),
        pytest.param("127.0.0.1:{p}:*:app", id="database"),
        pytest.param("127.0.0.1:{p}:app:*", id="username"),
    ],
)
def test_wildcard_entry_selects_password(pg0_home, pattern):
    """Given an entry that uses * in one of the first four fields, when pg0 start runs, expected
    the start succeeds with the entry password."""
    pw = generated_password()
    line = f"{pattern.format(p=TEST_PORT)}:{pgpass_escape(pw)}"
    file = write_pgpass(pg0_home.cwd, "pgpass", [line], 0o600)
    started = start_as_app(pg0_home, file)
    assert started.ok, f"{pattern}\n{started.output}"
    assert pg0_home.can_login(TEST_PORT, "app", "app", pw), pattern


def test_first_matching_entry_wins(pg0_home):
    """Given two matching entries, when pg0 start runs, expected the first one wins."""
    first, second = generated_password(), generated_password()
    lines = [
        entry("127.0.0.1", "*", "*", "app", first),
        entry("127.0.0.1", TEST_PORT, "app", "app", second),
    ]
    file = write_pgpass(pg0_home.cwd, "pgpass", lines, 0o600)
    started = start_as_app(pg0_home, file)
    assert started.ok, started.output
    assert pg0_home.can_login(TEST_PORT, "app", "app", first)
    assert not pg0_home.can_login(TEST_PORT, "app", "app", second)


def test_entry_with_password_postgres_is_accepted(pg0_home):
    """Given an entry whose password is postgres, when pg0 start runs, expected the start succeeds
    and the server accepts postgres (the rejection applies only when the password differs)."""
    file = write_pgpass(
        pg0_home.cwd, "pgpass",
        [entry("127.0.0.1", TEST_PORT, "postgres", "postgres", "postgres")], 0o600,
    )
    started = pg0_home.start(pgpassfile=file)
    assert started.ok, started.output
    assert pg0_home.can_login(TEST_PORT, "postgres", "postgres", "postgres")


def test_busy_default_port_uses_entry_for_chosen_port(pg0_home):
    """Given the default port 5432 is in use and no --port, when pg0 start chooses another port,
    expected the entry for the chosen port is the one that matches."""
    holder = socket.socket()
    try:
        holder.bind(("127.0.0.1", 5432))
    except OSError:
        pass  # another process holds 5432 already, which is the condition under test
    try:
        # The first run has no usable entry. Its error names the entry that it needs, with the chosen port.
        probe = write_pgpass(pg0_home.cwd, "probe", ["127.0.0.1:1:x:x:x"], 0o600)
        refused = pg0_home.start("--database", "app", "--username", "app", pgpassfile=probe, port=None)
        assert not refused.ok, refused.output
        needed = re.search(r"127\.0\.0\.1:(\d+):app:app", refused.stderr)
        assert needed, f"the error names the entry\n{refused.output}"
        chosen = int(needed.group(1))
        pg0_home.watched_ports.append(chosen)
        assert chosen != 5432, "pg0 chose another port than the busy default"

        pw = generated_password()
        file = write_pgpass(pg0_home.cwd, "pgpass", [entry("127.0.0.1", chosen, "app", "app", pw)], 0o600)
        started = pg0_home.start("--database", "app", "--username", "app", pgpassfile=file, port=None)
        assert started.ok, started.output
        assert pg0_home.can_login(chosen, "app", "app", pw)
        info = pg0_home.pg0("info", "--name", "t", pgpassfile=file)
        assert f"127.0.0.1:{chosen}/app" in info.stdout, info.output
    finally:
        holder.close()


@pytest.mark.parametrize("state", ["missing", "unmatched", "open"])
def test_password_flag_ignores_bad_file(pg0_home, state):
    """Given a start with -P and PGPASSFILE set to a missing file, an unmatched file, or an open
    file, when pg0 start runs, expected the start succeeds with the -P password and does not read the file."""
    pw = generated_password()
    if state == "missing":
        file = pg0_home.cwd / "absent"
    elif state == "unmatched":
        file = write_pgpass(pg0_home.cwd, "f", [entry("127.0.0.1", TEST_PORT + 1, "x", "x", "x")], 0o600)
    else:
        file = write_pgpass(
            pg0_home.cwd, "f", [entry("127.0.0.1", TEST_PORT, "postgres", "postgres", "x")], 0o644
        )
    started = pg0_home.start("-P", pw, pgpassfile=file)
    assert started.ok, f"{state}\n{started.output}"
    assert pg0_home.can_login(TEST_PORT, "postgres", "postgres", pw), state


@pytest.mark.parametrize("state", ["missing", "unmatched", "unreadable"])
def test_start_refuses_unusable_file(pg0_home, state):
    """Given a missing file, an unreadable file, or a file with no matching entry, when pg0 start
    runs, expected a non-zero exit that names PGPASSFILE, the file, and the entry, no data folder,
    no server, and no default password."""
    if state == "missing":
        file = pg0_home.cwd / "absent"
    elif state == "unmatched":
        file = write_pgpass(pg0_home.cwd, "other", [entry("127.0.0.1", TEST_PORT + 1, "app", "app", "x")], 0o600)
    else:
        file = write_pgpass(
            pg0_home.cwd, "unreadable", [entry("127.0.0.1", TEST_PORT, "app", "app", "x")], 0o600
        )
        pg0_home.make_unreadable(file)
    try:
        started = start_as_app(pg0_home, file)
        assert not started.ok, started.output
        assert "PGPASSFILE" in started.stderr and str(file) in started.stderr, (
            f"names the variable and the file\n{started.output}"
        )
        assert f"127.0.0.1:{TEST_PORT}:app:app" in started.stderr, f"names the entry\n{started.output}"
        assert_no_state(pg0_home)
    finally:
        if file.exists():
            pg0_home.make_readable(file)


@pytest.mark.parametrize("mode", [0o640, 0o604, 0o644, 0o660, 0o666, 0o610], ids=oct)
def test_open_file_is_refused_on_unix_and_accepted_on_windows(pg0_home, mode):
    """Given a file with a group or world permission bit, when pg0 start runs, expected on Unix a
    non-zero exit that names the file and says to remove group and world access, no data folder, no
    server. On Windows pg0 checks no access rights (src/pgpass.rs), so the start succeeds with the
    entry password."""
    pw = generated_password()
    file = write_pgpass(pg0_home.cwd, "pgpass", [entry("127.0.0.1", TEST_PORT, "app", "app", pw)], mode)
    started = start_as_app(pg0_home, file)
    if sys.platform == "win32":
        assert started.ok, f"mode {mode:o}\n{started.output}"
        assert pg0_home.can_login(TEST_PORT, "app", "app", pw)
        return
    assert not started.ok, f"mode {mode:o}\n{started.output}"
    error = started.stderr.lower()
    assert str(file) in started.stderr and "group" in error and "world" in error, started.output
    assert_no_state(pg0_home)


def test_start_accepts_owner_only_file(pg0_home):
    """Given a readable owner-only file (mode 0400), when pg0 start runs, expected the start succeeds."""
    pw = generated_password()
    file = write_pgpass(pg0_home.cwd, "pgpass", [entry("127.0.0.1", TEST_PORT, "app", "app", pw)], 0o400)
    started = start_as_app(pg0_home, file)
    assert started.ok, started.output
    assert pg0_home.can_login(TEST_PORT, "app", "app", pw)
