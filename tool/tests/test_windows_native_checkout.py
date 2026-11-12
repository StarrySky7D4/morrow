"""Windows Git checkouts must preserve pinned framework and launcher inputs."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
PATCH = "tool/flutter_windows_semantics/semantics.patch"
NATIVE_INPUTS = (
    "windows/CMakeLists.txt",
    "windows/runner/main.cpp",
    "windows/runner/office_clipboard.cpp",
    "windows/runner/Runner.rc",
    "windows/runner/runner.exe.manifest",
    "windows/runner/resources/app_icon.ico",
)


class WindowsNativeCheckoutTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        manifest = json.loads((ROOT / "tool/flutter_windows_semantics/manifest.json").read_bytes())
        cls.expected = {PATCH: manifest["patch_sha256"]}
        for relative in NATIVE_INPUTS:
            original = subprocess.check_output(
                ["git", "-c", f"safe.directory={ROOT.as_posix()}", "-c",
                 "core.longpaths=true", "show", f"HEAD:{relative}"], cwd=ROOT,
            )
            cls.expected[relative] = hashlib.sha256(original).hexdigest()

    def assert_inputs(self, root, paths):
        for relative in paths:
            with self.subTest(path=relative):
                self.assertEqual(
                    hashlib.sha256((root / relative).read_bytes()).hexdigest(),
                    self.expected[relative],
                )

    def test_working_patch_matches_fixed_manifest(self):
        self.assert_inputs(ROOT, (PATCH,))

    def test_working_launcher_inputs_match_canonical_git_bytes(self):
        self.assert_inputs(ROOT, NATIVE_INPUTS)

    def checkout(self, autocrlf):
        with tempfile.TemporaryDirectory(prefix="morrow-native-checkout-") as temporary:
            root = Path(temporary)
            repo, output = root / "repo", root / "checkout"
            repo.mkdir()
            output.mkdir()
            shutil.copyfile(ROOT / ".gitattributes", repo / ".gitattributes")
            for relative in self.expected:
                destination = repo / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ROOT / relative, destination)

            def git(*args):
                subprocess.run(
                    ["git", "-c", f"core.autocrlf={autocrlf}", *args],
                    cwd=repo, check=True, capture_output=True,
                )

            git("init", "--quiet")
            git("add", "--", ".gitattributes", *self.expected)
            git("checkout-index", "--all", f"--prefix={output.as_posix()}/")
            self.assert_inputs(output, self.expected)

    def test_windows_autocrlf_checkout_preserves_exact_inputs(self):
        self.checkout("true")

    def test_lf_checkout_preserves_exact_inputs(self):
        self.checkout("false")


if __name__ == "__main__":
    unittest.main()
