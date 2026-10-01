"""Independent Windows supervisor acceptance; control only, no HTTP or takeover.

Run only after the candidate's successful local check is approved for runtime:
  python tool/m03_supervisor_008_runtime.py --check CHECK --out FRESH_OUT
Each attempt requires a new output path. No process discovery or PID-tree cleanup.
"""
from pathlib import Path
import argparse
import ctypes
import hashlib
import json
import msvcrt
import os
import shutil
import sqlite3
import subprocess
import sys
import threading
import time
import traceback

ROOT = Path(__file__).resolve().parents[1]
SLOT = "supervisor008"
CASES = (
    "normal-close", "expiry-normal", "expiry-no-close", "controller-eof",
    "watch-host-death", "watch-ui-death", "heartbeat-timeout", "descendant-stdio",
    "supervisor-crash", "schema4-supervisor-rejected", "schema5-oldhost-rejected",
    "proof-tamper", "recovery-write-failure", "control-output-blocked", "queued-input-eof",
    "concurrent-owner-refusal",
)
BINARIES = ("morrow-native-supervisor.exe", "morrow-native-close-peer.exe",
            "morrow-native-stream-host.exe")
SOURCE_ROOTS = ("native_session_stream_001", "network_node_stream_001", "native_pipe_win_001",
                "contracts/experimental/agent_host_v3_http_stream", "core", "sdk/rust/contracts")
SOURCE_SUFFIXES = {".rs", ".toml", ".lock", ".proto", ".capnp", ".h", ".md"}
REQUIRED_SOURCES = (
    "native_session_stream_001/src/main.rs", "native_session_stream_001/src/authority.rs",
    "native_session_stream_001/src/controller_watch.rs", "native_session_stream_001/src/supervisor.rs",
    "native_session_stream_001/src/bin/morrow-native-supervisor.rs",
    "native_session_stream_001/src/bin/morrow-native-close-peer.rs", "native_pipe_win_001/src/job.rs",
)


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def dump(path, value):
    Path(path).write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def load(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def source_inventory(root):
    result = {}
    for name in SOURCE_ROOTS:
        for directory, folders, files in os.walk(root / name):
            folders[:] = [f for f in folders if f not in {"target", ".git", "build"}]
            for file in files:
                path = Path(directory) / file
                if path.suffix in SOURCE_SUFFIXES:
                    result[path.relative_to(root).as_posix()] = sha(path)
    return dict(sorted(result.items()))


def snapshot_candidate(root, check, out, overall_end=None):
    receipt = load(check / "receipt.json")
    require(receipt.get("source_unchanged") is True, "check source_unchanged is not true")
    commands = receipt.get("commands")
    require(isinstance(commands, list) and commands, "check commands missing")
    require(all(c.get("exit_code") == 0 for c in commands), "check command failed")
    sources = receipt.get("sources_before")
    require(isinstance(sources, dict) and sources, "check source hashes missing")
    require(receipt.get("sources_after") == sources, "check before/after source hashes differ")
    require(all(p in sources for p in REQUIRED_SOURCES), "check omits supervisor source inputs")
    require(source_inventory(root) == sources, "current source set/hashes differ from checked sources")
    executable_pins = checked_binary_pins(check, commands)
    require(out != check and check not in out.parents and out not in check.parents,
            "output and check paths must be independent")
    out.mkdir(parents=True, exist_ok=False)
    candidate = out / "candidate"
    candidate.mkdir()
    frozen = {}

    def copy(src, relative, expected=None):
        if overall_end is not None:
            require(time.monotonic() < overall_end, "overall deadline during candidate snapshot")
        if expected is not None:
            require(sha(src) == expected, "snapshot input no longer matches checked hash: " + str(src))
        destination = candidate / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(src, destination)
        require(sha(src) == sha(destination), "snapshot copy changed: " + str(src))
        if expected is not None:
            require(sha(destination) == expected, "snapshot output no longer matches checked hash: " + str(src))
        frozen[destination.relative_to(candidate).as_posix()] = sha(destination)

    for rel, expected in sources.items():
        source = (root / rel).resolve()
        source.relative_to(root)
        require(sha(source) == expected, "source drift: " + rel)
        copy(source, Path("source") / rel, expected)
    copy(check / "receipt.json", Path("check/receipt.json"))
    for command in commands:
        label = command.get("label", "")
        require(label and Path(label).name == label, "invalid check command label")
        log = check / (label + ".log")
        require(sha(log) == command.get("log_sha256"), "check log hash mismatch: " + label)
        copy(log, Path("check") / log.name, command["log_sha256"])
    for name in BINARIES:
        copy(check / "default-build" / name, Path(name), executable_pins["default-build/" + name])
    for command in commands:
        if command.get("label") == "default-build" and command.get("executable"):
            binary = (check / command["executable"]).resolve()
            binary.relative_to(check)
            require(sha(binary) == command.get("executable_sha256"), "checked executable hash mismatch")
    copy(Path(__file__), Path("runner.py"))
    manifest = {"version": 1, "sources": sources, "frozen_files": frozen,
                "runner_sha256": sha(Path(__file__)), "check_receipt_sha256": sha(check / "receipt.json"),
                "executables": {name: frozen[name] for name in BINARIES},
                "default_build_binary_pins": executable_pins,
                "scope": "real Windows control-only supervisor; no HTTP; no business replay"}
    dump(candidate / "manifest.json", manifest)
    manifest["manifest_sha256"] = sha(candidate / "manifest.json")
    return candidate, manifest


def checked_binary_pins(check, commands):
    check = check.resolve()
    builds = [command for command in commands if command.get("label") == "default-build"]
    require(len(builds) == 1 and builds[0].get("exit_code") == 0,
            "exactly one successful default-build command required")
    pins = builds[0].get("binaries")
    require(isinstance(pins, dict) and pins, "default-build binaries hash evidence missing; weak checks rejected")
    for name in BINARIES:
        relative = "default-build/" + name
        expected = pins.get(relative)
        require(isinstance(expected, str) and len(expected) == 64 and
                all(c in "0123456789abcdef" for c in expected), "checked binary SHA missing/invalid: " + relative)
        path = (check / relative).resolve()
        path.relative_to(check)
        require(path.is_file() and sha(path) == expected, "checked binary SHA mismatch: " + relative)
    # Also validate any additional copied default-build binary evidence supplied by the check.
    for relative, expected in pins.items():
        require(isinstance(relative, str) and relative.startswith("default-build/") and
                Path(relative).as_posix() == relative and ".." not in Path(relative).parts and
                Path(relative).suffix == ".exe", "invalid default-build binary evidence path")
        require(isinstance(expected, str) and len(expected) == 64 and
                all(c in "0123456789abcdef" for c in expected), "invalid default-build binary evidence hash")
        path = (check / relative).resolve()
        path.relative_to(check)
        require(path.is_file() and sha(path) == expected, "checked binary SHA mismatch: " + relative)
    return dict(pins)


def kernel():
    require(os.name == "nt", "Windows execution required")
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    api.OpenProcess.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32]
    api.OpenProcess.restype = ctypes.c_void_p
    api.GetProcessId.argtypes = [ctypes.c_void_p]
    api.GetProcessId.restype = ctypes.c_uint32
    api.GetExitCodeProcess.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32)]
    api.GetExitCodeProcess.restype = ctypes.c_int
    api.TerminateProcess.argtypes = [ctypes.c_void_p, ctypes.c_uint32]
    api.TerminateProcess.restype = ctypes.c_int
    api.WaitForSingleObject.argtypes = [ctypes.c_void_p, ctypes.c_uint32]
    api.WaitForSingleObject.restype = ctypes.c_uint32
    api.CloseHandle.argtypes = [ctypes.c_void_p]
    api.CloseHandle.restype = ctypes.c_int
    api.PeekNamedPipe.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint32,
                                 ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32), ctypes.c_void_p]
    api.PeekNamedPipe.restype = ctypes.c_int
    return api


