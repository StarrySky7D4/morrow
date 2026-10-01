"""Local Windows owner-recovery faults and independent raw SQLite evidence audit.

No builds, installed-library repair, CI, guest replay or production fault hooks.
The run command creates fresh isolated libraries and uses pinned packaged binaries.
The verify command is read-only. Synthetic tests qualify the auditor, not resources.
"""
from __future__ import annotations

import argparse
import copy
import ctypes
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import threading
import time

PROOF = ("resource_reclaimed", "gate_closed", "tree_empty", "child_exit",
         "stdout_eof", "stderr_eof", "control_reaped")
CASES = ("normal", "host-crash", "supervisor-crash", "ui-crash", "commit-before-exit")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha_bytes(raw):
    return hashlib.sha256(raw).hexdigest()


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def wire(value):
    # Mirrors serde_json::Value's compact array/string encoding used by the ledger.
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def write_json(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def complete(record):
    return (record.get("version") == 2 and all(record.get(k) is True for k in PROOF)
            and record.get("business_gate_revoked") is True
            and record.get("normal_shutdown") is False
            and record.get("business_outcome") == "Unknown"
            and record.get("protocol_ack") == "not_applicable"
            and type(record.get("child_exit_code")) is int)


def audit_rows(text, digest, rows):
    """Verify literal UTF-8 record/receipt hashes, sequence, head and identity bindings."""
    require(sha_bytes(text.encode("utf-8")) == digest, "owner raw-text SHA mismatch")
    owner = json.loads(text)
    previous = ""
    last = None
    for ordinal, row in enumerate(rows, 1):
        sequence, original_text, original_digest, receipt_text, receipt_digest, prior, head = row
        require(sequence == ordinal and prior == previous, "history sequence/previous-head mismatch")
        require(sha_bytes(original_text.encode("utf-8")) == original_digest, "archived raw-text SHA mismatch")
        require(sha_bytes(receipt_text.encode("utf-8")) == receipt_digest, "receipt raw-text SHA mismatch")
        require(sha_bytes(wire(list(row[:6]))) == head, "history head SHA mismatch")
        original, receipt = json.loads(original_text), json.loads(receipt_text)
        require(original.get("phase") == "Closing" and complete(original), "archived original lacks complete abnormal proof")
        require(original.get("recovery_history_head") == prior, "original history binding mismatch")
        require(receipt.get("version") == 1 and receipt.get("acknowledged_unknown") is True
                and receipt.get("original_supervisor_ended") is True, "receipt acknowledgement/end binding mismatch")
        for key in ("profile", "generation", "incarnation"):
            require(receipt.get(key) == original.get(key), "receipt identity mismatch: " + key)
        require(receipt.get("original_digest") == original_digest, "receipt original SHA mismatch")
        require(receipt.get("manager_sha256") == original.get("supervisor_sha256"), "receipt artifact mismatch")
        for key in ("manager_pid", "manager_creation_filetime"):
            require(type(receipt.get(key)) is int and receipt[key] > 0, "receipt manager identity missing")
        last = (original, original_digest, receipt_digest, head)
        previous = head
    if owner.get("phase") == "Recovered":
        require(last is not None, "Recovered owner has no archive")
        expected = copy.deepcopy(last[0])
        expected["phase"] = "Recovered"
        expected["recovery"] = {"version": 1, "original_digest": last[1],
                                "receipt_digest": last[2], "history_head": last[3]}
        require(owner == expected, "Recovered owner differs from literal archived original")
    elif owner.get("version") == 2:
        require(owner.get("recovery_history_head") == previous, "current owner history binding mismatch")
    return {"raw_record_sha256": digest, "history_head": previous, "history_count": len(rows),
            "raw_record_and_receipts_verified": True, "phase": owner.get("phase")}


def read_sqlite(path, backup=None, immutable=False):
    # mode=ro and an explicit transaction preserve one owner/history view including WAL.
    # SQLite read-only WAL connections may create cache sidecars. Sealed backup
    # files are complete standalone images and must be read with immutable=1.
    uri = Path(path).resolve().as_uri() + "?mode=ro" + ("&immutable=1" if immutable else "")
    connection = sqlite3.connect(uri, uri=True, timeout=.1)
    try:
        connection.execute("BEGIN")
        text, digest = connection.execute("SELECT record,digest FROM owner WHERE singleton=1").fetchone()
        present = connection.execute("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='owner_recovery'").fetchone()[0]
        owner = json.loads(text)
        require(owner.get("version") != 2 or present == 1, "v2 recovery table missing")
        rows = [list(row) for row in connection.execute("SELECT sequence,original_record,original_digest,receipt,receipt_digest,previous_head,head FROM owner_recovery ORDER BY sequence")] if present else []
        audit = audit_rows(text, digest, rows)
        if backup:
            target = sqlite3.connect(str(backup))
            try:
                connection.backup(target)
            finally:
                target.close()
        connection.rollback()
        return {"raw_record": text, "stored_digest": digest, "record": owner,
                "history_rows": rows, "audit": audit}
    finally:
        connection.close()


def snapshot(library, case, label):
    backup = case / (label + ".sqlite")
    result = read_sqlite(library / "workbench-supervisor.sqlite", backup)
    (case / (label + ".record.txt")).write_bytes(result["raw_record"].encode("utf-8"))
    result["sqlite_backup_sha256"] = sha(backup)
    write_json(case / (label + ".snapshot.json"), result)
    return result


class Windows:
    def __init__(self):
        require(os.name == "nt", "Real process faults require Windows")
        self.k = ctypes.WinDLL("kernel32", use_last_error=True)
        signatures = {
            "OpenProcess": ([ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32], ctypes.c_void_p),
            "GetProcessId": ([ctypes.c_void_p], ctypes.c_uint32),
            "WaitForSingleObject": ([ctypes.c_void_p, ctypes.c_uint32], ctypes.c_uint32),
            "TerminateProcess": ([ctypes.c_void_p, ctypes.c_uint32], ctypes.c_int),
            "GetExitCodeProcess": ([ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32)], ctypes.c_int),
            "GetProcessTimes": ([ctypes.c_void_p] + [ctypes.POINTER(ctypes.c_uint64)] * 4, ctypes.c_int),
            "CloseHandle": ([ctypes.c_void_p], ctypes.c_int),
        }
        for name, (args, result) in signatures.items():
            function = getattr(self.k, name)
            function.argtypes, function.restype = args, result
        self.n = ctypes.WinDLL("ntdll")
        for name in ("NtSuspendProcess", "NtResumeProcess"):
            function = getattr(self.n, name)
            function.argtypes, function.restype = [ctypes.c_void_p], ctypes.c_long

    def close_window(self, pid):
        user = ctypes.WinDLL("user32", use_last_error=True)
        callback_type = ctypes.WINFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p)
        user.EnumWindows.argtypes, user.EnumWindows.restype = [callback_type, ctypes.c_void_p], ctypes.c_int
        user.GetWindowThreadProcessId.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32)]
        user.PostMessageW.argtypes, user.PostMessageW.restype = [ctypes.c_void_p, ctypes.c_uint32, ctypes.c_void_p, ctypes.c_void_p], ctypes.c_int
        found = []

        @callback_type
        def visit(hwnd, _):
            owner = ctypes.c_uint32()
            user.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
            if owner.value == pid:
                found.append(hwnd)
            return 1

        require(user.EnumWindows(visit, None), "EnumWindows failed")
        require(found, "owned UI window absent")
        for hwnd in found:
            require(user.PostMessageW(hwnd, 0x10, None, None), "owned WM_CLOSE failed")
        return len(found)


