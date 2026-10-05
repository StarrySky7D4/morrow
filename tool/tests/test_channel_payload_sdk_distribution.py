"""Actual source closure, tamper, relocation and refusal tests; no compilation."""
import contextlib
import copy
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tool"))
import export_channel_payload_sdk as export
import verify_channel_payload_sdk as bundle


class ChannelPayloadDistributionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.files = {name: bundle.read_file(ROOT, name) for name in bundle.NAMES - {"README.md", "NOTICE.md", "tool/verify_channel_payload_sdk.py"}}
        cls.files["README.md"], cls.files["NOTICE.md"] = export.README, export.NOTICE
        cls.files["tool/verify_channel_payload_sdk.py"] = Path(bundle.__file__).read_bytes()
        cls.manifest = bundle.build_manifest(cls.files)
        bundle.verify_payload(cls.files, cls.manifest)

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="channel source spaces ")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def materialized(self, name="standalone source"):
        output = self.root / name
        bundle.materialize(self.files, self.manifest, output)
        return output

    def archive(self, files=None, manifest=None, name="sdk source.zip"):
        output = self.root / name
        temporary = export.archive_files(self.files if files is None else files, self.manifest if manifest is None else manifest, output)
        os.link(temporary, output)
        temporary.unlink()
        return output

    def reseal(self, files):
        return bundle.build_manifest(files)

    def test_relocated_directory_cli_is_independent_of_repository_and_tool_module_path(self):
        source = self.materialized()
        moved = self.root / "relocated source with spaces"
        source.rename(moved)
        result = subprocess.run([sys.executable, "-B", str(moved / "tool/verify_channel_payload_sdk.py"), "verify-directory", str(moved)],
                                cwd=self.root, capture_output=True, check=False)
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
        receipt = json.loads(result.stdout)
        self.assertEqual(receipt["status"], "VERIFIED_SOURCE_INVENTORY_ONLY")
        self.assertEqual(receipt["authority"], "none")
        self.assertEqual(receipt["build_execution"], "NOT_RUN")

    def test_zip_extract_and_directory_match_exact_bytes(self):
        archive = self.archive()
        self.assertEqual(bundle.verify_zip(archive), bundle.verify_directory(self.materialized()))
        output = self.root / "fresh extracted source"
        bundle.extract_zip(archive, output)
        for name, data in self.files.items():
            self.assertEqual(bundle.read_file(output, name), data)

    def test_existing_directory_is_not_replaced(self):
        output = self.root / "existing output"
        output.mkdir()
        marker = output / "sentinel"
        marker.write_bytes(b"keep")
        with self.assertRaises(FileExistsError):
            bundle.materialize(self.files, self.manifest, output)
        self.assertEqual(marker.read_bytes(), b"keep")

    def test_existing_file_output_is_not_replaced(self):
        output = self.root / "existing output"
        output.write_bytes(b"keep")
        with self.assertRaises(FileExistsError):
            bundle.materialize(self.files, self.manifest, output)
        self.assertEqual(output.read_bytes(), b"keep")

    def test_existing_archive_is_not_truncated(self):
        output = self.root / "existing.zip"
        output.write_bytes(b"keep")
        with self.assertRaises(bundle.BundleError):
            export.archive_files(self.files, self.manifest, output)
        self.assertEqual(output.read_bytes(), b"keep")

    def test_tampered_missing_and_extra_source_are_refused(self):
        for mode in ("tampered", "missing", "extra"):
            with self.subTest(mode=mode):
                output = self.materialized(mode)
                target = output / "README.md"
                if mode == "tampered":
                    target.write_bytes(b"tampered")
                elif mode == "missing":
                    target.unlink()
                else:
                    (output / "unselected.txt").write_bytes(b"extra")
                with self.assertRaises(bundle.BundleError):
                    bundle.verify_directory(output)

    def test_unselected_empty_directory_is_refused(self):
        output = self.materialized()
        (output / "unknown").mkdir()
        with self.assertRaises(bundle.BundleError):
            bundle.verify_directory(output)

    def test_self_rehashed_schema_cannot_change_native_contract_observation(self):
        files = dict(self.files)
        files["extensions/sse-event-v1/contracts/sse_event.capnp"] += b"\n"
        with self.assertRaisesRegex(bundle.BundleError, "schema differs"):
            bundle.verify_payload(files, self.reseal(files))

    def test_target_dependency_escape_and_cargo_patch_are_refused_even_when_rehashed(self):
        key = "extensions/sse-event-v1/Cargo.toml"
        for addition in (b'\n[target.\'cfg(windows)\'.dependencies]\nescape = {path="../../../../outside"}\n',
                         b'\n[patch.crates-io]\ncapnp = {path="../../../../outside"}\n'):
            files = dict(self.files)
            files[key] += addition
            with self.assertRaisesRegex(bundle.BundleError, "Cargo graph differs"):
                bundle.verify_payload(files, self.reseal(files))

    def test_workspace_members_and_dependency_path_are_exact(self):
        key = "extensions/ws-message-v1/Cargo.toml"
        for old, new in ((b'["guests/rust"]', b'["guests/rust", "extra"]'),
                         (b'"../../sdk/rust"', b'"/outside"')):
            files = dict(self.files)
            self.assertIn(old, files[key])
            files[key] = files[key].replace(old, new)
            with self.assertRaisesRegex(bundle.BundleError, "Cargo graph differs"):
                bundle.verify_payload(files, self.reseal(files))

    def test_registry_lock_drift_is_refused_without_changing_original_lock(self):
        files = dict(self.files)
        key = "extensions/sse-event-v1/Cargo.lock"
        self.assertIn(b'version = "1.0.4"', files[key])
        files[key] = files[key].replace(b'version = "1.0.4"', b'version = "1.0.5"')
        with self.assertRaisesRegex(bundle.BundleError, "registry differs"):
                bundle.verify_payload(files, self.reseal(files))

    def test_coordinated_three_lock_changes_cannot_widen_original_registry_pins(self):
        files = dict(self.files)
        for name in ("sdk/rust/Cargo.lock", "extensions/ws-message-v1/Cargo.lock", "extensions/sse-event-v1/Cargo.lock"):
            self.assertIn(b'version = "1.0.4"', files[name])
            files[name] = files[name].replace(b'version = "1.0.4"', b'version = "9.9.9"')
        with self.assertRaisesRegex(bundle.BundleError, "original SDK lock bytes differ"):
            bundle.verify_payload(files, self.reseal(files))

    def test_removed_path_dependency_edges_are_refused_when_registry_unchanged(self):
        files = dict(self.files)
        name = "extensions/sse-event-v1/Cargo.lock"
        old = b'name = "morrow-sse-event-guest"\nversion = "0.1.0"\ndependencies = [\n "morrow-plugin-sdk",\n "morrow-sse-event-v1",\n]'
        # CRLF is copied verbatim from the checked out library; normalize only test mutation.
        data = files[name].replace(b"\r\n", b"\n")
        self.assertIn(old, data)
        files[name] = data.replace(old, b'name = "morrow-sse-event-guest"\nversion = "0.1.0"')
        with self.assertRaisesRegex(bundle.BundleError, "path package"):
            bundle.verify_payload(files, self.reseal(files))

    def test_toml_boolean_and_integer_types_are_distinct(self):
        key = "extensions/sse-event-v1/Cargo.toml"
        for old, new in ((b"publish = false", b"publish = 0"), (b"optional = true", b"optional = 1")):
            files = dict(self.files)
            self.assertIn(old, files[key])
            files[key] = files[key].replace(old, new)
            with self.assertRaisesRegex(bundle.BundleError, "Cargo graph differs"):
                bundle.verify_payload(files, self.reseal(files))

    def test_missing_and_nonstring_registry_checksums_refuse_cleanly(self):
        import re
        key = "extensions/sse-event-v1/Cargo.lock"
        for changed in (b"checksum = 7", b"checksum = true", b"checksum = []", b"# removed checksum"):
            files = dict(self.files)
            data, count = re.subn(rb'checksum = "[0-9a-f]{64}"', changed, files[key], count=1)
            self.assertEqual(count, 1)
            files[key] = data
            with self.assertRaisesRegex(bundle.BundleError, "unsupported Cargo source"):
                bundle.verify_payload(files, self.reseal(files))

    def test_duplicate_manifest_members_and_bool_versions_are_refused(self):
        duplicate = self.manifest.replace(b'"schema": 1', b'"schema": 1, "schema": 1')
        with self.assertRaisesRegex(bundle.BundleError, "duplicate"):
            bundle.parse_manifest(duplicate)
        value = json.loads(self.manifest)
        value["contracts"]["sse-event-v1"]["version"] = True
        with self.assertRaises(bundle.BundleError):
            bundle.parse_manifest(json.dumps(value).encode())

    def test_manifest_authority_and_profile_cannot_widen(self):
        for key, changed in (("authority", "owner"), ("qualification", "SDK_FROZEN"), ("profile", "morrow-sdk-source-distribution-v1")):
            value = json.loads(self.manifest)
            value[key] = changed
            with self.assertRaises(bundle.BundleError):
                bundle.parse_manifest(json.dumps(value).encode())

    def test_manifest_file_size_count_and_total_bounds_are_checked(self):
        for mutate in (lambda v: v["files"]["README.md"].update(size=bundle.MAX_FILE_BYTES + 1),
                       lambda v: v.update(source_bytes=True),
                       lambda v: v["files"].update({"outside.txt": {"size": 0, "sha256": "0" * 64}})):
            value = json.loads(self.manifest)
            mutate(value)
            with self.assertRaises(bundle.BundleError):
                bundle.parse_manifest(json.dumps(value).encode())
        with self.assertRaises(bundle.BundleError):
            bundle.parse_manifest(b" " * (bundle.MAX_MANIFEST_BYTES + 1))

    def test_nonportable_paths_are_refused(self):
        for name in ("../outside", "/absolute", "a\\b", "C:stream", "CON.h", "a/aux.txt", "x/..", "x/", "x.", "x ", "a\x00b", "e\u0301.txt", "\ud800"):
            with self.subTest(name=repr(name)), self.assertRaises(bundle.BundleError):
                bundle.safe_name(name)

    def test_duplicate_and_case_colliding_archive_entries_are_refused(self):
        for changed in ("README.md", "readme.md"):
            output = self.root / ("case-" + changed + ".zip")
            with zipfile.ZipFile(output, "w") as archive:
                for name, data in {**self.files, bundle.MANIFEST: self.manifest}.items():
                    # Replace another entry so the exact file count cannot hide the collision.
                    archive.writestr(changed if name == "NOTICE.md" else name, data)
            with self.assertRaises(bundle.BundleError):
                bundle.verify_zip(output)

    def test_archive_link_path_escape_and_windows_reparse_attributes_are_refused(self):
        for mode in ("symlink", "escape", "reparse"):
            output = self.root / (mode + ".zip")
            with zipfile.ZipFile(output, "w") as archive:
                for name, data in {**self.files, bundle.MANIFEST: self.manifest}.items():
                    entry = zipfile.ZipInfo("../outside" if mode == "escape" and name == "README.md" else name)
                    entry.external_attr = ((stat.S_IFLNK | 0o777) << 16 if mode == "symlink" else 0x400 if mode == "reparse" else 0o100644 << 16) if name == "README.md" else 0o100644 << 16
                    archive.writestr(entry, data)
            with self.assertRaises(bundle.BundleError):
                bundle.verify_zip(output)

    def test_archive_bad_crc_and_unsupported_compression_are_refused(self):
        output = self.root / "bad zip.zip"
        output.write_bytes(b"not zip")
        with self.assertRaises(bundle.BundleError):
            bundle.verify_zip(output)
        output = self.root / "crc.zip"
        with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_STORED) as archive:
            for name, data in {**self.files, bundle.MANIFEST: self.manifest}.items():
                archive.writestr(name, data)
        with zipfile.ZipFile(output) as archive:
            entry = archive.getinfo("README.md")
            offset = entry.header_offset
        raw = bytearray(output.read_bytes())
        filename_size = int.from_bytes(raw[offset + 26:offset + 28], "little")
        extra_size = int.from_bytes(raw[offset + 28:offset + 30], "little")
        raw[offset + 30 + filename_size + extra_size] ^= 1
        output.write_bytes(raw)
        with self.assertRaises(bundle.BundleError):
            bundle.verify_zip(output)
        output = self.root / "unsupported.zip"
        with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_BZIP2) as archive:
            for name, data in {**self.files, bundle.MANIFEST: self.manifest}.items():
                archive.writestr(name, data)
        with self.assertRaises(bundle.BundleError):
            bundle.verify_zip(output)

    def test_fake_windows_reparse_metadata_is_refused_without_enabling_privileges(self):
        fake = type("Info", (), {"st_mode": stat.S_IFDIR, "st_file_attributes": 0x400})()
        with patch.object(Path, "lstat", return_value=fake), self.assertRaises(bundle.BundleError):
            bundle.checked_root(self.root)

    @unittest.skipUnless(os.name == "nt", "Windows junction fixture")
    def test_actual_windows_junction_root_is_refused_and_target_kept(self):
        target = self.materialized()
        link = self.root / "ordinary unprivileged junction"
        result = subprocess.run(["cmd.exe", "/d", "/c", "mklink", "/J", str(link), str(target)], capture_output=True)
        if result.returncode != 0:
            self.skipTest("ordinary junction creation unavailable; no privilege enabled")
        try:
            with self.assertRaises(bundle.BundleError):
                bundle.verify_directory(link)
        finally:
            # Remove this known junction itself, never enumerate/delete its target.
            os.rmdir(link)
        self.assertEqual(bundle.verify_directory(target)["status"], "VERIFIED_SOURCE_INVENTORY_ONLY")

    def test_export_source_drift_does_not_publish_archive(self):
        output = self.root / "unpublished.zip"
        changed = dict(self.files)
        changed["README.md"] += b"changed"
        with patch.object(export, "source_authority", return_value={"scope": "bounded"}), \
                patch.object(export, "snapshot", side_effect=[(self.files, self.manifest), (changed, self.reseal(changed))]), \
                self.assertRaisesRegex(bundle.BundleError, "changed during export"):
            export.create_archive(ROOT, output)
        self.assertFalse(output.exists())
        self.assertEqual(list(self.root.iterdir()), [])

    def test_export_racing_output_is_preserved(self):
        output = self.root / "racing.zip"
        real_archive = export.archive_files
        def race(files, manifest, destination):
            temporary = real_archive(files, manifest, destination)
            destination.write_bytes(b"other writer")
            return temporary
        with patch.object(export, "source_authority", return_value={"scope": "bounded"}), \
                patch.object(export, "snapshot", return_value=(self.files, self.manifest)), \
                patch.object(export, "archive_files", side_effect=race), self.assertRaises(FileExistsError):
            export.create_archive(ROOT, output)
        self.assertEqual(output.read_bytes(), b"other writer")
        self.assertEqual(list(self.root.iterdir()), [output])

    def test_unsigned_rehashed_document_is_consistent_but_not_a_signature(self):
        files = dict(self.files)
        files["README.md"] = b"changed unsigned document"
        result = bundle.verify_payload(files, self.reseal(files))
        self.assertEqual(result["semantics"], "NOT_PROVED")
        self.assertEqual(result["authority"], "none")


if __name__ == "__main__":
    unittest.main()
