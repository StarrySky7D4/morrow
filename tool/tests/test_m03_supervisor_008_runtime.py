"""Pure evidence-validator tests. These never start native processes or scenarios."""
from pathlib import Path
import copy
import hashlib
import importlib.util
import json
import tempfile
import unittest
from unittest.mock import patch

RUNNER = Path(__file__).resolve().parents[1] / "m03_supervisor_008_runtime.py"
if not RUNNER.exists():
    RUNNER = Path(__file__).with_name("supervisor_008_draft.py")
spec = importlib.util.spec_from_file_location("supervisor008_runtime", RUNNER)
runtime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runtime)


def final_fixture():
    claim = {"pid": 123, "session": 456, "epoch": 7}
    incarnation, grant = "ab" * 32, "cd" * 32
    flags = {k: True for k in ("revoke_persisted", "revoke_applied", "request_closed", "data_closed",
                              "connect_reaped", "read_reaped", "write_reaped", "owner_released",
                              "child_exited", "stdout_eof", "stderr_eof")}
    flags.update(worker_started=False, worker_joined=False, received_offset=0, issued_offset=0)
    events = [
        {"event": "job_bound", "ordinal": 1, "detail": {"child_pid": 123, "incarnation": incarnation,
         "handle_noninherited": True, "suspended_attach_before_execution": True,
         "atomic_job_at_creation": True, "limit_flags": 0x2000}},
        {"event": "authority_deadline_bound", "ordinal": 2, "detail": {"expiry_not_renewed": True}},
        {"event": "http_cancel_applied", "ordinal": 3, "detail": {"effect_fence": {"gate_closed": True}}},
        {"event": "job_empty", "ordinal": 4, "detail": {"active_processes": 0, "direct_exit": True,
         "stdout_eof": True, "stderr_eof": True}},
        {"event": "phase", "ordinal": 5, "detail": "Released"},
        {"event": "control_stdin_closed_reaped", "ordinal": 3, "detail": True},
    ]
    common = {"phase": "Released", "owner_retained": False, "exit_observed": True,
              "stdout_eof": True, "stderr_eof": True, **claim}
    snapshot = {**common, "events": events, "event_overflow": 0, "http": {"progress": flags}}
    owner = {**common, "grant_id": grant, "generation": 7, "automatic_takeover": False}
    recovery = {"phase": "Reclaimed", "grant_id": grant, "generation": 7, "profile": "ef" * 32,
                "supervisor_pid": 789, "incarnation": incarnation, "child_pid": 123,
                "session": 456, "epoch": 7, "snapshot_sha256": "12" * 32, "business_replay_allowed": False,
                **{k: True for k in ("tree_empty", "child_exit", "stdout_eof", "stderr_eof", "pipe_joined",
                                    "network_joined", "gate_closed")}}
    final = {"snapshot": snapshot, "state": {"owner": owner, "recovery": recovery,
                                             "profile_id": "ef" * 32}, "business_success_claimed": False}
    return final, grant, claim


def envelope(raw, compressed=None):
    if compressed is None:
        if len(raw) < 15:
            compressed = bytes([len(raw) << 4]) + raw
        else:
            remaining = len(raw) - 15
            extension = bytearray()
            while remaining >= 255:
                extension.append(255)
                remaining -= 255
            extension.append(remaining)
            compressed = b"\xf0" + bytes(extension) + raw
    return (b"MRNADM04\x01\x00" + len(raw).to_bytes(4, "little") + len(compressed).to_bytes(4, "little")
            + hashlib.sha256(raw).digest() + compressed).hex()