class Held:
    def __init__(self, win, pid, creation=None):
        self.win, self.pid = win, pid
        self.handle = win.k.OpenProcess(0x00101801, 0, pid)
        require(self.handle, "OpenProcess failed for owned PID " + str(pid))
        self.suspended = False
        self.resume_timer = None
        try:
            require(win.k.GetProcessId(self.handle) == pid, "held handle PID mismatch")
            times = [ctypes.c_uint64() for _ in range(4)]
            require(win.k.GetProcessTimes(self.handle, *(ctypes.byref(v) for v in times)), "GetProcessTimes failed")
            self.creation = times[0].value
            require(creation is None or creation == self.creation, "held creation identity mismatch")
        except BaseException:
            win.k.CloseHandle(self.handle)
            raise

    def live(self):
        status = self.win.k.WaitForSingleObject(self.handle, 0)
        require(status in (0, 258), "held wait failed")
        return status == 258

    def suspend(self):
        require(self.live(), "original exited before suspend")
        require(self.win.n.NtSuspendProcess(self.handle) >= 0, "owned suspend failed")
        self.suspended = True
        # A stuck management subprocess cannot leave the original stopped.
        self.resume_timer = threading.Timer(.9, self.resume)
        self.resume_timer.start()
        return self.live()

    def resume(self):
        if self.suspended:
            self.win.n.NtResumeProcess(self.handle)
            self.suspended = False

    def crash(self):
        require(self.win.k.TerminateProcess(self.handle, 72), "owned fault failed")

    def wait(self, seconds=7):
        started = time.monotonic()
        require(self.win.k.WaitForSingleObject(self.handle, int(seconds * 1000)) == 0, "held process cleanup deadline")
        require(self.win.k.GetProcessId(self.handle) == self.pid, "held identity drift")
        code = ctypes.c_uint32()
        require(self.win.k.GetExitCodeProcess(self.handle, ctypes.byref(code)), "exit code query failed")
        return {"retained_pid": self.pid, "creation_filetime": self.creation, "wait_signaled": True,
                "identity_matches": True, "exit_code": code.value, "elapsed_seconds": time.monotonic() - started}

    def dispose(self):
        try:
            if self.resume_timer:
                self.resume_timer.cancel()
                self.resume_timer.join()
            if self.suspended and self.live():
                self.win.n.NtResumeProcess(self.handle)
                self.suspended = False
            if self.live():
                self.crash()
                self.wait()
        finally:
            self.win.k.CloseHandle(self.handle)


