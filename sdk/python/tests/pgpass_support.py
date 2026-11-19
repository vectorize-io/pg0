"""Helpers for the PGPASSFILE tests.

The tests run the built ``pg0`` CLI (found through ``pg0._find_pg0()``, so
``PG0_EXECUTABLE`` selects the executable) against a real PostgreSQL server.
Every process writes beneath the folder of one test, and every invocation has a
deadline.
"""

import http.client
import json
import os
import re
import secrets
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from urllib.parse import quote

import pytest
import pg0

# Same port as TEST_PORT in test_pg0.py. It is below the ephemeral range, so no
# client socket takes it as a source port.
TEST_PORT = 15432

# `pg0 start` extracts the bundled PostgreSQL on the first run of a session.
START_TIMEOUT = 180
CLI_TIMEOUT = 60
PROBE_TIMEOUT = 30
UI_TIMEOUT = 60


class Run:
    """The result of one finite command."""

    def __init__(self, argv, returncode, stdout, stderr):
        self.argv = argv
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr

    @property
    def ok(self):
        return self.returncode == 0

    @property
    def output(self):
        """The text of a failure message: the command, its exit status, and both streams."""
        return (
            f"command: {' '.join(map(str, self.argv))}\n"
            f"exit status: {self.returncode}\n"
            f"stdout:\n{self.stdout}\nstderr:\n{self.stderr}"
        )


def _read(handle):
    handle.seek(0)
    return handle.read().decode("utf-8", errors="replace")


def run_command(argv, *, env, cwd, scratch, timeout):
    """Run a command that must finish. The command is complete when its process exits.

    Output goes to temporary files inside ``scratch``, because PostgreSQL keeps
    the output handles of pg0 open after pg0 exits and a pipe would never reach
    end of file.
    """
    scratch.mkdir(parents=True, exist_ok=True)
    # Like pg0._run_pg0, suppress the transient console window on Windows.
    extra = {"creationflags": subprocess.CREATE_NO_WINDOW} if sys.platform == "win32" else {}
    with tempfile.TemporaryFile(dir=scratch) as out, tempfile.TemporaryFile(dir=scratch) as err:
        process = subprocess.Popen(
            argv, stdin=subprocess.DEVNULL, stdout=out, stderr=err, env=env, cwd=cwd, **extra
        )
        try:
            returncode = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
            pytest.fail(
                f"command did not exit within {timeout} seconds\n"
                + Run(argv, None, _read(out), _read(err)).output
            )
        return Run(argv, returncode, _read(out), _read(err))


# --- password forms ---------------------------------------------------------


def generated_password():
    """A password built at run time. The ``:``, ``\\`` and ``'`` characters are part of the value."""
    hex_ = secrets.token_hex(12)
    return f"{hex_[:6]}:{hex_[6:12]}\\{hex_[12:18]}'{hex_[18:]}"


def sql_escape(pw):
    return pw.replace("'", "''")


def pgpass_escape(pw):
    return pw.replace("\\", "\\\\").replace(":", "\\:")


def url_encode(pw):
    return quote(pw, safe="")


def json_escape(pw):
    return json.dumps(pw)[1:-1]


def password_forms(pw):
    """Every form of the password that an output channel can hold."""
    return {
        "raw": pw,
        "json": json_escape(pw),
        "url": url_encode(pw),
        "pgpass": pgpass_escape(pw),
        "sql": sql_escape(pw),
    }


def find_forms(data, pw):
    """The names of the password forms that occur in the bytes."""
    if isinstance(data, str):
        data = data.encode()
    return [name for name, form in password_forms(pw).items() if form.encode() in data]


def assert_absent(channel, data, pw):
    found = find_forms(data, pw)
    assert not found, f"{channel} holds the password in its {', '.join(found)} form"


def files_under(root):
    """Every readable file under a folder with its bytes."""
    found = []
    for path in sorted(Path(root).rglob("*")):
        if path.is_file():
            try:
                found.append((path, path.read_bytes()))
            except OSError:
                continue
    return found


