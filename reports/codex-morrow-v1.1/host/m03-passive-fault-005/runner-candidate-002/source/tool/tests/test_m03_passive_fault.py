"""Negative evidence/binding checks; these tests start no process or HTTP server."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

PATH = Path(__file__).resolve().parents[1] / "m03_passive_fault_005.py"
loader = importlib.util.spec_from_file_location("passive005", PATH)
m = importlib.util.module_from_spec(loader)
loader.loader.exec_module(m)
IDENTITY = {"session": 7, "epoch": 1, "child_pid": 123, "attempt": 1,
            "operation_id_sha256": "a" * 64, "host_execution_config_sha256": "b" * 64}
PREFIX = (1360).to_bytes(4, "little") + b"test1234"  # Pure evidence stub, not a Cap'n Proto frame.
PREFIX_SHA = hashlib.sha256(PREFIX).hexdigest()


class EvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / "markers.jsonl"
        self.spec = m.spec_for("pipe-partial-close", "c" * 64)
        self.markers = m.Markers(self.path, self.spec, "d" * 64, IDENTITY)

    def record(self, **changes):
        row = {"identity": IDENTITY, "mode": "passive-observe", "scenario": "pipe-partial-close",
               "nonce": "c" * 64, "spec_sha256": "d" * 64, "ordinal": 1,
               "elapsed_ns": 42, "clock_domain": "guest-fixture-monotonic", "kind": "data_partial_frame_observed",
               "detail": {"read_id": 3, "read_issue_count": 2, "transferred_bytes": 8,
                          "buffered_bytes": 12, "declared_payload_bytes": 1360, "expected_frame_bytes": 1364,
                          "prefix_sha256": PREFIX_SHA, "prefix_hex": PREFIX.hex(), "io_index": 3,
                          "read_fragments": [{"io_index": 1, "read_id": 2, "transferred_bytes": 4, "bytes_hex": PREFIX[:4].hex()},
                                             {"io_index": 3, "read_id": 3, "transferred_bytes": 8, "bytes_hex": PREFIX[4:].hex()}]}}
        row.update(changes)
        return row

    def save(self, row):
        raw = json.dumps(row).encode() + b"\n"
        self.path.write_bytes(raw)
        return raw

    def test_foreign_identity_nonce_and_duplicate_ordinals_rejected(self):
        for changes in ({"identity": dict(IDENTITY, child_pid=124)}, {"nonce": "f" * 64}, {"ordinal": True}):
            self.save(self.record(**changes))
            with self.assertRaises(AssertionError):
                self.markers.read()
        raw = self.save(self.record())
        self.path.write_bytes(raw + raw)
        with self.assertRaises(AssertionError):
            self.markers.read()

    def test_rewrite_even_equivalent_json_is_rejected(self):
        row = self.record()
        self.save(row); self.markers.read()
        self.path.write_bytes(json.dumps(row, separators=(",", ":")).encode() + b"\n")
        with self.assertRaisesRegex(AssertionError, "raw evidence rewritten"):
            self.markers.read()

    def test_incomplete_record_never_becomes_close_witness(self):
        raw = self.save(self.record())
        self.path.write_bytes(raw[:-1])
        self.assertIsNone(self.markers.one("data_partial_frame_observed"))
        with self.assertRaisesRegex(AssertionError, "unfinished evidence"):
            self.markers.finish()
        self.path.write_bytes(raw)
        issued = {"original_frame_bytes": 1364, "prefix_sha256": PREFIX_SHA}
        witness = m.partial_witness(self.markers, issued)
        self.assertEqual(witness["marker_sha256"], hashlib.sha256(raw).hexdigest())
        self.assertEqual(witness["read_id"], 3)

    def test_partial_read_chain_is_accumulation_not_one_twelve_byte_read(self):
        self.save(self.record())
        io = [{"action": "os_issue", "kind": "Read", "id": 2, "bytes": 4, "error": None},
              {"action": "os_completion", "kind": "Read", "id": 2, "bytes": 4, "error": None},
              {"action": "os_issue", "kind": "Read", "id": 3, "bytes": 1360, "error": None},
              {"action": "os_completion", "kind": "Read", "id": 3, "bytes": 8, "error": None}]
        result = m.guest_reference_checks(self.markers, {"frames": [], "io": io})
        self.assertEqual([o["bytes"] for o in result["partial_read_chain"]], [4, 8])
        io[-1]["bytes"] = 7
        with self.assertRaises(AssertionError):
            m.guest_reference_checks(self.markers, {"frames": [], "io": io})

    def test_target_and_prefix_mismatch_rejected(self):
        self.save(self.record())
        for issued in ({"original_frame_bytes": 1365, "prefix_sha256": PREFIX_SHA},
                       {"original_frame_bytes": 1364, "prefix_sha256": "f" * 64}):
            with self.assertRaises(AssertionError):
                m.partial_witness(self.markers, issued)

    def test_fragment_substitution_cannot_use_a_matching_summary_hash(self):
        row = self.record()
        row["detail"]["read_fragments"][1]["bytes_hex"] = b"forged!!".hex()
        self.save(row)
        with self.assertRaises(AssertionError):
            m.partial_witness(self.markers, {"original_frame_bytes": 1364, "prefix_sha256": PREFIX_SHA})

    def test_candidate_drift_rejects_before_socket_or_process(self):
        manifest = Path(self.temp.name) / "manifest.json"
        manifest.write_text("{}")
        args = SimpleNamespace(host_manifest=manifest, host_manifest_sha256="0" * 64)
        with patch.object(m.subprocess, "Popen") as popen, patch.object(m.subprocess, "run") as run, patch.object(m.importlib.util, "spec_from_file_location") as importer:
            with self.assertRaisesRegex(AssertionError, "candidate manifest drift"):
                m.run(args)
            popen.assert_not_called(); run.assert_not_called(); importer.assert_not_called()

    def test_clock_and_first_cause_are_not_rewritten(self):
        for nonce in ("0" * 64, "x" * 64, True):
            with self.assertRaises(ValueError):
                m.spec_for("authority-deadline", nonce)
        first = {"event": "http_cancel_applied", "detail": {"code": 19}}
        later = {"event": "http_cancel_applied", "detail": {"code": 20}}
        with self.assertRaisesRegex(AssertionError, "first cause overwritten"):
            m.first_cancellation([first, later])

    def test_deadline_sample_uses_only_original_host_domain(self):
        domain = "host-authority-monotonic:7:1"
        events = [{"event": "authority_deadline_bound", "clock_domain": domain, "at_ns": 123,
                   "detail": {"created_offset_ns": 0, "lifetime_ms": 10000, "original_deadline_offset_ns": 10000000000,
                              "parent_deadline_matches_gate": True, "expiry_not_renewed": True}},
                  {"event": "challenge_deadline_sample", "clock_domain": domain, "at_ns": 456,
                   "detail": {"sample_offset_ns": 1234567, "remaining_ms": 9998, "original_deadline_offset_ns": 10000000000}}]
        self.assertEqual(m.deadline_binding(events, IDENTITY)["deadline_ns"], 10000000000)
        events[1]["clock_domain"] = "guest-fixture-monotonic"
        with self.assertRaises(AssertionError):
            m.deadline_binding(events, IDENTITY)
        events[1]["clock_domain"] = domain
        events[1]["detail"]["remaining_ms"] = 10000
        with self.assertRaises(AssertionError):
            m.deadline_binding(events, IDENTITY)

    def test_late_host_guest_or_runner_pin_failure_cannot_publish_expected(self):
        for stage in ("host", "guest", "runner"):
            result = {"qualification_result": "failed", "retained_actual_error": "Unknown"}
            def failed_pin():
                raise AssertionError(stage + " pin drift")
            with self.assertRaisesRegex(AssertionError, stage + " pin drift"):
                m.checked_outcome(result, "expected_fault_observed", [lambda: None, failed_pin])
            self.assertEqual(result["qualification_result"], "failed")
            self.assertEqual(m.qualification_exit_code(result), 1)
            self.assertEqual(result["retained_actual_error"], "Unknown")

    def test_hash_failure_before_evidence_seal_never_publishes_expected(self):
        run_dir = Path(self.temp.name) / "run"; run_dir.mkdir()
        evidence = Path(self.temp.name) / "guest"; evidence.mkdir()
        (evidence / "raw.json").write_text("{}")
        result = {"qualification_result": "expected_fault_observed", "first_reason": 19}
        with patch.object(m, "sha", side_effect=OSError("unreadable evidence")):
            self.assertFalse(m.seal_result(run_dir, evidence, result))
        published = json.loads((run_dir / "result.json").read_text())
        self.assertEqual(published["qualification_result"], "unconfirmed")
        self.assertEqual(m.qualification_exit_code(published), 1)
        self.assertEqual(published["first_reason"], 19)
        self.assertFalse((run_dir / "evidence-manifest.json").exists())


if __name__ == "__main__":
    unittest.main()