def validate_bundle(candidate, expected):
    require(sha(candidate / "manifest.json") == expected, "candidate manifest SHA mismatch")
    manifest = json.loads((candidate / "manifest.json").read_text("utf-8"))
    require(manifest.get("bundle"), "packaged Release bundle manifest required")
    bundle = candidate / "bundle"
    for relative, digest in manifest["bundle"].items():
        path = (bundle / relative).resolve()
        path.relative_to(bundle.resolve())
        require(path.is_file() and sha(path) == digest, "candidate artifact SHA mismatch: " + relative)
    return bundle, manifest


def launch(bundle, library, probe=None):
    args = [str(bundle / "morrow_studio.exe"), "--data-directory=" + str(library), "--locale=en"]
    if probe:
        args.append("--startup-check=" + str(probe))
    info = subprocess.STARTUPINFO()
    info.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    info.wShowWindow = 0
    with (library.parent / "desktop.stdout").open("wb") as output, (library.parent / "desktop.stderr").open("wb") as error:
        return subprocess.Popen(args, cwd=bundle, startupinfo=info, stdout=output, stderr=error)


def manage(bundle, library, case, label, token=None, acknowledge=True, popen=False):
    args = [str(bundle / "morrow-workbench-supervisor.exe"),
            "--owner-preview" if token is None else "--owner-recover", str(library),
            str(bundle / "morrow-workbench-host.exe"), str(bundle / "plugins/workbench.morrowplugin")]
    if token is not None:
        args.append(token)
        if acknowledge:
            args.append("--acknowledge-unknown")
    if popen:
        return subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    result = subprocess.run(args, capture_output=True, timeout=12)
    return save_command(case, label, args, result.returncode, result.stdout, result.stderr)


def save_command(case, label, args, code, stdout, stderr):
    (case / (label + ".stdout")).write_bytes(stdout)
    (case / (label + ".stderr")).write_bytes(stderr)
    value = json.loads(stdout) if stdout.strip() else None
    row = {"args": args, "exit_code": code, "value": value,
           "stdout_sha256": sha_bytes(stdout), "stderr_sha256": sha_bytes(stderr)}
    write_json(case / (label + ".command.json"), row)
    return row


def wait_active(library, process, win, held):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        try:
            current = read_sqlite(library / "workbench-supervisor.sqlite")
            record = current["record"]
            if record["phase"] == "Active":
                supervisor = Held(win, record["supervisor_pid"], record["supervisor_creation_filetime"])
                held.append(supervisor)
                host = Held(win, record["child_pid"], record["child_creation_filetime"])
                held.append(host)
                return current, supervisor, host
        except (OSError, sqlite3.Error, TypeError):
            pass
        require(process.poll() is None, "UI exited before Active")
        time.sleep(.005)
    raise ValueError("Active owner deadline")


