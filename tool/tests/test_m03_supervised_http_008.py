"""Pure regression checks. No process, socket, build, or HTTP request is started."""
import ast
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
TOOL = HERE if (HERE / "m03_supervised_http_008.py").exists() else HERE.parent
BASE = TOOL / "m03_passive_fault_006.py"


def module(name):
    spec = importlib.util.spec_from_file_location(name, TOOL / (name + ".py"))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


m = module("m03_supervised_http_008")
f = module("m03_supervised_http_008_freeze")


class SupervisedEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.identity = {"child_pid": 123, "session": 7, "epoch": 1}
        recovery = dict(phase="Reclaimed", child_pid=123, session=7, epoch=1, grant_id="grant",
                        business_replay_allowed=False)
        recovery.update({k: True for k in ("tree_empty", "child_exit", "stdout_eof", "stderr_eof",
                                          "pipe_joined", "network_joined", "gate_closed")})
        self.final = {"state": {"supervised": True, "recovery": recovery, "owner": {"grant_id": "grant"}},
                      "snapshot": {"http": {"progress": {"worker_started": True, "worker_joined": True,
                                  "revoke_applied": True, "revoke_persisted": True}}}}
        self.events = [{"event": "job_empty", "detail": {"active_processes": 0, "direct_exit": True,
                        "stdout_eof": True, "stderr_eof": True}},
                       {"event": "control_stdin_closed_reaped", "detail": True},
                       {"event": "network_worker_joined", "detail": {"joined": True}}]
        self.heartbeats = [{"received_ns": 42, "result": {"received": True, "authority_deadline_renewed": False}}]

    def check(self):
        return m.supervised_terminal_checks(self.final, self.events, self.identity, self.heartbeats)

    def test_complete_proof_and_original_ttl(self):
        proof = self.check()
        self.assertTrue(proof["RecoveryReclaimed"])
        self.assertEqual(proof["original_ttl_ms"], 10000)
        self.assertFalse(proof["heartbeat_renews_authority"])

    def test_each_missing_recovery_resource_rejected(self):
        for field in ("tree_empty", "child_exit", "stdout_eof", "stderr_eof", "pipe_joined", "network_joined", "gate_closed"):
            with self.subTest(field=field):
                self.final["state"]["recovery"][field] = False
                with self.assertRaises(m.Unconfirmed):
                    self.check()
                self.final["state"]["recovery"][field] = True

    def test_retained_recovery_and_wrong_child_rejected(self):
        for field, wrong in (("phase", "Retained"), ("child_pid", 124), ("session", 8), ("epoch", 2), ("grant_id", "other"), ("business_replay_allowed", True)):
            old = self.final["state"]["recovery"][field]
            self.final["state"]["recovery"][field] = wrong
            with self.assertRaises(m.Unconfirmed):
                self.check()
            self.final["state"]["recovery"][field] = old

    def test_missing_duplicate_or_unconfirmed_native_event_rejected(self):
        original = copy.deepcopy(self.events)
        for index in range(3):
            for replacement in (original[:index] + original[index+1:], original + [original[index]]):
                self.events = replacement
                with self.assertRaises(m.Unconfirmed):
                    self.check()
        self.events = copy.deepcopy(original)
        self.events[0]["detail"]["active_processes"] = 1
        with self.assertRaises(m.Unconfirmed):
            self.check()
        self.events = copy.deepcopy(original)
        self.events[1]["detail"] = False
        with self.assertRaises(m.Unconfirmed):
            self.check()

    def test_vacuous_worker_and_unpersisted_revoke_rejected(self):
        progress = self.final["snapshot"]["http"]["progress"]
        for field in progress:
            progress[field] = False
            with self.assertRaises(m.Unconfirmed):
                self.check()
            progress[field] = True

    def test_missing_heartbeat_or_authority_renewal_rejected(self):
        self.heartbeats = []
        with self.assertRaises(m.Unconfirmed):
            self.check()
        self.heartbeats = [{"result": {"received": True, "authority_deadline_renewed": True}}]
        with self.assertRaises(m.Unconfirmed):
            self.check()

    def test_retained_reply_matching_does_not_steal_rows(self):
        cls = m.make_supervised_host(object)
        host = cls.__new__(cls)
        host.changed = threading.Condition()
        host.reader_ended = False
        host.rows = [{"value": {"event": "operator_result", "action": "inspect", "id": 1}},
                     {"value": {"event": "proposal"}},
                     {"value": {"event": "operator_result", "action": "inspect", "id": 2}},
                     {"value": {"event": "final"}}]
        self.assertEqual(host.wait(lambda r: r.get("action") == "inspect", after_index=2)["value"]["id"], 2)
        self.assertEqual(host.wait(lambda r: r.get("event") == "proposal")["value"]["event"], "proposal")
        self.assertEqual(host.wait(lambda r: r.get("event") == "final")["value"]["event"], "final")
        self.assertEqual(len(host.rows), 4)

    def test_heartbeat_and_qualification_share_lock_and_newline_transport(self):
        cls = m.make_supervised_host(object)
        host = cls.__new__(cls)
        host.command_lock = threading.Lock()
        host.changed = threading.Condition()
        host.reader_ended = False
        host.rows, host.commands = [], []
        observed = []

        def write(data):
            self.assertTrue(host.command_lock.locked())
            self.assertTrue(data.endswith(b"\n"))
            value = json.loads(data)
            observed.append(value)
            result = {"received": True, "authority_deadline_renewed": False} if value["action"] == "heartbeat" else {"accepted": True}
            with host.changed:
                host.rows.append({"line_received_ns": m.time.monotonic_ns(), "value": {
                    "event": "operator_result", "action": value["action"], "ok": True, "result": result}})
                host.changed.notify_all()

        host.proc = SimpleNamespace(poll=lambda: None, stdin=SimpleNamespace(write=write, flush=lambda: None))
        host.command("heartbeat")
        host.command("qualification_close_data", grant_id="grant", witness={"original": True})
        self.assertEqual([r["action"] for r in observed], ["heartbeat", "qualification_close_data"])
        self.assertEqual(len(host.commands), 2)

    def test_original_pure_assertions_preserved_verbatim_ast(self):
        base = ast.parse(BASE.read_text(encoding="utf-8"))
        newer = ast.parse((TOOL / "m03_supervised_http_008.py").read_text(encoding="utf-8"))
        actual = {n.name: ast.dump(n, include_attributes=False) for n in newer.body if isinstance(n, (ast.FunctionDef, ast.ClassDef))}
        for node in base.body:
            if isinstance(node, (ast.FunctionDef, ast.ClassDef)) and node.name not in {"run", "main"}:
                self.assertEqual(actual[node.name], ast.dump(node, include_attributes=False), node.name)


