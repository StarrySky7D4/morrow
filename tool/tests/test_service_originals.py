"""Restored originals must be complete and hash-pinned before qualification."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_plugin_service_sdk import load_original_packages

class OriginalPackagesTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.records = {}
        for language in ("rust", "c", "cpp"):
            data = b"test-original-" + language.encode()
            digest = hashlib.sha256(data).hexdigest()
            path = self.root / language / "dist" / (digest + ".mplugin")
            path.parent.mkdir(parents=True)
            path.write_bytes(data)
            self.records[language] = {"path": str(path.relative_to(self.root)), "sha256": digest}
        self.manifest = self.root / "packages.json"
    def save(self):
        self.manifest.write_text(json.dumps(self.records))
    def test_exact_manifest_and_explicit_relocation_preserve_bytes(self):
        self.save()
        original = load_original_packages(self.manifest)
        for record in self.records.values(): record["path"] = "/old-machine/unavailable.mplugin"
        self.save()
        self.assertEqual(load_original_packages(self.manifest, self.root), original)
        with self.assertRaises(ValueError): load_original_packages(self.manifest)
    def test_missing_language_unknown_field_bad_digest_or_changed_original_fail(self):
        self.save()
        originals = load_original_packages(self.manifest)
        Path(originals["rust"]["path"]).write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "hash mismatch"): load_original_packages(self.manifest)
        del self.records["rust"]
        self.save()
        with self.assertRaisesRegex(ValueError, "all three"): load_original_packages(self.manifest)
        self.records["rust"] = {"path": "x", "sha256": "../escape"}
        self.save()
        with self.assertRaisesRegex(ValueError, "digest"): load_original_packages(self.manifest)
        self.records["rust"] = {"path": "x", "sha256": "0"*64, "fallback": "x"}
        self.save()
        with self.assertRaisesRegex(ValueError, "entry"): load_original_packages(self.manifest)
    def test_relocation_uses_pinned_filename_without_guessing_an_alternate(self):
        self.save()
        path = self.root / self.records["c"]["path"]
        path.rename(path.parent / "newest.mplugin")
        with self.assertRaisesRegex(ValueError, "missing"): load_original_packages(self.manifest, self.root)
