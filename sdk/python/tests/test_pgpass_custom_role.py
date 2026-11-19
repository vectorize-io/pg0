"""A start with a username and database other than postgres, using only the password file."""

import pytest

from .pgpass_support import assert_absent, assert_files_absent, start_from_file


@pytest.fixture
def started(pg0_home):
    """The only entry of the file is for the role appuser. No entry exists for the role postgres."""
    return start_from_file(pg0_home, "appuser", "app")


def test_start_with_custom_user_creates_role_database_and_grant(started):
    """Given a username and a database that differ from postgres and a file with only that one
    entry, when pg0 start runs, expected the role and the database exist, the role holds the
    privilege grant, and the start needed no entry for the postgres role."""

    def as_appuser(sql):
        """Run a query as the role appuser. libpq reads the password file that PGPASSFILE names."""
        run = started.home.psql(started.port, "appuser", "app", sql, pgpassfile=started.file)
        assert run.ok, run.output
        return run.stdout.strip()

    assert as_appuser("select count(*) from pg_roles where rolname = 'appuser'") == "1"
    assert as_appuser("select count(*) from pg_database where datname = 'app'") == "1"
    assert as_appuser("select has_database_privilege('appuser', 'app', 'CONNECT')::int") == "1"
    assert as_appuser("select has_database_privilege('appuser', 'app', 'CREATE')::int") == "1"
    # The role is a superuser, so the privilege functions are true even without a grant.
    # The grant itself is an entry in the database ACL.
    acl = as_appuser("select datacl::text from pg_database where datname = 'app'")
    assert "appuser=CTc/" in acl, f"the database ACL holds the grant of appuser: {acl}"


def test_start_with_custom_user_leaks_no_password(started):
    """Given the same start, when the bootstrap steps run, expected no password in output or
    generated files, in any form that SQL text or URLs use (the leak scan)."""
    home = started.home
    assert "Creating user 'appuser'" in started.started.stdout, started.started.output
    assert_absent("stdout", started.started.stdout, started.pw)
    assert_absent("stderr", started.started.stderr, started.pw)
    scanned = assert_files_absent([home.state, home.tmp], started.pw)
    assert any(path.parent == home.state / "instances" / "t" / "data" / "log" for path in scanned), (
        "the scan reached the server log"
    )