class OwnedProcess:
    """A retained kernel process object, opened only for our Popen/trusted fixture."""
    def __init__(self, pid, provenance):
        require(isinstance(pid, int) and not isinstance(pid, bool) and pid > 0, "invalid owned PID")
        self.pid, self.provenance, self.api = pid, provenance, kernel()
        self.h = self.api.OpenProcess(0x100000 | 0x1000 | 1, False, pid)
        require(self.h and self.api.GetProcessId(self.h) == pid,
                "cannot retain exact owned process handle: " + str(pid))
        self.cleanup_terminated = False

    def wait(self, milliseconds):
        require(self.h is not None, "closed process handle")
        result = self.api.WaitForSingleObject(self.h, max(0, int(milliseconds)))
        require(result in (0, 258), "WaitForSingleObject failed")
        return result == 0

    def alive(self):
        return not self.wait(0)

    def evidence(self):
        value = ctypes.c_uint32()
        require(self.api.GetExitCodeProcess(self.h, ctypes.byref(value)), "GetExitCodeProcess failed")
        return {"pid": self.pid, "provenance": self.provenance,
                "retained_handle_pid": self.api.GetProcessId(self.h),
                "wait_signaled": self.wait(0), "exit_code": value.value,
                "cleanup_terminated": self.cleanup_terminated}

    def terminate(self, code=71):
        if self.alive():
            require(self.api.GetProcessId(self.h) == self.pid, "owned handle identity drift")
            require(self.api.TerminateProcess(self.h, code), "owned process termination failed")

    def cleanup(self):
        if self.alive():
            self.cleanup_terminated = True
            self.terminate()
        require(self.wait(5000), "owned process cleanup exit unconfirmed")

    def close(self):
        if self.h:
            require(self.api.CloseHandle(self.h), "process handle close failed")
            self.h = None


class Deadline:
    def __init__(self, seconds, overall):
        self.end = min(time.monotonic() + seconds, overall)

    def remaining(self, cap=None):
        value = self.end - time.monotonic()
        if value <= 0:
            raise TimeoutError("case/overall deadline reached")
        return min(value, cap) if cap is not None else value


class Supervisor:
    def __init__(self, context, label, mode, watch, ttl_ms=8000, timeout_ms=1000):
        self.context, self.deadline = context, context.deadline
        self.folder = context.case / label
        self.folder.mkdir()
        self.cwd = context.fresh_cwd(label)
        self.peer_file = self.folder / "peer.json"
        self.rows, self.reader_errors = [], []
        self.condition, self.write_lock = threading.Condition(), threading.Lock()
        self.eof = False
        self.pause_requested, self.reader_paused, self.reader_resume = threading.Event(), threading.Event(), threading.Event()
        self.heartbeat_stop, self.heartbeat_thread = threading.Event(), None
        self.heartbeat_sent, self.heartbeat_error = 0, None
        args = [str(context.supervisor), "serve", "--profile", str(context.profile),
                "--slot", SLOT, "--client", str(context.peer), "--sha256", sha(context.peer),
                "--work-dir", str(self.cwd), "--plugin-id", "synthetic008", "--role", "qualification",
                "--operation", "supervisor008", "--http-origin", "http://127.0.0.1:1",
                "--ttl-ms", str(ttl_ms), "--close-ms", "300", "--controller-timeout-ms", str(timeout_ms),
                "--client-arg", mode, "--client-arg", str(self.peer_file)]
        require(1 <= len(watch) <= 2, "one/two held watch processes required")
        for process in watch:
            require(process.alive(), "watch process already exited")
            args += ["--watch-pid", str(process.pid)]
        dump(self.folder / "launch.json", {"args": args, "environment": context.env,
                                           "watch_handles": [p.evidence() for p in watch],
                                           "cwd_empty_at_launch": not any(self.cwd.iterdir())})
        self.raw = (self.folder / "stdout.jsonl").open("xb")
        self.commands_raw = (self.folder / "stdin.jsonl").open("xb")
        self.errors = (self.folder / "stderr.txt").open("xb")
        self.p = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.errors,
                                  env=context.env, cwd=self.cwd, creationflags=0x08000000)
        context.supervisors.append(self)
        self.owned = context.retain(self.p.pid, "runner Popen supervisor " + label)

        def read():
            try:
                for line in self.p.stdout:
                    if self.pause_requested.is_set():
                        self.reader_paused.set()
                        self.reader_resume.wait()
                    self.raw.write(line)
                    self.raw.flush()
                    row = json.loads(line)
                    require(isinstance(row, dict), "supervisor row is not object")
                    with self.condition:
                        self.rows.append(row)
                        self.condition.notify_all()
            except Exception as error:
                with self.condition:
                    self.reader_errors.append(repr(error))
            finally:
                with self.condition:
                    self.eof = True
                    self.condition.notify_all()
        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()
        proposal = self.wait(lambda r: r.get("event") == "proposal")
        require(proposal.get("host_pid") == self.p.pid, "proposal host identity mismatch")
        self.start_heartbeats()

    def wait(self, predicate, start=0):
        with self.condition:
            cursor = start
            while True:
                for row in self.rows[cursor:]:
                    if predicate(row):
                        return row
                cursor = len(self.rows)
                require(not self.reader_errors, "supervisor reader error: " + repr(self.reader_errors))
                require(not self.eof, "supervisor EOF before required observation")
                self.condition.wait(self.deadline.remaining(.2))

    def send(self, action, **fields):
        with self.write_lock:
            require(not self.p.stdin.closed, "trusted stdin closed")
            data = (json.dumps({"action": action, **fields}) + "\n").encode()
            require(len(data) <= 1024, "trusted command too large")
            self.p.stdin.write(data)
            self.p.stdin.flush()
            self.commands_raw.write((json.dumps({"at_monotonic": time.monotonic(),
                                                 "command": {"action": action, **fields}}) + "\n").encode())
            self.commands_raw.flush()

    def cmd(self, action, expected=True, **fields):
        with self.condition:
            start = len(self.rows)
        self.send(action, **fields)
        row = self.wait(lambda r: r.get("event") == "operator_result" and r.get("action") == action, start)
        require(row.get("ok") is expected, "unexpected operator result: " + repr(row))
        return row["result"] if expected else row

    def start_heartbeats(self):
        require(self.heartbeat_thread is None, "duplicate heartbeat thread")

        def pulse():
            while not self.heartbeat_stop.is_set() and self.p.poll() is None:
                try:
                    self.send("heartbeat")
                    self.heartbeat_sent += 1
                except (BrokenPipeError, OSError, AssertionError) as error:
                    self.heartbeat_error = repr(error)
                    break
                self.heartbeat_stop.wait(.08)
        self.heartbeat_thread = threading.Thread(target=pulse, daemon=True)
        self.heartbeat_thread.start()

    def stop_heartbeats(self):
        self.heartbeat_stop.set()
        if self.heartbeat_thread:
            self.heartbeat_thread.join(2)
            require(not self.heartbeat_thread.is_alive(), "heartbeat thread failed to join")

    def block_control_output(self):
        self.pause_requested.set()
        self.send("inspect")
        require(self.reader_paused.wait(self.deadline.remaining(2)), "stdout capture did not pause")
        # Bounded input fits the anonymous stdin buffer; inspection output exceeds stdout capacity.
        # Stay below the native controller's eight-command queue capacity until watch death.
        for _ in range(5):
            self.send("inspect")
        api = kernel()
        handle = msvcrt.get_osfhandle(self.p.stdout.fileno())
        available = ctypes.c_uint32()
        while True:
            require(api.PeekNamedPipe(handle, None, 0, None, ctypes.byref(available), None),
                    "cannot observe our anonymous stdout backlog")
            if available.value >= 4096:
                break
            self.deadline.remaining()
            time.sleep(.01)
        dump(self.folder / "stdout-blocked.json", {"reader_paused": True,
                                                   "kernel_pipe_available_bytes": available.value,
                                                   "at_monotonic": time.monotonic(),
                                                   "bounded_inspect_input_lines": 6})

    def resume_output(self):
        self.pause_requested.clear()
        self.reader_resume.set()

    def close_stdin(self):
        self.stop_heartbeats()
        with self.write_lock:
            try:
                self.p.stdin.close()
            except (BrokenPipeError, OSError) as error:
                # A saturated native queue can terminate before BufferedWriter's final flush.
                dump(self.folder / "stdin-close.json", {"error": repr(error),
                                                         "closed": self.p.stdin.closed})
                require(self.p.stdin.closed, "trusted input did not close after native disconnect")

    def finish(self):
        rc = self.p.wait(timeout=self.deadline.remaining(8))
        self.exited_at = time.monotonic()
        self.stop_heartbeats()
        self.reader.join(self.deadline.remaining(3))
        require(not self.reader.is_alive() and not self.reader_errors, "raw stdout reader join failed")
        dump(self.folder / "rows.json", self.rows)
        dump(self.folder / "process.json", {"exit_code": rc, "process": self.owned.evidence(),
                                            "heartbeat_sent": self.heartbeat_sent,
                                            "heartbeat_error": self.heartbeat_error})
        return rc

    def observations(self):
        return [r["observation"] for r in self.rows if r.get("event") == "host_observation"]

    def close_files(self):
        self.resume_output()
        self.stop_heartbeats()
        if self.p.poll() is None:
            self.owned.terminate()
        self.p.wait(timeout=5)
        self.reader.join(3)
        require(not self.reader.is_alive(), "cleanup reader join failed")
        for stream in (self.p.stdin, self.p.stdout, self.errors, self.raw, self.commands_raw):
            if not stream.closed:
                stream.close()
        dump(self.folder / "rows.json", self.rows)


