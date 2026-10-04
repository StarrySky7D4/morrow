"""Pure preparation checks; no compiler, server or frozen-artifact writes."""
import hashlib
import importlib.util
from pathlib import Path
import struct
import tempfile
import unittest

PATH = Path(__file__).resolve().parents[1] / "prepare_ws_channel_fixtures.py"
spec = importlib.util.spec_from_file_location("prepare_ws_channel_fixtures", PATH)
subject = importlib.util.module_from_spec(spec)
spec.loader.exec_module(subject)

class FixturePreparationTests(unittest.TestCase):
    def test_three_small_layouts_bind_exact_schema_and_payload(self):
        digest = hashlib.sha256(subject.SCHEMA.read_bytes()).digest()
        for kind, payload, close in ((0, "W15 雪 🙂".encode(), None), (1, bytes([0, 255, 128, 10, 13, 0, 37]), None), (4, b"", 1000)):
            raw = subject.envelope(kind, payload, digest, close)
            self.assertEqual(raw[40:72], digest)
            self.assertEqual(raw[72:72+len(payload)], payload)
            self.assertEqual(struct.unpack_from("<Q", raw, 0)[0], (len(raw)//8-1)<<32)
            self.assertEqual(struct.unpack_from("<Q", raw, 16)[0], 1|(kind<<16)|(int(close is not None)<<32)|((close or 0)<<48))

    def test_rejects_unbounded_or_nonfixture_input(self):
        for args in ((0, b"x"*126, b"d"*32), (9, b"", b"d"*32), (0, b"", b"short"), (0, b"", b"d"*32, 1000)):
            with self.assertRaises(ValueError):
                subject.envelope(*args)

    def test_existing_destination_is_preserved(self):
        with tempfile.TemporaryDirectory() as temp:
            target=Path(temp); marker=target/"keep"; marker.write_bytes(b"original")
            with self.assertRaises(ValueError):
                subject.prepare(target)
            self.assertEqual(marker.read_bytes(), b"original")

    def test_repository_destination_is_rejected_before_creation(self):
        target=subject.ROOT/"must-not-create-ws-guests"
        with self.assertRaises(ValueError):
            subject.prepare(target)
        self.assertFalse(target.exists())

    def test_parent_traversal_cannot_escape_into_repository(self):
        target = subject.ROOT.parent / "evidence-c01" / ".." / subject.ROOT.name / "must-not-create-ws-guests"
        with self.assertRaises(ValueError):
            subject.prepare(target)
        self.assertFalse((subject.ROOT / "must-not-create-ws-guests").exists())

    def test_symbolic_link_parent_is_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp); (root/"real").mkdir(); (root/"alias").symlink_to(root/"real", target_is_directory=True)
            with self.assertRaises(ValueError):
                subject.prepare(root/"alias"/"new")
            self.assertFalse((root/"real/new").exists())

if __name__ == "__main__":
    unittest.main()
