"""Read-only candidate integrity and no-reseal behavior; only copies are corrupted."""
from pathlib import Path
import shutil
import sys
import tempfile
from types import SimpleNamespace
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import plugin_transport_baseline as baseline

class TransportBaselineTests(unittest.TestCase):
    def test_original_inventory(self):
        self.assertEqual(baseline.verify(), 17)

    def test_changed_missing_and_extra_originals_fail_without_repair(self):
        pin = baseline.BASELINE.with_suffix(".sha256").read_text().strip()
        for mode in ("changed", "missing", "extra"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as tmp:
                folder = Path(tmp) / "copy"
                shutil.copytree(baseline.BASELINE, folder)
                file = folder / "rust-service.mplugin"
                if mode == "changed":
                    file.write_bytes(file.read_bytes() + b"changed")
                elif mode == "missing":
                    file.unlink()
                else:
                    (folder / "unlisted.wasm").write_bytes(b"unlisted")
                before = {p.name: p.read_bytes() for p in folder.iterdir() if p.is_file()}
                with self.assertRaises(ValueError):
                    baseline.verify(folder, pin)
                self.assertEqual(before, {p.name: p.read_bytes() for p in folder.iterdir() if p.is_file()})

    def test_capture_refuses_existing_directory_before_tools_or_inputs(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            marker = folder / "keep"
            marker.write_bytes(b"original")
            with self.assertRaisesRegex(ValueError, "new baseline ID"):
                baseline.capture(SimpleNamespace(directory=folder))
            self.assertEqual(marker.read_bytes(), b"original")
            self.assertEqual(list(folder.iterdir()), [marker])

if __name__ == "__main__":
    unittest.main()
