"""Synthetic fail-closed fixtures; passing these does not qualify execution."""
from __future__ import annotations

import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[1] / "verify_session_exec_r2.py"
SPEC = importlib.util.spec_from_file_location("session_exec_r2", SCRIPT)
gate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gate)
REPOSITORY = SCRIPT.parents[1]


class SessionExecR2Tests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="session-exec-r2-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name) / "repository with spaces"
        self.root.mkdir()
        files = {
            "README.md": "Synthetic repository documentation.\n", "LICENSE": "Synthetic license input.\n",
            gate.GATE: SCRIPT.read_bytes(), gate.GATE_TEST: Path(__file__).read_bytes(),
            gate.CRATE + "/Cargo.toml": '[workspace]\n[package]\nname="morrow-agent-session-exec-v1-r2"\nversion="0.1.0-experimental.1"\n[dependencies]\nmorrow-core={path="../../core"}\n',
            gate.CRATE + "/Cargo.lock": "# frozen R2 lock\n", gate.CRATE + "/build.rs": "fn main() {}\n",
            gate.CRATE + "/README.md": "Local session and safe-exec-basic only.\n",
            gate.SCHEMA: "@0xd9d0d7c2cc464d13;\n",
            gate.CRATE + "/src/lib.rs": 'pub const PROFILE: &str = "agent-session-exec-v1";\npub const VERSION: u16 = 1;\npub const REVISION: u16 = 2;\n',
            gate.CRATE + "/tests/codec.rs": "#[test]\nfn canonical() {}\n",
            gate.CRATE + "/tests/session.rs": "#[test]\nfn durable() {}\n",
            gate.CRATE + "/tests/safe_exec.rs": "#[test]\nfn one_shot() {}\n",
            "core/Cargo.toml": '[workspace]\n[package]\nname="morrow-core"\nversion="0.1.0"\n',
            "core/Cargo.lock": "# frozen Core lock\n", "core/build.rs": "fn main() {}\n",
            "core/README.md": "Core source.\n", "core/src/lib.rs": "pub struct Store;\n",
            "core/schemas/content.proto": 'syntax="proto3";\n',
            "core/tests/schemas/future.proto": 'syntax="proto3";\n',
            "core/tests/agent_ledger.rs": "#[test]\nfn ledger() {}\n",
            "core/tests/unselected.rs": "#[test]\nfn retained_but_unrun() {}\n",
            "core/include/core.h": "/* retained ABI input */\n",
            "core/examples/reader.rs": "fn main() {}\n",
            gate.R1_ARCHIVE: (REPOSITORY / gate.R1_ARCHIVE).read_bytes(),
        }
        for name, data in files.items():
            self.write(name, data)
        self.expected = {"codec": ["canonical"], "session": ["durable"], "safe_exec": ["one_shot"], "core:agent_ledger": ["ledger"]}
        self.evidence = {"execution_root": str(self.root), "environment": {name: os.environ.get(name) for name in gate.ENVIRONMENT_KEYS}, "tools": {}, "tests": {}, "external_qualifications": {}}
        for name, flag, output in (("rustc", "-Vv", "rustc synthetic\nhost: synthetic\n"),
                                   ("cargo", "-V", "cargo synthetic\n"),
                                   ("capnp", "--version", "Cap'n Proto version synthetic\n"),
                                   ("cc", "--version", "cc synthetic\n")):
            binary = self.write("fixture-bin/" + name, "#!/bin/sh\nexit 0\n")
            binary.chmod(0o755)
            record = self.record(name, [str(binary), flag], output)
            record["executable"] = {"path": str(binary), "sha256": gate.sha(binary.read_bytes())}
            self.evidence["tools"][name] = record
        fixture_environment = mock.patch.dict(os.environ, {
            "RUSTC": self.evidence["tools"]["rustc"]["executable"]["path"],
            "CC": self.evidence["tools"]["cc"]["executable"]["path"],
        })
        fixture_environment.start()
        self.addCleanup(fixture_environment.stop)
        self.evidence["environment"] = {name: os.environ.get(name) for name in gate.ENVIRONMENT_KEYS}
        cargo = self.evidence["tools"]["cargo"]["argv"][0]
        for key, names in self.expected.items():
            manifest = "core/Cargo.toml" if key.startswith("core:") else gate.CRATE + "/Cargo.toml"
            argv = [cargo, "test", "--locked", "--offline", "--manifest-path", manifest,
                    "--target-dir", "build/qualification", "--test", key.removeprefix("core:")]
            self.evidence["tests"][key] = {"cwd": str(self.root), **self.record(key.replace(":", "-"), argv, self.output_fixture(names))}
        consumer_cwd = self.root.parent / "independent consumer"
        source_root = self.root.parent / "exported source"
        consumer_manifest = '[workspace]\n[package]\nname="independent-consumer"\nversion="0.0.0"\nedition="2024"\n[dependencies]\nmorrow-agent-session-exec-v1-r2={path="../exported source/extensions/agent-session-exec-v1-r2",default-features=false}\n'
        inputs = {}
        for name, value in (("Cargo.toml", consumer_manifest), ("Cargo.lock", "# prepared consumer lock\n"), ("src/main.rs", "fn main() {}\n")):
            relative = "evidence/consumer-inputs/" + name
            path = self.write(relative, value)
            inputs[relative] = gate.sha(path.read_bytes())
        inventory = gate.source_inventory(self.root)
        self.evidence["consumer"] = {"cwd": str(consumer_cwd), "source_root": str(source_root), "inputs": inputs,
            "source_inventory_before": inventory.copy(), "source_inventory_after": inventory.copy(),
            **self.record("consumer", [cargo, "check", "--locked", "--offline", "--manifest-path", "Cargo.toml", "--target-dir", str(self.root / "build/consumer")], "", 'Finished `dev` profile [unoptimized] target(s) in 0.01s\n')}
        python_binary = self.write("fixture-bin/python3", "#!/bin/sh\nexit 0\n")
        python_binary.chmod(0o755)
        python_names = gate.python_test_names(self.root)
        python_output = "".join(name.split(".", 1)[1] + " (__main__." + name + ") ... ok\n" for name in python_names)
        python_output += "\nRan " + str(len(python_names)) + " tests in 0.01s\n\nOK\n"
        self.evidence["python_tests"] = {"cwd": str(self.root), "expected_names": python_names,
            "executable": {"path": str(python_binary), "sha256": gate.sha(python_binary.read_bytes())},
            **self.record("python-gate-tests", [str(python_binary), gate.GATE_TEST, "-v"], "", python_output)}
        self.manifest = gate.create_manifest(self.root, self.evidence, self.expected)
        self.baseline = self.root / "reviewed-r2.json"
        self.repin()

    def write(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(value.encode() if isinstance(value, str) else value)
        return path

    def log(self, name, value):
        relative = "evidence/" + name
        path = self.write(relative, value)
        return {"path": relative, "sha256": gate.sha(path.read_bytes())}

    def record(self, name, argv, stdout, stderr=""):
        return {"argv": argv, "exit_code": 0, "stdout": self.log(name + ".stdout", stdout), "stderr": self.log(name + ".stderr", stderr)}

    @staticmethod
    def output_fixture(names, *, ignored=0, filtered=0, failed=0):
        return (f"running {len(names)} tests\n" + "".join(f"test {name} ... ok\n" for name in names)
            + f"test result: ok. {len(names)} passed; {failed} failed; {ignored} ignored; 0 measured; {filtered} filtered out; finished in 0.01s\n")

    def repin(self):
        self.baseline.write_text(json.dumps(self.manifest, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        self.pin = gate.sha(self.baseline.read_bytes())

    def verify(self, **kwargs):
        return gate.verify_freeze(self.root, self.baseline, self.pin, **kwargs)

    def test_valid_reviewed_pin_binds_r2_only_and_retains_all_core_tests(self):
        result = self.verify()
        self.assertEqual(result["scope"], ["session", "safe-exec-basic"])
        self.assertEqual(result["recorded_tests_verified"], 4)
        self.assertEqual(result["recorded_commands_verified"], 10)
        self.assertEqual(result["recorded_python_tests_verified"], len(gate.python_test_names(self.root)))
        self.assertIs(result["reviewer_authenticated"], False)
        self.assertIs(result["current_reexecution"], False)
        self.assertTrue(all(result[name] is False for name in ("sdk26_frozen", "production_transport_verified", "os_sandbox_verified")))
        self.assertIn("core/tests/unselected.rs", self.manifest["source_inventory"])
        self.assertIn("core/include/core.h", self.manifest["source_inventory"])
        self.assertIn(gate.GATE_TEST, self.manifest["source_inventory"])

    def test_external_pin_is_mandatory_and_self_rehash_cannot_replace_it(self):
        for invalid in (None, "", "f" * 63):
            with self.assertRaises(gate.FreezeError):
                gate.verify_freeze(self.root, self.baseline, invalid)
        old_pin = self.pin
        self.manifest["expected_test_names"]["codec"] = ["substitution"]
        self.repin()
        with self.assertRaisesRegex(gate.FreezeError, "root pin mismatch"):
            gate.verify_freeze(self.root, self.baseline, old_pin)

    def test_changed_missing_and_extra_declared_source_fail(self):
        path = self.root / "core/tests/unselected.rs"
        original = path.read_bytes()
        path.write_bytes(original + b"// drift\n")
        with self.assertRaises(gate.FreezeError): self.verify()
        path.unlink()
        with self.assertRaises(gate.FreezeError): self.verify()
        path.write_bytes(original)
        self.write("core/src/extra.rs", "// unreviewed\n")
        with self.assertRaises(gate.FreezeError): self.verify()

    def test_new_crate_core_and_ancestor_configurations_are_rejected(self):
        for relative in (".cargo/config.toml", "core/.cargo/config.toml", gate.CRATE + "/.cargo/config.toml"):
            path = self.write(relative, "[build]\nrustflags=['--cfg','unreviewed']\n")
            with self.assertRaises(gate.FreezeError): self.verify()
            path.unlink(); path.parent.rmdir()
        self.write("Cargo.toml", "[workspace]\n")
        with self.assertRaises(gate.FreezeError): self.verify()

    def test_linked_source_leaf_and_ancestor_are_rejected(self):
        leaf = self.root / "core/src/lib.rs"
        saved = self.write("copy.rs", leaf.read_bytes())
        leaf.unlink(); leaf.symlink_to(saved)
        with self.assertRaisesRegex(gate.FreezeError, "linked"): self.verify()
        leaf.unlink(); leaf.write_bytes(saved.read_bytes())
        alias = self.root.parent / "linked-source"
        alias.symlink_to(self.root, target_is_directory=True)
        with self.assertRaisesRegex(gate.FreezeError, "linked"):
            gate.verify_freeze(alias, self.baseline, self.pin)

    @unittest.skipUnless(hasattr(os, "mkfifo"), "requires FIFO")
    def test_nonregular_source_rejected_before_read(self):
        path = self.root / "core/src/lib.rs"
        path.unlink(); os.mkfifo(path)
        with self.assertRaisesRegex(gate.FreezeError, "ordinary file"): self.verify()

    def test_actual_tool_executable_digest_and_command_are_bound(self):
        path = Path(self.evidence["tools"]["cargo"]["argv"][0])
        path.write_bytes(b"#!/bin/sh\nexit 7\n")
        with self.assertRaisesRegex(gate.FreezeError, "executable identity"): self.verify()
        self.assertIs(self.verify(check_executables=False)["current_executable_inputs_verified"], False)

    def test_test_evidence_rejects_failure_zero_filtered_and_extra_names(self):
        saved = copy.deepcopy(self.manifest)
        for output in (self.output_fixture([]), self.output_fixture(["durable"], failed=1), self.output_fixture(["durable"], ignored=1),
                       self.output_fixture(["durable"], filtered=1), self.output_fixture(["durable", "extra"])):
            self.manifest = copy.deepcopy(saved)
            self.manifest["evidence"]["tests"]["session"]["stdout"] = self.log("session.stdout", output)
            self.repin()
            with self.assertRaises(gate.FreezeError): self.verify()

    def test_test_command_cannot_add_feature_or_skip_offline(self):
        for argv in (self.manifest["evidence"]["tests"]["codec"]["argv"] + ["--features", "unreviewed"],
                     [a for a in self.manifest["evidence"]["tests"]["codec"]["argv"] if a != "--offline"]):
            self.manifest["evidence"]["tests"]["codec"]["argv"] = argv
            self.repin()
            with self.assertRaises(gate.FreezeError): self.verify()

    def test_consumer_source_before_after_and_completion_are_required(self):
        original = copy.deepcopy(self.manifest)
        for field in ("source_inventory_after", "exit_code", "argv", "stderr"):
            self.manifest = copy.deepcopy(original)
            consumer = self.manifest["evidence"]["consumer"]
            if field == "source_inventory_after": consumer[field] = {}
            elif field == "exit_code": consumer[field] = 101
            elif field == "argv": consumer[field] += ["--features", "host"]
            else: consumer[field] = self.log("consumer.stderr", "")
            self.repin()
            with self.assertRaises(gate.FreezeError): self.verify()

    def test_consumer_must_import_exported_revision_two_crate(self):
        path = self.root / "evidence/consumer-inputs/Cargo.toml"
        path.write_bytes(path.read_bytes().replace(b"agent-session-exec-v1-r2", b"agent-session-exec-v1"))
        self.manifest["evidence"]["consumer"]["inputs"]["evidence/consumer-inputs/Cargo.toml"] = gate.sha(path.read_bytes())
        self.repin()
        with self.assertRaises(gate.FreezeError): self.verify()

    def test_r2_contract_rejects_revision_one_and_boolean_version(self):
        self.manifest["contract"]["revision"] = True
        self.repin()
        with self.assertRaises(gate.FreezeError): self.verify()
        source = self.root / gate.CRATE / "src/lib.rs"
        source.write_bytes(source.read_bytes().replace(b"REVISION: u16 = 2", b"REVISION: u16 = 1"))
        with self.assertRaises(gate.FreezeError): gate.contract_identity(self.root)

    def test_scope_cannot_expand_to_sdk_or_production(self):
        self.manifest["bounds"]["sdk26_frozen"] = True
        self.repin()
        with self.assertRaises(gate.FreezeError): self.verify()

    def test_real_r1_archive_preserves_237_inputs_and_81_recorded_tests(self):
        result = gate.verify_r1_archive(self.root / gate.R1_ARCHIVE)
        self.assertEqual(result["source_files_verified"], 237)
        self.assertEqual(result["recorded_tests_verified"], 81)
        path = self.root / gate.R1_ARCHIVE
        path.write_bytes(path.read_bytes() + b"unreviewed\n")
        with self.assertRaisesRegex(gate.FreezeError, "raw identity"): gate.verify_r1_archive(path)

    def test_extra_source_roots_preserve_relative_path_dependency_closure(self):
        self.write("host/Cargo.toml", '[package]\nname="fixture-host"\nversion="0.0.0"\n[dependencies]\nmissing={path="../missing"}\n')
        self.write("host/Cargo.lock", "# locked\n"); self.write("host/src/lib.rs", "pub struct Host;\n")
        with self.assertRaisesRegex(gate.FreezeError, "local dependency missing"):
            gate.source_inventory(self.root, ["host"])
        self.write("missing/Cargo.toml", '[package]\nname="fixture-dependency"\nversion="0.0.0"\n')
        self.write("missing/Cargo.lock", "# locked\n"); self.write("missing/src/lib.rs", "pub struct Dependency;\n")
        inventory = gate.source_inventory(self.root, ["host", "missing"])
        self.assertIn("missing/src/lib.rs", inventory)

    def test_reference_inputs_bind_callers_without_claiming_their_upstream_graph(self):
        self.write("caller/Cargo.toml", '[package]\nname="reference-caller"\nversion="0.0.0"\n[dependencies]\nupstream={path="../unexported-upstream"}\n')
        self.write("caller/src/lib.rs", "// independently qualified caller reference\n")
        self.write("caller/rust-toolchain.toml", '[toolchain]\nchannel="1.95.0"\n')
        inventory = gate.source_inventory(self.root, extra_inputs=["caller"])
        self.assertIn("caller/Cargo.toml", inventory)
        self.assertIn("caller/rust-toolchain.toml", inventory)
        self.assertFalse(any(name.startswith("unexported-upstream/") for name in inventory))

    def test_final_export_roundtrip_is_portable_and_deterministic(self):
        first = self.root.parent / "source-first.tar.gz"
        second = self.root.parent / "source-second.tar.gz"
        gate.export_source(self.root, self.baseline, self.pin, first)
        gate.export_source(self.root, self.baseline, self.pin, second)
        self.assertEqual(first.read_bytes(), second.read_bytes())
        result = gate.verify_source_archive(first, self.baseline, self.pin)
        self.assertIs(result["portable_source_archive_verified"], True)
        self.assertIs(result["current_executable_inputs_verified"], False)

    def test_archive_rejects_extra_missing_duplicate_linked_and_traversal_members(self):
        original = self.root.parent / "reviewed.tar.gz"
        gate.export_source(self.root, self.baseline, self.pin, original)
        entries = gate.archive_entries(original)
        for kind in ("extra", "missing", "duplicate", "linked", "traversal", "fifo"):
            path = self.root.parent / (kind + ".tar.gz")
            with tarfile.open(path, "w:gz", format=tarfile.USTAR_FORMAT) as archive:
                selected = dict(entries)
                if kind == "missing": selected.pop("core/tests/unselected.rs")
                for name, body in selected.items():
                    member = tarfile.TarInfo(name); member.size = len(body)
                    archive.addfile(member, io.BytesIO(body))
                if kind != "missing":
                    member = tarfile.TarInfo("../escape" if kind == "traversal" else "core/src/lib.rs" if kind == "duplicate" else "extra-input")
                    if kind == "linked": member.type = tarfile.SYMTYPE; member.linkname = "core/src/lib.rs"
                    elif kind == "fifo": member.type = tarfile.FIFOTYPE
                    archive.addfile(member, io.BytesIO(b""))
            with self.subTest(kind=kind), self.assertRaises(gate.FreezeError):
                gate.verify_source_archive(path, self.baseline, self.pin)

    def test_candidate_export_never_claims_reviewed_freeze(self):
        output = self.root.parent / "candidate.tar.gz"
        result = gate.export_candidate(self.root, output)
        self.assertEqual(result["status"], "unreviewed_source_candidate")
        entries = gate.archive_entries(output)
        self.assertNotIn(gate.BUNDLE_BASELINE, entries)
        self.assertIn("candidate-source.json", entries)
        self.assertIn("LICENSE", entries)

    def test_recorded_or_current_extra_compiler_flags_are_rejected(self):
        self.manifest["evidence"]["environment"]["RUSTFLAGS"] = "--cfg unreviewed"
        self.repin()
        with self.assertRaises(gate.FreezeError): self.verify()
        for name in ("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS", "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER", "RUSTDOC", "CARGO_BUILD_TARGET"):
            with mock.patch.dict(os.environ, {name: "unreviewed"}), self.assertRaises(gate.FreezeError):
                gate.source_inventory(self.root)

    def test_external_cargo_home_configuration_is_rejected(self):
        path = self.write("fixture-cargo-home/config.toml", "[build]\nrustflags=['unreviewed']\n")
        with mock.patch.dict(os.environ, {"CARGO_HOME": str(path.parent)}), self.assertRaises(gate.FreezeError):
            gate.source_inventory(self.root)

    def test_consumer_cannot_write_build_outputs_into_source_or_own_inputs(self):
        original = copy.deepcopy(self.manifest)
        for parent in ("source_root", "cwd"):
            self.manifest = copy.deepcopy(original)
            consumer = self.manifest["evidence"]["consumer"]
            consumer["argv"][7] = consumer[parent] + "/target"
            self.repin()
            with self.assertRaises(gate.FreezeError): self.verify()

    def test_consumer_cannot_enable_host_or_an_extra_dependency(self):
        path = self.root / "evidence/consumer-inputs/Cargo.toml"
        saved = path.read_bytes()
        for content in (saved.replace(b"default-features=false", b'default-features=false,features=["host"]'), saved + b'extra="=1.0.0"\n'):
            path.write_bytes(content)
            self.manifest["evidence"]["consumer"]["inputs"]["evidence/consumer-inputs/Cargo.toml"] = gate.sha(content)
            self.repin()
            with self.assertRaises(gate.FreezeError): self.verify()

    def test_reviewed_export_rechecks_source_bytes_before_writing(self):
        output = self.root.parent / "raced-source.tar.gz"
        original = gate.verify_freeze
        def after_verify(*args, **kwargs):
            result = original(*args, **kwargs)
            source = self.root / "core/src/lib.rs"
            source.write_bytes(source.read_bytes() + b"// changed after verification\n")
            return result
        with mock.patch.object(gate, "verify_freeze", side_effect=after_verify), self.assertRaisesRegex(gate.FreezeError, "bytes changed"):
            gate.export_source(self.root, self.baseline, self.pin, output)
        self.assertFalse(output.exists())

    def test_reviewed_export_rechecks_log_bytes_before_writing(self):
        output = self.root.parent / "raced-log.tar.gz"
        original = gate.verify_freeze
        def after_verify(*args, **kwargs):
            result = original(*args, **kwargs)
            path = self.root / self.manifest["evidence"]["tests"]["codec"]["stdout"]["path"]
            path.write_bytes(path.read_bytes() + b"drift\n")
            return result
        with mock.patch.object(gate, "verify_freeze", side_effect=after_verify), self.assertRaisesRegex(gate.FreezeError, "bytes changed"):
            gate.export_source(self.root, self.baseline, self.pin, output)
        self.assertFalse(output.exists())

    def test_hard_linked_source_input_is_rejected(self):
        leaf = self.root / "core/src/lib.rs"
        os.link(leaf, self.root / "second-link.rs")
        with self.assertRaisesRegex(gate.FreezeError, "hard-linked"): self.verify()

    def extra_group_fixture(self):
        self.write("host/Cargo.toml", '[package]\nname="fixture-host"\nversion="0.0.0"\n[dependencies]\nmorrow-core={path="../core"}\n')
        self.write("host/Cargo.lock", "# host lock\n")
        self.write("host/src/lib.rs", "#[test] fn unit_case() {}\n")
        self.write("host/tests/package_route.rs", "#[test] fn route_case() {}\n")
        groups = {"host_lib": {"manifest": "host/Cargo.toml", "kind": "lib", "target": None, "expected_names": ["unit_case"]},
                  "host_route": {"manifest": "host/Cargo.toml", "kind": "test", "target": "package_route", "expected_names": ["route_case"]}}
        cargo = self.evidence["tools"]["cargo"]["argv"][0]
        for key, group in groups.items():
            suffix = ["--lib"] if group["kind"] == "lib" else ["--test", group["target"]]
            argv = [cargo, "test", "--locked", "--offline", "--manifest-path", group["manifest"], "--target-dir", "build/host", *suffix]
            self.evidence["tests"][key] = {"cwd": str(self.root), **self.record(key, argv, self.output_fixture(group["expected_names"]))}
        inventory = gate.source_inventory(self.root, ["host"])
        self.evidence["consumer"]["source_inventory_before"] = inventory.copy()
        self.evidence["consumer"]["source_inventory_after"] = inventory.copy()
        return groups

    def test_extra_source_bound_lib_and_integration_groups_pass(self):
        groups = self.extra_group_fixture()
        self.manifest = gate.create_manifest(self.root, self.evidence, self.expected, extra_source_roots=["host"], extra_test_groups=groups)
        self.repin()
        result = self.verify()
        self.assertEqual(result["recorded_tests_verified"], 6)
        self.assertEqual(result["recorded_commands_verified"], 12)

    def test_extra_group_manifest_target_and_kind_fail_closed(self):
        original = self.extra_group_fixture()
        for field, value in (("manifest", "unreviewed/Cargo.toml"), ("target", "missing"), ("kind", "bin")):
            groups = copy.deepcopy(original)
            groups["host_route"][field] = value
            with self.subTest(field=field), self.assertRaises(gate.FreezeError):
                gate.create_manifest(self.root, self.evidence, self.expected, extra_source_roots=["host"], extra_test_groups=groups)

    def test_one_extra_crate_cannot_mix_target_directories(self):
        groups = self.extra_group_fixture()
        self.evidence["tests"]["host_route"]["argv"][7] = "build/different-host"
        with self.assertRaises(gate.FreezeError):
            gate.create_manifest(self.root, self.evidence, self.expected, extra_source_roots=["host"], extra_test_groups=groups)

    def test_python_gate_evidence_requires_actual_binary_full_names_and_no_skips(self):
        original = copy.deepcopy(self.manifest)
        for field in ("argv", "expected_names", "exit_code", "stderr", "executable"):
            self.manifest = copy.deepcopy(original)
            record = self.manifest["evidence"]["python_tests"]
            if field == "argv": record[field] += ["SessionExecR2Tests.test_valid_reviewed_pin_binds_r2_only_and_retains_all_core_tests"]
            elif field == "expected_names": record[field] = record[field][:-1]
            elif field == "exit_code": record[field] = 1
            elif field == "stderr": record[field] = self.log("python-gate-tests.stderr", "Ran 1 test in 0.01s\n\nOK (skipped=1)\n")
            else: record[field]["sha256"] = "0" * 64
            self.repin()
            with self.subTest(field=field), self.assertRaises(gate.FreezeError): self.verify()

    def test_external_qualification_binds_only_reviewed_reference_inputs(self):
        reference_manifest = self.write("caller/Cargo.toml", '[package]\nname="fixture-caller"\nversion="0.0.0"\n[dependencies]\nupstream={path="../separate-upstream"}\n')
        self.write("caller/Cargo.lock", "# caller lock\n")
        self.write("caller/src/lib.rs", "#[test] fn caller_case() {}\n")
        references = ["caller/Cargo.lock", "caller/Cargo.toml", "caller/src/lib.rs"]
        inventory = gate.source_inventory(self.root, extra_inputs=references)
        self.evidence["consumer"]["source_inventory_before"] = inventory.copy()
        self.evidence["consumer"]["source_inventory_after"] = inventory.copy()
        cargo = self.evidence["tools"]["cargo"]["argv"][0]
        record = {"manifest": "caller/Cargo.toml", "kind": "lib", "target": None, "expected_names": ["caller_case"],
                  "reference_inputs": references, "cwd": str(self.root),
                  **self.record("external-caller", [cargo, "test", "--locked", "--offline", "--manifest-path", "caller/Cargo.toml", "--target-dir", "build/external", "--lib"], self.output_fixture(["caller_case"]))}
        self.evidence["external_qualifications"]["actual_caller"] = record
        self.manifest = gate.create_manifest(self.root, self.evidence, self.expected, extra_inputs=references)
        self.repin()
        result = self.verify()["external_qualifications"]["actual_caller"]
        self.assertEqual(result["recorded_tests_verified"], 1)
        self.assertIs(result["upstream_build_graph_source_closed"], False)
        self.assertIs(result["production_qualification"], False)
        self.assertIs(result["os_qualification"], False)
        record["reference_inputs"] = [str(reference_manifest)]
        with self.assertRaises(gate.FreezeError):
            gate.create_manifest(self.root, self.evidence, self.expected, extra_inputs=references)

    def test_cli_requires_reviewed_pin_and_r1_cli_is_read_only(self):
        missing = subprocess.run([sys.executable, str(SCRIPT), "verify", "--baseline", str(self.baseline)], capture_output=True, text=True)
        self.assertEqual(missing.returncode, 2)
        valid = subprocess.run([sys.executable, str(SCRIPT), "verify-r1", "--source-archive", str(self.root / gate.R1_ARCHIVE), "--json"], capture_output=True, text=True)
        self.assertEqual(valid.returncode, 0, valid.stderr)
        self.assertEqual(json.loads(valid.stdout)["recorded_tests_verified"], 81)


if __name__ == "__main__":
    unittest.main()