def sqlite_json(value):
    if isinstance(value, bytes):
        return {"type": "blob", "bytes": len(value), "hex": value.hex(),
                "sha256": hashlib.sha256(value).hexdigest()}
    return value


def ledger_capture(profile, folder, label):
    """Consistent read-only backup plus exact raw rows; no ledger normalization."""
    evidence = {}
    backup_end = time.monotonic() + 5

    def progress(status, remaining, total):
        if time.monotonic() > backup_end:
            raise TimeoutError("bounded SQLite evidence backup timed out")
    for name in ("native-admissions.sqlite", "coordination.sqlite"):
        path = profile / name
        if not path.exists():
            continue
        with sqlite3.connect(path.as_uri() + "?mode=ro", uri=True, timeout=.5) as db:
            tables = db.execute("SELECT name,sql FROM sqlite_master WHERE type='table' ORDER BY name").fetchall()
            rows = {}
            for table, sql in tables:
                quoted = '"' + table.replace('"', '""') + '"'
                cursor = db.execute("SELECT * FROM " + quoted)
                rows[table] = {"sql": sql, "columns": [item[0] for item in cursor.description],
                               "rows": [[sqlite_json(v) for v in row] for row in cursor.fetchall()]}
            evidence[name] = {"user_version": db.execute("PRAGMA user_version").fetchone()[0],
                              "tables": rows,
                              "triggers": db.execute("SELECT name,sql FROM sqlite_master WHERE type='trigger' ORDER BY name").fetchall()}
            backup = folder / (label + "-" + name)
            with sqlite3.connect(backup) as destination:
                db.backup(destination, pages=64, progress=progress, sleep=.02)
    dump(folder / (label + "-raw-rows.json"), evidence)
    return evidence


def payload(raw, table):
    rows = raw["native-admissions.sqlite"]["tables"].get(table, {}).get("rows", [])
    require(len(rows) == 1, "expected one raw " + table + " row")
    return rows[0][-1]["hex"]


def unpack_record(blob_hex):
    """Bounded native Protobuf/LZ4 envelope decoder for independent ledger evidence."""
    blob = bytes.fromhex(blob_hex)
    require(50 <= len(blob) <= 8352 and blob[:8] == b"MRNADM04" and blob[8:10] == b"\x01\x00",
            "raw record envelope invalid")
    size = int.from_bytes(blob[10:14], "little")
    require(size <= 8192 and int.from_bytes(blob[14:18], "little") == len(blob) - 50,
            "raw record size invalid")
    compressed, decoded, cursor = blob[50:], bytearray(), 0

    def extended(value):
        nonlocal cursor
        if value == 15:
            while True:
                require(cursor < len(compressed), "LZ4 extension truncated")
                addition = compressed[cursor]
                cursor += 1
                value += addition
                if addition != 255:
                    break
        return value

    while cursor < len(compressed):
        token = compressed[cursor]
        cursor += 1
        literals = extended(token >> 4)
        require(cursor + literals <= len(compressed) and len(decoded) + literals <= size,
                "LZ4 literals out of bounds")
        decoded.extend(compressed[cursor:cursor + literals])
        cursor += literals
        if cursor == len(compressed):
            break
        require(cursor + 2 <= len(compressed), "LZ4 offset truncated")
        distance = int.from_bytes(compressed[cursor:cursor + 2], "little")
        cursor += 2
        count = extended(token & 15) + 4
        require(0 < distance <= len(decoded) and len(decoded) + count <= size, "LZ4 match out of bounds")
        for _ in range(count):
            decoded.append(decoded[-distance])
    raw = bytes(decoded)
    require(len(raw) == size and hashlib.sha256(raw).digest() == blob[18:50], "raw record integrity invalid")
    fields, cursor = {}, 0

    def varint():
        nonlocal cursor
        value = 0
        for shift in range(0, 70, 7):
            require(cursor < len(raw), "Protobuf varint truncated")
            byte = raw[cursor]
            cursor += 1
            value |= (byte & 127) << shift
            if byte < 128:
                return value
        raise AssertionError("Protobuf varint oversized")

    while cursor < len(raw):
        tag = varint()
        field, wire = tag >> 3, tag & 7
        require(field > 0 and field not in fields, "Protobuf field invalid/duplicated")
        if wire == 0:
            fields[field] = varint()
        elif wire == 2:
            length = varint()
            require(cursor + length <= len(raw), "Protobuf bytes truncated")
            fields[field] = raw[cursor:cursor + length]
            cursor += length
        else:
            raise AssertionError("unexpected native Protobuf wire type")
    return fields


def assert_raw_reclaimed(raw, final, grant, peer_sha):
    owner = unpack_record(payload(raw, "owner"))
    recovery = unpack_record(payload(raw, "recovery"))
    approvals = raw["native-admissions.sqlite"]["tables"]["approvals"]["rows"]
    matches = [row for row in approvals if row[1] == grant]
    require(len(matches) == 1, "raw approval binding missing")
    approval = unpack_record(matches[0][-1]["hex"])
    require(owner[2] == recovery[3] == approval[2] == grant.encode(), "raw grant binding mismatch")
    require(owner[3] == recovery[4] == approval[3], "raw issuer binding mismatch")
    require(owner[4] == recovery[5] == approval[12], "raw generation binding mismatch")
    require(owner[5] == b"Released" and recovery[8] == b"Reclaimed", "raw release phases missing")
    require(owner[6] == recovery[9] and owner[7] == recovery[10] and owner[8] == recovery[11],
            "raw process binding mismatch")
    require(all(owner.get(i, 0) == 1 for i in (9, 10, 11)), "raw owner exit flags missing")
    require(all(recovery.get(i, 0) == 1 for i in range(15, 22)), "raw Recovery resource flags missing")
    require(owner[12] == recovery[14] == bytes.fromhex(final["state"]["recovery"]["snapshot_sha256"]),
            "raw snapshot digest binding mismatch")
    require(recovery[12] == approval[9] == bytes.fromhex(peer_sha), "raw artifact binding mismatch")
    require(recovery[13] == approval[10], "raw config binding mismatch")
    require(recovery[6].decode() == final["state"]["recovery"]["incarnation"], "raw incarnation mismatch")