def run_case(bundle, out, name, win, settle_seconds=1):
    case = out / name
    case.mkdir()
    library = case / "library"
    library.mkdir()
    probe = case / "startup.json" if name == "normal" else None
    process = launch(bundle, library, probe)
    write_json(case / "process-start.json", {"ui_pid": process.pid, "owned_launch": True,
               "library": str(library), "bundle": str(bundle), "wall_time_unix": time.time()})
    print(json.dumps({"case": name, "event": "owned_ui_started", "ui_pid": process.pid}), flush=True)
    held = []
    ui = Held(win, process.pid)
    held.append(ui)
    try:
        initial, supervisor, host = wait_active(library, process, win, held)
        active_observed_at = time.monotonic()
        write_json(case / "process-identities.json", {"ui": {"pid": ui.pid, "creation_filetime": ui.creation},
                   "supervisor": {"pid": supervisor.pid, "creation_filetime": supervisor.creation},
                   "host": {"pid": host.pid, "creation_filetime": host.creation}})
        print(json.dumps({"case": name, "event": "owned_handles_retained", "ui_pid": ui.pid,
                         "supervisor_pid": supervisor.pid, "host_pid": host.pid}), flush=True)
        initial = snapshot(library, case, "initial")
        before_live = initial["raw_record"]
        live = manage(bundle, library, case, "live-preview")
        require(live["exit_code"] == 0 and live["value"]["eligible"] is False, "live owner preview unexpectedly eligible")
        refused = manage(bundle, library, case, "live-recover", live["value"]["preview_token"])
        require(refused["exit_code"] == 2, "live owner recovery unexpectedly accepted")
        require(read_sqlite(library / "workbench-supervisor.sqlite")["raw_record"] == before_live, "live refusal mutated original")
        require(ui.live() and supervisor.live() and host.live(), "live resources ended during refusal")
        time.sleep(settle_seconds)
        window = None
        fault_timing = None
        if name != "normal":
            elapsed = time.monotonic() - active_observed_at
            fault_timing = {"active_observation_to_fault_seconds": elapsed,
                            "post_active_settle_seconds": settle_seconds,
                            "early_500ms_window_observed": elapsed < .5,
                            "clock": "monotonic; retained original Active identity to owned TerminateProcess call"}
        if name == "normal":
            require(process.wait(timeout=40) == 0, "normal desktop exit failed")
        elif name == "supervisor-crash":
            supervisor.crash()
        elif name == "ui-crash":
            ui.crash()
        else:
            host.crash()
            if name == "commit-before-exit":
                deadline = time.monotonic() + 7
                while time.monotonic() < deadline and supervisor.live():
                    try:
                        value = read_sqlite(library / "workbench-supervisor.sqlite")
                        if complete(value["record"]) and value["record"]["phase"] == "Closing":
                            if supervisor.suspend():
                                committed = snapshot(library, case, "committed-live")
                                require(complete(committed["record"]), "suspended original lacks complete committed proof")
                                preview = manage(bundle, library, case, "committed-live-preview")
                                require(preview["value"]["reason"] == "original_supervisor_still_live", "complete live original was not excluded")
                                denial = manage(bundle, library, case, "committed-live-recover", preview["value"]["preview_token"])
                                require(denial["exit_code"] == 2, "suspended live original recovery accepted")
                                require(read_sqlite(library / "workbench-supervisor.sqlite")["raw_record"] == committed["raw_record"], "live commit refusal mutated original")
                                require(supervisor.suspended and supervisor.live(), "bounded suspend expired during live-proof management")
                                window = {"status": "observed", "original_wait_timeout_after_commit": True,
                                          "original_pid": supervisor.pid, "original_creation_filetime": supervisor.creation,
                                          "raw_commit_sha256": committed["stored_digest"],
                                          "live_reason": preview["value"]["reason"], "live_recovery_exit": denial["exit_code"],
                                          "injection": "NtSuspendProcess on retained owned original after read-only committed full-proof observation, then TerminateProcess"}
                                supervisor.crash()
                            break
                    except sqlite3.Error:
                        pass
                if window is None:
                    window = {"status": "not_observed", "reason": "original exited before an observed complete committed proof could be suspended; no precise-window claim"}
        if fault_timing is not None:
            write_json(case / "fault-injection.json", fault_timing)
        proofs = {"host": host.wait(), "supervisor": supervisor.wait()}
        if ui.live():
            win.close_window(ui.pid)
            require(process.wait(timeout=15) == 0, "native UI close failed")
        proofs["ui"] = ui.wait()
        write_json(case / "held-process-exits.json", proofs)
        original = snapshot(library, case, "original-ended")
        record = original["record"]
        result = {"case": name, "status": "passed", "held_process_proofs": proofs,
                  "post_active_settle_seconds": settle_seconds,
                  "fault_timing": fault_timing,
                  "live_owner_refused": True, "window": window,
                  "resources": {"held_process_exits": proofs, "production_original_proof": {key: record.get(key) for key in PROOF},
                                "production_full_abnormal_proof": complete(record)},
                  "business": {"outcome": record.get("business_outcome"), "normal_shutdown": record.get("normal_shutdown"), "replay": False},
                  "protocol": {"ack": record.get("protocol_ack"), "ui_exit_is_protocol_ack": False}}
        if name == "normal":
            require(record["phase"] == "Released" and record["normal_shutdown"] is True, "normal owner not Released")
            require(all(record.get(key) is True for key in PROOF), "normal resource proof incomplete")
            stages = json.loads(probe.read_text("utf-8"))
            require("workbench_frame" in stages and "revealed_frame" in stages and "error" not in stages, "actual Flutter frames missing")
            result["actual_flutter_frames"] = stages
        else:
            require(record["business_outcome"] == "Unknown" and record.get("normal_shutdown") is not True, "fault invented normal/business completion")
            if name != "supervisor-crash":
                require(record.get("normal_shutdown") is False, "complete fault proof lacks explicit abnormal outcome")
            require(record["generation"] == initial["record"]["generation"] and record["incarnation"] == initial["record"]["incarnation"], "fault changed owner identity")
            preview = manage(bundle, library, case, "ended-preview")
            require(preview["exit_code"] == 0, "ended preview failed")
            result["ended_preview"] = preview["value"]
            token = preview["value"]["preview_token"]
            if name == "supervisor-crash":
                require(preview["value"]["reason"] == "original_resource_proof_missing", "crash before proof did not preserve uncertainty")
                denial = manage(bundle, library, case, "missing-proof-recover", token)
                require(denial["exit_code"] == 2, "missing proof accepted")
                after = snapshot(library, case, "unchanged")
                require(after["raw_record"] == original["raw_record"] and not after["history_rows"], "missing-proof refusal mutated owner/archive")
            else:
                require(preview["value"]["eligible"] is True and complete(record), "complete ended original not eligible")
                no_ack = manage(bundle, library, case, "no-ack-recover", token, acknowledge=False)
                require(no_ack["exit_code"] == 2, "Unknown acknowledgment bypass")
                require(read_sqlite(library / "workbench-supervisor.sqlite")["raw_record"] == original["raw_record"], "no-ack refusal mutated owner")
                if name == "host-crash":
                    processes = [manage(bundle, library, case, "cas", token, popen=True) for _ in range(2)]
                    managers = []
                    for index, manager in enumerate(processes):
                        stdout, stderr = manager.communicate(timeout=12)
                        managers.append(save_command(case, "cas-" + str(index), manager.args, manager.returncode, stdout, stderr))
                    require(sorted(row["exit_code"] for row in managers) == [0, 2], "CAS recovery did not choose exactly one manager")
                    result["concurrent_manager_exit_codes"] = [row["exit_code"] for row in managers]
                else:
                    repair = manage(bundle, library, case, "recover", token)
                    require(repair["exit_code"] == 0, "explicit recovery failed")
                recovered = snapshot(library, case, "recovered")
                require(recovered["record"]["phase"] == "Recovered", "resource-only owner missing")
                require(recovered["history_rows"][0][1:3] == [original["raw_record"], original["stored_digest"]]
                        or tuple(recovered["history_rows"][0][1:3]) == (original["raw_record"], original["stored_digest"]), "literal original archive not preserved")
                stale = manage(bundle, library, case, "stale-recover", token)
                require(stale["exit_code"] == 2, "stale preview accepted")
                direct = subprocess.run([str(bundle / "morrow-workbench-host.exe"), str(library / "workbench.db"), str(bundle / "plugins/workbench.morrowplugin")], capture_output=True, timeout=12)
                (case / "direct-host.stdout").write_bytes(direct.stdout)
                (case / "direct-host.stderr").write_bytes(direct.stderr)
                require(direct.returncode != 0, "direct host bypassed retained owner")
                require(read_sqlite(library / "workbench-supervisor.sqlite")["raw_record"] == recovered["raw_record"], "stale/direct refusal mutated recovered owner")
                result.update(archive_exact_original=True, stale_preview_exit=stale["exit_code"], direct_host_exit=direct.returncode,
                              recovery_audit=recovered["audit"])
        write_json(case / "result.json", result)
        return result
    finally:
        for retained in reversed(held):
            retained.dispose()
        print(json.dumps({"case": name, "event": "all_owned_handles_cleaned", "pids": [retained.pid for retained in held]}), flush=True)