def assert_files_absent(roots, pw):
    """Scan every file under the folders. Return the paths that the scan read."""
    scanned = []
    for root in roots:
        for path, data in files_under(root):
            assert_absent(str(path), data, pw)
            scanned.append(path)
    return scanned


# --- password files ---------------------------------------------------------


def entry(host, port, db, user, pw):
    return f"{host}:{port}:{db}:{user}:{pgpass_escape(pw)}"


def write_pgpass(directory, name, lines, mode):
    """Write a password file with the given permission mode."""
    path = Path(directory) / name
    path.write_text("\n".join(lines) + "\n")
    path.chmod(mode)
    return path


# --- processes --------------------------------------------------------------


def powershell(script, *, env, scratch):
    """Run a PowerShell script on Windows. Windows has no `ps`, so the process and permission steps use it."""
    return run_command(
        ["powershell.exe", "-NoProfile", "-Command", script],
        env=env, cwd=None, scratch=scratch, timeout=PROBE_TIMEOUT,
    )


def process_args(pid, *, env, scratch):
    """The command line of a live process, as text."""
    if sys.platform == "win32":
        return powershell(
            f'(Get-CimInstance Win32_Process -Filter "ProcessId={int(pid)}").CommandLine',
            env=env, scratch=scratch,
        ).stdout
    return run_command(
        ["ps", "-ww", "-o", "command=", "-p", str(pid)],
        env=env, cwd=None, scratch=scratch, timeout=PROBE_TIMEOUT,
    ).stdout


def _windows_process_environment(pid):
    """The environment block of a live 64-bit process, read through the Windows API.

    Windows has no command that prints another process's environment, so this reads it from the
    process's PEB, as psutil does: PEB.ProcessParameters, then its Environment and EnvironmentSize.
    """
    import ctypes
    from ctypes import wintypes

    assert ctypes.sizeof(ctypes.c_void_p) == 8, "the PEB offsets below are for 64-bit processes"
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    ntdll = ctypes.WinDLL("ntdll")
    kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel32.OpenProcess.restype = wintypes.HANDLE
    kernel32.ReadProcessMemory.argtypes = [
        wintypes.HANDLE, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t, ctypes.POINTER(ctypes.c_size_t),
    ]
    kernel32.ReadProcessMemory.restype = wintypes.BOOL
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    ntdll.NtQueryInformationProcess.argtypes = [
        wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.ULONG, ctypes.POINTER(wintypes.ULONG),
    ]
    ntdll.NtQueryInformationProcess.restype = ctypes.c_long

    class ProcessBasicInformation(ctypes.Structure):
        _fields_ = [
            ("ExitStatus", ctypes.c_long),
            ("PebBaseAddress", ctypes.c_void_p),
            ("AffinityMask", ctypes.c_void_p),
            ("BasePriority", ctypes.c_long),
            ("UniqueProcessId", ctypes.c_void_p),
            ("InheritedFromUniqueProcessId", ctypes.c_void_p),
        ]

    process_query_information, process_vm_read = 0x0400, 0x0010
    handle = kernel32.OpenProcess(process_query_information | process_vm_read, False, int(pid))
    if not handle:
        error = ctypes.get_last_error()
        if error == 87:  # ERROR_INVALID_PARAMETER: the process has exited, as /proc/<pid> is gone on Linux.
            return ""
        raise OSError(error, f"OpenProcess failed for pid {pid}")

    def read(address, size):
        buffer = ctypes.create_string_buffer(size)
        done = ctypes.c_size_t()
        if not kernel32.ReadProcessMemory(handle, ctypes.c_void_p(address), buffer, size, ctypes.byref(done)):
            raise OSError(ctypes.get_last_error(), f"ReadProcessMemory failed for pid {pid}")
        return buffer.raw[: done.value]

    try:
        info = ProcessBasicInformation()
        status = ntdll.NtQueryInformationProcess(handle, 0, ctypes.byref(info), ctypes.sizeof(info), None)
        assert status == 0, f"NtQueryInformationProcess failed for pid {pid}: {status:#x}"
        parameters = int.from_bytes(read(info.PebBaseAddress + 0x20, 8), "little")
        environment = int.from_bytes(read(parameters + 0x80, 8), "little")
        size = int.from_bytes(read(parameters + 0x3F0, 8), "little")
        assert 0 < size < 4 * 1024 * 1024, f"implausible environment size {size} for pid {pid}"
        return read(environment, size).decode("utf-16-le", "replace").replace("\0", "\n")
    finally:
        kernel32.CloseHandle(handle)


