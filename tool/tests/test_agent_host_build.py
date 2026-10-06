"""Fail-closed tests for the new G0 exporter, independent of compilation."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[1] / "agent_host_build.py"
SPEC = importlib.util.spec_from_file_location("agent_host_build", SCRIPT)
build = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(build)
KIT = SCRIPT.parents[1] / "reports/codex-morrow-v1.1/host/host-kit-003"


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


class KitIntegrity(unittest.TestCase):
    """Self-rehashed manifests must not hide missing qualification material."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="host-kit-integrity-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.kit = self.root / "kit"
        shutil.copytree(KIT, self.kit)

    def edit(self, relative, change):
        path = self.kit / relative
        data = json.loads(path.read_bytes())
        change(data)
        path.write_text(json.dumps(data), encoding="utf-8")

    def rehash(self):
        self.edit("manifest.json", lambda value: value.update(files=build.inventory(self.kit, {"manifest.json"})))

    def test_real_kit_verification_does_not_claim_execution_or_freeze(self):
        result = build.verify_kit(KIT)
        self.assertEqual(result["file_count"], 180)
        self.assertEqual(result["verification_scope"], "qualification_kit_integrity")
        self.assertIs(result["evidence_execution_verified"], False)
        self.assertIs(result["sdk_frozen"], False)

    def test_missing_license_is_rejected_even_after_rehash(self):
        (self.kit / "LICENSE").unlink()
        self.edit("manifest.json", lambda value: value.update(
            source_files=[entry for entry in value["source_files"] if entry["path"] != "LICENSE"]))
        self.rehash()
        with self.assertRaisesRegex(ValueError, "incomplete kit shape"):
            build.verify_kit(self.kit)

    def test_source_inventory_is_checked_independently_of_outer_inventory(self):
        path = self.kit / "src/lib.rs"
        path.write_bytes(path.read_bytes() + b"\n// altered\n")
        self.rehash()
        with self.assertRaisesRegex(ValueError, "kit input mismatch: src/lib.rs"):
            build.verify_kit(self.kit)

    def test_generated_identity_cannot_be_self_rehashed(self):
        (self.kit / "generated/identity.rs").write_text("pub const SCHEMA_DIGEST: [u8; 32] = [0; 32];\n")
        self.edit("generated/manifest.json", lambda value: value.update(
            files=build.inventory(self.kit / "generated", {"manifest.json"})))
        self.rehash()
        with self.assertRaisesRegex(ValueError, "generated schema digest"):
            build.verify_kit(self.kit)

    def test_vector_schema_digest_is_checked_after_outer_rehash(self):
        self.edit("vectors/manifest.json", lambda value: value.update(schema_sha256="0" * 64))
        self.rehash()
        with self.assertRaisesRegex(ValueError, "vector identity"):
            build.verify_kit(self.kit)

    def test_truncated_vector_corpus_is_rejected_after_all_inventories_refresh(self):
        vectors = json.loads((self.kit / "vectors/manifest.json").read_bytes())
        for vector in vectors["vectors"][1:]:
            for direction in ("request", "reply"):
                name = vector[direction]
                (self.kit / "vectors" / name).unlink()
                for suffix in ("json", "stdout.txt", "stderr.txt"):
                    (self.kit / "evidence" / f"decode-{name}.{suffix}").unlink()
        self.edit("vectors/manifest.json", lambda value: value.update(vectors=value["vectors"][:1]))
        self.rehash()
        with self.assertRaisesRegex(ValueError, "vector coverage"):
            build.verify_kit(self.kit)

    def test_failed_or_zero_test_evidence_is_not_qualification(self):
        self.edit("evidence/consumer-tests.json", lambda value: value.update(exit_code=1))
        self.rehash()
        with self.assertRaisesRegex(ValueError, "successful command"):
            build.verify_kit(self.kit)
        self.edit("evidence/consumer-tests.json", lambda value: value.update(exit_code=0))
        (self.kit / "evidence/consumer-tests.stdout.txt").write_text("test result: ok. 0 passed; 0 failed\n")
        self.rehash()
        with self.assertRaisesRegex(ValueError, "meaningful tests"):
            build.verify_kit(self.kit)

    def test_fake_enabled_default_evidence_is_rejected(self):
        path = self.kit / "evidence/default-no-fake.json"
        original = path.read_bytes()
        for options in (["--all-features"], ["--features=qualification"], ["--features", "qualification"]):
            with self.subTest(options=options):
                path.write_bytes(original)
                self.edit("evidence/default-no-fake.json", lambda value: value["args"].extend(options))
                self.rehash()
                with self.assertRaisesRegex(ValueError, "default build evidence"):
                    build.verify_kit(self.kit)

    def test_unrelated_crate_and_nonindependent_target_evidence_are_rejected(self):
        path = self.kit / "evidence/consumer-tests.json"
        original = path.read_bytes()
        for option, replacement, message in (("--manifest-path", "C:\\unrelated\\Cargo.toml", "wrong-manifest"),
                                               ("--target-dir", None, "non-independent target")):
            with self.subTest(option=option):
                path.write_bytes(original)
                def change(value):
                    value["args"][value["args"].index(option) + 1] = replacement or value["cwd"]
                self.edit("evidence/consumer-tests.json", change)
                self.rehash()
                with self.assertRaisesRegex(ValueError, message):
                    build.verify_kit(self.kit)

    def test_decode_evidence_must_use_the_exported_schema_and_frame(self):
        record = "evidence/decode-00-before-hello.request.capnp.json"
        self.edit(record, lambda value: value["args"].__setitem__(3, "OtherRoot"))
        self.rehash()
        with self.assertRaisesRegex(ValueError, "wrong schema or root"):
            build.verify_kit(self.kit)

    def test_individually_valid_commands_must_share_one_kit_and_target(self):
        consumer = "evidence/consumer-tests.json"
        def unrelated(value):
            value["cwd"] = "C:\\unrelated"
            value["args"][value["args"].index("--manifest-path") + 1] = "C:\\unrelated\\Cargo.toml"
            value["args"][value["args"].index("--target-dir") + 1] = "C:\\unrelated-target"
        self.edit(consumer, unrelated)
        self.rehash()
        with self.assertRaisesRegex(ValueError, "different kit"):
            build.verify_kit(self.kit)
        shutil.copyfile(KIT / consumer, self.kit / consumer)
        self.edit("evidence/default-no-fake.json", lambda value: value["args"].__setitem__(
            value["args"].index("--target-dir") + 1, "C:\\unrelated-target"))
        self.rehash()
        with self.assertRaisesRegex(ValueError, "different target"):
            build.verify_kit(self.kit)

    def test_family_scope_cannot_promote_drafts(self):
        self.edit("manifest.json", lambda value: value["implemented_families"].append("agent-content-v1"))
        with self.assertRaisesRegex(ValueError, "family classification"):
            build.verify_kit(self.kit)

    def test_duplicate_inventory_and_boolean_size_are_rejected(self):
        self.edit("manifest.json", lambda value: value["files"].append(value["files"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate file"):
            build.verify_kit(self.kit)
        self.edit("manifest.json", lambda value: value["files"].pop())
        self.edit("manifest.json", lambda value: value["files"][0].update(bytes=True))
        with self.assertRaisesRegex(ValueError, "invalid file inventory"):
            build.verify_kit(self.kit)

    def test_duplicate_json_keys_are_rejected(self):
        (self.kit / "manifest.json").write_text('{"status":"failed","status":"complete"}')
        with self.assertRaisesRegex(ValueError, "duplicate JSON"):
            build.verify_kit(self.kit)

    def test_nonregular_schema_is_rejected_before_reading(self):
        schema = self.kit / "agent_host.capnp"
        schema.unlink()
        if not hasattr(os, "mkfifo"):
            self.skipTest("FIFO fixture requires POSIX")
        os.mkfifo(schema)
        with mock.patch.object(build, "sha", side_effect=AssertionError("must not read FIFO")):
            with self.assertRaisesRegex(ValueError, "nonregular schema"):
                build.verify_kit(self.kit)

    def test_root_and_leaf_links_and_unsafe_paths_are_rejected(self):
        linked = self.root / "linked"
        linked.symlink_to(self.kit, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "linked"):
            build.verify_kit(linked)
        leaf = self.kit / "src/lib.rs"
        target = self.root / "lib.rs"
        shutil.copyfile(leaf, target)
        leaf.unlink()
        leaf.symlink_to(target)
        with self.assertRaisesRegex(ValueError, "linked"):
            build.verify_kit(self.kit)
        for relative in ("../outside", "C:/file", "src\\lib.rs", "/file", "src//lib.rs"):
            with self.subTest(relative=relative), self.assertRaises(ValueError):
                build.safe_child(self.kit, relative)


if __name__ == "__main__":
    unittest.main()