def verify(directory, require_all=False):
    directory = Path(directory).resolve()
    manifest = json.loads((directory / "evidence-manifest.json").read_text("utf-8"))
    expected_files = manifest["files"]
    actual_files = {p.relative_to(directory).as_posix() for p in directory.rglob("*") if p.is_file() and p != directory / "evidence-manifest.json"}
    require(actual_files == set(expected_files), "evidence file inventory changed")
    for relative, digest in expected_files.items():
        path = (directory / relative).resolve()
        path.relative_to(directory)
        require(sha(path) == digest, "evidence SHA mismatch: " + relative)
    receipt = json.loads((directory / "receipt.json").read_text("utf-8"))
    inputs = receipt["inputs"]
    require(sha(directory / "input-manifest.json") == inputs["candidate_manifest_sha256"], "copied input manifest mismatch")
    require(inputs["tool_sha256"] == sha(directory / "runner.py"), "runner SHA mismatch")
    validate_bundle(Path(inputs["candidate"]), inputs["candidate_manifest_sha256"])
    audits = []
    for snapshot_path in directory.rglob("*.snapshot.json"):
        snapshot_data = json.loads(snapshot_path.read_text("utf-8"))
        audit = audit_rows(snapshot_data["raw_record"], snapshot_data["stored_digest"], snapshot_data["history_rows"])
        backup = snapshot_path.with_name(snapshot_path.name.replace(".snapshot.json", ".sqlite"))
        require(sha(backup) == snapshot_data["sqlite_backup_sha256"], "SQLite backup SHA mismatch")
        independently_read = read_sqlite(backup, immutable=True)
        require(independently_read["raw_record"] == snapshot_data["raw_record"] and independently_read["history_rows"] == snapshot_data["history_rows"], "SQLite backup differs from literal snapshot")
        raw_file = snapshot_path.with_name(snapshot_path.name.replace(".snapshot.json", ".record.txt"))
        require(raw_file.read_bytes() == snapshot_data["raw_record"].encode("utf-8"), "literal record file differs")
        audits.append({"snapshot": snapshot_path.relative_to(directory).as_posix(), **audit})
    require(receipt["candidate_unchanged"] is True, "candidate changed during faults")
    for result in receipt["results"]:
        case = directory / result["directory"]
        initial = read_sqlite(case / "initial.sqlite", immutable=True)["record"]
        ended = read_sqlite(case / "original-ended.sqlite", immutable=True)["record"]
        proofs = result["held_process_proofs"]
        for role, pid_key, creation_key in (("host", "child_pid", "child_creation_filetime"),
                                            ("supervisor", "supervisor_pid", "supervisor_creation_filetime")):
            require(proofs[role]["retained_pid"] == initial[pid_key]
                    and proofs[role]["creation_filetime"] == initial[creation_key]
                    and proofs[role]["wait_signaled"] is True and proofs[role]["identity_matches"] is True,
                    "held proof differs from original identity: " + role)
        require(proofs["ui"]["wait_signaled"] is True and proofs["ui"]["identity_matches"] is True,
                "UI held exit proof missing")
        require(result["resources"]["production_original_proof"] == {key: ended.get(key) for key in PROOF},
                "resource claims differ from raw original proof")
        require(result["resources"]["production_full_abnormal_proof"] == complete(ended),
                "full-proof claim differs from independent audit")
        require(result["business"]["outcome"] == ended["business_outcome"]
                and result["business"]["normal_shutdown"] == ended.get("normal_shutdown")
                and result["business"]["replay"] is False, "business claim differs from raw original")
        require(result["protocol"]["ack"] == ended["protocol_ack"]
                and result["protocol"]["ui_exit_is_protocol_ack"] is False, "protocol claim differs from raw original")
        if result["window"] and result["window"]["status"] == "observed":
            committed = read_sqlite(case / "committed-live.sqlite", immutable=True)
            require(complete(committed["record"]) and committed["record"]["phase"] == "Closing", "precise-window original proof missing")
            require(result["window"]["raw_commit_sha256"] == committed["stored_digest"]
                    and result["window"]["original_wait_timeout_after_commit"] is True
                    and result["window"]["original_creation_filetime"] == initial["supervisor_creation_filetime"],
                    "precise-window identity/raw commit mismatch")
            live_preview = json.loads((case / "committed-live-preview.stdout").read_bytes())
            require(live_preview["reason"] == "original_supervisor_still_live" and live_preview["eligible"] is False,
                    "precise-window live exclusion missing")
    if require_all:
        require(all(row["status"] == "passed" for row in receipt["matrix"]), "required matrix has failure or unobserved window")
    return {"status": "verified", "matrix": receipt["matrix"], "sqlite_snapshots": len(audits), "audits": audits,
            "scope": receipt["scope"], "candidate_manifest_sha256": inputs["candidate_manifest_sha256"]}