def process_environment(pid, *, env, scratch):
    """The environment of a live process, as text."""
    if sys.platform == "win32":
        return _windows_process_environment(pid)
    if sys.platform.startswith("linux"):
        try:
            return Path(f"/proc/{pid}/environ").read_bytes().decode("utf-8", "replace")
        except OSError:
            return ""
    return run_command(
        ["ps", "-ww", "-E", "-o", "command=", "-p", str(pid)],
        env=env, cwd=None, scratch=scratch, timeout=PROBE_TIMEOUT,
    ).stdout


def process_table(*, env, scratch):
    """Rows of (pid, ppid, command) for every process."""
    if sys.platform == "win32":
        listing = powershell(
            'Get-CimInstance Win32_Process | ForEach-Object '
            '{ "$($_.ProcessId) $($_.ParentProcessId) $($_.CommandLine)" }',
            env=env, scratch=scratch,
        )
    else:
        listing = run_command(
            ["ps", "-axww", "-o", "pid=,ppid=,command="],
            env=env, cwd=None, scratch=scratch, timeout=PROBE_TIMEOUT,
        )
    rows = []
    for line in listing.stdout.splitlines():
        parts = line.split(None, 2)
        if len(parts) >= 2 and parts[0].isdigit() and parts[1].isdigit():
            rows.append((int(parts[0]), int(parts[1]), parts[2] if len(parts) == 3 else ""))
    return rows


def current_user_sid(*, env, scratch):
    """The SID of the Windows user, as test_runtime_paths.py reads it."""
    return powershell(
        "[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value",
        env=env, scratch=scratch,
    ).stdout.strip()


def grant_user_full_control(folder, *, env, scratch):
    """On Windows, give the user full control of a folder, as test_runtime_paths.py does.

    Elevated Windows sessions can make pytest's mode=0700 directory owned by Administrators,
    and initdb removes that group from its token. No step on other platforms.
    """
    if sys.platform != "win32":
        return
    sid = current_user_sid(env=env, scratch=scratch)
    granted = run_command(
        ["icacls", str(folder), "/grant:r", f"*{sid}:(OI)(CI)(F)"],
        env=env, cwd=None, scratch=scratch, timeout=PROBE_TIMEOUT,
    )
    assert granted.ok, granted.output


def make_unreadable(path, *, env, scratch):
    """Deny the owner read access to a file, so that opening it fails on every platform."""
    if sys.platform == "win32":
        # chmod cannot deny read access on Windows, so deny it in the ACL.
        sid = current_user_sid(env=env, scratch=scratch)
        denied = run_command(
            ["icacls", str(path), "/deny", f"*{sid}:(R)"],
            env=env, cwd=None, scratch=scratch, timeout=PROBE_TIMEOUT,
        )
        assert denied.ok, denied.output
    else:
        Path(path).chmod(0o000)


def make_readable(path, *, env, scratch):
    """Undo make_unreadable, so that pytest can delete the folder."""
    if sys.platform == "win32":
        sid = current_user_sid(env=env, scratch=scratch)
        run_command(
            ["icacls", str(path), "/remove:d", f"*{sid}"],
            env=env, cwd=None, scratch=scratch, timeout=PROBE_TIMEOUT,
        )
    else:
        Path(path).chmod(0o600)


def port_accepts_connections(port):
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=2):
            return True
    except OSError:
        return False


# --- the isolated pg0 home --------------------------------------------------


