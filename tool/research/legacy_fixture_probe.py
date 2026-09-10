"""Synthetic migration inputs only; not an importer or a new storage backend."""
from pathlib import Path
import hashlib
import json
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "test/fixtures/legacy"

class LegacyFixtures(unittest.TestCase):
    def load(self, name):
        return json.loads((FIXTURES / (name + ".json")).read_bytes())

    def test_all_six_sources_restore_exact_bytes(self):
        # Include corrupt input in recovery; do not parse/reencode the original.
        paths = sorted(FIXTURES.glob("*.json"))
        self.assertEqual(len(paths), 6)
        for path in paths:
            source = path.read_bytes()
            with tempfile.TemporaryDirectory(prefix="legacy-", dir=ROOT / "build") as directory:
                backup = Path(directory) / "original.snapshot"
                backup.write_bytes(source)
                restored = Path(directory) / "restored.snapshot"
                restored.write_bytes(backup.read_bytes())
                self.assertEqual(restored.read_bytes(), source)
                self.assertEqual(hashlib.sha256(path.read_bytes()).digest(), hashlib.sha256(source).digest())

    def test_content_and_two_completed_scopes_are_distinct(self):
        data = self.load("complete")
        card = data["ideas"][0]
        self.assertEqual(card["id"], "synthetic-1")
        self.assertEqual(card["description"], "# 正文\n保留原文")
        self.assertNotEqual(data["completed"], card["completed"])

    def test_unknown_fields_must_not_be_projected_away(self):
        data = self.load("unknown_fields")
        self.assertEqual(data["future_config"]["nested"], [1, True, None])
        self.assertEqual(data["ideas"][0]["future_extension"], {"body": [0, 255], "format": 9})

    def test_missing_attachment_is_a_reference_not_empty_content(self):
        attachment = self.load("missing_attachment")["ideas"][0]["attachments"][0]
        self.assertEqual(attachment["size"], 42)
        self.assertFalse((FIXTURES / attachment["source"]["location"]).exists())

    def test_duplicate_reference_keeps_both_uses(self):
        attachments = self.load("duplicate_reference")["ideas"][0]["attachments"]
        self.assertEqual(len(attachments), 2)
        self.assertEqual(attachments[0]["source"], attachments[1]["source"])

    def test_corrupt_snapshot_fails_without_becoming_empty_data(self):
        with self.assertRaises(json.JSONDecodeError):
            self.load("corrupt")

    def test_display_time_is_not_an_invented_timestamp(self):
        card = self.load("legacy_defaults")["ideas"][0]
        self.assertEqual(card["time"], "刚刚")
        self.assertNotIn("created_at", card)
        self.assertNotIn("stage", card)

if __name__ == "__main__":
    (ROOT / "build").mkdir(exist_ok=True)
    unittest.main(verbosity=2)
