from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import tempfile
import sys
import unittest
from unittest.mock import patch

MODULE = Path(__file__).resolve().parents[1] / "preflight.py"
spec = importlib.util.spec_from_file_location("linux_preflight", MODULE)
preflight = importlib.util.module_from_spec(spec)
spec.loader.exec_module(preflight)


class PreflightTests(unittest.TestCase):
    def test_missing_prerequisites_never_enable_product(self):
        with patch.object(preflight.shutil, "which", return_value=None):
            result = preflight.inventory({"PATH": ""})
        self.assertFalse(result["gtk_compile_prerequisites_available"])
        self.assertFalse(result["protected_product_available"])
        self.assertTrue(result["product_blockers"])

    def test_local_env_stays_process_local(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "usr").mkdir()
            original = {"PATH": "/usr/bin", "HOME": "/not-used"}
            actual = preflight.local_environment(root, original)
            self.assertEqual(original, {"PATH": "/usr/bin", "HOME": "/not-used"})
            self.assertEqual(actual["PKG_CONFIG_SYSROOT_DIR"], str(root))
            self.assertTrue(actual["PATH"].startswith(str(root / "usr/bin")))
            self.assertNotIn("/etc", actual["PKG_CONFIG_LIBDIR"])

    def test_gtk_inventory_is_not_product_qualification(self):
        fake = type("Completed", (), {"returncode": 0, "stdout": "3.24.49\n"})()
        with patch.object(preflight.shutil, "which", return_value="/tools/program"), \
             patch.object(preflight.subprocess, "run", return_value=fake), \
             patch.object(preflight, "package_version", return_value="3.24.49"):
            result = preflight.inventory({"PATH": "/tools", "DISPLAY": ":0"})
        self.assertTrue(result["gtk_compile_prerequisites_available"])
        self.assertFalse(result["protected_product_available"])
        self.assertTrue(result["display_environment_present"])

    def test_noisy_tool_output_is_bounded_and_refused(self):
        command = [sys.executable, "-c", "import sys; sys.stdout.write('x' * 1048576)"]
        self.assertIsNone(preflight.bounded_stdout(command, dict(os.environ), limit=128, timeout=2))

    def test_package_version_rejects_extra_lines_and_non_ascii(self):
        for value in (b"3.24.49\n{arbitrary-json}\n", b"\xff3.24.49", b"version 3.24.49"):
            with patch.object(preflight, "bounded_stdout", return_value=value):
                self.assertIsNone(preflight.package_version("pkg-config", "gtk+-3.0", {}))
        with patch.object(preflight, "bounded_stdout", return_value=b"3.24.49\n"):
            self.assertEqual(preflight.package_version("pkg-config", "gtk+-3.0", {}), "3.24.49")

    def test_tool_version_streams_are_discarded(self):
        fake = type("Completed", (), {"returncode": 0})()
        with patch.object(preflight.shutil, "which", return_value="/tools/program"), \
             patch.object(preflight.subprocess, "run", return_value=fake) as run, \
             patch.object(preflight, "package_version", return_value="3.24.49"):
            preflight.inventory({"PATH": "/tools"})
        for call in run.call_args_list:
            self.assertEqual(call.kwargs["stdout"], preflight.subprocess.DEVNULL)
            self.assertEqual(call.kwargs["stderr"], preflight.subprocess.DEVNULL)
            self.assertNotIn("capture_output", call.kwargs)

    def test_quiet_stuck_tool_is_refused_within_timeout(self):
        command = [sys.executable, "-c", "import time; time.sleep(30)"]
        self.assertIsNone(preflight.bounded_stdout(command, dict(os.environ), timeout=0.05))


if __name__ == "__main__":
    unittest.main()
