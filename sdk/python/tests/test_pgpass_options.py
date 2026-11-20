"""The explicit password, the default password, and PGPASSFILE: which one `pg0 start` uses."""

import json

from .pgpass_support import TEST_PORT, entry, generated_password, json_escape, write_pgpass


def saved_password(home):
    return json.loads(home.instance_json())["password"]


def test_password_flag_without_file_is_stored(pg0_home):
    """Given -P and no PGPASSFILE, when pg0 start runs, expected the server accepts that password
    and instance.json stores it as before."""
    pw = generated_password()
    started = pg0_home.start("-P", pw)
    assert started.ok, started.output
    assert json_escape(pw) in pg0_home.instance_json(), "the legacy path still stores the -P password"
    assert pg0_home.can_login(TEST_PORT, "postgres", "postgres", pw)


def test_default_password_without_flag_or_file(pg0_home):
    """Given neither -P nor PGPASSFILE, when pg0 start runs, expected the password is postgres."""
    started = pg0_home.start()
    assert started.ok, started.output
    assert saved_password(pg0_home) == "postgres"
    assert pg0_home.can_login(TEST_PORT, "postgres", "postgres", "postgres")


def test_empty_pgpassfile_gives_default_password(pg0_home):
    """Given PGPASSFILE set to an empty value, when pg0 start runs with no -P, expected the
    password is postgres."""
    started = pg0_home.start(pgpassfile="")
    assert started.ok, started.output
    assert saved_password(pg0_home) == "postgres"
    assert pg0_home.can_login(TEST_PORT, "postgres", "postgres", "postgres")


def test_password_flag_beats_matching_file(pg0_home):
    """Given -P and PGPASSFILE that names a file with a matching entry for another password, when
    pg0 start runs, expected the -P password is stored and used, not the file password."""
    flag_pw, file_pw = generated_password(), generated_password()
    file = write_pgpass(
        pg0_home.cwd, "pgpass", [entry("127.0.0.1", TEST_PORT, "postgres", "postgres", file_pw)], 0o600
    )
    started = pg0_home.start("-P", flag_pw, pgpassfile=file)
    assert started.ok, started.output
    text = pg0_home.instance_json()
    assert json_escape(flag_pw) in text
    assert json_escape(file_pw) not in text
    assert pg0_home.can_login(TEST_PORT, "postgres", "postgres", flag_pw)
    assert not pg0_home.can_login(TEST_PORT, "postgres", "postgres", file_pw)


def test_explicit_postgres_flag_beats_file(pg0_home):
    """Given an explicit -P equal to the value postgres and a PGPASSFILE with a different
    password, when pg0 start runs, expected the explicit postgres wins (an explicit -P counts even
    when it equals the default)."""
    file_pw = generated_password()
    file = write_pgpass(
        pg0_home.cwd, "pgpass", [entry("127.0.0.1", TEST_PORT, "postgres", "postgres", file_pw)], 0o600
    )
    started = pg0_home.start("-P", "postgres", pgpassfile=file)
    assert started.ok, started.output
    assert saved_password(pg0_home) == "postgres"
    assert pg0_home.can_login(TEST_PORT, "postgres", "postgres", "postgres")
