import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from shutil import copyfile


class PreviewSourceTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.root = self.directory / "project"
        (self.root / "tool").mkdir(parents=True)
        self.script = self.root / "tool/package_preview_source.py"
        copyfile(Path(__file__).resolve().parents[1] / self.script.name, self.script)
        (self.root / "app.txt").write_text("preview source", encoding="utf-8")
        self.archive = self.directory / "release.zip"

    def capture(self, *flags):
        return subprocess.run(
            [sys.executable, str(self.script), str(self.archive), *flags],
            capture_output=True, text=True, check=False,
        )

    def assert_source_zip_refused(self):
        result = self.capture()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("project Git worktree root", result.stderr)
        self.assertFalse(self.archive.exists())

    def test_plain_source_directory(self):
        self.assert_source_zip_refused()

    def test_ignored_child_of_another_repository(self):
        subprocess.run(["git", "init", "-q", str(self.directory)], check=True)
        (self.directory / ".gitignore").write_text("project/\n", encoding="ascii")
        self.assert_source_zip_refused()

    def test_own_checkout_tracks_working_tree_bytes(self):
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        subprocess.run(["git", "add", "app.txt"], cwd=self.root, check=True)
        (self.root / "app.txt").write_text("updated", encoding="utf-8")
        result = self.capture()
        self.assertEqual(result.returncode, 0, result.stderr)
        with zipfile.ZipFile(self.archive) as archive:
            self.assertEqual(archive.read("app.txt"), b"updated")
            self.assertIn("tool/package_preview_source.py", archive.namelist())
        result = self.capture("--check")
        self.assertEqual(result.returncode, 0, result.stderr)

