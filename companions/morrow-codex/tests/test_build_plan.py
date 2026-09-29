"""Failure-path qualification; all fixtures and receipts stay in this repository.

These are build-entry tests, not upstream replacement or G0 acceptance tests.
Fixtures are retained under out/test-workspaces for review.
"""

import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import tarfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("build_plan", ROOT / "tools" / "build_plan.py")
build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build)


def put_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


class BuildPlanTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        base = ROOT / "out" / "test-workspaces"
        base.mkdir(parents=True, exist_ok=True)
        cls.suite_root = Path(tempfile.mkdtemp(prefix="build-entry-", dir=base))
        cls.suite_root.resolve().relative_to(ROOT)
        print("Retained test evidence: " + str(cls.suite_root), file=sys.stderr)

    def setUp(self):
        self.repo = self.suite_root / self._testMethodName
        (self.repo / "tools").mkdir(parents=True)
        shutil.copyfile(ROOT / "tools" / "build-contract.json", self.repo / "tools" / "build-contract.json")
        fixture_contract = self.repo / "tools" / "build-contract.json"
        contract = json.loads(fixture_contract.read_text(encoding="utf-8"))
        contract["toolchain_lock"] = None
        put_json(fixture_contract, contract)
        self.lock = {"schema_version": 1, "sources": []}
        for name, (url, commit) in build.PINNED.items():
            source = self.repo / "upstream" / name
            source.mkdir(parents=True)
            (source / "LICENSE").write_bytes((name + " license fixture\n").encode())
            (source / "Cargo.toml").write_bytes(b"# source fixture\n")
            archive = self.repo / "archives" / (name + ".tar.gz")
            archive.parent.mkdir(exist_ok=True)
            with tarfile.open(archive, "w:gz") as tar:
                for file in sorted(source.iterdir()):
                    tar.add(file, arcname=name + "-" + commit + "/" + file.name)
            manifest = self.repo / "manifests" / (name + ".json")
            put_json(manifest, {"schema_version": 1, "files": [
                {"path": file.name, "sha256": build.sha256(file)} for file in sorted(source.iterdir())]})
            self.lock["sources"].append({
                "id": name, "kind": "archive", "url": url, "commit": commit,
                "path": str(source.relative_to(self.repo)),
                "archive": {"path": str(archive.relative_to(self.repo)), "sha256": build.sha256(archive)},
                "files_manifest": {"path": str(manifest.relative_to(self.repo)), "sha256": build.sha256(manifest)},
                "license_files": [{"path": "LICENSE", "sha256": build.sha256(source / "LICENSE")}],
            })
        self.save_lock()

    def save_lock(self):
        put_json(self.repo / "sources.lock.json", self.lock)

    def invoke(self, *args):
        text = io.StringIO()
        with contextlib.redirect_stdout(text):
            code = build.main(list(args), repo_root=self.repo)
        receipt = json.loads(text.getvalue())
        self.assertEqual(code, receipt["exit_code"])
        self.assertEqual(receipt["gates"], {"P-00": "not_claimed", "G0": "not_claimed"})
        self.assertFalse(receipt["network_used"])
        self.assertFalse(receipt["locks_updated"])
        self.assertFalse(receipt["old_dist_reused"])
        self.assertEqual(receipt["artifacts"], [])
        if "output_dir" in receipt:
            output = Path(receipt["output_dir"])
            output.relative_to(self.repo)
            self.assertEqual(receipt, json.loads((output / "status.json").read_text(encoding="utf-8")))
        return code, receipt

    def blocked(self, expected, *args):
        code, receipt = self.invoke(*(args or ("preflight", "--scope", "sources")))
        self.assertNotEqual(code, 0)
        self.assertEqual(receipt["error"]["code"], expected)
        return receipt

    def test_sources_scope_pass_is_not_gate_completion(self):
        code, receipt = self.invoke("preflight", "--scope", "sources")
        self.assertEqual(code, 0)
        self.assertEqual(receipt["status"], "passed_scoped_checks")
        self.assertEqual(len(receipt["checks"]), 2)

    def test_each_run_uses_new_output(self):
        _, first = self.invoke("preflight", "--scope", "sources")
        _, second = self.invoke("preflight", "--scope", "sources")
        self.assertNotEqual(first["output_dir"], second["output_dir"])

    def test_all_unimplemented_stages_fail_even_with_old_dist(self):
        old = self.repo / "dist"
        old.mkdir()
        stale = old / "success.exe"
        stale.write_bytes(b"old successful artifact")
        for stage in build.STAGES[1:]:
            with self.subTest(stage=stage):
                receipt = self.blocked("not_implemented", stage)
                self.assertEqual(receipt["status"], "not_implemented")
        self.assertEqual(stale.read_bytes(), b"old successful artifact")

    def test_missing_source_lock(self):
        self.blocked("input_missing", "preflight", "--scope", "sources", "--source-lock", "missing.json")

    def test_missing_source(self):
        self.lock["sources"][0]["path"] = "not-present"
        self.save_lock()
        self.blocked("input_missing")

    def test_both_sources_required(self):
        self.lock["sources"].pop()
        self.save_lock()
        self.blocked("sources_missing")

    def test_wrong_commit_rejected(self):
        self.lock["sources"][0]["commit"] = "0" * 40
        self.save_lock()
        self.blocked("source_pin")

    def test_wrong_url_rejected(self):
        self.lock["sources"][0]["url"] = "https://example.invalid/codex"
        self.save_lock()
        self.blocked("source_pin")

    def test_wrong_archive_digest_rejected(self):
        self.lock["sources"][0]["archive"]["sha256"] = "0" * 64
        self.save_lock()
        self.blocked("digest_mismatch")

    def test_missing_digest_rejected(self):
        self.lock["sources"][0]["archive"].pop("sha256")
        self.save_lock()
        self.blocked("digest_missing")

    def test_dirty_archive_tree_rejected(self):
        (self.repo / "upstream" / "codex" / "Cargo.toml").write_bytes(b"changed\n")
        self.blocked("source_tree_mismatch")

    def test_untracked_archive_file_rejected(self):
        (self.repo / "upstream" / "codex" / "extra.rs").write_bytes(b"unrecorded\n")
        self.blocked("source_tree_mismatch")

    def test_rewritten_manifest_cannot_hide_dirty_archive_tree(self):
        entry = self.lock["sources"][0]
        source = self.repo / "upstream" / "codex"
        (source / "Cargo.toml").write_bytes(b"modified extracted source")
        manifest = self.repo / entry["files_manifest"]["path"]
        put_json(manifest, {"schema_version": 1, "files": [
            {"path": file.name, "sha256": build.sha256(file)} for file in sorted(source.iterdir())]})
        entry["files_manifest"]["sha256"] = build.sha256(manifest)
        self.save_lock()
        self.blocked("archive_tree_mismatch")

    def test_archive_path_escape_rejected_without_extraction(self):
        entry = self.lock["sources"][0]
        archive = self.repo / entry["archive"]["path"]
        with tarfile.open(archive, "w:gz") as tar:
            member = tarfile.TarInfo("../escape")
            member.size = 1
            tar.addfile(member, io.BytesIO(b"x"))
        entry["archive"]["sha256"] = build.sha256(archive)
        self.save_lock()
        self.blocked("archive_path")
        self.assertFalse((self.repo / "escape").exists())

    def test_partial_snapshot_checked_but_never_complete(self):
        for entry in self.lock["sources"]:
            entry["kind"] = "partial_snapshot"
            manifest = self.repo / entry["files_manifest"]["path"]
            document = json.loads(manifest.read_text(encoding="utf-8"))
            for item in document["files"]:
                data = (self.repo / entry["path"] / item["path"]).read_bytes()
                item["git_blob_sha1"] = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
            put_json(manifest, document)
            entry["files_manifest"]["sha256"] = build.sha256(manifest)
        self.save_lock()
        receipt = self.blocked("source_incomplete")
        self.assertEqual(len(receipt["checks"]), 2)
        self.assertEqual(receipt["checks"][0]["source"]["git_blobs_verified"], 2)

    def test_partial_snapshot_wrong_git_blob_rejected(self):
        entry = self.lock["sources"][0]
        entry["kind"] = "partial_snapshot"
        manifest = self.repo / entry["files_manifest"]["path"]
        document = json.loads(manifest.read_text(encoding="utf-8"))
        document["files"][0]["git_blob_sha1"] = "0" * 40
        put_json(manifest, document)
        entry["files_manifest"]["sha256"] = build.sha256(manifest)
        self.save_lock()
        self.blocked("source_blob_mismatch")

    def test_license_digest_rejected(self):
        self.lock["sources"][0]["license_files"][0]["sha256"] = "0" * 64
        self.save_lock()
        self.blocked("digest_mismatch")

    def test_missing_license_evidence(self):
        self.lock["sources"][0]["license_files"] = []
        self.save_lock()
        self.blocked("license_missing")

    def test_source_path_escape(self):
        self.lock["sources"][0]["path"] = "../outside"
        self.save_lock()
        self.blocked("path_escape")

    def test_existing_output_preserved(self):
        output = self.repo / "out" / "existing"
        output.mkdir(parents=True)
        status = output / "status.json"
        status.write_bytes(b"existing evidence")
        receipt = self.blocked("output_exists", "preflight", "--scope", "sources", "--output-dir", str(output))
        self.assertNotIn("output_dir", receipt)
        self.assertEqual(status.read_bytes(), b"existing evidence")

    def test_output_escape(self):
        receipt = self.blocked("path_escape", "preflight", "--scope", "sources", "--output-dir", "../escape")
        self.assertNotIn("output_dir", receipt)
        self.assertFalse((self.repo.parent / "escape").exists())

    def test_output_symlink_rejected(self):
        outside = self.repo / "real"
        outside.mkdir()
        link = self.repo / "linked"
        try:
            link.symlink_to(outside, target_is_directory=True)
        except OSError as error:
            self.skipTest("OS does not grant symlink fixture creation: " + str(error))
        self.blocked("path_link", "preflight", "--scope", "sources", "--output-dir", "linked/new")
        self.assertFalse((outside / "new").exists())

    def test_source_symlink_rejected(self):
        source = self.repo / "upstream" / "codex"
        try:
            (source / "link").symlink_to(source / "LICENSE")
        except OSError as error:
            self.skipTest("OS does not grant symlink fixture creation: " + str(error))
        self.blocked("path_link")

    def test_baseline_candidate_scope(self):
        self.lock["baseline"] = {"path": "upstream/codex", "commit": "candidate", "branch": "codex/candidate"}
        self.save_lock()
        with patch.object(build, "git", side_effect=[b"candidate\n", b"codex/candidate\n"]):
            code, receipt = self.invoke("preflight", "--scope", "baseline")
        self.assertEqual(code, 0)
        baseline = receipt["checks"][0]["baseline"]
        self.assertEqual(baseline["status"], "candidate_only")
        self.assertFalse(baseline["m00_delivery_verified"])

    def test_baseline_mismatch(self):
        self.lock["baseline"] = {"path": "upstream/codex", "commit": "wanted", "branch": "wanted"}
        self.save_lock()
        with patch.object(build, "git", side_effect=[b"different", b"different"]):
            self.blocked("baseline_mismatch", "preflight", "--scope", "baseline")

    def test_full_preflight_missing_kit(self):
        with patch.object(build, "check_baseline", return_value={"status": "candidate_only"}):
            self.blocked("kit_missing", "preflight", "--scope", "full")

    def kit_args(self, reviewed=True):
        kit = self.repo / "kit-fixture"
        kit.mkdir()
        manifest = kit / "manifest.json"
        put_json(manifest, {"fixture": "opaque host manifest; no Schema definition"})
        review = self.repo / "kit-review.json"
        put_json(review, {"manifest_sha256": build.sha256(manifest) if reviewed else "0" * 64,
                          "status": "qualification_only"})
        return ["--host-kit", str(kit), "--host-kit-manifest", "manifest.json",
                "--host-kit-sha256", build.sha256(manifest), "--host-kit-review", str(review)]

    def test_kit_review_digest_mismatch(self):
        args = self.kit_args(reviewed=False)
        with patch.object(build, "check_baseline", return_value={}):
            self.blocked("kit_unreviewed", "preflight", *args)

    def test_kit_review_required(self):
        args = self.kit_args()[:-2]
        with patch.object(build, "check_baseline", return_value={}):
            self.blocked("kit_unreviewed", "preflight", *args)

    def test_full_preflight_missing_toolchain_lock(self):
        args = self.kit_args()
        with patch.object(build, "check_baseline", return_value={}):
            self.blocked("toolchain_lock_missing", "preflight", *args)

    def test_full_preflight_never_claims_complete(self):
        with patch.object(build, "check_baseline", return_value={}), \
             patch.object(build, "verify_source", return_value={}), \
             patch.object(build, "check_kit", return_value={}), \
             patch.object(build, "check_graphs", return_value={}):
            self.blocked("full_preflight_incomplete", "preflight")

    def test_fixed_command_environment_disables_implicit_network_and_redirects(self):
        result = type("Result", (), {"returncode": 0, "stdout": b"ok"})()
        with patch.dict(os.environ, {"GIT_DIR": "outside", "GIT_WORK_TREE": "outside", "GIT_CONFIG_COUNT": "1"}), \
             patch.object(build.subprocess, "run", return_value=result) as command:
            build.run_fixed(["rustup", "--version"], self.repo, toolchain="pinned")
        env = command.call_args.kwargs["env"]
        self.assertNotIn("GIT_DIR", env)
        self.assertNotIn("GIT_WORK_TREE", env)
        self.assertNotIn("GIT_CONFIG_COUNT", env)
        self.assertEqual(env["GIT_NO_LAZY_FETCH"], "1")
        self.assertEqual(env["RUSTUP_AUTO_INSTALL"], "0")
        self.assertEqual(env["RUSTUP_TOOLCHAIN"], "pinned")

    def test_recorded_git_dirty_patch_and_untracked(self):
        source = self.repo / "upstream" / "codex"
        patch_data = b"diff --git a/one b/one\nrecorded binary-safe patch\n"
        patch_path = self.repo / "evidence.patch"
        patch_path.write_bytes(patch_data)
        extra = source / "extra.rs"
        extra.write_bytes(b"tracked in receipt, not in Git\n")
        entry = {"id": "codex", "dirty": {"allowed": True, "patch_path": "evidence.patch",
                 "patch_sha256": build.sha256(patch_path),
                 "untracked": [{"path": "extra.rs", "sha256": build.sha256(extra)}]}}
        with patch.object(build, "git", side_effect=[patch_data, b"extra.rs\0", b""]):
            receipt = build.verify_dirty(self.repo, entry, source)
        self.assertTrue(receipt["dirty"])
        with patch.object(build, "git", side_effect=[patch_data + b"changed", b"extra.rs\0", b""]):
            with self.assertRaises(build.Blocked) as raised:
                build.verify_dirty(self.repo, entry, source)
        self.assertEqual(raised.exception.code, "dirty_patch_mismatch")

    def test_unrecorded_git_dirty_rejected(self):
        with patch.object(build, "git", side_effect=[b"diff", b"", b""]):
            with self.assertRaises(build.Blocked) as raised:
                build.verify_dirty(self.repo, {"id": "codex", "dirty": {"allowed": False}}, self.repo)
        self.assertEqual(raised.exception.code, "dirty_unrecorded")

    def test_ignored_git_file_must_be_recorded(self):
        empty = self.repo / "empty.patch"
        empty.write_bytes(b"")
        (self.repo / "ignored.bin").write_bytes(b"changes build inputs")
        entry = {"id": "codex", "dirty": {"allowed": True, "patch_path": "empty.patch",
                 "patch_sha256": build.sha256(empty), "untracked": []}}
        with patch.object(build, "git", side_effect=[b"", b"", b"ignored.bin\0"]):
            with self.assertRaises(build.Blocked) as raised:
                build.verify_dirty(self.repo, entry, self.repo)
        self.assertEqual(raised.exception.code, "dirty_untracked_mismatch")

    def graph_contract(self):
        lock = self.repo / "toolchain.lock.json"
        put_json(lock, {"toolchain": "fixture-x86_64-pc-windows-msvc",
                        "versions": {"rustup": "rustup fixture", "rustc": "rustc fixture", "cargo": "cargo fixture"},
                        "command": "never execute this lock value"})
        contract = {"toolchain_lock": {"path": "toolchain.lock.json", "sha256": build.sha256(lock)}, "graphs": {}}
        for name, target in (("native", "x86_64-pc-windows-msvc"), ("wasm", "wasm32-unknown-unknown")):
            manifest = self.repo / name / "Cargo.toml"
            cargo_lock = self.repo / name / "Cargo.lock"
            manifest.parent.mkdir()
            manifest.write_bytes(b"# manifest fixture\n")
            cargo_lock.write_bytes(b"# lock fixture\n")
            contract["graphs"][name] = {"status": "implemented", "manifest": str(manifest.relative_to(self.repo)),
                "manifest_sha256": build.sha256(manifest), "lock": str(cargo_lock.relative_to(self.repo)),
                "lock_sha256": build.sha256(cargo_lock), "target": target, "target_dir": "out/target/" + name}
        return contract

    def tool_responses(self, targets=b"x86_64-pc-windows-msvc\nwasm32-unknown-unknown\n"):
        return [b"fixture-x86_64-pc-windows-msvc (default)\n", b"rustup fixture", b"rustc fixture", b"cargo fixture", targets]

    def graph_blocked(self, code, contract, responses=None):
        with patch.object(build, "run_fixed", side_effect=responses or self.tool_responses()):
            with self.assertRaises(build.Blocked) as raised:
                build.check_graphs(self.repo, contract)
        self.assertEqual(raised.exception.code, code)

    def test_toolchain_not_installed_does_not_attempt_install(self):
        contract = self.graph_contract()
        with patch.object(build, "run_fixed", return_value=b"different-toolchain\n") as command:
            with self.assertRaises(build.Blocked) as raised:
                build.check_graphs(self.repo, contract)
        self.assertEqual(raised.exception.code, "toolchain_missing")
        command.assert_called_once_with(["rustup", "toolchain", "list"], self.repo)

    def test_toolchain_wrong_version(self):
        self.graph_blocked("toolchain_mismatch", self.graph_contract(),
                           [b"fixture-x86_64-pc-windows-msvc\n", b"rustup different"])

    def test_missing_build_graph(self):
        contract = self.graph_contract()
        contract["graphs"]["native"]["status"] = "not_implemented"
        self.graph_blocked("graph_not_implemented", contract)

    def test_missing_cargo_lock(self):
        contract = self.graph_contract()
        contract["graphs"]["native"]["lock"] = "missing.lock"
        self.graph_blocked("input_missing", contract)

    def test_changed_cargo_lock(self):
        contract = self.graph_contract()
        (self.repo / "native" / "Cargo.lock").write_bytes(b"new unapproved lock")
        self.graph_blocked("digest_mismatch", contract)

    def test_missing_target(self):
        self.graph_blocked("target_missing", self.graph_contract(), self.tool_responses(b"x86_64-pc-windows-msvc\n"))

    def test_native_wasm_cannot_share_target_directory(self):
        contract = self.graph_contract()
        contract["graphs"]["wasm"]["target_dir"] = "out/target/native"
        self.graph_blocked("graph_output_shared", contract)

    def test_native_wasm_cannot_share_manifest_and_lock(self):
        contract = self.graph_contract()
        contract["graphs"]["wasm"]["manifest"] = contract["graphs"]["native"]["manifest"]
        self.graph_blocked("graph_inputs_shared", contract)

    def test_fixed_tool_commands_no_lock_command_execution(self):
        contract = self.graph_contract()
        with patch.object(build, "run_fixed", side_effect=self.tool_responses()) as command:
            result = build.check_graphs(self.repo, contract)
        self.assertEqual(result["dependency_cache"], "not_validated_no_deps_stage")
        calls = [call.args[0] for call in command.call_args_list]
        self.assertEqual(calls, [["rustup", "toolchain", "list"], ["rustup", "--version"],
            ["rustup", "run", "fixture-x86_64-pc-windows-msvc", "rustc", "--version"],
            ["rustup", "run", "fixture-x86_64-pc-windows-msvc", "cargo", "--version"],
            ["rustup", "target", "list", "--installed", "--toolchain", "fixture-x86_64-pc-windows-msvc"]])


if __name__ == "__main__":
    unittest.main(verbosity=2)
