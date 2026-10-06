"""Disposable synthetic gate fixtures; no real runtime qualification claimed."""
from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[1] / "verify_session_exec_freeze.py"
SPEC = importlib.util.spec_from_file_location("session_exec_freeze", SCRIPT)
gate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gate)


class SessionExecFreezeTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="session-exec-freeze-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name) / "repository with spaces"
        self.root.mkdir()
        files = {
            gate.CRATE + "/Cargo.toml": '[package]\nname="fixture"\nversion="0.1.0-experimental.1"\n',
            gate.CRATE + "/Cargo.lock": "# disposable lock identity\n",
            gate.CRATE + "/build.rs": "fn main() {}\n",
            gate.CRATE + "/README.md": "Local contract only.\n",
            gate.SCHEMA: "@0xd9d0d7c2cc464d13;\n",
            gate.CRATE + "/src/lib.rs": 'pub const PROFILE: &str = "agent-session-exec-v1";\npub const VERSION: u16 = 1;\npub const REVISION: u16 = 1;\n',
            gate.CRATE + "/tests/codec.rs": "#[test]\nfn contract_identity() {}\n",
            gate.CRATE + "/tests/session.rs": "#[test]\nfn real_store_reopen() {}\n",
            gate.CRATE + "/tests/safe_exec.rs": "#[test]\nfn no_replay_after_consumption() {}\n",
            gate.CRATE + "/examples/roundtrip.rs": "fn main() {}\n",
            "core/Cargo.toml": '[package]\nname="core-fixture"\nversion="0.1.0"\n',
            "core/Cargo.lock": "# frozen Core lock fixture\n",
            "core/build.rs": "fn main() {}\n",
            "core/src/lib.rs": "pub struct Store;\n",
            "core/schemas/content.proto": 'syntax="proto3";\n',
            "core/tests/schemas/future.proto": 'syntax="proto3";\n',
            "core/tests/agent_ledger.rs": "#[test]\nfn ledger_cas() {}\n",
        }
        for name, value in files.items():
            self.write(name, value)
        self.expected = {"codec": ["contract_identity"], "session": ["real_store_reopen"],
                         "safe_exec": ["no_replay_after_consumption"], "core:agent_ledger": ["ledger_cas"]}
        self.evidence = {"tools": {}, "tests": {}}
        for name, flag, output in (("rustc", "-Vv", "rustc 1.95.0\nhost: x86_64-unknown-linux-gnu\n"),
                                   ("cargo", "-V", "cargo 1.95.0\n"),
                                   ("capnp", "--version", "Cap'n Proto version 1.4.0\n")):
            self.evidence["tools"][name] = self.record(name, [name, flag], output)
        for key, names in self.expected.items():
            manifest = "core/Cargo.toml" if key.startswith("core:") else gate.CRATE + "/Cargo.toml"
            argv = ["cargo", "test", "--locked", "--offline", "--manifest-path", manifest,
                    "--target-dir", "build/qualification", "--test", key.removeprefix("core:")]
            self.evidence["tests"][key] = {"cwd": str(self.root), **self.record(key.replace(":", "-"), argv, self.output_fixture(names))}
        self.manifest = gate.create_manifest(self.root, self.evidence, self.expected)
        self.baseline = self.root / "reviewed-freeze.json"
        self.repin()

    def write(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(value.encode() if isinstance(value, str) else value)
        return path

    def log_entry(self, name, data):
        path = "evidence/" + name
        self.write(path, data)
        return {"path": path, "sha256": hashlib.sha256(data.encode()).hexdigest()}

    def record(self, name, argv, output):
        return {"argv": argv, "exit_code": 0, "stdout": self.log_entry(name + ".stdout", output),
                "stderr": self.log_entry(name + ".stderr", "")}

    @staticmethod
    def output_fixture(names, *, passed=None, failed=0, ignored=0, filtered=0):
        if passed is None:
            passed = len(names)
        return (f"running {len(names)} tests\n" + "".join(f"test {name} ... ok\n" for name in names)
                + f"test result: ok. {passed} passed; {failed} failed; {ignored} ignored; 0 measured; {filtered} filtered out; finished in 0.01s\n")

    def repin(self):
        self.baseline.write_text(json.dumps(self.manifest, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        self.pin = hashlib.sha256(self.baseline.read_bytes()).hexdigest()

    def verify(self):
        return gate.verify_freeze(self.root, self.baseline, self.pin)

    def replace_stdout(self, key, value):
        self.manifest["evidence"]["tests"][key]["stdout"] = self.log_entry(key.replace(":", "-") + ".stdout", value)
        self.repin()

    def test_valid_explicit_pin_reports_only_the_two_local_scopes(self):
        before = {path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()}
        result = self.verify()
        self.assertEqual(result["status"], "local_contract_frozen")
        self.assertEqual(result["scope"], ["session", "safe-exec-basic"])
        self.assertEqual(result["recorded_tests_verified"], 4)
        self.assertEqual(result["source_files_verified"], 17)
        self.assertIs(result["sdk26_frozen"], False)
        self.assertIs(result["production_transport_verified"], False)
        self.assertIs(result["os_sandbox_verified"], False)
        self.assertIs(result["execution_extensions_deferred"], True)
        self.assertIs(result["evidence_execution_repeated"], False)
        self.assertIs(result["reviewer_authenticated"], False)
        self.assertEqual(before, {path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()})

    def test_self_rehash_cannot_replace_the_explicit_independent_pin(self):
        reviewed = self.pin
        changed = self.root / gate.CRATE / "src/lib.rs"
        changed.write_bytes(changed.read_bytes() + b"// drift\n")
        self.manifest["source_inventory"] = gate.source_inventory(self.root)
        self.repin()
        with self.assertRaisesRegex(gate.FreezeError, "root pin mismatch"):
            gate.verify_freeze(self.root, self.baseline, reviewed)

    def test_changed_missing_or_extra_source_is_rejected_without_repair(self):
        source = self.root / gate.CRATE / "src/lib.rs"
        saved = source.read_bytes()
        source.write_bytes(saved + b"// changed\n")
        with self.assertRaisesRegex(gate.FreezeError, "source closure"):
            self.verify()
        source.unlink()
        with self.assertRaises((OSError, gate.FreezeError)):
            self.verify()
        source.write_bytes(saved)
        extra = self.write(gate.CRATE + "/src/unreviewed.rs", "pub struct Unreviewed;\n")
        with self.assertRaisesRegex(gate.FreezeError, "source closure"):
            self.verify()
        extra.unlink()
        self.assertEqual(self.verify()["recorded_tests_verified"], 4)

    def test_manifest_cannot_omit_or_add_source_entries_even_with_new_reviewed_pin(self):
        for action in ("omit", "extra"):
            with self.subTest(action=action):
                saved = copy.deepcopy(self.manifest)
                if action == "omit":
                    self.manifest["source_inventory"].pop("core/tests/schemas/future.proto")
                else:
                    self.manifest["source_inventory"]["unrelated.rs"] = "0" * 64
                self.repin()
                with self.assertRaisesRegex(gate.FreezeError, "source closure"):
                    self.verify()
                self.manifest = saved

    def test_leaf_and_ancestor_symlinks_are_rejected(self):
        leaf = self.root / gate.CRATE / "src/lib.rs"
        saved = self.write("copy.rs", leaf.read_bytes())
        leaf.unlink()
        leaf.symlink_to(saved)
        with self.assertRaisesRegex(gate.FreezeError, "linked"):
            self.verify()
        leaf.unlink()
        leaf.write_bytes(saved.read_bytes())
        linked = self.root.parent / "linked-parent"
        linked.symlink_to(self.root, target_is_directory=True)
        with self.assertRaisesRegex(gate.FreezeError, "linked"):
            gate.verify_freeze(linked, self.baseline, self.pin)

    @unittest.skipUnless(hasattr(os, "mkfifo"), "requires POSIX FIFO")
    def test_nonregular_source_and_log_are_rejected_before_open(self):
        for path in (self.root / gate.CRATE / "src/lib.rs",
                     self.root / self.manifest["evidence"]["tests"]["session"]["stdout"]["path"]):
            saved = path.read_bytes()
            path.unlink()
            os.mkfifo(path)
            with self.assertRaisesRegex(gate.FreezeError, "ordinary file"):
                self.verify()
            path.unlink()
            path.write_bytes(saved)

    def test_missing_changed_or_linked_logs_do_not_pass_digest_checks(self):
        entry = self.manifest["evidence"]["tests"]["session"]["stdout"]
        path = self.root / entry["path"]
        saved = path.read_bytes()
        path.unlink()
        with self.assertRaises(OSError):
            self.verify()
        path.write_bytes(saved + b"drift\n")
        with self.assertRaisesRegex(gate.FreezeError, "log digest"):
            self.verify()
        path.unlink()
        path.symlink_to(self.write("same-output.txt", saved))
        with self.assertRaisesRegex(gate.FreezeError, "linked"):
            self.verify()

    def test_failed_zero_ignored_filtered_or_extra_cases_do_not_pass(self):
        values = [self.output_fixture([], passed=0), self.output_fixture(["real_store_reopen"], failed=1),
                  self.output_fixture(["real_store_reopen"], ignored=1), self.output_fixture(["real_store_reopen"], filtered=1),
                  self.output_fixture(["real_store_reopen", "unreviewed_case"]),
                  self.output_fixture(["substitute_case"]),
                  self.output_fixture(["real_store_reopen"]) + self.output_fixture([])]
        for output in values:
            with self.subTest(output=output):
                self.replace_stdout("session", output)
                with self.assertRaises(gate.FreezeError):
                    self.verify()

    def test_nonzero_process_missing_group_or_empty_names_are_rejected(self):
        original = copy.deepcopy(self.manifest)
        for edit in ("process", "group", "names"):
            with self.subTest(edit=edit):
                self.manifest = copy.deepcopy(original)
                if edit == "process":
                    self.manifest["evidence"]["tests"]["session"]["exit_code"] = 101
                elif edit == "group":
                    self.manifest["evidence"]["tests"].pop("core:agent_ledger")
                else:
                    self.manifest["expected_test_names"]["session"] = []
                self.repin()
                with self.assertRaises(gate.FreezeError):
                    self.verify()

    def test_scope_or_boolean_claims_cannot_expand_qualification(self):
        original = copy.deepcopy(self.manifest)
        for field in ["scope", *gate.BOUNDS]:
            with self.subTest(field=field):
                self.manifest = copy.deepcopy(original)
                if field == "scope":
                    self.manifest["scope"].append("pty")
                else:
                    self.manifest["bounds"][field] = not gate.BOUNDS[field]
                self.repin()
                with self.assertRaises(gate.FreezeError):
                    self.verify()
        self.manifest = copy.deepcopy(original)
        self.manifest["bounds"]["sdk26_frozen"] = 0
        self.repin()
        with self.assertRaises(gate.FreezeError):
            self.verify()

    def test_wrong_contract_identity_and_noninteger_versions_are_rejected(self):
        for field, value in (("schema_sha256", "0" * 64), ("crate_version", "1.0.0"),
                             ("version", True), ("revision", 1.0)):
            with self.subTest(field=field):
                saved = copy.deepcopy(self.manifest)
                self.manifest["contract"][field] = value
                self.repin()
                with self.assertRaisesRegex(gate.FreezeError, "contract identity"):
                    self.verify()
                self.manifest = saved

    def test_unlocked_fake_feature_selector_and_foreign_cwd_are_rejected(self):
        original = copy.deepcopy(self.manifest)
        for edit in ("unlocked", "features", "selector", "cwd", "manifest", "target"):
            with self.subTest(edit=edit):
                self.manifest = copy.deepcopy(original)
                record = self.manifest["evidence"]["tests"]["session"]
                if edit == "unlocked":
                    record["argv"].remove("--locked")
                elif edit == "features":
                    record["argv"].extend(["--features", "qualification"])
                elif edit == "selector":
                    record["argv"].append("tiny-subset")
                elif edit == "cwd":
                    record["cwd"] = str(self.root.parent)
                elif edit == "manifest":
                    record["argv"][5] = "elsewhere/Cargo.toml"
                else:
                    record["argv"][7] = "../foreign-target"
                self.repin()
                with self.assertRaises(gate.FreezeError):
                    self.verify()

    def test_one_profile_cannot_mix_target_directories(self):
        self.manifest["evidence"]["tests"]["session"]["argv"][7] = "build/other"
        self.repin()
        with self.assertRaisesRegex(gate.FreezeError, "mix target"):
            self.verify()

    def test_build_output_and_future_core_build_input_cannot_be_hidden(self):
        output = self.root / gate.CRATE / "src/target"
        output.mkdir()
        with self.assertRaisesRegex(gate.FreezeError, "build output"):
            self.verify()
        output.rmdir()
        path = self.root / "core/tests/schemas/future.proto"
        path.write_bytes(path.read_bytes() + b"// changed build input\n")
        with self.assertRaisesRegex(gate.FreezeError, "source closure"):
            self.verify()

    def test_all_core_fixtures_are_bound_without_adding_formal_test_groups(self):
        path = self.write("core/tests/storage_migration.rs", "#[test]\nfn preserves_v24_events() {}\n")
        with self.assertRaisesRegex(gate.FreezeError, "source closure"):
            self.verify()
        self.manifest["source_inventory"] = gate.source_inventory(self.root)
        self.repin()
        result = self.verify()
        self.assertEqual(result["source_files_verified"], 18)
        self.assertEqual(result["recorded_tests_verified"], 4)
        self.assertNotIn("core:storage_migration", self.manifest["expected_test_names"])
        path.write_bytes(path.read_bytes() + b"// migration fixture changed\n")
        with self.assertRaisesRegex(gate.FreezeError, "source closure"):
            self.verify()

    def test_unfrozen_crate_top_level_source_config_or_target_is_rejected(self):
        for name in ("extra.rs", ".cargo/config.toml", "target/compiled-artifact", "unknown/config.json"):
            with self.subTest(name=name):
                path = self.write(gate.CRATE + "/" + name, "unreviewed input\n")
                with self.assertRaisesRegex(gate.FreezeError, "top-level input"):
                    self.verify()
                path.unlink()
                folder = path.parent
                while folder != self.root / gate.CRATE:
                    folder.rmdir()
                    folder = folder.parent

    def test_core_and_workspace_cargo_configuration_is_rejected(self):
        for directory in (self.root / "core", self.root, self.root.parent):
            with self.subTest(directory=str(directory)):
                folder = directory / ".cargo"
                folder.mkdir()
                path = folder / "config.toml"
                path.write_bytes(b'[build]\nrustflags=["--cfg", "unreviewed"]\n')
                with self.assertRaisesRegex(gate.FreezeError, "Cargo configuration"):
                    self.verify()
                path.unlink()
                folder.rmdir()

    def test_metadata_pin_bounds_duplicate_keys_and_reparse_ancestors_are_rejected(self):
        for value in (None, "", "G" * 64, True):
            with self.subTest(pin=value), self.assertRaises(gate.FreezeError):
                gate.verify_freeze(self.root, self.baseline, value)
        self.baseline.write_bytes(b'{"profile":"a","profile":"b"}')
        self.pin = hashlib.sha256(self.baseline.read_bytes()).hexdigest()
        with self.assertRaisesRegex(gate.FreezeError, "duplicate"):
            self.verify()
        self.repin()
        original = Path.lstat

        def reparse(path):
            if path == self.root:
                return SimpleNamespace(st_mode=stat.S_IFDIR, st_file_attributes=0x400)
            return original(path)

        with mock.patch.object(Path, "lstat", reparse):
            with self.assertRaisesRegex(gate.FreezeError, "reparse"):
                self.verify()

    def test_sources_changing_during_evidence_verification_abort(self):
        original = gate.verify_evidence

        def drift(*arguments):
            count = original(*arguments)
            path = self.root / "core/src/lib.rs"
            path.write_bytes(path.read_bytes() + b"// concurrent drift\n")
            return count

        with mock.patch.object(gate, "verify_evidence", side_effect=drift):
            with self.assertRaisesRegex(gate.FreezeError, "changed while verifying"):
                self.verify()

    def test_cli_requires_pin_and_emits_machine_readable_limited_scope(self):
        base = [sys.executable, str(SCRIPT), "--root", str(self.root), "--baseline", str(self.baseline)]
        missing = subprocess.run(base, capture_output=True, text=True, timeout=15)
        self.assertNotEqual(missing.returncode, 0)
        result = subprocess.run([*base, "--expected-pin", self.pin, "--json"], capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["status"], "local_contract_frozen")
        failed = subprocess.run([*base, "--expected-pin", "0" * 64], capture_output=True, text=True, timeout=15)
        self.assertNotEqual(failed.returncode, 0)


if __name__ == "__main__":
    unittest.main()