def run(args):
    candidate = Path(args.candidate).resolve()
    bundle, _ = validate_bundle(candidate, args.expected_manifest_sha256)
    out = Path(args.out).resolve()
    require(out.name.startswith("m03-owner-recovery-011-"), "output must use new 011 evidence name")
    require(not out.exists(), "output already exists; preserve prior evidence")
    out.mkdir(parents=True)
    shutil.copy2(candidate / "manifest.json", out / "input-manifest.json")
    shutil.copy2(Path(__file__), out / "runner.py")
    inputs = {"candidate": str(candidate), "candidate_manifest_sha256": args.expected_manifest_sha256,
              "tool_sha256": sha(Path(__file__)), "candidate_scope": args.scope,
              "post_active_settle_seconds": args.settle_seconds,
              "fresh_isolated_libraries_only": True}
    matrix, results = [], []
    failure = None
    win = Windows()
    try:
        for name in args.cases:
            attempts = args.window_attempts if name == "commit-before-exit" else 1
            observed = False
            for attempt in range(attempts):
                actual_name = name if attempts == 1 else name + "-" + str(attempt + 1)
                result = run_case(bundle, out, name, win, args.settle_seconds)
                if actual_name != name:
                    (out / name).rename(out / actual_name)
                results.append({"directory": actual_name, **result})
                observed = name != "commit-before-exit" or result["window"]["status"] == "observed"
                if name == "host-crash" and args.settle_seconds == 0:
                    observed = observed and result["fault_timing"]["early_500ms_window_observed"]
                print(json.dumps({"case": actual_name, "status": "passed" if observed else "not_observed"}), flush=True)
                if observed:
                    break
            matrix.append({"case": name, "status": "passed" if observed else "not_observed", "attempts": attempt + 1})
    except BaseException as error:
        failure = str(error)
        matrix.append({"case": name, "status": "failed", "detail": failure})
    finally:
        unchanged = False
        try:
            validate_bundle(candidate, args.expected_manifest_sha256)
            unchanged = True
        except BaseException as error:
            failure = failure or str(error)
        for name in CASES:
            if name not in {row["case"] for row in matrix}:
                matrix.append({"case": name, "status": "not_run", "reason": "not selected or earlier failure"})
        receipt = {"version": 1, "inputs": inputs, "candidate_unchanged": unchanged,
                   "matrix": matrix, "results": results, "failure": failure, "ci": False,
                   "scope": args.scope + "; local real packaged Windows faults only; excludes final integrated candidate qualification, full G0 and SDK freeze"}
        write_json(out / "receipt.json", receipt)
        files = {p.relative_to(out).as_posix(): sha(p) for p in out.rglob("*") if p.is_file()}
        write_json(out / "evidence-manifest.json", {"version": 1, "files": files, "ci": False})
    require(failure is None, failure)
    summary = verify(out)
    print(json.dumps({key: value for key, value in summary.items() if key != "audits"}), flush=True)
    return 0 if all(row["status"] == "passed" for row in matrix if row["case"] in args.cases) else 3


