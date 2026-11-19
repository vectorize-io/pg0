"""The password from PGPASSFILE stays out of output, files, process arguments and environment, and logs."""

import json
import os
import subprocess
import sys

import pytest

from .pgpass_support import (
    TEST_PORT,
    assert_absent,
    assert_files_absent,
    entry,
    files_under,
    find_forms,
    generated_password,
    password_forms,
    pgpass_escape,
    process_args,
    process_environment,
    start_from_file,
    write_pgpass,
)

@pytest.fixture
def started(pg0_home):
    return start_from_file(pg0_home, "app", "app")


def test_leak_scan_finds_every_form_in_every_channel(tmp_path):
    """Given a password in each of its forms, when the scans read bytes, a file under a scanned
    folder, and the arguments and environment of a live process, expected each scan reports each form."""
    pw = generated_password()
    forms = password_forms(pw)
    everything = " ".join(forms.values())
    assert sorted(find_forms(everything, pw)) == sorted(forms)

    scanned = tmp_path / "scanned"
    scanned.mkdir()
    (scanned / "planted").write_text(everything)
    with pytest.raises(AssertionError, match="holds the password"):
        assert_files_absent([scanned], pw)
    assert [p.name for p, _ in files_under(scanned)] == ["planted"]

    env = {**os.environ, "PGPASSFILE_SCAN_TARGET": everything}
    # The interpreter is not a protected system binary, so `ps -E` can read its environment.
    process = subprocess.Popen(
        [sys.executable, "-c", "import time; time.sleep(60)", everything],
        stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env,
    )
    try:
        args = process_args(process.pid, env=env, scratch=tmp_path / "capture")
        environment = process_environment(process.pid, env=env, scratch=tmp_path / "capture")
    finally:
        process.kill()
        process.wait()
    assert sorted(find_forms(args, pw)) == sorted(forms), "the argument scan reads every form"
    assert sorted(find_forms(environment, pw)) == sorted(forms), "the environment scan reads every form"


def test_no_password_in_output_of_any_command(started):
    """Given a start with PGPASSFILE, when each pg0 command runs with the same PGPASSFILE,
    expected none prints the password."""
    home = started.home
    commands = [
        ["info", "--name", "t"],
        ["list"],
        ["psql", "--name", "t", "-c", "select 1"],
        ["logs", "--name", "t"],
        ["stop", "--name", "t"],
    ]
    runs = {"start": started.started}
    for command in commands:
        runs[command[0]] = home.pg0(*command, pgpassfile=started.file)
    for name, run in runs.items():
        assert run.ok, f"{name}\n{run.output}"
        assert run.stdout.strip(), f"the scan captured the stdout of {name}"
        assert_absent(f"stdout of {name}", run.stdout, started.pw)
        assert_absent(f"stderr of {name}", run.stderr, started.pw)
    assert f"127.0.0.1:{TEST_PORT}/app" in runs["info"].stdout, "info printed the connection URI"


def test_no_password_or_temp_pgpass_in_any_file(started):
    """Given a start with PGPASSFILE, when it finishes, expected instance.json, every file under
    PG0_HOME, and every new file in TMPDIR hold no form of the password, and no .pgpass temp file exists."""
    home = started.home
    stopped = home.stop()
    assert stopped.ok, stopped.output
    scanned = assert_files_absent([home.state, home.tmp], started.pw)
    assert home.state / "instances" / "t" / "instance.json" in scanned, "the scan read instance.json"
    assert any(p.parent.name == "log" for p in scanned), "the scan read the server log"
    assert all("pgpass" not in p.name for p in scanned if home.tmp in p.parents), "no .pgpass temp file"
    assert json.loads(home.instance_json())["password"] == "", "instance.json holds no password"
    assert pgpass_escape(started.pw).encode() in started.file.read_bytes(), "the user's file still holds it"


def test_no_password_in_process_args_or_environment(started):
    """Given a running instance, when the processes that pg0 started are listed, expected no form
    of the password in any argument list or environment."""
    home = started.home
    pids = home.server_pids()
    assert len(pids) > 1, f"the scan found the postmaster and its children: {pids}"
    data_dir = str(home.state / "instances" / "t" / "data")
    for pid in pids:
        args = home.process_args(pid)
        if pid == pids[0]:
            assert data_dir in args, f"the postmaster arguments name its data folder: {args}"
        assert_absent(f"args of {pid}", args, started.pw)
        environment = home.process_environment(pid)
        if pid == pids[0]:
            assert "PG0_HOME" in environment, "the scan read the environment of the postmaster"
        assert_absent(f"env of {pid}", environment, started.pw)


def test_failed_start_leaks_no_password(pg0_home):
    """Given a failed start with PGPASSFILE (an entry for the wrong port), when it fails,
    expected the error output, the server log, and the files hold no password."""
    pw = generated_password()
    file = write_pgpass(pg0_home.cwd, "pgpass", [entry("127.0.0.1", TEST_PORT + 1, "app", "app", pw)], 0o600)
    failed = pg0_home.start("--username", "app", "--database", "app", pgpassfile=file)
    assert not failed.ok, failed.output
    assert "PGPASSFILE" in failed.stderr, f"the scan captured the error\n{failed.output}"
    assert_absent("stderr", failed.stderr, pw)
    assert_absent("stdout", failed.stdout, pw)
    assert_files_absent([pg0_home.state, pg0_home.tmp], pw)


def test_server_log_has_no_password(started):
    """Given a start with a custom username, when the server log is read, expected no password form in it."""
    home = started.home
    stopped = home.stop()
    assert stopped.ok, stopped.output
    log = home.server_log()
    assert "database system is ready to accept connections" in log, "the scan found the server log"
    assert_absent("server log", log, started.pw)