class Pg0Home:
    """One test's PG0_HOME, TMPDIR, HOME and working directory, all beneath ``root``.

    The native installation lives in a folder that all tests of a session share.
    ``cleanup`` stops every instance and every ``pg0 ui`` process that the test
    started, drops the instance state, and fails when anything remains.
    """

    def __init__(self, root, installation):
        self.root = Path(root)
        self.installation = Path(installation)
        self.state = self.root / "home"
        self.tmp = self.root / "tmp"
        self.cwd = self.root / "cwd"
        self.user_home = self.root / "user-home"
        self.capture = self.root / "capture"
        for folder in (self.state, self.tmp, self.cwd, self.user_home, self.capture):
            folder.mkdir()
        self.ui_servers = []
        self.watched_ports = [TEST_PORT]
        grant_user_full_control(self.root, env=self.env(), scratch=self.capture)

    # environment and commands

    def env(self, pgpassfile=None, extra=None):
        """The environment of a child. ``pgpassfile`` None leaves PGPASSFILE unset; a string sets it."""
        env = {
            k: v
            for k, v in os.environ.items()
            if not k.startswith("PG0_") and k not in ("PGPASSFILE", "PGPASSWORD", "PGOPTIONS")
        }
        env.update(
            PG0_HOME=str(self.state),
            PG0_INSTALLATION_DIR=str(self.installation),
            TMPDIR=str(self.tmp),
            HOME=str(self.user_home),
        )
        if sys.platform == "win32":
            # Windows takes its temporary folder from TMP and TEMP, and libpq and psql read
            # their per-user files from APPDATA, not HOME.
            env.update(TMP=str(self.tmp), TEMP=str(self.tmp), APPDATA=str(self.user_home))
        if pgpassfile is not None:
            env["PGPASSFILE"] = str(pgpassfile)
        env.update(extra or {})
        return env

    def pg0(self, *args, pgpassfile=None, cwd=None, timeout=CLI_TIMEOUT):
        return run_command(
            [pg0._find_pg0(), *map(str, args)],
            env=self.env(pgpassfile), cwd=cwd or self.cwd, scratch=self.capture, timeout=timeout,
        )

    def start(self, *args, pgpassfile=None, name="t", port=TEST_PORT):
        """``pg0 start`` with the instance name and, unless ``port`` is None, the test port."""
        port_args = ["--port", port] if port is not None else []
        return self.pg0(
            "start", "--name", name, *port_args, *args, pgpassfile=pgpassfile, timeout=START_TIMEOUT
        )

    def stop(self, name="t", pgpassfile=None):
        return self.pg0("stop", "--name", name, pgpassfile=pgpassfile)

    # state

    def instance_json(self, name="t"):
        return (self.state / "instances" / name / "instance.json").read_text()

    def instance_names(self):
        folder = self.state / "instances"
        return sorted(p.name for p in folder.iterdir()) if folder.is_dir() else []

    def server_log(self, name="t"):
        """The text of the server log files of an instance."""
        folder = self.state / "instances" / name / "data" / "log"
        return "\n".join(
            data.decode("utf-8", "replace") for _, data in files_under(folder)
        ) if folder.is_dir() else ""

    def psql_binary(self):
        found = sorted(self.installation.glob("*/bin/psql.exe" if sys.platform == "win32" else "*/bin/psql"))
        assert found, f"no psql in the installation under {self.installation}"
        return found[0]

    def psql(self, port, user, db, sql, *, password=None, pgpassfile=None):
        """Run a query with the bundled psql. libpq reads PGPASSWORD or the file named by PGPASSFILE."""
        extra = {"PGCONNECT_TIMEOUT": "10"}
        if password is not None:
            extra["PGPASSWORD"] = password
        return run_command(
            [str(self.psql_binary()), "-h", "127.0.0.1", "-p", str(port), "-U", user, "-d", db, "-Atc", sql],
            env=self.env(pgpassfile, extra), cwd=self.cwd, scratch=self.capture, timeout=PROBE_TIMEOUT,
        )

    def can_login(self, port, db, user, password):
        """Try a login with the password. The test process sets PGPASSWORD for its own probe."""
        return self.psql(port, user, db, "select 1", password=password).ok

    def server_pids(self, name="t"):
        """The postmaster of the instance (the process whose command line names its data folder) and its children."""
        data = str(self.state / "instances" / name / "data")
        rows = process_table(env=self.env(), scratch=self.capture)
        masters = [pid for pid, _, command in rows if data in command]
        if not masters:
            return []
        return [masters[0]] + [pid for pid, ppid, _ in rows if ppid == masters[0]]

    def process_args(self, pid):
        return process_args(pid, env=self.env(), scratch=self.capture)

    def process_environment(self, pid):
        return process_environment(pid, env=self.env(), scratch=self.capture)

    def make_unreadable(self, path):
        make_unreadable(path, env=self.env(), scratch=self.capture)

    def make_readable(self, path):
        make_readable(path, env=self.env(), scratch=self.capture)

    # lifecycle

    def check_port_free(self):
        with socket.socket() as probe:
            if sys.platform != "win32":
                # On Windows SO_REUSEADDR lets a bind succeed on a port that a server holds.
                probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            try:
                probe.bind(("127.0.0.1", TEST_PORT))
            except OSError as e:
                pytest.fail(
                    f"port {TEST_PORT} is held by another process ({e}). "
                    "Stop that process before the tests run."
                )

    def cleanup(self):
        """Stop and drop everything the test started, then check that nothing remains."""
        problems = []
        logs = {name: self.server_log(name) for name in self.instance_names()}
        for ui in self.ui_servers:
            ui.kill()
        for name in self.instance_names():
            for command in (["stop", "--name", name], ["drop", "--name", name, "--force"]):
                try:
                    self.pg0(*command)
                except pytest.fail.Exception as e:
                    problems.append(str(e))
        left = self.instance_names()
        if left:
            problems.append(f"instance state remains under {self.state}: {left}")
        for port in self.watched_ports:
            deadline = time.monotonic() + 10
            while port_accepts_connections(port) and time.monotonic() < deadline:
                time.sleep(0.1)
            if port_accepts_connections(port):
                problems.append(f"a server still listens on port {port}")
        root = str(self.root)
        leftovers = [
            f"{pid}: {command}"
            for pid, _, command in process_table(env=self.env(), scratch=self.capture)
            if root in command
        ]
        if leftovers:
            problems.append("processes of the test remain:\n" + "\n".join(leftovers))
        if problems:
            log_text = "\n".join(f"server log of {n}:\n{t}" for n, t in logs.items())
            pytest.fail("cleanup check failed:\n" + "\n".join(problems) + "\n" + log_text)