def audit_native(args):
    """Read actual Flutter native-test libraries and archive their final SQLite view."""
    evidence, out = Path(args.directory).resolve(), Path(args.out).resolve()
    candidate = Path(args.candidate).resolve()
    require(not out.exists(), "audit output exists; preserve prior evidence")
    require(sha(candidate / "manifest.json") == args.expected_manifest_sha256, "native candidate manifest mismatch")
    manifest = json.loads((candidate / "manifest.json").read_text("utf-8"))
    require(manifest.get("files"), "native candidate artifact inventory missing")
    for relative, digest in manifest["files"].items():
        path = (candidate / relative).resolve()
        path.relative_to(candidate)
        require(sha(path) == digest, "native candidate artifact mismatch: " + relative)
    out.mkdir(parents=True)
    shutil.copy2(candidate / "manifest.json", out / "input-manifest.json")
    shutil.copy2(Path(__file__), out / "runner.py")
    reports = []
    cases = (("host-crash-library", "host-crash-explicit-recovery"),
             ("supervisor-crash-library", "supervisor-crash-unprovable"),
             ("concurrent-recovery-library", "concurrent-managers"),
             ("live-owner-library", "live-owner-refused"))
    for library_name, result_name in cases:
        case = out / result_name
        case.mkdir()
        source = evidence / (result_name + ".json")
        shutil.copy2(source, case / "native-result.json")
        result = json.loads(source.read_text("utf-8"))
        final = snapshot(evidence / library_name, case, "final")
        original = result["preview"]["record"]
        for role, source_key, pid_key, creation_key in (("host", "child", "child_pid", "child_creation_filetime"),
                                                       ("supervisor", "supervisor", "supervisor_pid", "supervisor_creation_filetime")):
            held = result[source_key]
            require(held["retained_pid"] == original[pid_key]
                    and held["creation_filetime"] == original[creation_key]
                    and held["wait_signaled"] is True and held["get_process_id_matches"] is True,
                    "native held identity mismatch: " + role)
        if result_name in ("host-crash-explicit-recovery", "concurrent-managers"):
            require(len(final["history_rows"]) == 1, "native recovery archive count mismatch")
            require(final["history_rows"][0][2] == result["preview"]["original_digest"]
                    and json.loads(final["history_rows"][0][1]) == original, "native preview differs from literal archive")
            require(final["record"]["generation"] == original["generation"] + 1,
                    "native next supervisor generation mismatch")
        elif result_name == "supervisor-crash-unprovable":
            require(not final["history_rows"] and final["stored_digest"] == result["preview"]["original_digest"],
                    "native unprovable original was changed")
            require(result["recover_refused"] == 2 and not complete(final["record"]), "native missing proof was accepted")
        elif result_name == "live-owner-refused":
            require(result["refusal_exit"] == 2 and result["live_before_close"] is True
                    and result["after"]["original_digest"] == result["preview"]["original_digest"], "native live refusal not preserved")
        reports.append({"case": result_name, "status": "verified", "audit": final["audit"],
                        "source_result_sha256": sha(source), "ui_process_fault_qualified": False,
                        "business_outcome": final["record"]["business_outcome"],
                        "protocol_ack": final["record"].get("protocol_ack")})
    receipt = {"version": 1, "reports": reports, "candidate_manifest_sha256": args.expected_manifest_sha256,
               "tool_sha256": sha(Path(__file__)), "ci": False,
               "scope": "read-only actual native test SQLite/held identities; packaged UI faults are separately qualified"}
    write_json(out / "receipt.json", receipt)
    write_json(out / "evidence-manifest.json", {"version": 1,
               "files": {p.relative_to(out).as_posix(): sha(p) for p in out.rglob("*") if p.is_file()}})
    print(json.dumps(receipt, ensure_ascii=False), flush=True)
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    execution = sub.add_parser("run")
    execution.add_argument("--candidate", required=True)
    execution.add_argument("--expected-manifest-sha256", required=True)
    execution.add_argument("--out", required=True)
    execution.add_argument("--scope", default="frozen prior 010 Release; not final integrated 011")
    execution.add_argument("--cases", nargs="+", choices=CASES, default=list(CASES))
    execution.add_argument("--window-attempts", type=int, default=6)
    execution.add_argument("--settle-seconds", type=float, default=1,
                           help="post-Active delay before owned fault; use 0 for real early-host crash regression")
    validation = sub.add_parser("verify")
    validation.add_argument("directory")
    validation.add_argument("--require-all", action="store_true")
    native_audit = sub.add_parser("audit-native")
    native_audit.add_argument("directory")
    native_audit.add_argument("--out", required=True)
    native_audit.add_argument("--candidate", required=True)
    native_audit.add_argument("--expected-manifest-sha256", required=True)
    args = parser.parse_args(argv)
    if args.command == "verify":
        print(json.dumps(verify(args.directory, args.require_all), ensure_ascii=False, indent=2))
        return 0
    if args.command == "audit-native":
        return audit_native(args)
    require(1 <= args.window_attempts <= 12, "window attempts must be 1..12")
    require(0 <= args.settle_seconds <= 10, "settle seconds must be 0..10")
    return run(args)


if __name__ == "__main__":
    raise SystemExit(main())
