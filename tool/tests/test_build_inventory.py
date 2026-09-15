"""Independent inventory checks using disposable source trees, never original guests.

The fixed baseline is copied only to exercise its integrity verifier; these tests
do not build or execute guests, open application databases, or run Flutter.
"""
from contextlib import ExitStack, redirect_stderr, redirect_stdout
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import inspect_build_inventory as inventory


REPO = Path(__file__).resolve().parents[2]


class InventoryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="morrow inventory 审查 ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "source"
        self.root.mkdir()
        required = [
            "pubspec.yaml", "core/src/plugin_package.rs", "core/src/store.rs",
            "sdk/rust/src/dependency_call.rs",
            "sdk/compat/guest-v1-rc1.sha256", "android/app/build.gradle.kts",
            "lib/plugins/bootstrap_native.dart",
        ]
        required += [folder + "/Cargo.toml" for folder in inventory.CRATES]
        required += [item[1] for item in inventory.PROTOCOLS]
        required += ["sdk/rust/contracts/" + item[3] for item in inventory.PROTOCOLS if item[3]]
        for name in required:
            self.copy(name)
        for folder in ("core/schemas", "audit/schemas", "workbench_host/schemas", "plugins/workbench/schemas",
                       "sdk/rust/contracts", "sdk/compat/guest-v1-rc1"):
            shutil.copytree(REPO / folder, self.root / folder, dirs_exist_ok=True)
        # A deterministic synthetic Gradle source, not a claim about merged APK metadata.
        self.write("android/app/build.gradle.kts",
                   "\n".join(f"{name} = flutter.{symbol}" for name, symbol in (
                       ("minSdk", "minSdkVersion"), ("targetSdk", "targetSdkVersion"),
                       ("compileSdk", "compileSdkVersion"), ("ndkVersion", "ndkVersion"))) + "\n")

    def copy(self, name):
        destination = self.root / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(REPO / name, destination)

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8", newline="\n")

    def edit(self, name, transform):
        path = self.root / name
        self.write(name, transform(path.read_text(encoding="utf-8")))

    def collect(self):
        return inventory.collect(self.root)

    def fake_sdk(self):
        sdk = Path(self.temp.name) / "Flutter SDK 空间"
        version = sdk / "bin/cache/flutter.version.json"
        version.parent.mkdir(parents=True)
        version.write_text(json.dumps(dict(frameworkVersion="fixture-only",
            frameworkRevision="not-a-build", dartSdkVersion="fixture-dart")), encoding="utf-8")
        extension = sdk / "packages/flutter_tools/gradle/src/main/kotlin/FlutterExtension.kt"
        extension.parent.mkdir(parents=True)
        extension.write_text('val minSdkVersion: Int = 24\nval targetSdkVersion: Int = 36\n'
                             'val compileSdkVersion: Int = 36\nval ndkVersion: String = "28.2.13676358"\n',
                             encoding="utf-8")
        return sdk, extension

    def invoke_main(self, args, version_effect=None):
        out, err = io.StringIO(), io.StringIO()
        with ExitStack() as stack:
            stack.enter_context(patch.object(inventory, "git_state", return_value=("a" * 40, "fixture changes")))
            stack.enter_context(patch.object(inventory, "run_version", side_effect=version_effect,
                                             return_value="NOT_RUN fixture"))
            stack.enter_context(redirect_stdout(out))
            stack.enter_context(redirect_stderr(err))
            result = inventory.main(["--root", str(self.root), *args])
        return result, out.getvalue(), err.getvalue()

    def test_original_copied_sources_and_fixed_baseline_are_consistent(self):
        data = self.collect()
        self.assertEqual(data["frozen_count"], 36)
        self.assertEqual(len(data["protocols"]), 4)
        self.assertEqual(len(data["mirrors"]), 5)
        self.assertTrue(data["schemas"])
        self.assertEqual(data["pin"], (REPO / "sdk/compat/guest-v1-rc1.sha256").read_text().strip())

    def test_every_host_sdk_version_mismatch_is_rejected(self):
        for name, path, symbol, sdk_file in inventory.PROTOCOLS:
            with self.subTest(protocol=name):
                saved = (self.root / path).read_bytes()
                self.edit(path, lambda text: re.sub(
                    rf"(^pub const {symbol}: u16 = )[0-9]+;", r"\g<1>999;", text, flags=re.MULTILINE))
                with self.assertRaisesRegex(inventory.InventoryError, "Host/SDK version mismatch"):
                    self.collect()
                (self.root / path).write_bytes(saved)

    def test_missing_and_duplicate_application_or_protocol_declarations_are_rejected(self):
        for path, pattern in [
            ("pubspec.yaml", r"^version: .+$"),
            ("core/src/task.rs", r"^pub const VERSION: u16 = [0-9]+;$"),
        ]:
            saved = (self.root / path).read_text(encoding="utf-8")
            declaration = re.search(pattern, saved, re.MULTILINE).group()
            for replacement in ("", declaration + "\n" + declaration):
                with self.subTest(path=path, replacement=replacement):
                    self.write(path, saved.replace(declaration, replacement))
                    with self.assertRaisesRegex(inventory.InventoryError, "expected one source declaration"):
                        self.collect()
            self.write(path, saved)

    def test_all_current_schema_mirrors_are_compared(self):
        for name in inventory.CONTRACTS:
            path = self.root / "sdk/rust/contracts" / name
            saved = path.read_bytes()
            with self.subTest(schema=name):
                path.write_bytes(saved + b"\n// changed contract\n")
                with self.assertRaisesRegex(inventory.InventoryError, "Host/SDK schema mismatch"):
                    self.collect()
            path.write_bytes(saved)

    def test_crlf_only_schema_difference_preserves_declared_normalized_contract(self):
        path = self.root / "sdk/rust/contracts/runtime.capnp"
        path.write_bytes(path.read_bytes().replace(b"\r\n", b"\n").replace(b"\n", b"\r\n"))
        self.assertEqual(len(self.collect()["mirrors"]), 5)

    def test_database_migration_and_all_acceptance_predicates_must_agree(self):
        path = "core/src/store.rs"
        saved = (self.root / path).read_text(encoding="utf-8")
        current = self.collect()["database"]
        modifications = [
            saved.replace(f'pragma_update(None, "user_version", {current})',
                          f'pragma_update(None, "user_version", {current + 1})'),
            re.sub(r'!matches!\(version, ([0-9]+)\.\.=[0-9]+\)',
                   lambda m: f'!matches!(version, {m[1]}..={current + 1})', saved, count=1),
            saved + f"\n!matches!(version, 4..={current})\n",
        ]
        for changed in modifications:
            with self.subTest(change=modifications.index(changed)):
                self.write(path, changed)
                with self.assertRaises(inventory.InventoryError):
                    self.collect()
        self.write(path, saved)

    def test_missing_source_file_fails_instead_of_skipping(self):
        (self.root / "audit/Cargo.toml").unlink()
        with self.assertRaises(OSError):
            self.collect()

    def test_missing_schema_directory_is_not_silently_omitted(self):
        folder = self.root / "plugins/workbench/schemas"
        # This is a disposable copied tree only.
        shutil.rmtree(folder)
        with self.assertRaisesRegex(inventory.InventoryError, "Missing source schema directory"):
            self.collect()

    def test_new_schema_is_collected_with_raw_and_normalized_fingerprints(self):
        name = "plugins/workbench/schemas/review-added.proto"
        raw = b'syntax = "proto3";\r\nmessage AddedFixture {}\r\n'
        (self.root / name).write_bytes(raw)
        data = self.collect()
        rows = {path: (raw_hash, normalized) for path, raw_hash, normalized in data["schemas"]}
        self.assertEqual(rows[name], (inventory.digest(raw),
                                      inventory.digest(raw.replace(b"\r\n", b"\n"))))
        self.assertEqual(data["reader"].files[name], raw)

    def test_duplicate_capability_name_or_number_is_rejected(self):
        path = "core/schemas/plugin_package.proto"
        saved = (self.root / path).read_text(encoding="utf-8")
        block = re.search(r"enum Capability \{([^}]+)\}", saved).group(1)
        name, number = re.search(r"([A-Z][A-Z0-9_]*)\s*=\s*([0-9]+);", block).groups()
        for extra in (f"{name} = 999;", f"REVIEW_DUPLICATE = {number};"):
            with self.subTest(extra=extra):
                self.write(path, saved.replace("enum Capability {", "enum Capability {\n " + extra))
                with self.assertRaises(inventory.InventoryError):
                    self.collect()
        self.write(path, saved)

    def test_duplicate_accepted_guest_abi_is_rejected(self):
        self.edit("core/src/plugin_package.rs", lambda text: re.sub(
            r"!matches!\(manifest\.guest_abi_version, ([0-9 |]+)\)",
            lambda m: "!matches!(manifest.guest_abi_version, " + m[1] + " | " + m[1].split("|")[0].strip() + ")", text))
        with self.assertRaises(inventory.InventoryError):
            self.collect()

    def test_changed_frozen_package_and_module_are_rejected_without_touching_originals(self):
        for name in ("rust-transform.mplugin", "c-task.wasm"):
            original = REPO / "sdk/compat/guest-v1-rc1" / name
            original_sha = inventory.digest(original.read_bytes())
            path = self.root / "sdk/compat/guest-v1-rc1" / name
            saved = path.read_bytes()
            with self.subTest(name=name):
                path.write_bytes(saved[:-1] + bytes([saved[-1] ^ 1]))
                with self.assertRaisesRegex(ValueError, "Frozen fixture changed"):
                    self.collect()
                self.assertEqual(inventory.digest(original.read_bytes()), original_sha)
            path.write_bytes(saved)

    def test_source_change_during_collect_is_rejected(self):
        real_verify = inventory.baseline.verify
        def change_after_verification(folder, pin):
            count = real_verify(folder, pin)
            self.edit("core/src/task.rs", lambda text: text + "\n// concurrent edit\n")
            return count
        with patch.object(inventory.baseline, "verify", side_effect=change_after_verification):
            with self.assertRaisesRegex(inventory.InventoryError, "Source changed during inspection"):
                self.collect()

    def test_main_rechecks_sources_after_tool_observations_and_writes_nothing(self):
        output = Path(self.temp.name) / "inventory.md"
        changed = False
        def change_source(command, root):
            nonlocal changed
            if not changed:
                self.edit("pubspec.yaml", lambda text: text + "\n# concurrent source edit\n")
                changed = True
            return "fixture tool"
        result, _, err = self.invoke_main(["--output", str(output)], change_source)
        self.assertEqual(result, 1)
        self.assertIn("Source changed during inspection", err)
        self.assertFalse(output.exists())

    def test_main_refuses_overwrite_and_preserves_existing_file(self):
        output = Path(self.temp.name) / "existing.md"
        output.write_bytes(b"original user document\x00")
        result, _, err = self.invoke_main(["--output", str(output)])
        self.assertEqual(result, 1)
        self.assertIn("INVENTORY FAILED", err)
        self.assertEqual(output.read_bytes(), b"original user document\x00")

    def test_old_artifact_never_becomes_current_build_or_device_pass(self):
        path = Path(self.temp.name) / "old-release.exe"
        path.write_bytes(b"synthetic old build, never executed")
        os.utime(path, (1, 1))
        output = inventory.render(self.collect(), ("a" * 40, ""),
                                  [inventory.artifact(path)], [])
        self.assertIn(hashlib.sha256(path.read_bytes()).hexdigest(), output)
        self.assertIn("UNVERIFIED; not promoted to a current build", output)
        self.assertIn("No build, guest execution, device test or channel check is performed", output)
        for name in ("Windows", "macOS", "Linux", "Android", "iOS/iPadOS", "Web/PWA"):
            self.assertIn(f"| {name} | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN |", output)
        self.assertNotIn("PASS_SCOPED", output.split("## Inspected artifacts")[1])

    def test_markdown_cells_cannot_inject_images_links_html_or_table_rows(self):
        payload = '<img src=x>|![pass](https://example.invalid/image)\n[claim](https://example.invalid)\r\x60'
        output = inventory.render(self.collect(), ("a" * 40, payload),
                                  [(payload, 1, "a" * 64)], [("tool", payload)])
        self.assertNotIn("<img src=x>", output)
        self.assertNotIn("![pass](", output)
        self.assertNotIn("[claim](", output)
        self.assertNotIn("|![pass]", output)
        self.assertIn("&lt;img", output)

    def test_artifact_opened_object_identity_must_match_current_path(self):
        path = Path(self.temp.name) / "artifact"
        displaced = Path(self.temp.name) / "displaced-old-object"
        path.write_bytes(b"NEW CONTENT")
        displaced.write_bytes(b"OLD CONTENT")
        original_open = Path.open
        def old_handle_for_replaced_name(selected, *args, **kwargs):
            # Model a replacement between open and stat with two real file identities.
            # Python's ordinary Windows handle forbids rename while open; no unsafe
            # or platform share-mode changes are needed for this identity check.
            return original_open(displaced if selected == path else selected, *args, **kwargs)
        with patch.object(Path, "open", old_handle_for_replaced_name):
            with self.assertRaisesRegex(inventory.InventoryError, "Artifact changed"):
                inventory.artifact(path)

    def test_main_creates_only_new_derived_report_without_build_claim(self):
        output = Path(self.temp.name) / "new report 空间.md"
        result, out, err = self.invoke_main(["--output", str(output)])
        self.assertEqual(result, 0, err)
        self.assertIn("Created derived inventory", out)
        text = output.read_text(encoding="utf-8")
        self.assertIn("derived engineering output", text)
        self.assertIn("UNRESOLVED", text)
        self.assertIn("| Android | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN |", text)

    def test_main_rechecks_frozen_bytes_after_tool_observations(self):
        output = Path(self.temp.name) / "inventory.md"
        changed = False
        def change_frozen(command, root):
            nonlocal changed
            if not changed:
                path = self.root / "sdk/compat/guest-v1-rc1/rust-ui.mplugin"
                path.write_bytes(b"changed disposable frozen copy")
                changed = True
            return "fixture tool"
        result, _, err = self.invoke_main(["--output", str(output)], change_frozen)
        self.assertEqual(result, 1)
        self.assertIn("Frozen fixture changed", err)
        self.assertFalse(output.exists())

    def test_flutter_without_sdk_does_not_infer_installed_or_resolved_defaults(self):
        facts = inventory.flutter_facts(self.root, None)
        self.assertEqual(len(facts), 1)
        self.assertIn("UNRESOLVED", facts[0][1])
        self.assertIn("no SDK command was run", facts[0][1])

    def test_flutter_literal_defaults_are_source_facts_not_merged_apk(self):
        sdk, _ = self.fake_sdk()
        facts = dict(inventory.flutter_facts(self.root, sdk))
        self.assertIn("24", facts["Android source default minSdk"])
        self.assertIn("not merged APK metadata", facts["Android source default minSdk"])
        self.assertEqual(facts["Flutter cached version (not a build)"], "fixture-only")

    def test_flutter_dynamic_project_expression_is_unresolved(self):
        sdk, _ = self.fake_sdk()
        self.edit("android/app/build.gradle.kts", lambda text: text.replace(
            "minSdk = flutter.minSdkVersion", "minSdk = providers.gradleProperty(\"minimum\").get().toInt()"))
        facts = inventory.flutter_facts(self.root, sdk)
        self.assertTrue(any("minSdk" in key and "UNRESOLVED" in value for key, value in facts))
        self.assertFalse(any(key == "Android source default minSdk" for key, _ in facts))

    def test_flutter_sdk_dynamic_or_wrong_typed_values_are_never_reported_resolved(self):
        sdk, extension = self.fake_sdk()
        saved = extension.read_text(encoding="utf-8")
        for value in ("getMinimum()", "24 + 1", '"24"', "false"):
            with self.subTest(value=value):
                extension.write_text(saved.replace("Int = 24", "Int = " + value), encoding="utf-8")
                try:
                    facts = inventory.flutter_facts(self.root, sdk)
                except inventory.InventoryError:
                    continue  # An explicit fail-closed inspection is also acceptable.
                self.assertTrue(any("minSdk" in key and "UNRESOLVED" in item for key, item in facts))
                self.assertFalse(any(key == "Android source default minSdk" for key, _ in facts))

    def test_flutter_missing_or_duplicate_sdk_declaration_fails_closed(self):
        sdk, extension = self.fake_sdk()
        saved = extension.read_text(encoding="utf-8")
        for text in (saved.replace("val minSdkVersion: Int = 24\n", ""),
                     saved + "\nval minSdkVersion: Int = 26\n"):
            with self.subTest(text=text):
                extension.write_text(text, encoding="utf-8")
                with self.assertRaises(inventory.InventoryError):
                    inventory.flutter_facts(self.root, sdk)


if __name__ == "__main__":
    unittest.main()