# --- an instance started from a password file ---------------------------------


class Instance:
    """An instance named ``t`` that ``pg0 start`` created with the password from a file."""

    def __init__(self, home, user, db, started, pw, file):
        self.home = home
        self.port = TEST_PORT
        self.user = user
        self.db = db
        self.started = started
        self.pw = pw
        self.file = file


def start_from_file(home, user, db):
    """Start instance ``t`` with a generated password from a mode 0600 file with one entry."""
    pw = generated_password()
    line = entry("127.0.0.1", TEST_PORT, db, user, pw)
    assert line.endswith(pgpass_escape(pw)), "the entry holds the escaped generated password"
    file = write_pgpass(home.cwd, "pgpass", [line], 0o600)
    started = home.start("--username", user, "--database", db, pgpassfile=file)
    assert started.ok, started.output
    return Instance(home, user, db, started, pw, file)


FILE_STATES = ["unset", "empty", "missing", "unreadable", "open", "unmatched"]


def apply_file_state(instance, state):
    """Make the password file of the instance unusable in the named way.

    Return the value for PGPASSFILE: None (unset), a string, or a path.
    """
    if state == "unset":
        return None
    if state == "empty":
        return ""
    if state == "missing":
        instance.file.unlink()
        return instance.file
    if state == "unreadable":
        instance.home.make_unreadable(instance.file)
        return instance.file
    if state == "open":
        # pg0 refuses a file with group or world access on Unix. On Windows it checks no access rights.
        instance.file.chmod(0o644)
        return instance.file
    other = [entry("127.0.0.1", instance.port + 1, instance.db, instance.user, "x")]
    return write_pgpass(instance.home.cwd, "other", other, 0o600)