class ReclamationEvidenceTests(unittest.TestCase):
    def check(self, value):
        final, grant, claim = value
        return runtime.assert_reclaimed(final, grant, claim, 789)

    def test_complete_proof_is_accepted(self):
        self.check(final_fixture())

    def test_each_durable_recovery_resource_flag_is_required(self):
        for flag in ("tree_empty", "child_exit", "stdout_eof", "stderr_eof", "pipe_joined", "network_joined", "gate_closed"):
            with self.subTest(flag=flag):
                value = final_fixture()
                value[0]["state"]["recovery"][flag] = False
                with self.assertRaisesRegex(AssertionError, "Recovery proof lacks"):
                    self.check(value)

    def test_each_identity_binding_rejects_substitution(self):
        for key, replacement in (("grant_id", "00" * 32), ("generation", 8), ("child_pid", 124),
                                 ("session", 457), ("epoch", 8), ("supervisor_pid", 790),
                                 ("profile", "00" * 32), ("incarnation", "00" * 32)):
            with self.subTest(key=key):
                value = final_fixture()
                value[0]["state"]["recovery"][key] = replacement
                with self.assertRaises(AssertionError):
                    self.check(value)

    def test_tree_nonempty_and_release_before_job_empty_are_rejected(self):
        value = final_fixture()
        value[0]["snapshot"]["events"][3]["detail"]["active_processes"] = 1
        with self.assertRaisesRegex(AssertionError, "Job empty"):
            self.check(value)
        value = final_fixture()
        value[0]["snapshot"]["events"][3]["ordinal"] = 6
        with self.assertRaisesRegex(AssertionError, "release precedes"):
            self.check(value)

    def test_job_breakaway_and_unsuspended_launch_are_rejected(self):
        for field, replacement in (("limit_flags", 0x2800), ("handle_noninherited", False),
                                   ("suspended_attach_before_execution", False), ("atomic_job_at_creation", False)):
            value = final_fixture()
            value[0]["snapshot"]["events"][0]["detail"][field] = replacement
            with self.subTest(field=field), self.assertRaises(AssertionError):
                self.check(value)

    def test_http_worker_or_data_cannot_pass_control_only_proof(self):
        value = final_fixture()
        value[0]["snapshot"]["http"]["progress"].update(worker_started=True, worker_joined=True)
        with self.assertRaisesRegex(AssertionError, "HTTP worker"):
            self.check(value)
        value = final_fixture()
        value[0]["snapshot"]["http"]["progress"]["received_offset"] = 1
        with self.assertRaisesRegex(AssertionError, "HTTP bytes"):
            self.check(value)

    def test_kernel_exit_is_not_durable_reclamation(self):
        value = final_fixture()
        value[0]["state"]["recovery"]["phase"] = "Registered"
        with self.assertRaisesRegex(AssertionError, "not Reclaimed"):
            self.check(value)

    def test_control_stdin_reap_must_be_true_and_precede_release(self):
        for missing in (True, False):
            value = final_fixture()
            if missing:
                value[0]["snapshot"]["events"] = [e for e in value[0]["snapshot"]["events"]
                                                   if e["event"] != "control_stdin_closed_reaped"]
            else:
                value[0]["snapshot"]["events"][-1]["detail"] = False
            with self.subTest(missing=missing), self.assertRaisesRegex(AssertionError, "completion reap proof"):
                self.check(value)
        value = final_fixture()
        value[0]["snapshot"]["events"][-1]["ordinal"] = 6
        with self.assertRaisesRegex(AssertionError, "release precedes control stdin"):
            self.check(value)


class RawLedgerDecoderTests(unittest.TestCase):
    def test_literal_record_and_match_backreference(self):
        self.assertEqual(runtime.unpack_record(envelope(b"\x08\x01\x12\x03abc")), {1: 1, 2: b"abc"})
        # tag2/length8 + literal abcd, followed by offset4 match4, then final literal tag3/1.
        raw = b"\x12\x08abcdabcd\x18\x01"
        compressed = b"\x60\x12\x08abcd\x04\x00\x20\x18\x01"
        self.assertEqual(runtime.unpack_record(envelope(raw, compressed)), {2: b"abcdabcd", 3: 1})

    def test_long_literal_extension_is_bounded(self):
        raw = b"\x12\xac\x02" + b"x" * 300
        self.assertEqual(runtime.unpack_record(envelope(raw))[2], b"x" * 300)

    def test_truncation_digest_tamper_and_zero_offset_reject(self):
        valid = bytes.fromhex(envelope(b"\x08\x01"))
        damaged = bytearray(valid)
        damaged[18] ^= 1
        for blob in (valid[:17], bytes(damaged), bytes.fromhex(envelope(b"\x08\x01", b"\x00\x00\x00"))):
            with self.subTest(blob=blob.hex()), self.assertRaises(AssertionError):
                runtime.unpack_record(blob.hex())

    def test_duplicate_protobuf_fields_reject(self):
        with self.assertRaisesRegex(AssertionError, "duplicated"):
            runtime.unpack_record(envelope(b"\x08\x01\x08\x01"))


