"""Exercise the actual sync CLI in disposable repositories, never source contracts."""
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

REPOSITORY = Path(__file__).resolve().parents[2]
SCHEMAS = ("runtime.capnp", "content.proto", "task.capnp", "ui.capnp", "dependency_call.capnp")
VERSIONS = ("version.txt", "task-version.txt", "ui-version.txt")


class ContractSyncTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name) / "repository with spaces"
        names = ["tool/sync_plugin_sdk_contracts.py"]
        names += [f"core/src/{name}.rs" for name in ("runtime", "task", "ui")]
        names += [f"core/schemas/{name}" for name in SCHEMAS]
        names += [f"sdk/rust/contracts/{name}" for name in SCHEMAS + VERSIONS]
        for name in names:
            destination = self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(REPOSITORY / name, destination)
        self.target = self.root / "sdk/rust/contracts/dependency_call.capnp"
        self.source = self.root / "core/schemas/dependency_call.capnp"

    def run_sync(self, *arguments):
        return subprocess.run(
            [sys.executable, str(self.root / "tool/sync_plugin_sdk_contracts.py"), *arguments],
            cwd=self.root.parent,  # Must resolve its own root rather than use the shell cwd.
            capture_output=True, text=True, encoding="utf-8", timeout=30, check=False,
        )

    def snapshot(self):
        return {
            path.relative_to(self.root).as_posix(): path.read_bytes()
            for path in self.root.rglob("*") if path.is_file()
        }

    def test_original_contracts_check_without_mutation(self):
        before = self.snapshot()
        result = self.run_sync("--check")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Verified guest SDK contracts.", result.stdout)
        self.assertEqual(self.snapshot(), before)

    def test_dependency_only_drift_is_detected_without_repair(self):
        # Every other copied contract/version is unchanged. This failed to be checked
        # when dependency_call.capnp was omitted from the synchronization inventory.
        self.source.write_bytes(self.source.read_bytes() + b"\n# Changed dependency contract\n")
        before = self.snapshot()
        result = self.run_sync("--check")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Stale guest SDK contract: dependency_call.capnp", result.stderr)
        self.assertEqual(self.snapshot(), before)

    def test_missing_dependency_normal_sync_restores_and_check_passes(self):
        original = self.target.read_text(encoding="utf-8").encode("utf-8")
        self.target.unlink()
        result = self.run_sync("--check")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Stale guest SDK contract: dependency_call.capnp", result.stderr)
        self.assertFalse(self.target.exists(), "check must never create a contract")
        result = self.run_sync()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Updated guest SDK contract snapshots.", result.stdout)
        self.assertEqual(self.target.read_bytes(), original)
        self.assertEqual(self.target.read_bytes(), self.source.read_text(encoding="utf-8").encode("utf-8"))
        result = self.run_sync("--check")
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