# --- pg0 ui -----------------------------------------------------------------


class UiServer:
    """A running ``pg0 ui``. The tests talk to its JSON endpoint (``POST /api``) over HTTP.

    The page opens a browser through ``open``, ``xdg-open``, or ``rundll32``, so a
    stand-in of each name is found first. The server binds 127.0.0.1 on a port
    that the system picks.
    """

    LINK = re.compile(r":(\d+)/#token=(\S+)")

    def __init__(self, home, pgpassfile=None):
        fake_bin = home.root / "bin"
        fake_bin.mkdir(exist_ok=True)
        executable = pg0._find_pg0()
        extra = {}
        if sys.platform == "win32":
            # Windows resolves rundll32 from the folder of the running executable before System32,
            # so pg0 runs from a copy beside a stand-in rundll32 that only prints an error and exits.
            executable = str(shutil.copy2(executable, fake_bin / "pg0.exe"))
            system32 = Path(os.environ.get("SystemRoot", r"C:\Windows")) / "System32"
            shutil.copy2(system32 / "whoami.exe", fake_bin / "rundll32.exe")
            # Its own process group lets stop() send Ctrl-Break to pg0 ui alone.
            extra = {"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP}
        else:
            for name in ("open", "xdg-open"):
                script = fake_bin / name
                script.write_text("#!/bin/sh\nexit 0\n")
                script.chmod(0o755)
        env = home.env(pgpassfile)
        env["PATH"] = f"{fake_bin}{os.pathsep}{env.get('PATH', '')}"
        self._home = home
        self._out = open(home.capture / "ui.stdout", "w+b")
        self._err = open(home.capture / "ui.stderr", "w+b")
        self._process = subprocess.Popen(
            [executable, "ui", "--host", "127.0.0.1", "--port", "0"],
            stdin=subprocess.DEVNULL, stdout=self._out, stderr=self._err, env=env, cwd=home.cwd, **extra,
        )
        home.ui_servers.append(self)
        self.port, self.token = self._wait_for_link()

    def _text(self, handle):
        handle.flush()
        return _read(handle)

    def _wait_for_link(self):
        deadline = time.monotonic() + UI_TIMEOUT
        while time.monotonic() < deadline:
            match = self.LINK.search(self._text(self._out))
            if match:
                return int(match.group(1)), match.group(2)
            if self._process.poll() is not None:
                break
            time.sleep(0.1)
        self.kill()
        pytest.fail(
            f"pg0 ui did not print its link within {UI_TIMEOUT} seconds\n"
            f"stdout:\n{self._text(self._out)}\nstderr:\n{self._text(self._err)}"
        )

    @property
    def pid(self):
        return self._process.pid

    def api(self, body):
        """POST a JSON body to /api and return the status code and the response body."""
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=UI_TIMEOUT)
        try:
            connection.request(
                "POST", "/api", body=body,
                headers={"x-pg0-token": self.token, "Content-Type": "application/json"},
            )
            response = connection.getresponse()
            return response.status, response.read().decode("utf-8", "replace")
        finally:
            connection.close()

    def stdout_text(self):
        return self._text(self._out)

    def stderr_text(self):
        return self._text(self._err)

    def stop(self):
        """Send Ctrl-C (Ctrl-Break on Windows), wait for the shutdown, and return what pg0 ui printed."""
        sent = signal.CTRL_BREAK_EVENT if sys.platform == "win32" else signal.SIGINT
        self._process.send_signal(sent)
        try:
            self._process.wait(timeout=UI_TIMEOUT)
        except subprocess.TimeoutExpired:
            self.kill()
            pytest.fail(f"pg0 ui did not stop within {UI_TIMEOUT} seconds of {sent!r}")
        return self.stdout_text(), self.stderr_text()

    def kill(self):
        if self._process.poll() is None:
            self._process.kill()
            self._process.wait()