class FreezePinTests(unittest.TestCase):
    def test_frozen_manifest_compatible_with_original_and_strict_verifiers(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, check, out = [Path(temporary) / name for name in ("root", "check", "out")]
            root.mkdir(); check.mkdir()
            for relative in f.REQUIRED:
                p = root / relative
                p.parent.mkdir(parents=True, exist_ok=True); p.write_bytes(b"pure source fixture")
            (root / "tool").mkdir()
            for name in ("m03_supervised_http_008.py", "m03_supervised_http_008_freeze.py", "m03_passive_fault_006.py"):
                (root / "tool" / name).write_bytes((TOOL / name).read_bytes())
            (root / "tool/m03_revoke_barriers_011.py").write_bytes(b"pure helper fixture")
            sources = f.inventory(root)
            receipt = {"source_unchanged": True, "sources_before": sources, "sources_after": sources, "commands": []}
            for label in ("default-build", "feature-build"):
                (check / label).mkdir()
                log = check / (label + ".log"); log.write_bytes(b"pure fixture: not build evidence")
                pins = {}
                for name in f.BINARIES:
                    binary = check / label / name; binary.write_bytes(b"pure bytes: not executable")
                    pins[label + "/" + name] = f.sha(binary)
                receipt["commands"].append({"label": label, "exit_code": 0, "binaries": pins, "log_sha256": f.sha(log),
                    "features": [] if label == "default-build" else ["qualification-pipe-fault"],
                    "executable": label + "/morrow-native-stream-host.exe",
                    "executable_sha256": pins[label + "/morrow-native-stream-host.exe"]})
            f.dump(check / "receipt.json", receipt)
            guest_binary = Path(temporary) / "guest.exe"; guest_binary.write_bytes(b"pure guest fixture")
            guest_manifest = Path(temporary) / "guest.json"
            f.dump(guest_manifest, {"executable": str(guest_binary), "executable_sha256": f.sha(guest_binary), "input_sha256": {}, "source_files": {}})
            f.freeze(root, check, out, guest_manifest, f.sha(guest_manifest))
            with patch.object(m, "ROOT", root):
                for name in ("default-manifest.json", "manifest.json"):
                    original, _ = m.verify_candidate(out / name, f.sha(out / name))
                    strict, _ = m.verify_supervised_candidate(out / name, f.sha(out / name))
                    self.assertEqual(original, strict)
            with self.assertRaises(FileExistsError):
                f.freeze(root, check, out, guest_manifest, f.sha(guest_manifest))

    def test_old_missing_or_tampered_binary_pins_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, check = Path(temporary) / "root", Path(temporary) / "check"
            root.mkdir(); check.mkdir()
            for relative in f.REQUIRED:
                p = root / relative
                p.parent.mkdir(parents=True, exist_ok=True); p.write_bytes(b"pure source fixture")
            sources = f.inventory(root)
            receipt = {"source_unchanged": True, "sources_before": sources, "sources_after": sources, "commands": []}
            for label in ("default-build", "feature-build"):
                (check / label).mkdir()
                log = check / (label + ".log"); log.write_bytes(b"pure fixture: not actual build")
                pins = {}
                for name in f.BINARIES:
                    p = check / label / name; p.write_bytes(b"pure binary fixture: never executable")
                    pins[label + "/" + name] = f.sha(p)
                receipt["commands"].append({"label": label, "exit_code": 0, "features": [] if label == "default-build" else ["qualification-pipe-fault"],
                                           "log_sha256": f.sha(log), "binaries": pins,
                                           "executable": label + "/morrow-native-stream-host.exe",
                                           "executable_sha256": pins[label + "/morrow-native-stream-host.exe"]})
            f.dump(check / "receipt.json", receipt)
            f.validate_check(root, check)
            del receipt["commands"][0]["binaries"]
            f.dump(check / "receipt.json", receipt)
            with self.assertRaises(AssertionError):
                f.validate_check(root, check)
            receipt["commands"][0]["binaries"] = {"default-build/" + n: f.sha(check / "default-build" / n) for n in f.BINARIES}
            f.dump(check / "receipt.json", receipt)
            (check / "default-build/morrow-native-supervisor.exe").write_bytes(b"changed")
            with self.assertRaises(AssertionError):
                f.validate_check(root, check)


if __name__ == "__main__":
    unittest.main()
