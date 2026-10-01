"""Checker boundary tests; synthetic data is never product qualification evidence."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import stat
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ENTRY = Path(__file__).parent / "m03_g0_graph_011_check.py"
if not ENTRY.exists():
    ENTRY = Path(__file__).parents[1] / "m03_g0_graph_011_check.py"
spec = importlib.util.spec_from_file_location("g0_graph_checker", ENTRY)
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class GraphBoundaryTests(unittest.TestCase):
    def test_missing_graph_is_reported_for_both_targets(self):
        with tempfile.TemporaryDirectory() as directory:
            rows = checker.contract_audit(Path(directory), {})
        self.assertEqual([row["graph"] for row in rows], ["native", "wasm"])
        self.assertTrue(all("graph_not_implemented" in row["problems"] for row in rows))
        self.assertTrue(all(row["product_build_verified"] is False for row in rows))

    def test_graphs_cannot_share_inputs_or_nested_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("Cargo.toml", "Cargo.lock"):
                (root / name).write_text("fixed", encoding="utf-8")
            graph = {"status": "implemented", "manifest": "Cargo.toml", "lock": "Cargo.lock",
                     "manifest_sha256": checker.sha(root / "Cargo.toml"),
                     "lock_sha256": checker.sha(root / "Cargo.lock"), "target_dir": "out/native"}
            rows = checker.contract_audit(root, {"graphs": {"native": dict(graph, target=checker.NATIVE),
                "wasm": dict(graph, target=checker.WASM, target_dir="out/native/wasm")}})
        self.assertIn("graph_input_shared", rows[1]["problems"])
        self.assertIn("graph_output_shared", rows[1]["problems"])

    def test_tampered_digest_is_blocked(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "manifest").write_text("changed", encoding="utf-8")
            rows = checker.contract_audit(root, {"graphs": {"native": {
                "status": "implemented", "target": checker.NATIVE, "manifest": "manifest",
                "manifest_sha256": "0" * 64, "target_dir": "out"}}})
        self.assertIn("manifest_digest_unbound", rows[0]["problems"])

    def test_vendor_checksum_mismatch_and_missing_are_distinct(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lock = root / "Cargo.lock"
            lock.write_text('[[package]]\nname="a"\nversion="1"\nsource="registry+x"\nchecksum="expected"\n'
                            '[[package]]\nname="b"\nversion="2"\nsource="registry+x"\nchecksum="other"\n', encoding="utf-8")
            (root / "vendor/a-1").mkdir(parents=True)
            (root / "vendor/a-1/Cargo.toml").write_text("", encoding="utf-8")
            (root / "vendor/a-1/.cargo-checksum.json").write_text('{"package":"tampered"}', encoding="utf-8")
            result = checker.lock_audit(lock, root / "vendor")
        self.assertEqual([row["name"] for row in result["vendor_missing"]], ["b"])
        self.assertEqual([row["name"] for row in result["vendor_checksum_mismatches"]], ["a"])

    def test_metadata_failure_is_preserved_with_source_lock_unchanged(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "Cargo.toml"
            manifest.write_text("[workspace]\n", encoding="utf-8")
            (root / "Cargo.lock").write_text("version=4\n", encoding="utf-8")
            failed = subprocess.CompletedProcess([], 101, b"", b"missing pinned Git dependency in offline mode")
            with patch.object(checker.subprocess, "run", return_value=failed) as invoked:
                result = checker.cargo_metadata(manifest, root / "run", root / "vendor", root / "toolchain")
            self.assertEqual(result["exit_code"], 101)
            self.assertTrue(result["inputs_unchanged"])
            self.assertFalse(result["product_build_verified"])
            self.assertEqual((root / "run/stderr.txt").read_bytes(), failed.stderr)
            args = invoked.call_args
            self.assertIn("--offline", args.args[0])
            self.assertIn("--locked", args.args[0])
            self.assertEqual(args.kwargs["shell"], False)
            self.assertTrue(Path(args.kwargs["env"]["CARGO_HOME"]).is_relative_to(root / "run"))

    def test_existing_output_is_never_reused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            companion, output = root / "companion", root / "evidence"
            companion.mkdir()
            output.mkdir()
            marker = output / "keep"
            marker.write_text("unchanged", encoding="utf-8")
            with self.assertRaises(FileExistsError):
                checker.audit(companion, output)
            self.assertEqual(marker.read_text("utf-8"), "unchanged")

    def test_personal_provider_cargo_and_git_environment_are_not_inherited(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with patch.dict(os.environ, {"OPENAI_API_KEY": "test-secret", "CARGO_HOME": "personal",
                                         "GIT_CONFIG_COUNT": "1", "RUSTC_WRAPPER": "arbitrary"}):
                env = checker.isolated_environment(root, root / "toolchain")
            self.assertNotIn("OPENAI_API_KEY", env)
            self.assertNotIn("GIT_CONFIG_COUNT", env)
            self.assertNotIn("RUSTC_WRAPPER", env)
            self.assertNotEqual(env["CARGO_HOME"], "personal")

    def test_companion_output_and_relative_escape_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaises(ValueError):
                checker.audit(root, root / "out")
            with self.assertRaises(ValueError):
                checker.child(root, "../escape")

    def test_source_leaf_link_binds_text_without_reading_target(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            leaf = root / "LICENSE"
            leaf.write_text("stand-in directory entry", encoding="utf-8")
            original_lstat = Path.lstat
            def inspected(path, *args, **kwargs):
                return SimpleNamespace(st_mode=stat.S_IFLNK) if path == leaf else original_lstat(path, *args, **kwargs)
            with patch.object(Path, "lstat", autospec=True, side_effect=inspected), \
                 patch.object(checker.os, "readlink", return_value="missing-target"):
                self.assertEqual(checker.inventory(root), {"LICENSE": {"kind": "symlink", "target": "missing-target"}})

    def test_invalid_fresh_input_saves_blocked_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            companion = root / "companion"
            companion.mkdir()
            output = root / "evidence"
            self.assertEqual(checker.main(["--companion", str(companion), "--output", str(output)]), 2)
            report = json.loads((output / "result.json").read_text("utf-8"))
            self.assertEqual(report["status"], "blocked")
            self.assertEqual(report["exit_code"], 2)
            self.assertIn("error", report)


if __name__ == "__main__":
    unittest.main()
