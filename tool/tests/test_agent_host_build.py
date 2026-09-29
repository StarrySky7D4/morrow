"""Fail-closed tests for the new G0 exporter, independent of compilation."""
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[1] / "agent_host_build.py"
SPEC = importlib.util.spec_from_file_location("agent_host_build", SCRIPT)
build = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(build)


class ExportSafety(unittest.TestCase):
    def test_missing_cli_input_is_nonzero(self):
        for command in [[], ["export-sdk"], ["preflight"], ["verify-kit"]]:
            result = subprocess.run([sys.executable, str(SCRIPT), *command], capture_output=True)
            self.assertNotEqual(result.returncode, 0)

    def test_existing_output_is_rejected_without_touching_success(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output = root / "old"
            output.mkdir()
            marker = output / "manifest.json"
            marker.write_text("old-success", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "already exists"):
                build.export_sdk(root / "missing-baseline", output, root / "target")
            self.assertEqual(marker.read_text(encoding="utf-8"), "old-success")

    def test_missing_baseline_never_creates_success(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with self.assertRaises(FileNotFoundError):
                build.export_sdk(root / "missing", root / "new", root / "target")
            self.assertFalse((root / "new").exists())

    def test_changed_or_missing_legacy_input_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "old.capnp"
            source.write_bytes(b"old-original")
            baseline = root / "baseline.json"
            baseline.write_text(json.dumps({"head": build.EXPECTED_HEAD, "branch": "codex/io-safety-refactor",
                "legacy_inputs": [{"path": "old.capnp", "bytes": 12,
                "raw_sha256": hashlib.sha256(b"old-original").hexdigest()}]}), encoding="utf-8")
            self.assertEqual(len(build.verify_inputs(root, baseline)["legacy_inputs"]), 1)
            source.write_bytes(b"new-original")
            with self.assertRaisesRegex(ValueError, "changed legacy"):
                build.verify_inputs(root, baseline)
            source.unlink()
            with self.assertRaises(FileNotFoundError):
                build.verify_inputs(root, baseline)

    def test_partial_kit_and_manifest_path_escape_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with self.assertRaises(FileNotFoundError):
                build.verify_kit(root)
            with self.assertRaises(ValueError):
                build.safe_child(root, "../elsewhere")
            build.write_json(root / "manifest.json", {"status": "failed"})
            with self.assertRaisesRegex(ValueError, "completed"):
                build.verify_kit(root)

    def test_failed_build_keeps_evidence_without_success_and_cannot_reuse_candidate(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "source"
            source.mkdir()
            (source / "agent_host.capnp").write_bytes(b"fixture-schema")
            baseline = root / "baseline.json"
            baseline.write_text("{}", encoding="utf-8")
            output = root / "failed-kit"
            with mock.patch.object(build, "SOURCE", source), mock.patch.object(build, "preflight", return_value={}), \
                    mock.patch.object(build, "execute", side_effect=RuntimeError("simulated compiler failure")):
                with self.assertRaisesRegex(RuntimeError, "compiler failure"):
                    build.export_sdk(baseline, output, root / "target")
                self.assertFalse((output / "manifest.json").exists())
                self.assertEqual(json.loads((output / "failure.json").read_text(encoding="utf-8"))["status"], "failed")
                with self.assertRaisesRegex(ValueError, "already exists"):
                    build.export_sdk(baseline, output, root / "target")


if __name__ == "__main__":
    unittest.main()