class Context:
    def __init__(self, case, candidate, env, deadline):
        self.case, self.candidate, self.env, self.deadline = case, candidate, env, deadline
        self.profile = case / "profile"
        self.profile.mkdir()
        self.supervisor, self.peer, self.oldhost = (candidate / n for n in BINARIES)
        self.owned, self.supervisors, self.helpers = [], [], []
        self.additional_contexts = []
        self.result = {"case": case.name, "passed": False, "http_requests": 0,
                       "http_scope": "fixture emits no HTTP requests; endpoint 127.0.0.1:1; no HTTP approval"}
        self.cwd_root = case.parent / "case-cwds" / case.name
        self.cwd_root.mkdir(parents=True)

    def fresh_cwd(self, label):
        path = self.cwd_root / label
        path.mkdir()
        require(not any(path.iterdir()), "fresh cwd is not empty")
        return path

    def retain(self, pid, provenance):
        process = OwnedProcess(pid, provenance)
        self.owned.append(process)
        return process

    def helper(self, label):
        folder = self.case / (label + "-helper")
        folder.mkdir()
        args = [sys.executable, "-I", "-c", "import time; time.sleep(60)"]
        process = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                   stderr=subprocess.DEVNULL, cwd=self.fresh_cwd(label + "-helper"),
                                   env=self.env, creationflags=0x08000000)
        self.helpers.append(process)
        retained = self.retain(process.pid, "runner Popen " + label + " controller helper")
        dump(folder / "process.json", {"args": args, "process": retained.evidence()})
        return retained

    def cli(self, executable, mode, label, expected=0, reason=None):
        args = [str(executable), mode, "--profile", str(self.profile), "--slot", SLOT]
        if executable == self.supervisor:
            args += ["--controller-timeout-ms", "1000"]
        cwd = self.fresh_cwd(label + "-cli")
        dump(self.case / (label + "-launch.json"), {"args": args, "environment": self.env,
                                                   "cwd": str(cwd), "cwd_empty_at_launch": True})
        process = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, cwd=cwd, env=self.env, creationflags=0x08000000)
        # These finite read/init processes are our own Popen; no guest can be launched in these modes.
        try:
            stdout, stderr = process.communicate(timeout=self.deadline.remaining(8))
        except BaseException:
            process.kill()
            process.communicate(timeout=5)
            raise
        (self.case / (label + ".stdout")).write_bytes(stdout)
        (self.case / (label + ".stderr")).write_bytes(stderr)
        dump(self.case / (label + "-exit.json"), {"pid": process.pid, "exit_code": process.returncode})
        require(process.returncode == expected, "unexpected " + label + " exit: " + str(process.returncode))
        if reason:
            require(reason in stderr.decode("utf-8", "replace"), "missing concrete refusal reason: " + label)
        return [json.loads(line) for line in stdout.splitlines()]

    def init(self, old=False):
        self.cli(self.oldhost if old else self.supervisor, "init", "init")

    def inspect(self, label):
        rows = self.cli(self.supervisor, "inspect", label)
        require(len(rows) == 1 and rows[0].get("event") == "inspection", "inspect response missing")
        state = rows[0]["state"]
        dump(self.case / (label + ".json"), state)
        return state

    def cleanup(self):
        errors = []
        for context in self.additional_contexts:
            try:
                context.cleanup()
                ledger_capture(context.profile, context.case, "final-evidence")
                dump(context.case / "result.json", context.result)
                self.result.setdefault("additional_owner_contexts", []).append(context.result)
                if context.result.get("cleanup_errors") or context.result.get("guest_fallback_cleanup_used"):
                    self.result["passed"] = False
                    errors.append("additional owner cleanup did not complete safely")
            except Exception as error:
                errors.append(repr(error))
        # Close only our own supervisor first; its Job must close before any guest fallback cleanup.
        for supervisor in self.supervisors:
            try:
                supervisor.close_files()
            except Exception as error:
                errors.append(repr(error))
        for process in self.owned:
            try:
                process.cleanup()
                evidence = process.evidence()
                self.result.setdefault("owned_process_cleanup", []).append(evidence)
                if process.cleanup_terminated and "fixture" in process.provenance:
                    self.result["guest_fallback_cleanup_used"] = True
                    self.result["passed"] = False
                process.close()
            except Exception as error:
                errors.append(repr(error))
        for helper in self.helpers:
            try:
                helper.wait(timeout=5)
            except Exception as error:
                errors.append(repr(error))
        if errors:
            self.result["cleanup_errors"] = errors
            self.result["passed"] = False


def events(snapshot, name):
    return [e for e in snapshot["events"] if e.get("event") == name]


def assert_no_http(snapshot):
    progress = snapshot["http"]["progress"]
    require(progress.get("worker_started") is False, "unexpected HTTP worker start")
    require(progress.get("received_offset") == 0 and progress.get("issued_offset") == 0,
            "unexpected HTTP bytes")
    require(not any(e.get("event") in {"http_proposal", "http_open_started", "network_worker_joined"}
                    for e in snapshot["events"]), "unexpected HTTP/network operation")


def assert_expiry_no_close_receipt(final, claim, receipt):
    """A killed fixture's Stop receipt proves the intended negative was exercised."""
    snapshot = final["snapshot"]
    require(receipt.get("mode") == "expiry-no-close" and
            receipt.get("phase") == "stop-observed-no-close", "no-close Stop receipt mode/phase mismatch")
    for field in ("child_pid", "session", "epoch"):
        expected = claim["pid"] if field == "child_pid" else claim[field]
        require(type(receipt.get(field)) is int and receipt[field] == expected,
                "no-close Stop receipt " + field + " binding mismatch")
    require(receipt.get("stop_received") is True and type(receipt.get("stop_code")) is int and
            receipt["stop_code"] == 20 and receipt.get("no_close_submitted") is True and
            receipt.get("http_not_invoked") is True and receipt.get("ack_received") is False and
            receipt.get("terminal_protocol_complete") is False, "no-close Stop receipt terminal facts invalid")
    raw_hex = receipt.get("stop_raw_hex")
    require(isinstance(raw_hex, str) and 24 <= len(raw_hex) <= 65544 and len(raw_hex) % 2 == 0 and
            all(c in "0123456789abcdef" for c in raw_hex), "no-close Stop raw hex invalid")
    frame = bytes.fromhex(raw_hex)
    length = int.from_bytes(frame[:4], "little")
    require(8 <= length <= 32768 and length % 8 == 0 and len(frame) == length + 4,
            "no-close Stop raw frame length invalid")
    sent = events(snapshot, "control_frame_sent")
    require(sum(e.get("detail", {}).get("raw_hex") == raw_hex for e in sent) == 1,
            "fixture Stop receipt does not match actual host-sent raw frame")
    received = events(snapshot, "control_frame_received")
    require(len(received) == 1 and received[0].get("detail", {}).get("raw_hex"),
            "no-close probe submitted extra guest control frames after initial Hello")
    for event in ("expiry_teardown_close_received", "expiry_teardown_ack_written", "close_ack_written", "close_ack_pending"):
        require(not events(snapshot, event), "no-close probe observed Close/ACK: " + event)
    reasons = events(snapshot, "session_close_reason")
    require(len(reasons) == 1 and reasons[0]["detail"].get("reason") == 20 and
            snapshot["http"]["progress"].get("error_code") == 20, "no-close probe lacks original expiry reason20")
    require(final.get("expired_terminal_protocol_complete") is False and final.get("business_success_claimed") is False,
            "no-close negative claims terminal/business success")
    require(any(e.get("detail", {}).get("reason") == "fixed teardown deadline"
                for e in events(snapshot, "close_ack_failed")), "no-close negative lacks fixed teardown failure")
    require(any(e.get("detail", {}).get("reason") == 20 for e in events(snapshot, "job_terminate_requested")) and
            any(e.get("detail", {}).get("ok") is True for e in events(snapshot, "kill_requested")),
            "no-close negative lacks actual successful Job termination")
    require(snapshot.get("exit_code") == 1 and snapshot.get("exit_observed") is True and
            snapshot.get("stdout_eof") is True and snapshot.get("stderr_eof") is True and
            any(e.get("detail", {}).get("code") == 1 for e in events(snapshot, "exit")),
            "no-close negative lacks real killed child exit1/stdio EOF")
    require(any(e.get("detail", {}).get("active_processes") == 0 and
                all(e.get("detail", {}).get(flag) is True for flag in ("direct_exit", "stdout_eof", "stderr_eof"))
                for e in events(snapshot, "job_empty")), "no-close negative lacks actual Job empty barrier")
    assert_no_http(snapshot)


