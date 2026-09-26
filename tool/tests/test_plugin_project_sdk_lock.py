"""SDK pins detect implementation drift without compilers or network access."""
import contextlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import morrow_plugin as tool
import plugin_sdk_lock as lock
from test_plugin_project import write_config


class SdkLockTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.sdk = self.root / "SDK relocated 空间"
        for name in lock.ROOT_FILES:
            path = self.sdk / name
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(tool.ROOT / "sdk" / name, path)
        for name in lock.TREES:
            shutil.copytree(tool.ROOT / "sdk" / name, self.sdk / name)
        self.project = self.root / "project"
        self.args = SimpleNamespace(path=str(self.project), sdk_root=str(tool.ROOT / "sdk"), language="c",
                                    kind="io", id="org.example.locked", version="1.0.0", name="Locked",
                                    allow_network=False, lock_sdk=True, require_sdk_lock=True, update=False)
        with contextlib.redirect_stdout(io.StringIO()):
            tool.new_project(self.args)
        self.args.sdk_root = str(self.sdk)

    def test_lock_is_portable_deterministic_and_excludes_build_artifacts(self):
        before = (self.project / lock.LOCK_NAME).read_bytes()
        report = lock.verify(self.project, self.sdk, required=True)
        self.assertEqual(report["status"], "verified")
        self.assertNotIn(str(tool.ROOT).encode(), before)
        self.assertEqual(before, lock.encode(lock.inventory(self.sdk)))
        (self.sdk / "rust/target").mkdir()
        (self.sdk / "rust/target/cache").write_bytes(b"unrelated build cache")
        self.assertEqual(lock.verify(self.project, self.sdk), report)
        self.assertEqual((self.project / lock.LOCK_NAME).read_bytes(), before)

    def test_rust_relocation_preserves_pins_after_explicit_cargo_rebinding(self):
        self.args.path = str(self.root / "rust-project")
        self.args.language = "rust"
        self.args.sdk_root = str(tool.ROOT / "sdk")
        with contextlib.redirect_stdout(io.StringIO()):
            tool.new_project(self.args)
        project = Path(self.args.path)
        pin = (project / lock.LOCK_NAME).read_bytes()
        self.args.sdk_root = str(self.sdk)
        with self.assertRaisesRegex(tool.ToolError, "selected SDK path"):
            tool.preflight_project(self.args)
        cargo = tool.read_toml(project / "Cargo.toml")
        cargo["dependencies"]["morrow-plugin-sdk"]["path"] = (self.sdk / "rust").as_posix()
        write_config(project / "Cargo.toml", cargo)
        self.assertEqual(tool.preflight_project(self.args)[-1]["status"], "verified")
        self.assertEqual((project / lock.LOCK_NAME).read_bytes(), pin)

    def test_changed_added_removed_sources_stop_validate_build_and_pack(self):
        source = self.sdk / "rust/src/io.rs"
        original = source.read_bytes()
        for change in ("changed", "added", "removed"):
            extra = self.sdk / "cpp/include/new.hpp"
            if change == "changed":
                source.write_bytes(original + b"\n// implementation changed without version drift\n")
            elif change == "added":
                extra.write_bytes(b"new header")
            else:
                source.unlink()
            for command in (tool.validate_project, tool.compile_project, tool.pack_project):
                with self.subTest(change=change, command=command.__name__), \
                     mock.patch.object(tool, "run", side_effect=AssertionError("external tool ran")), \
                     mock.patch.object(tool, "executable", side_effect=AssertionError("tool queried")):
                    with self.assertRaises((lock.SdkLockError, tool.ToolError, OSError)):
                        command(self.args)
                    self.assertFalse((self.project / "build").exists())
            source.write_bytes(original)
            extra.unlink(missing_ok=True)

    def test_existing_lock_requires_explicit_update_and_failure_preserves_it(self):
        path = self.project / lock.LOCK_NAME
        before = path.read_bytes()
        with self.assertRaisesRegex(lock.SdkLockError, "already exists"):
            lock.write(self.project, self.sdk)
        changed = self.sdk / "c/src/morrow_plugin_sdk.c"
        changed.write_bytes(changed.read_bytes() + b"\n/* reviewed update */\n")
        with mock.patch.object(lock.os, "replace", side_effect=OSError("publication failed")):
            with self.assertRaisesRegex(OSError, "publication failed"):
                lock.write(self.project, self.sdk, update=True)
        self.assertEqual(path.read_bytes(), before)
        self.assertEqual(list(self.project.glob("sdk-lock-*.tmp")), [])
        self.args.update = True
        with contextlib.redirect_stdout(io.StringIO()):
            tool.lock_project_sdk(self.args)
        self.assertNotEqual(path.read_bytes(), before)
        self.assertEqual(lock.verify(self.project, self.sdk)["status"], "verified")

    def test_update_still_requires_compatible_contracts(self):
        path = self.project / lock.LOCK_NAME
        before = path.read_bytes()
        contract = self.sdk / "rust/contracts/io.capnp"
        contract.write_bytes(contract.read_bytes() + b"\n# incompatible\n")
        self.args.update = True
        with self.assertRaisesRegex(tool.ToolError, "IO contracts differ"):
            tool.lock_project_sdk(self.args)
        self.assertEqual(path.read_bytes(), before)

    def test_absent_lock_legacy_and_required_modes(self):
        (self.project / lock.LOCK_NAME).unlink()
        self.assertEqual(lock.verify(self.project, self.sdk), {"status": "absent"})
        with self.assertRaisesRegex(lock.SdkLockError, "required"):
            tool.preflight_project(self.args)
        self.args.require_sdk_lock = False
        self.assertEqual(tool.preflight_project(self.args)[-1]["status"], "absent")
        with self.assertRaisesRegex(lock.SdkLockError, "absent"):
            lock.write(self.project, self.sdk, update=True)
        lock.write(self.project, self.sdk)

    def test_duplicate_escape_unknown_fields_and_boolean_size_are_rejected(self):
        data = (self.project / lock.LOCK_NAME).read_bytes()
        text = data.decode()
        entry = text[text.index("[[files]]"):].split("\n\n", 1)[0]
        cases = [text + "\n" + entry, text.replace('path = "LICENSE"', 'path = "../LICENSE"'),
                 text.replace("schema = 1", "schema = true"), text.replace("schema = 1", "schema = 2"),
                 text.replace("schema = 1", "schema = 1\nunexpected = true"),
                 text.replace('profile = "' + lock.PROFILE + '"', 'profile = "unknown"')]
        import re
        cases.append(re.sub(r"size = [0-9]+", "size = true", text, count=1))
        cases.append(re.sub(r'sha256 = "[0-9a-f]+"', 'sha256 = "bad"', text, count=1))
        for value in cases:
            with self.subTest(value=value[:80]):
                with self.assertRaises(lock.SdkLockError):
                    lock.parse(value.encode())
        with self.assertRaisesRegex(lock.SdkLockError, "missing required"):
            lock.encode({"rust/src/a.rs": {"size": 0, "sha256": "0" * 64}})

    def symlink(self, path, target, directory=False):
        try:
            path.symlink_to(target, target_is_directory=directory)
        except (OSError, NotImplementedError) as error:
            self.skipTest(str(error))

    def test_lock_and_library_symlinks_rejected_without_following(self):
        path = self.project / lock.LOCK_NAME
        outside = self.root / "outside"
        outside.write_bytes(path.read_bytes())
        path.unlink()
        self.symlink(path, outside)
        before = outside.read_bytes()
        for action in (lambda: lock.verify(self.project, self.sdk), lambda: lock.write(self.project, self.sdk, update=True)):
            with self.assertRaisesRegex(lock.SdkLockError, "symlink|junction"):
                action()
        self.assertEqual(outside.read_bytes(), before)
        path.unlink()
        path.write_bytes(before)
        self.symlink(self.sdk / "c/include/escape.h", outside)
        with self.assertRaisesRegex(lock.SdkLockError, "symlink|junction"):
            lock.verify(self.project, self.sdk)

    def test_file_count_total_bytes_and_metadata_limits_are_enforced(self):
        for name, value in (("MAX_FILES", 1), ("MAX_TOTAL_BYTES", 4), ("MAX_FILE_BYTES", 4)):
            with self.subTest(name=name), mock.patch.object(lock, name, value):
                with self.assertRaises(lock.SdkLockError):
                    lock.inventory(self.sdk)
        with mock.patch.object(lock, "MAX_LOCK_BYTES", 8):
            with self.assertRaises(lock.SdkLockError):
                lock.verify(self.project, self.sdk)

    def test_creation_does_not_overwrite_a_concurrently_created_lock(self):
        path = self.project / lock.LOCK_NAME
        path.unlink()
        real_link = os.link
        def raced(source, destination):
            Path(destination).write_bytes(b"another writer")
            return real_link(source, destination)
        with mock.patch.object(lock.os, "link", side_effect=raced):
            with self.assertRaises(FileExistsError):
                lock.write(self.project, self.sdk)
        self.assertEqual(path.read_bytes(), b"another writer")
        self.assertEqual(list(self.project.glob("sdk-lock-*.tmp")), [])

    def test_real_cli_lock_and_strict_validate_without_toolchain(self):
        (self.project / lock.LOCK_NAME).unlink()
        env = {**os.environ, "PATH": ""}
        cli = [sys.executable, str(tool.ROOT / "tool/morrow_plugin.py")]
        for arguments in (["lock-sdk", str(self.project)], ["validate", str(self.project), "--require-sdk-lock"]):
            result = subprocess.run([*cli, *arguments, "--sdk-root", str(self.sdk)], env=env,
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stderr, "")
        result = subprocess.run([*cli, "lock-sdk", str(self.project), "--sdk-root", str(self.sdk)],
                                env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertIn("already exists", result.stderr)

    def test_sdk_or_lock_change_during_compile_prevents_packaging(self):
        self.args.sysroot = str(self.root)
        source = self.sdk / "c/src/morrow_plugin_sdk.c"
        original = source.read_bytes()
        lock_path = self.project / lock.LOCK_NAME
        pin = lock_path.read_bytes()
        for change in ("source", "lock", "lock-rewritten"):
            def compiler(arguments, **_kwargs):
                values = [str(v) for v in arguments]
                if "-o" in values:
                    output = Path(values[values.index("-o") + 1])
                    if output.name == "plugin.wasm":
                        output.write_bytes(b"\0asm\1\0\0\0")
                        if change == "source":
                            source.write_bytes(original + b"\n/* changed during build */\n")
                        elif change == "lock":
                            lock_path.unlink()
                        else:
                            lock_path.write_bytes(pin + b"\n")
                return ""
            with self.subTest(change=change), mock.patch.object(tool, "executable", side_effect=lambda name: name), \
                 mock.patch.object(tool, "run", side_effect=compiler), mock.patch.object(tool, "host_tool") as packager:
                with self.assertRaises(lock.SdkLockError):
                    tool.pack_project(self.args)
                packager.assert_not_called()
                self.assertFalse((self.project / "dist").exists())
            source.write_bytes(original)
            lock_path.write_bytes(pin)


if __name__ == "__main__":
    unittest.main()