class CandidateGatesTests(unittest.TestCase):
    def test_failed_check_cannot_create_candidate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            check = root / "check"
            check.mkdir()
            out = root / "attempt"
            runtime.dump(check / "receipt.json", {"source_unchanged": True,
                                                  "commands": [{"exit_code": 2}]})
            with self.assertRaisesRegex(AssertionError, "command failed"):
                runtime.snapshot_candidate(root, check, out)
            self.assertFalse(out.exists())

    def test_new_sources_after_check_are_rejected_before_snapshot(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            check = root / "check"
            check.mkdir()
            sources = {p: "a" * 64 for p in runtime.REQUIRED_SOURCES}
            runtime.dump(check / "receipt.json", {"source_unchanged": True,
                                                  "commands": [{"exit_code": 0}],
                                                  "sources_before": sources, "sources_after": sources})
            with patch.object(runtime, "source_inventory", return_value={**sources, "new.rs": "b" * 64}):
                with self.assertRaisesRegex(AssertionError, "current source set/hashes"):
                    runtime.snapshot_candidate(root, check, root / "attempt")
            self.assertFalse((root / "attempt").exists())

    def binary_check(self, root):
        check = root / "check"
        (check / "default-build").mkdir(parents=True)
        pins = {}
        for name in runtime.BINARIES:
            path = check / "default-build" / name
            path.write_bytes(("checked binary " + name).encode())
            pins["default-build/" + name] = runtime.sha(path)
        commands = [{"label": "default-build", "exit_code": 0, "binaries": pins}]
        return check, commands

    def test_all_three_checked_binary_hashes_are_required(self):
        with tempfile.TemporaryDirectory() as directory:
            check, commands = self.binary_check(Path(directory))
            self.assertEqual(runtime.checked_binary_pins(check, commands), commands[0]["binaries"])
            weak = [{"label": "default-build", "exit_code": 0,
                     "executable": "default-build/morrow-native-stream-host.exe"}]
            with self.assertRaisesRegex(AssertionError, "weak checks rejected"):
                runtime.checked_binary_pins(check, weak)

    def test_supervisor_and_peer_hash_substitution_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            check, commands = self.binary_check(Path(directory))
            for name in runtime.BINARIES[:2]:
                changed = copy.deepcopy(commands)
                changed[0]["binaries"]["default-build/" + name] = "00" * 32
                with self.subTest(name=name), self.assertRaisesRegex(AssertionError, "SHA mismatch"):
                    runtime.checked_binary_pins(check, changed)

    def test_supervisor_and_peer_missing_hash_evidence_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            check, commands = self.binary_check(Path(directory))
            for name in runtime.BINARIES[:2]:
                changed = copy.deepcopy(commands)
                del changed[0]["binaries"]["default-build/" + name]
                with self.subTest(name=name), self.assertRaisesRegex(AssertionError, "SHA missing/invalid"):
                    runtime.checked_binary_pins(check, changed)


class LiveOwnerRefusalTests(unittest.TestCase):
    def evidence(self):
        final, grant, claim = final_fixture()
        before = copy.deepcopy(final["state"])
        before["owner"].update(phase="Active", owner_retained=True, locally_observed=True)
        before["recovery"].update(phase="Retained")
        after = copy.deepcopy(before)
        after["owner"]["locally_observed"] = False
        raw = {"native-admissions.sqlite": {"tables": {
            "owner": {"rows": [[1, {"hex": "01"}]]},
            "recovery": {"rows": [[1, {"hex": "02"}]]},
        }}}
        refusal = {"ok": False, "error": "owner busy or unavailable: StorageBusy"}
        return refusal, before, after, raw, copy.deepcopy(raw), [], False

    def test_storage_busy_without_mutation_or_guest_is_accepted(self):
        runtime.assert_live_owner_refusal(*self.evidence())

    def test_generic_exit_error_is_not_live_owner_refusal(self):
        evidence = list(self.evidence())
        evidence[0] = {"ok": False, "error": "unexpected failure"}
        with self.assertRaisesRegex(AssertionError, "StorageBusy/retained"):
            runtime.assert_live_owner_refusal(*evidence)

    def test_raw_owner_mutation_and_spawn_are_rejected(self):
        evidence = list(self.evidence())
        evidence[4]["native-admissions.sqlite"]["tables"]["owner"]["rows"][0][-1]["hex"] = "ff"
        with self.assertRaisesRegex(AssertionError, "raw old owner"):
            runtime.assert_live_owner_refusal(*evidence)
        evidence = list(self.evidence())
        evidence[5] = [{"event": "spawn", "detail": {"pid": 999}}]
        with self.assertRaisesRegex(AssertionError, "extra guest"):
            runtime.assert_live_owner_refusal(*evidence)


class NoCloseStopReceiptTests(unittest.TestCase):
    def evidence(self):
        final, grant, claim = final_fixture()
        raw_hex = (b"\x08\x00\x00\x00" + b"\x00" * 8).hex()
        final["expired_terminal_protocol_complete"] = False
        final["snapshot"]["exit_code"] = 1
        final["snapshot"]["http"]["progress"]["error_code"] = 20
        final["snapshot"]["events"] += [
            {"event": "control_frame_sent", "detail": {"raw_hex": raw_hex}},
            {"event": "control_frame_received", "detail": {"raw_hex": "initial-Hello-fixture"}},
            {"event": "session_close_reason", "detail": {"reason": 20}},
            {"event": "close_ack_failed", "detail": {"reason": "fixed teardown deadline"}},
            {"event": "job_terminate_requested", "detail": {"reason": 20}},
            {"event": "kill_requested", "detail": {"ok": True}},
            {"event": "exit", "detail": {"code": 1}},
        ]
        receipt = {"mode": "expiry-no-close", "phase": "stop-observed-no-close", "child_pid": claim["pid"],
                   "session": claim["session"], "epoch": claim["epoch"], "stop_received": True,
                   "stop_code": 20, "no_close_submitted": True, "http_not_invoked": True,
                   "ack_received": False, "terminal_protocol_complete": False, "stop_raw_hex": raw_hex}
        return final, claim, receipt

    def test_bound_stop_receipt_and_actual_job_negative_are_accepted(self):
        runtime.assert_expiry_no_close_receipt(*self.evidence())

    def test_each_stop_receipt_identity_substitution_is_rejected(self):
        for field in ("child_pid", "session", "epoch"):
            evidence = self.evidence()
            evidence[2][field] += 1
            with self.subTest(field=field), self.assertRaisesRegex(AssertionError, "binding mismatch"):
                runtime.assert_expiry_no_close_receipt(*evidence)

    def test_stop_raw_mismatch_and_terminal_ack_are_rejected(self):
        evidence = self.evidence()
        evidence[2]["stop_raw_hex"] = (b"\x08\x00\x00\x00" + b"\x01" * 8).hex()
        with self.assertRaisesRegex(AssertionError, "actual host-sent raw"):
            runtime.assert_expiry_no_close_receipt(*evidence)
        evidence = self.evidence()
        evidence[0]["snapshot"]["events"].append({"event": "close_ack_written", "detail": {}})
        with self.assertRaisesRegex(AssertionError, "Close/ACK"):
            runtime.assert_expiry_no_close_receipt(*evidence)

    def test_exit_two_alone_without_actual_job_kill_is_rejected(self):
        evidence = self.evidence()
        evidence[0]["snapshot"]["events"] = [e for e in evidence[0]["snapshot"]["events"]
                                              if e["event"] != "job_terminate_requested"]
        with self.assertRaisesRegex(AssertionError, "successful Job termination"):
            runtime.assert_expiry_no_close_receipt(*evidence)

    def test_empty_and_truncated_final_diagnostic_preserve_raw_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "peer.json"
            for raw in (b"", b'{"mode": "expiry-no-close"'):
                path.write_bytes(raw)
                fixture, info = runtime.killed_fixture_diagnostic(path)
                self.assertIsNone(fixture)
                self.assertFalse(info["final_fixture_result_completed"])
                self.assertEqual(info["raw_sha256"], hashlib.sha256(raw).hexdigest())
                self.assertEqual((path.parent / info["preserved_raw_file"]).read_bytes(), raw)

    def test_parseable_contradictory_final_diagnostic_is_not_ignored(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "peer.json"
            fixture = {"mode": "expiry-no-close", "http_not_invoked": True,
                       "ack_received": False, "stop_received": True}
            runtime.dump(path, fixture)
            parsed, info = runtime.killed_fixture_diagnostic(path)
            self.assertEqual(parsed, fixture)
            self.assertTrue(info["complete_json"])
            fixture["ack_received"] = True
            runtime.dump(path, fixture)
            with self.assertRaisesRegex(AssertionError, "contradicts"):
                runtime.killed_fixture_diagnostic(path)


if __name__ == "__main__":
    unittest.main()