def killed_fixture_diagnostic(path):
    """Preserve an unfinished post-kill diagnostic; reject every parseable contradiction."""
    info = {"present": path.exists(), "complete_json": False, "final_fixture_result_completed": False}
    if not path.exists():
        info["reason"] = "Job-terminated fixture did not create its final diagnostic"
        return None, info
    raw = path.read_bytes()
    preserved = path.with_name(path.name + ".final-raw.bin")
    preserved.write_bytes(raw)
    info.update(raw_bytes=len(raw), raw_sha256=hashlib.sha256(raw).hexdigest(),
                preserved_raw_file=preserved.name)
    try:
        fixture = json.loads(raw.decode("utf-8-sig"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        info.update(reason="Job-terminated fixture left an empty or truncated final diagnostic",
                    parse_error=repr(error))
        return None, info
    require(isinstance(fixture, dict), "parseable no-close final fixture is not an object")
    require(fixture.get("mode") == "expiry-no-close" and fixture.get("http_not_invoked") is True and
            fixture.get("ack_received") is False and fixture.get("stop_received") is True,
            "parseable no-close final fixture contradicts synchronous Stop receipt")
    if "terminal_protocol_complete" in fixture:
        require(fixture["terminal_protocol_complete"] is False, "parseable no-close final fixture claims terminal success")
    info.update(complete_json=True, final_fixture_result_completed=True)
    return fixture, info


def assert_reclaimed(final, grant, claim, supervisor_pid, expected_incarnation=None):
    """Check runtime resources and durable proof together, never infer from exit 2."""
    snapshot, state = final["snapshot"], final["state"]
    owner, recovery = state["owner"], state["recovery"]
    require(snapshot["phase"] == "Released" and snapshot["owner_retained"] is False,
            "runtime owner release missing")
    require(owner["phase"] == "Released" and owner["owner_retained"] is False,
            "durable owner release missing")
    for record in (snapshot, owner):
        for field in ("exit_observed", "stdout_eof", "stderr_eof"):
            require(record.get(field) is True, "owner release lacks " + field)
    require(snapshot.get("event_overflow") == 0, "snapshot evidence overflow")
    progress = snapshot["http"]["progress"]
    for flag in ("revoke_persisted", "revoke_applied", "request_closed", "data_closed", "connect_reaped",
                 "read_reaped", "write_reaped", "owner_released", "child_exited", "stdout_eof", "stderr_eof"):
        require(progress.get(flag) is True, "resource proof lacks " + flag)
    if progress.get("worker_started"):
        require(progress.get("worker_joined") is True, "network worker join missing")
    require(recovery["phase"] == "Reclaimed", "durable Recovery is not Reclaimed")
    for flag in ("tree_empty", "child_exit", "stdout_eof", "stderr_eof", "pipe_joined", "network_joined", "gate_closed"):
        require(recovery.get(flag) is True, "Recovery proof lacks " + flag)
    require(recovery.get("business_replay_allowed") is False, "Recovery permits business replay")
    require(recovery["grant_id"] == owner["grant_id"] == grant, "grant binding mismatch")
    require(recovery["generation"] == owner["generation"] == claim["epoch"], "generation binding mismatch")
    require(recovery["profile"] == state["profile_id"], "profile binding mismatch")
    require(recovery["supervisor_pid"] == supervisor_pid, "supervisor PID binding mismatch")
    for field in ("session", "epoch"):
        require(recovery[field] == owner[field] == snapshot[field] == claim[field], field + " binding mismatch")
    require(recovery["child_pid"] == owner["pid"] == snapshot["pid"] == claim["pid"], "child PID binding mismatch")
    require(isinstance(recovery["incarnation"], str) and len(recovery["incarnation"]) == 64,
            "incarnation missing")
    int(recovery["incarnation"], 16)
    if expected_incarnation:
        require(recovery["incarnation"] == expected_incarnation, "incarnation changed after registration")
    digest = recovery.get("snapshot_sha256", "")
    require(len(digest) == 64 and len(bytes.fromhex(digest)) == 32, "snapshot digest missing")
    bound = events(snapshot, "job_bound")
    require(len(bound) == 1, "Job binding observation missing/duplicated")
    detail = bound[0]["detail"]
    require(detail.get("child_pid") == claim["pid"] and detail.get("incarnation") == recovery["incarnation"],
            "Job incarnation/child binding mismatch")
    require(detail.get("handle_noninherited") is True and detail.get("suspended_attach_before_execution") is True,
            "Job launch ordering/noninheritance proof missing")
    require(detail.get("atomic_job_at_creation") is True, "Job was not bound atomically at process creation")
    require(detail.get("limit_flags") == 0x2000, "Job limits include breakaway or omit kill-on-close")
    empty = events(snapshot, "job_empty")
    require(len(empty) == 1 and empty[0]["detail"].get("active_processes") == 0, "Job empty proof missing")
    require(all(empty[0]["detail"].get(k) is True for k in ("direct_exit", "stdout_eof", "stderr_eof")),
            "Job empty observed without direct/stdio exits")
    require(empty[0]["ordinal"] < next(e["ordinal"] for e in snapshot["events"]
                                      if e.get("event") == "phase" and e.get("detail") == "Released"),
            "release precedes Job empty observation")
    control_reaped = events(snapshot, "control_stdin_closed_reaped")
    require(control_reaped and all(e.get("detail") is True for e in control_reaped),
            "control stdin close/completion reap proof missing")
    require(all(e["ordinal"] < next(row["ordinal"] for row in snapshot["events"]
                                    if row.get("event") == "phase" and row.get("detail") == "Released")
                for e in control_reaped),
            "release precedes control stdin close/completion reap")
    gates = events(snapshot, "http_cancel_applied")
    require(gates and all(e["detail"].get("effect_fence", {}).get("gate_closed") is True for e in gates),
            "effect gate close evidence missing")
    deadlines = events(snapshot, "authority_deadline_bound")
    require(len(deadlines) == 1 and deadlines[0]["detail"].get("expiry_not_renewed") is True,
            "original authority deadline proof missing")
    require(final.get("business_success_claimed") is False, "cleanup claims business success")
    assert_no_http(snapshot)
    return recovery


def claim_session(context, label, mode, watch, ttl_ms=8000):
    supervisor = Supervisor(context, label, mode, watch, ttl_ms)
    grant = supervisor.cmd("approve")["grant_id"]
    claim = supervisor.cmd("claim", grant_id=grant)
    dump(supervisor.folder / "claim.json", {"grant": grant, "claim": claim})
    return supervisor, grant, claim


def fixture_handles(context, supervisor, claim):
    while not supervisor.peer_file.exists():
        context.deadline.remaining()
        time.sleep(.01)
    while True:
        try:
            fixture = load(supervisor.peer_file)
            break
        except json.JSONDecodeError:
            context.deadline.remaining()
            time.sleep(.01)
    require(fixture.get("direct_pid") == claim["pid"] and fixture.get("inherited_stdio") is True,
            "trusted fixture direct/stdio binding mismatch")
    require(fixture.get("descendant_pid") != claim["pid"], "fixture descendant equals direct PID")
    descendant = context.retain(fixture["descendant_pid"], "trusted close-peer fixture descendant")
    dump(supervisor.folder / "fixture-handle.json", descendant.evidence())
    return descendant


def registered_state(supervisor, grant, claim):
    state = supervisor.cmd("inspect", grant_id=grant)
    recovery = state["recovery"]
    require(state["owner"]["owner_retained"] is True and recovery["phase"] in {"Registered", "Retained"},
            "registered durable owner/proof missing")
    require(recovery["grant_id"] == grant and recovery["child_pid"] == claim["pid"]
            and recovery["session"] == claim["session"] and recovery["epoch"] == claim["epoch"],
            "registration binding mismatch")
    dump(supervisor.folder / "registered.json", state)
    return state


def complete_reclamation(context, supervisor, grant, claim, expected_exit, incarnation=None):
    final = supervisor.wait(lambda r: r.get("event") == "final")
    code = supervisor.finish()
    require(code == expected_exit, "unexpected supervisor final exit: " + str(code))
    recovery = assert_reclaimed(final, grant, claim, supervisor.p.pid, incarnation)
    state = context.inspect(supervisor.folder.name + "-persistent-inspect")
    owner_keys = {k for k in final["state"]["owner"] if k != "locally_observed"}
    require({k: state["owner"].get(k) for k in owner_keys} ==
            {k: final["state"]["owner"].get(k) for k in owner_keys} and state["recovery"] == recovery,
            "restart inspect disagrees with final durable reclamation")
    require(state["owner"]["locally_observed"] is False, "new inspect issuer claims old local ownership")
    raw = ledger_capture(context.profile, context.case, supervisor.folder.name + "-reclaimed")
    require(raw["native-admissions.sqlite"]["user_version"] == 5, "supervisor ledger is not schema 5")
    assert_raw_reclaimed(raw, final, grant, sha(context.peer))
    context.result.update(resource_reclamation_proven=True, ledger_reclaimed=True,
                          final_exit_code=code, recovery_binding=recovery)
    return final


def assert_retained(state, grant, claim):
    owner, recovery = state["owner"], state["recovery"]
    require(owner["owner_retained"] is True and owner["phase"] != "Released"
            and owner["automatic_takeover"] is False, "unproved durable owner released/taken over")
    require(recovery["phase"] in {"Registered", "Retained"} and recovery["grant_id"] == grant,
            "unconfirmed recovery phase/binding changed")
    require(recovery["child_pid"] == claim["pid"] and recovery["session"] == claim["session"]
            and recovery["epoch"] == claim["epoch"], "retained process binding changed")
    require(recovery["tree_empty"] is False and recovery["gate_closed"] is False,
            "kernel exit/failed write fabricated persisted reclamation flags")


def restart_refusals(context, watch, grant, claim, before, label):
    restart = Supervisor(context, label, "descendant-hold", watch)
    state = restart.cmd("inspect", grant_id=grant)
    dump(restart.folder / "before-refusals.json", state)
    assert_retained(state, grant, claim)
    require(state["owner"]["locally_observed"] is False, "restart claims local observation of old owner")
    require(state["grant"]["live_in_this_issuer"] is False, "persistent grant became live on restart")
    require(state["owner"] == before["owner"] or
            {k: v for k, v in state["owner"].items() if k != "locally_observed"} ==
            {k: v for k, v in before["owner"].items() if k != "locally_observed"}, "restart altered owner")
    require(state["recovery"] == before["recovery"], "restart altered Recovery")
    old = restart.cmd("claim", False, grant_id=grant)
    require("no live approval" in old.get("error", ""), "old grant refusal reason missing")
    new_grant = restart.cmd("approve")["grant_id"]
    refusal = restart.cmd("claim", False, grant_id=new_grant)
    require("no automatic crash takeover" in refusal.get("error", ""), "new claim takeover refusal missing")
    after = restart.cmd("inspect", grant_id=new_grant)
    require(after["owner"] == state["owner"] and after["recovery"] == state["recovery"],
            "refused claim mutated durable owner/Recovery")
    require(not restart.peer_file.exists() and not any(e.get("event") == "spawn" for e in restart.observations()),
            "refused restart claim spawned a guest")
    restart.cmd("quit")
    restart.wait(lambda r: r.get("event") == "final_without_session")
    require(restart.finish() == 2, "restart refusal exit should be 2")
    dump(restart.folder / "after-refusals.json", after)
    context.result.update(old_grant_not_live=True, old_claim_rejected=True, no_automatic_takeover=True,
                          new_claim_rejected=True, rejected_claim_no_spawn=True, persistent_owner_preserved=True)


def durable_owner_fields(state):
    return {k: v for k, v in state["owner"].items() if k != "locally_observed"}


def assert_live_owner_refusal(refusal, before, after, raw_before, raw_after, observations, peer_exists):
    error = refusal.get("error", "")
    require(refusal.get("ok") is False and
            ("StorageBusy" in error or "local owner retained" in error or
             "owner unconfirmed: no automatic crash takeover" in error),
            "same-profile live claim lacks StorageBusy/retained refusal")
    require(durable_owner_fields(before) == durable_owner_fields(after) and
            before["recovery"] == after["recovery"], "live contender altered old owner/Recovery")
    require(payload(raw_before, "owner") == payload(raw_after, "owner") and
            payload(raw_before, "recovery") == payload(raw_after, "recovery"),
            "live contender changed raw old owner/Recovery bytes")
    require(not peer_exists and not any(e.get("event") in {"spawn", "job_bound"} for e in observations),
            "live contender spawned an extra guest")


def run_concurrent_owners(context):
    context.init()
    host = context.helper("host")
    watch = [host]
    first, grant_a, claim_a = claim_session(context, "owner-a", "descendant-hold", watch, 10000)
    descendant_a = fixture_handles(context, first, claim_a)
    direct_a = context.retain(claim_a["pid"], "trusted close-peer fixture concurrent direct A")
    first.wait(lambda r: r.get("event") == "authority_observation" and
               r["observation"].get("event") == "owner_phase" and
               r["observation"].get("detail", {}).get("phase") == "Active")
    before_a = registered_state(first, grant_a, claim_a)

    second_folder = context.case / "second-owner"
    second_folder.mkdir()
    second_context = Context(second_folder, context.candidate, context.env, context.deadline)
    context.additional_contexts.append(second_context)
    second_context.init()
    second, grant_b, claim_b = claim_session(second_context, "owner-b", "descendant-hold", watch, 10000)
    descendant_b = fixture_handles(second_context, second, claim_b)
    direct_b = second_context.retain(claim_b["pid"], "trusted close-peer fixture concurrent direct B")
    second.wait(lambda r: r.get("event") == "authority_observation" and
                r["observation"].get("event") == "owner_phase" and
                r["observation"].get("detail", {}).get("phase") == "Active")
    before_b = registered_state(second, grant_b, claim_b)
    require(context.profile != second_context.profile and
            before_a["profile_id"] != before_b["profile_id"], "concurrent owners share a profile identity")
    require(first.p.pid != second.p.pid and claim_a["pid"] != claim_b["pid"], "concurrent process identity reused")
    active_handles = (first.owned, direct_a, descendant_a, second.owned, direct_b, descendant_b)
    require(all(p.alive() for p in active_handles), "two supervisor/guest trees did not actually overlap")
    dump(context.case / "actual-overlap.json", {"at_monotonic": time.monotonic(),
                                               "profile_a": str(context.profile),
                                               "profile_b": str(second_context.profile),
                                               "active_owner_count": 2,
                                               "handles": [p.evidence() for p in active_handles]})
    raw_before = ledger_capture(context.profile, context.case, "live-owner-before-contender")
    ledger_capture(second_context.profile, second_context.case, "live-second-owner")
    # The challenger serves the first profile but has no Session/Job/guest authority.
    challenger = Supervisor(context, "same-profile-contender", "descendant-hold", watch, 10000)
    contender_grant = challenger.cmd("approve")["grant_id"]
    refusal = challenger.cmd("claim", False, grant_id=contender_grant)
    after = challenger.cmd("inspect", grant_id=contender_grant)
    raw_after = ledger_capture(context.profile, context.case, "live-owner-after-contender")
    assert_live_owner_refusal(refusal, before_a, after, raw_before, raw_after,
                             challenger.observations(), challenger.peer_file.exists())
    require(all(p.alive() for p in active_handles), "contender interfered with two live owner trees")
    dump(challenger.folder / "refusal-evidence.json", {"refusal": refusal, "state": after,
                                                     "existing_owner_handles": [p.evidence() for p in active_handles],
                                                     "extra_guest_started": False})
    challenger.cmd("quit")
    challenger.wait(lambda r: r.get("event") == "final_without_session")
    require(challenger.finish() == 2, "live same-profile contender did not exit 2")

    first.cmd("stop")
    final_a = complete_reclamation(context, first, grant_a, claim_a, 0, before_a["recovery"]["incarnation"])
    require(direct_a.wait(int(context.deadline.remaining(3) * 1000)) and
            descendant_a.wait(int(context.deadline.remaining(3) * 1000)), "first concurrent owner tree exit unconfirmed")
    require(second.owned.alive() and direct_b.alive() and descendant_b.alive(),
            "stopping first profile affected second live owner")
    second.cmd("stop")
    final_b = complete_reclamation(second_context, second, grant_b, claim_b, 0, before_b["recovery"]["incarnation"])
    require(direct_b.wait(int(context.deadline.remaining(3) * 1000)) and
            descendant_b.wait(int(context.deadline.remaining(3) * 1000)), "second concurrent owner tree exit unconfirmed")
    require(all(p.wait(0) and not p.cleanup_terminated for p in active_handles),
            "concurrent owner tree proof depended on test cleanup")
    require(events(final_a["snapshot"], "job_terminate_requested") and
            events(final_b["snapshot"], "job_terminate_requested"), "concurrent descendants were not Job-terminated")
    dump(context.case / "all-concurrent-tree-exits-before-cleanup.json",
         {"handles": [p.evidence() for p in active_handles], "active_owner_count": 0})
    second_context.result["passed"] = True
    context.result.update(two_actual_owner_trees_overlapped=True, maximum_active_owner_count=2,
                          same_profile_live_claim_rejected=True, refusal_error=refusal["error"],
                          refused_claim_no_extra_guest=True, original_owner_raw_bytes_preserved=True,
                          first_stop_preserved_second_live_tree=True, both_profiles_reclaimed=True,
                          all_actual_tree_exits_confirmed=True, guest_cleanup_before_verdict=False,
                          second_recovery_binding=final_b["state"]["recovery"])


def run_case(context):
    name = context.case.name
    if name == "concurrent-owner-refusal":
        run_concurrent_owners(context)
        return
    if name.startswith("schema"):
        old = name == "schema4-supervisor-rejected"
        context.init(old)
        before = ledger_capture(context.profile, context.case, "before-refusal")
        expected_schema = 4 if old else 5
        require(before["native-admissions.sqlite"]["user_version"] == expected_schema, "wrong schema fixture")
        target = context.supervisor if old else context.oldhost
        rows = context.cli(target, "inspect", "mode-refusal", 2, "profile mode mismatch; no implicit migration")
        require(not rows, "schema refusal emitted usable inspection")
        after = ledger_capture(context.profile, context.case, "after-refusal")
        require(after == before and not after["native-admissions.sqlite"]["tables"]["owner"]["rows"],
                "schema refusal modified ledger or created owner")
        context.result.update(schema_refused=expected_schema, no_implicit_migration=True,
                              raw_ledger_unchanged=True, no_spawn=True)
        return

    context.init()
    host = context.helper("host")
    watch = [host]
    if name == "watch-ui-death":
        watch.append(context.helper("ui"))
    mode = {"normal-close": "normal", "proof-tamper": "normal", "expiry-normal": "expiry-normal",
            "expiry-no-close": "expiry-no-close", "descendant-stdio": "descendant-stdio"}.get(name, "descendant-hold")
    ttl = 700 if name.startswith("expiry-") else 8000
    supervisor, grant, claim = claim_session(context, "first", mode, watch, ttl)

    if name in {"normal-close", "expiry-normal", "expiry-no-close", "proof-tamper"}:
        final = complete_reclamation(context, supervisor, grant, claim, 2 if name == "expiry-no-close" else 0)
        if name == "expiry-no-close":
            receipt_file = Path(str(supervisor.peer_file) + ".stop-receipt.json")
            require(receipt_file.is_file(), "killed no-close fixture's synchronous Stop20 receipt is missing")
            receipt = load(receipt_file)
            assert_expiry_no_close_receipt(final, claim, receipt)
            fixture, diagnostic = killed_fixture_diagnostic(supervisor.peer_file)
            dump(supervisor.folder / "final-fixture-diagnostic.json", diagnostic)
            context.result.update(expected_resource_cleanup_negative=True, synchronous_stop_receipt_validated=True,
                                  complete_fixture_json_present=fixture is not None,
                                  final_fixture_diagnostic=diagnostic,
                                  terminal_protocol_complete=False, actual_job_deadline_termination=True)
        else:
            fixture = load(supervisor.peer_file)
            require(fixture.get("http_not_invoked") is True, "fixture HTTP exclusion missing")
        if name in {"normal-close", "proof-tamper"}:
            require(fixture.get("ack_received") is True and fixture.get("stop_received") is False,
                    "normal Close terminal ACK/Stop proof wrong")
            require(len(events(final["snapshot"], "close_ack_written")) == 1, "normal ACK write missing")
        elif name == "expiry-normal":
            require(final.get("expired_terminal_protocol_complete") is True and fixture.get("ack_received") is True
                    and fixture.get("stop_received") is True, "expiry terminal proof incomplete")
            bound = events(final["snapshot"], "expiry_teardown_bound")
            require(len(bound) == 1 and bound[0]["detail"].get("authority_deadline_renewed") is False,
                    "expiry renewed authority")
        else:
            require(context.result.get("expected_resource_cleanup_negative") is True,
                    "expiry-no-close resource negative was not proven")
        if name == "proof-tamper":
            before = ledger_capture(context.profile, context.case, "before-tamper")
            with sqlite3.connect(context.profile / "native-admissions.sqlite", timeout=.5) as db:
                changed = db.execute("UPDATE recovery SET payload=substr(payload,1,17) WHERE singleton=1").rowcount
                require(changed == 1, "fresh Recovery tamper did not update one row")
            corrupt = ledger_capture(context.profile, context.case, "after-tamper")
            require(payload(corrupt, "owner") == payload(before, "owner"), "tamper changed owner")
            require(len(bytes.fromhex(payload(corrupt, "recovery"))) == 17, "tamper was not exact truncation")
            rows = context.cli(context.supervisor, "inspect", "tamper-refusal", 2, "record envelope")
            require(not rows, "corrupt proof yielded inspection")
            after = ledger_capture(context.profile, context.case, "after-tamper-refusal")
            require(after == corrupt, "corrupt proof refusal mutated ledger")
            context.result.update(tampered_proof_refused=True, no_spawn_after_tamper=True,
                                  raw_owner_preserved=True, corruption_bytes=17)
        return

    descendant = fixture_handles(context, supervisor, claim)
    if name == "descendant-stdio":
        supervisor.wait(lambda r: r.get("event") == "host_observation"
                        and r["observation"].get("event") == "exit")
        require(descendant.alive(), "descendant did not hold inherited stdio before Job cleanup")
        # No external termination is permitted before complete_reclamation/handle exit assertions.
        final = complete_reclamation(context, supervisor, grant, claim, 2)
        require(descendant.wait(int(context.deadline.remaining(3) * 1000)), "Job did not end inherited-stdio descendant")
        require(events(final["snapshot"], "job_terminate_requested"), "descendant stdio cleanup did not request Job termination")
        require(not descendant.cleanup_terminated, "test cleanup supplied descendant exit proof")
        dump(context.case / "descendant-kernel-exit.json", descendant.evidence())
        context.result.update(direct_exit_before_inherited_stdio_eof=True, job_terminated_descendant=True,
                              guest_cleanup_before_verdict=False)
        return

    direct = context.retain(claim["pid"], "trusted close-peer fixture direct child")
    require(direct.alive() and descendant.alive(), "hold fixture was not alive at registration")
    before = registered_state(supervisor, grant, claim)
    ledger_capture(context.profile, context.case, "registered")
    incarnation = before["recovery"]["incarnation"]

    if name == "control-output-blocked":
        supervisor.cmd("heartbeat")
        supervisor.block_control_output()
        started = time.monotonic()
        host.terminate(74)
        require(host.wait(int(context.deadline.remaining(2) * 1000)), "blocked-output watch target exit unconfirmed")
        # Keep stdin held and heartbeats enabled. No output drain or guest cleanup before kernel exits.
        require(supervisor.owned.wait(int(min(context.deadline.remaining(), 5.5) * 1000)),
                "blocked control output exceeded first-watch-loss five-second bound plus sampling allowance")
        elapsed = time.monotonic() - started
        require(elapsed <= 5.5 and not supervisor.p.stdin.closed, "blocked output bypassed bounded watch reclamation")
        require(direct.wait(int(context.deadline.remaining(2) * 1000)) and
                descendant.wait(int(context.deadline.remaining(2) * 1000)), "blocked-output exit leaked guest tree")
        dump(context.case / "blocked-output-kernel-exits.json", {"seconds_since_watch_termination": elapsed,
                                                                "native_hard_bound_seconds": 5,
                                                                "external_sampling_allowance_seconds": .5,
                                                                "supervisor": supervisor.owned.evidence(),
                                                                "direct": direct.evidence(), "descendant": descendant.evidence()})
        supervisor.resume_output()
        require(supervisor.finish() == 2, "blocked control output did not fail closed")
        after = context.inspect("blocked-output-persistent-inspect")
        raw = ledger_capture(context.profile, context.case, "blocked-output-ledger")
        if after["owner"]["phase"] == "Released":
            require(after["recovery"]["phase"] == "Reclaimed" and
                    all(after["recovery"].get(flag) is True for flag in
                        ("tree_empty", "child_exit", "stdout_eof", "stderr_eof", "pipe_joined", "network_joined", "gate_closed")),
                    "blocked output persisted an incomplete release")
            assert_raw_reclaimed(raw, {"state": after}, grant, sha(context.peer))
        else:
            assert_retained(after, grant, claim)
        require(not direct.cleanup_terminated and not descendant.cleanup_terminated, "test cleanup supplied blocked-output proof")
        context.result.update(output_backpressure_observed=True, watch_loss_bounded_seconds=elapsed,
                              guest_tree_exit_before_output_drain=True, held_stdin_during_watch_loss=True,
                              guest_cleanup_before_verdict=False, durable_phase=after["owner"]["phase"])
        return

    if name == "supervisor-crash":
        require(supervisor.heartbeat_sent > 0, "no heartbeats before forced crash")
        require(not any(r.get("event") == "controller_lost" for r in supervisor.rows), "controller lost before forced crash")
        # Only kill our retained supervisor, with heartbeats still running until termination.
        supervisor.owned.terminate(72)
        require(supervisor.owned.wait(int(context.deadline.remaining(5) * 1000)), "supervisor crash exit unconfirmed")
        # Kernel Job-close is tested BEFORE cleanup can terminate any guest.
        require(direct.wait(int(context.deadline.remaining(5) * 1000)), "Job-close did not terminate direct child")
        require(descendant.wait(int(context.deadline.remaining(5) * 1000)), "Job-close did not terminate descendant")
        dump(context.case / "kernel-tree-exit-before-cleanup.json", {"direct": direct.evidence(),
                                                                   "descendant": descendant.evidence(),
                                                                   "supervisor": supervisor.owned.evidence()})
        require(not direct.cleanup_terminated and not descendant.cleanup_terminated, "cleanup supplied crash proof")
        supervisor.finish()
        require(not any(r.get("event") == "final" for r in supervisor.rows), "forced crash reported reclamation")
        retained = context.inspect("after-kernel-exit")
        assert_retained(retained, grant, claim)
        restart_refusals(context, watch, grant, claim, retained, "restart")
        ledger_capture(context.profile, context.case, "after-restart-refusals")
        context.result.update(kernel_job_close_tree_exit=True, guest_cleanup_before_verdict=False,
                              kernel_exit_did_not_release_owner=True, heartbeat_active_until_crash=True)
        return

    if name == "recovery-write-failure":
        with sqlite3.connect(context.profile / "native-admissions.sqlite", timeout=.5) as db:
            db.execute("CREATE TRIGGER runtime008_recovery_abort BEFORE UPDATE ON recovery "
                       "BEGIN SELECT RAISE(ABORT, 'runtime008 injected recovery update failure'); END")
        armed = ledger_capture(context.profile, context.case, "trigger-armed")
        require(any(t[0] == "runtime008_recovery_abort" for t in armed["native-admissions.sqlite"]["triggers"]),
                "actual recovery SQLite trigger missing")
        supervisor.close_stdin()
        supervisor.wait(lambda r: r.get("event") == "owner_error" and
                        "runtime008 injected recovery update failure" in r.get("error", ""))
        require(supervisor.finish() == 2, "actual recovery write failure did not exit 2 boundedly")
        require(direct.wait(int(context.deadline.remaining(3) * 1000)) and
                descendant.wait(int(context.deadline.remaining(3) * 1000)), "write failure leaked Job tree")
        require(not any(r.get("event") == "final" for r in supervisor.rows), "failed Recovery commit emitted final release")
        observations = supervisor.observations()
        require(any(e.get("event") == "job_empty" and e["detail"].get("active_processes") == 0
                    for e in observations), "write failure lacks actual Job resource cleanup evidence")
        require(any(e.get("event") == "control_stdin_closed_reaped" and e.get("detail") is True
                    for e in observations), "write failure lacks actual control stdin close/reap evidence")
        require(any(e.get("event") == "http_cancel_applied" and
                    e["detail"].get("effect_fence", {}).get("gate_closed") is True
                    for e in observations), "write failure lacks gate close evidence")
        retained = context.inspect("write-failure-persistent-inspect")
        assert_retained(retained, grant, claim)
        after = ledger_capture(context.profile, context.case, "after-write-failure")
        require(payload(after, "owner") == payload(armed, "owner") and
                payload(after, "recovery") == payload(armed, "recovery"), "failed transaction changed owner/Recovery")
        restart_refusals(context, watch, grant, claim, retained, "restart")
        ledger_capture(context.profile, context.case, "after-write-failure-refusals")
        context.result.update(actual_sqlite_abort=True, bounded_exit_code=2, gate_closed_and_job_empty=True,
                              failed_transaction_preserved_owner_and_recovery=True, owner_not_cleared=True)
        return

    eof_started = None
    if name in {"controller-eof", "queued-input-eof"}:
        if name == "queued-input-eof":
            supervisor.stop_heartbeats()
            queued = 0
            try:
                for _ in range(40):
                    supervisor.send("inspect")
                    queued += 1
                    supervisor.send("heartbeat")
                    queued += 1
            except (BrokenPipeError, OSError) as error:
                # Queue saturation may already have caused the native sticky disconnect.
                context.result["queued_input_write_error"] = repr(error)
            require(queued >= 8, "queued EOF probe did not issue enough input to exercise controller backlog")
            context.result["queued_input_commands_before_eof"] = queued
        eof_started = time.monotonic()
        supervisor.close_stdin()
        expected_exit = 2
    elif name.startswith("watch-"):
        target = watch[1] if name == "watch-ui-death" else watch[0]
        heartbeat = supervisor.cmd("heartbeat")
        require(heartbeat.get("authority_deadline_renewed") is False, "heartbeat renewed business authority")
        pulses = supervisor.heartbeat_sent
        require(not supervisor.p.stdin.closed, "watch death stdin not retained")
        death_at = time.monotonic()
        target.terminate(73)
        require(target.wait(int(context.deadline.remaining(3) * 1000)), "watched helper exit unconfirmed")
        dump(context.case / "watch-target-exit.json", target.evidence())
        supervisor.wait(lambda r: r.get("event") == "controller_lost")
        require(time.monotonic() - death_at < .5, "watch loss was not observed before heartbeat timeout")
        require(not supervisor.p.stdin.closed and supervisor.heartbeat_sent >= pulses,
                "watch death closed controller input/heartbeat unexpectedly")
        if len(watch) == 2:
            require(watch[0].alive(), "host watch died during UI-only test")
        context.result.update(watch_target_exit_confirmed=True, held_stdin_during_watch_death=True,
                              heartbeat_sent_at_watch_death=pulses, heartbeat_sent_after_loss=supervisor.heartbeat_sent)
        expected_exit = 2
    elif name == "heartbeat-timeout":
        supervisor.stop_heartbeats()
        require(not supervisor.p.stdin.closed and all(p.alive() for p in watch), "timeout fixture lost input/watch")
        expected_exit = 2
    else:
        raise AssertionError("unsupported case: " + name)
    lost = supervisor.wait(lambda r: r.get("event") == "controller_lost")
    require(lost.get("business_gate_closed") is True and lost.get("authority_deadline_renewed") is False,
            "controller loss gate/deadline proof missing")
    final = complete_reclamation(context, supervisor, grant, claim, expected_exit, incarnation)
    if eof_started is not None:
        elapsed = supervisor.exited_at - eof_started
        require(elapsed <= 5.5, "queued/input EOF exceeded bounded reclamation window")
        context.result["seconds_since_input_eof"] = elapsed
    require(direct.wait(int(context.deadline.remaining(3) * 1000)) and
            descendant.wait(int(context.deadline.remaining(3) * 1000)), "controller loss did not terminate held fixture tree")
    require(events(final["snapshot"], "job_terminate_requested"), "controller loss tree was not Job terminated")
    dump(context.case / "tree-exit-before-cleanup.json", {"direct": direct.evidence(), "descendant": descendant.evidence()})
    context.result.update(controller_loss_gate_closed=True, authority_ttl_not_renewed=True,
                          guest_cleanup_before_verdict=False)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--case-timeout", type=float, default=25)
    parser.add_argument("--overall-timeout", type=float, default=240)
    args = parser.parse_args(argv)
    require(os.name == "nt", "runtime runner is Windows-only")
    require(10 <= args.case_timeout <= 120 and 30 <= args.overall_timeout <= 1800, "timeout range")
    out, check = args.out.resolve(), args.check.resolve()
    out.relative_to(ROOT)
    require(not out.exists(), "fresh output path required; never reuse failed attempts")
    local = os.environ.get("LOCALAPPDATA")
    require(local and Path(local).is_dir(), "real LOCALAPPDATA directory required")
    env = {key: os.environ[key] for key in ("SYSTEMROOT", "WINDIR", "COMSPEC") if key in os.environ}
    env["LOCALAPPDATA"] = local
    overall = time.monotonic() + args.overall_timeout
    candidate, manifest = snapshot_candidate(ROOT, check, out, overall)
    results = []
    for name in CASES:
        case = out / name
        case.mkdir()
        context = Context(case, candidate, env, Deadline(args.case_timeout, overall))
        started = time.monotonic()
        print(json.dumps({"case": name, "status": "running", "path": str(case)}), flush=True)
        try:
            run_case(context)
            context.result["passed"] = True
        except Exception as error:
            context.result["error"] = repr(error)
            (case / "failure.txt").write_text(traceback.format_exc(), encoding="utf-8")
        finally:
            context.cleanup()
            try:
                ledger_capture(context.profile, case, "final-evidence")
            except Exception as error:
                context.result["evidence_capture_error"] = repr(error)
                context.result["passed"] = False
            context.result["seconds"] = time.monotonic() - started
            dump(case / "result.json", context.result)
            results.append(context.result)
        print(json.dumps({"case": name, "passed": context.result["passed"]}), flush=True)
        if not context.result["passed"]:
            break
    unchanged = source_inventory(ROOT) == manifest["sources"] and sha(Path(__file__)) == manifest["runner_sha256"]
    candidate_unchanged = (sha(candidate / "manifest.json") == manifest["manifest_sha256"] and
                           all(sha(candidate / p) == h for p, h in manifest["frozen_files"].items()))
    result = {"status": "passed" if len(results) == len(CASES) and all(r["passed"] for r in results)
              and unchanged and candidate_unchanged else "failed", "cases": results,
              "sources_unchanged": unchanged, "candidate_unchanged": candidate_unchanged,
              "expected_cases": list(CASES), "http_requests": 0, "scenario_replayed": False,
              "scope": "control-only actual Windows supervisor ownership, kernel Job cleanup, fail-closed durable Recovery; no product/HTTP qualification"}
    dump(out / "result.json", result)
    files = {p.relative_to(out).as_posix(): sha(p) for p in sorted(out.rglob("*")) if p.is_file()}
    dump(out / "evidence-manifest.json", {"version": 1, "files": files,
                                          "candidate_manifest_sha256": manifest["manifest_sha256"],
                                          "check_receipt_sha256": manifest["check_receipt_sha256"],
                                          "runner_sha256": manifest["runner_sha256"], "includes_guest_fixture": True,
                                          "scenario_replayed": False})
    print(json.dumps({"path": str(out), "status": result["status"], "completed_cases": len(results)}), flush=True)
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
