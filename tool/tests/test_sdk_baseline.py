"""Tampering tests use disposable copies, never the original guest fixtures."""
import hashlib
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import verify_plugin_sdk_baseline as baseline

class BaselineTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.folder = Path(self.temp.name) / "baseline"
        shutil.copytree(baseline.BASELINE, self.folder)
        self.pin = baseline.BASELINE.with_suffix(".sha256").read_text().strip()

    def verify(self):
        return baseline.verify(self.folder, self.pin)

    def change_manifest(self, edit, repin=False):
        path = self.folder / "SHA256SUMS"
        body = edit(path.read_text(encoding="ascii"))
        path.write_text(body, encoding="ascii", newline="\n")
        if repin:  # Exercise parser/inventory after a valid outer hash.
            self.pin = hashlib.sha256(path.read_bytes()).hexdigest()

    def test_original_inventory(self):
        self.assertEqual(self.verify(), 36)

    def test_changed_wasm(self):
        p = self.folder / "c-task.wasm"
        data = bytearray(p.read_bytes()); data[-1] ^= 1; p.write_bytes(data)
        with self.assertRaisesRegex(ValueError, "Frozen fixture changed"):
            self.verify()

    def test_changed_package(self):
        p = self.folder / "rust-ui.mplugin"
        data = bytearray(p.read_bytes()); data[-1] ^= 1; p.write_bytes(data)
        with self.assertRaisesRegex(ValueError, "Frozen fixture changed"):
            self.verify()

    def test_refreshed_manifest_does_not_bypass_pin(self):
        p = self.folder / "cpp-transform.wasm"; p.write_bytes(b"replacement")
        self.change_manifest(lambda text: "\n".join(hashlib.sha256(p.read_bytes()).hexdigest()+"  cpp-transform.wasm" if line.endswith("  cpp-transform.wasm") else line for line in text.splitlines())+"\n")
        with self.assertRaisesRegex(ValueError, "root mismatch"):
            self.verify()

    def test_missing_file(self):
        (self.folder / "c-ui.wasm").unlink()
        with self.assertRaisesRegex(ValueError, "missing files"):
            self.verify()

    def test_unlisted_file(self):
        (self.folder / "replacement.wasm").write_bytes(b"unlisted")
        with self.assertRaisesRegex(ValueError, "Unexpected or missing files"):
            self.verify()

    def test_missing_entry(self):
        self.change_manifest(lambda text: "\n".join(text.splitlines()[1:])+"\n", True)
        with self.assertRaisesRegex(ValueError, "Missing baseline fixtures"):
            self.verify()

    def test_duplicate_entry(self):
        self.change_manifest(lambda text: text+text.splitlines()[0]+"\n", True)
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.verify()

    def test_path_escape(self):
        self.change_manifest(lambda text: text.replace("  c-task.wasm", "  ../c-task.wasm"), True)
        with self.assertRaisesRegex(ValueError, "Unexpected"):
            self.verify()

    def test_empty_manifest(self):
        self.change_manifest(lambda _: "", True)
        with self.assertRaisesRegex(ValueError, "Missing baseline fixtures"):
            self.verify()

    def test_invalid_pin(self):
        self.pin = "bad pin"
        with self.assertRaisesRegex(ValueError, "Invalid baseline root"):
            self.verify()

if __name__ == "__main__":
    unittest.main()
