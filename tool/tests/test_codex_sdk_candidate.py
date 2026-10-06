"""Current-host candidate gates use disposable copies of preserved Codex kits."""
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "tool/verify_codex_sdk_candidate.py"
SPEC = importlib.util.spec_from_file_location("codex_sdk_candidate", SCRIPT)
candidate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(candidate)
KIT = ROOT / "companions/morrow-codex/sdk/host-kit-003-copy"
REVIEW = ROOT / "companions/morrow-codex/receipts/host-kit-review-003/review.json"


class CodexCandidateTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="codex-sdk-candidate-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.kit = self.root / "candidate with spaces"
        shutil.copytree(KIT, self.kit)
        self.review = self.root / "consumer review.json"
        shutil.copyfile(REVIEW, self.review)

    def verify(self, **arguments):
        return candidate.verify_candidate(self.kit, self.review, **arguments)

    def edit_review(self, **changes):
        document = json.loads(self.review.read_bytes())
        document.update(changes)
        self.review.write_text(json.dumps(document), encoding="utf-8")

    def snapshot(self, root):
        return {p.relative_to(root).as_posix(): p.read_bytes()
                for p in root.rglob("*") if p.is_file()}

    def canonical_copy(self):
        root = self.root / "canonical"
        shutil.copytree(candidate.CANONICAL, root)
        return root

    def rebind_manifest_and_review(self, changed_leaf):
        path = self.kit / changed_leaf
        manifest_path = self.kit / "manifest.json"
        manifest = json.loads(manifest_path.read_bytes())
        for inventory in ("source_files", "files"):
            for entry in manifest[inventory]:
                if entry["path"] == changed_leaf:
                    entry.update(bytes=path.stat().st_size, sha256=hashlib.sha256(path.read_bytes()).hexdigest())
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        self.edit_review(manifest_sha256=hashlib.sha256(manifest_path.read_bytes()).hexdigest())

    def test_real_archived_kit_and_review_match_current_host_without_mutation(self):
        before_kit = self.snapshot(KIT)
        before_review = REVIEW.read_bytes()
        result = candidate.verify_candidate(KIT, REVIEW)
        self.assertEqual(result["status"], "qualification_only")
        self.assertEqual(result["files_verified"], 180)
        self.assertEqual(result["source_files_verified"], 12)
        self.assertEqual((result["wire_major"], result["wire_revision"]), (1, 1))
        self.assertEqual(result["sdk_freeze"], "OPEN")
        self.assertEqual(result["p02_qualification"], "NOT_RUN")
        self.assertIs(result["production_binding_available"], False)
        self.assertEqual(self.snapshot(KIT), before_kit)
        self.assertEqual(REVIEW.read_bytes(), before_review)

    def test_review_cannot_bypass_modified_missing_or_extra_leaves(self):
        for change in ("modified", "missing", "extra"):
            with self.subTest(change=change):
                leaf = self.kit / "src/lib.rs"
                saved = leaf.read_bytes()
                if change == "modified":
                    leaf.write_bytes(saved + b"\n// changed after review\n")
                elif change == "missing":
                    leaf.unlink()
                else:
                    (self.kit / "unreviewed.txt").write_bytes(b"extra")
                with self.assertRaises((ValueError, OSError)):
                    self.verify()
                leaf.write_bytes(saved)
                (self.kit / "unreviewed.txt").unlink(missing_ok=True)

    def test_review_manifest_and_schema_drift_fail_without_repair(self):
        for field in ("manifest_sha256", "schema_sha256"):
            with self.subTest(field=field):
                shutil.copyfile(REVIEW, self.review)
                self.edit_review(**{field: "0" * 64})
                before = self.snapshot(self.root)
                with self.assertRaisesRegex(candidate.CandidateError, "digest mismatch"):
                    self.verify()
                self.assertEqual(self.snapshot(self.root), before)

    def test_ready_or_stable_review_does_not_expand_qualification(self):
        for status in ("ready", "stable", None, True):
            with self.subTest(status=status):
                self.edit_review(status=status)
                with self.assertRaisesRegex(candidate.CandidateError, "qualification_only"):
                    self.verify()

    def test_review_version_requires_exact_integer(self):
        for version in (True, "1", 2, None):
            with self.subTest(version=version):
                self.edit_review(schema_version=version)
                with self.assertRaisesRegex(candidate.CandidateError, "review schema"):
                    self.verify()

    def test_canonical_raw_schema_and_api_version_drift_are_not_refreshed(self):
        canonical = self.canonical_copy()
        schema = canonical / "agent_host.capnp"
        original = schema.read_bytes()
        schema.write_bytes(original + b"\n# unreviewed canonical change\n")
        with self.assertRaisesRegex(candidate.CandidateError, "canonical schema"):
            self.verify(canonical=canonical)
        schema.write_bytes(original)
        api = canonical / "src/lib.rs"
        api.write_bytes(api.read_bytes().replace(b"pub const REVISION: u16 = 1;", b"pub const REVISION: u16 = 2;"))
        with self.assertRaisesRegex(candidate.CandidateError, "wire version"):
            self.verify(canonical=canonical)

    def test_rehashed_schema_and_review_still_fail_current_host_identity(self):
        schema = self.kit / "agent_host.capnp"
        schema.write_bytes(schema.read_bytes() + b"\n# changed candidate\n")
        digest = hashlib.sha256(schema.read_bytes()).hexdigest()
        manifest_path = self.kit / "manifest.json"
        manifest = json.loads(manifest_path.read_bytes())
        manifest["schema_sha256"] = digest
        for entry in manifest["files"]:
            if entry["path"] == "agent_host.capnp":
                entry.update(bytes=schema.stat().st_size, sha256=digest)
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        self.edit_review(manifest_sha256=hashlib.sha256(manifest_path.read_bytes()).hexdigest(), schema_sha256=digest)
        with self.assertRaisesRegex(candidate.CandidateError, "canonical schema"):
            self.verify()

    def test_rehashed_api_or_manifest_source_cannot_replace_current_host_source(self):
        for name in ("src/lib.rs", "Cargo.toml"):
            with self.subTest(name=name):
                path = self.kit / name
                saved_leaf = path.read_bytes()
                saved_manifest = (self.kit / "manifest.json").read_bytes()
                saved_review = self.review.read_bytes()
                path.write_bytes(saved_leaf + b"\n# modified distribution source\n")
                self.rebind_manifest_and_review(name)
                with self.assertRaisesRegex(ValueError, "kit input mismatch"):
                    self.verify()
                path.write_bytes(saved_leaf)
                (self.kit / "manifest.json").write_bytes(saved_manifest)
                self.review.write_bytes(saved_review)

    def test_missing_extra_or_changed_canonical_source_is_rejected(self):
        canonical = self.canonical_copy()
        path = canonical / "Cargo.toml"
        original = path.read_bytes()
        path.unlink()
        with self.assertRaises(OSError):
            self.verify(canonical=canonical)
        path.write_bytes(original + b"\n# canonical source drift\n")
        with self.assertRaisesRegex(ValueError, "kit input mismatch"):
            self.verify(canonical=canonical)
        path.write_bytes(original)
        (canonical / "additional-source.rs").write_bytes(b"pub struct Additional;\n")
        with self.assertRaisesRegex(candidate.CandidateError, "source inventory mismatch"):
            self.verify(canonical=canonical)

    def test_build_outputs_in_canonical_tree_are_an_error(self):
        canonical = self.canonical_copy()
        target = canonical / "target"
        target.mkdir()
        with self.assertRaisesRegex(candidate.CandidateError, "canonical build output"):
            self.verify(canonical=canonical)
        (target / "compiled-artifact").write_bytes(b"output is not source")
        with self.assertRaisesRegex(candidate.CandidateError, "canonical build output"):
            self.verify(canonical=canonical)

    def test_canonical_source_change_during_leaf_check_aborts(self):
        canonical = self.canonical_copy()
        original = candidate.host_build.verify_kit

        def change_canonical(path):
            result = original(path)
            selected = canonical / "src/fake.rs"
            selected.write_bytes(selected.read_bytes() + b"\n// changed during validation\n")
            return result

        with mock.patch.object(candidate.host_build, "verify_kit", side_effect=change_canonical):
            with self.assertRaisesRegex(candidate.CandidateError, "canonical sources changed"):
                self.verify(canonical=canonical)

    def test_duplicate_metadata_is_rejected_before_verification(self):
        self.review.write_bytes(b'{"status":"ready","status":"qualification_only"}')
        with self.assertRaisesRegex(candidate.CandidateError, "duplicate"):
            self.verify()

    def test_metadata_bounds_are_enforced(self):
        self.review.write_bytes(b" " * (candidate.MAX_METADATA_BYTES + 1))
        with self.assertRaisesRegex(candidate.CandidateError, "byte limit"):
            self.verify()

    def test_path_escape_in_manifest_is_rejected_despite_bound_review(self):
        manifest_path = self.kit / "manifest.json"
        manifest = json.loads(manifest_path.read_bytes())
        manifest["files"][0]["path"] = "../outside.rs"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        self.edit_review(manifest_sha256=hashlib.sha256(manifest_path.read_bytes()).hexdigest())
        with self.assertRaises((ValueError, OSError)):
            self.verify()

    def test_leaf_and_parent_links_are_rejected(self):
        leaf = self.kit / "src/lib.rs"
        saved = self.root / "same-byte-leaf.rs"
        shutil.copyfile(leaf, saved)
        leaf.unlink()
        leaf.symlink_to(saved)
        with self.assertRaises(ValueError):
            self.verify()
        leaf.unlink()
        shutil.copyfile(saved, leaf)
        linked = self.root / "linked-parent"
        linked.symlink_to(self.root, target_is_directory=True)
        with self.assertRaisesRegex(candidate.CandidateError, "linked"):
            candidate.verify_candidate(linked / self.kit.name, self.review)
        with self.assertRaisesRegex(candidate.CandidateError, "linked"):
            candidate.verify_candidate(self.kit, linked / self.review.name)

    def test_parent_traversal_and_windows_reparse_ancestors_are_rejected(self):
        with self.assertRaisesRegex(candidate.CandidateError, "parent traversal"):
            candidate.verify_candidate(self.kit / ".." / self.kit.name, self.review)
        ordinary_lstat = Path.lstat

        def reparse_lstat(path):
            if path == self.root:
                return SimpleNamespace(st_mode=stat.S_IFDIR, st_file_attributes=0x400)
            return ordinary_lstat(path)

        with mock.patch.object(Path, "lstat", reparse_lstat):
            with self.assertRaisesRegex(candidate.CandidateError, "reparse"):
                self.verify()

    def test_review_identity_change_during_leaf_check_aborts(self):
        original = candidate.host_build.verify_kit

        def change_review(path):
            result = original(path)
            self.review.write_bytes(self.review.read_bytes() + b"\n")
            return result

        with mock.patch.object(candidate.host_build, "verify_kit", side_effect=change_review):
            with self.assertRaisesRegex(candidate.CandidateError, "changed while checking"):
                self.verify()

    def test_cli_json_is_explicitly_qualification_only_and_invalid_input_is_nonzero(self):
        process = subprocess.run([sys.executable, "-B", str(SCRIPT), "--kit", str(self.kit),
                                  "--review", str(self.review), "--json"], capture_output=True, text=True)
        self.assertEqual(process.returncode, 0, process.stderr)
        receipt = json.loads(process.stdout)
        self.assertEqual(receipt["sdk_freeze"], "OPEN")
        self.assertEqual(receipt["status"], "qualification_only")
        self.edit_review(status="stable")
        process = subprocess.run([sys.executable, "-B", str(SCRIPT), "--kit", str(self.kit),
                                  "--review", str(self.review), "--json"], capture_output=True, text=True)
        self.assertEqual(process.returncode, 1)
        self.assertEqual(process.stdout, "")
        self.assertIn("qualification_only", process.stderr)


if __name__ == "__main__":
    unittest.main()
