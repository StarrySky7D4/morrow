"""Offline project admission: no compiler, network, build output or authority."""
import contextlib
import copy
import hashlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import tomllib
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import morrow_plugin as tool
from test_plugin_project import write_config


class PreflightTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()

    def create(self, language="rust", kind="transform", service_http=False):
        args = SimpleNamespace(path=str(self.root / f"{language}-{kind}-{service_http} 空间"),
                               sdk_root=str(tool.ROOT / "sdk"), language=language, kind=kind,
                               id="org.example.preflight", version="1.2.3", name='预检 "插件"',
                               service_http=service_http, allow_network=False)
        with contextlib.redirect_stdout(io.StringIO()):
            tool.new_project(args)
        return args, Path(args.path)

    def capture(self, args):
        output = io.StringIO()
        with contextlib.redirect_stdout(output), \
             mock.patch.object(tool, "run", side_effect=AssertionError("external tool ran")), \
             mock.patch.object(tool, "executable", side_effect=AssertionError("toolchain inspected")), \
             mock.patch.object(tool, "host_tool", side_effect=AssertionError("host ran")):
            tool.validate_project(args)
        return tomllib.loads(output.getvalue())

    @staticmethod
    def files(root):
        return {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in root.rglob("*") if p.is_file()}

    def test_all_21_starters_are_read_only_and_report_their_declarations(self):
        for language in tool.LANGUAGES:
            profiles = [(kind, False) for kind in tool.KINDS] + [("service", True)]
            for kind, outbound in profiles:
                with self.subTest(language=language, kind=kind, outbound=outbound):
                    args, root = self.create(language, kind, outbound)
                    before = self.files(root)
                    result = self.capture(args)
                    _, config, _ = tool.project(root)
                    self.assertEqual(result["packager_arguments"], tool.package_arguments(config))
                    self.assertEqual(result["plugin_id"], config["plugin"]["id"])
                    self.assertEqual(result["content_capabilities"], config["plugin"]["capabilities"])
                    self.assertEqual(result["io_capabilities"], config.get("io", {}).get("capabilities", []))
                    self.assertEqual(result["dependency_slots"], [d["slot"] for d in config.get("dependencies", [])])
                    self.assertFalse(result["permissions_granted"])
                    self.assertFalse(result["plugin_executed"])
                    names = {c["name"] for c in result["contracts"]}
                    self.assertEqual("io.capnp" in names, kind == "io" or outbound)
                    self.assertEqual("service.capnp" in names, kind == "service")
                    for contract in result["contracts"]:
                        data = (tool.ROOT / "sdk/rust/contracts" / contract["name"]).read_bytes()
                        self.assertEqual(contract["sha256"], hashlib.sha256(data).hexdigest())
                    self.assertEqual(self.files(root), before)
                    self.assertFalse((root / "build").exists())
                    self.assertFalse((root / "dist").exists())

    def test_cli_succeeds_with_empty_path_and_unavailable_sysroot(self):
        args, root = self.create("cpp", "service", True)
        result = subprocess.run([sys.executable, "-X", "utf8", str(tool.ROOT / "tool/morrow_plugin.py"),
                                 "validate", str(root / "plugin.toml"), "--sysroot", str(root / "missing")],
                                env={**os.environ, "PATH": ""}, capture_output=True, text=True, encoding="utf-8")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(tomllib.loads(result.stdout)["result"], "valid")
        self.assertEqual(result.stderr, "")
        self.assertFalse((root / "build").exists())

    def test_invalid_project_has_no_partial_success_output(self):
        _, root = self.create()
        config = tool.read_toml(root / "plugin.toml")
        config["budget"]["fuel"] = True
        write_config(root / "plugin.toml", config)
        output, error = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(error):
            status = tool.main(["validate", str(root)])
        self.assertEqual(status, 1)
        self.assertEqual(output.getvalue(), "")
        self.assertIn("budget.fuel", error.getvalue())
        self.assertFalse((root / "build").exists())

    def test_invalid_rust_binding_fails_before_output_directory_or_compiler(self):
        args, root = self.create()
        original = tool.read_toml(root / "Cargo.toml")
        variants = []
        for field, value in [("name", "wrong"), ("path", "missing.rs"), ("crate-type", ["rlib"])]:
            changed = copy.deepcopy(original)
            changed["lib"][field] = value
            variants.append(changed)
        for field, value in [("path", "other-sdk"), ("features", [])]:
            changed = copy.deepcopy(original)
            changed["dependencies"]["morrow-plugin-sdk"][field] = value
            variants.append(changed)
        for changed in variants:
            write_config(root / "Cargo.toml", changed)
            for command in (tool.validate_project, tool.compile_project):
                with self.subTest(command=command.__name__, changed=changed), \
                     mock.patch.object(tool, "run", side_effect=AssertionError("compiler ran")), \
                     mock.patch.object(tool, "executable", side_effect=AssertionError("tool queried")):
                    with self.assertRaises(tool.ToolError):
                        command(args)
                    self.assertFalse((root / "build").exists())

    def test_existing_artifacts_are_neither_removed_nor_selected(self):
        args, root = self.create()
        (root / "build").mkdir()
        (root / "dist").mkdir()
        (root / "build/plugin.wasm").write_bytes(b"old invalid binary")
        (root / "dist/old.mplugin").write_bytes(b"previous package")
        before = self.files(root)
        self.assertEqual(self.capture(args)["result"], "valid")
        self.assertEqual(self.files(root), before)

    def test_deep_output_symlink_still_rejected_without_writes(self):
        args, root = self.create("c")
        build = root / "build/cache"
        build.mkdir(parents=True)
        outside = self.root / "untouched"
        outside.mkdir()
        (outside / "keep").write_bytes(b"keep")
        try:
            (build / "link").symlink_to(outside, target_is_directory=True)
        except (OSError, NotImplementedError) as error:
            self.skipTest(str(error))
        with self.assertRaisesRegex(tool.ToolError, "symlink|junction"):
            self.capture(args)
        self.assertEqual((outside / "keep").read_bytes(), b"keep")

    def external_sdk(self):
        external = self.root / "external-sdk"
        # Only files read by the preflight; not a deployable SDK distribution.
        names = ["rust/Cargo.toml", "c/include/morrow_plugin_task.h", "cpp/include/morrow_plugin_task.hpp"]
        names += ["rust/contracts/" + name for name in (
            "runtime.capnp", "content.proto", "task.capnp", "ui.capnp", "dependency_call.capnp",
            "io.capnp", "service.capnp", "service_resources.capnp", "version.txt", "task-version.txt", "ui-version.txt")]
        names += ["rust/src/" + name + ".rs" for name in ("dependency_call", "io", "service", "service_resources")]
        names += [f"{lang}/include/morrow_plugin_{name}.{ext}" for lang, ext in (("c", "h"), ("cpp", "hpp"))
                  for name in ("io", "service")]
        for name in names:
            destination = external / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(tool.ROOT / "sdk" / name, destination)
        return external

    def test_service_outbound_schema_and_version_drift_fail_before_build(self):
        args, root = self.create("c", "service", True)
        external = self.external_sdk()
        args.sdk_root = str(external)
        self.assertEqual(self.capture(args)["result"], "valid")
        for name in ("rust/contracts/io.capnp", "rust/src/io.rs"):
            path = external / name
            original = path.read_text()
            changed = original + "\n# contract drift\n" if name.endswith("capnp") else original.replace(
                "pub const VERSION: u16 = 1;", "pub const VERSION: u16 = 65535;")
            self.assertNotEqual(original, changed)
            path.write_text(changed)
            for command in (tool.validate_project, tool.compile_project):
                with self.subTest(name=name, command=command.__name__), \
                     mock.patch.object(tool, "run", side_effect=AssertionError("compiler ran")):
                    with self.assertRaisesRegex(tool.ToolError, "IO contracts differ|IO codec versions differ"):
                        command(args)
            self.assertFalse((root / "build").exists())
            path.write_text(original)

    def test_custom_outbound_service_without_directory_still_checks_io(self):
        args, root = self.create("c", "service")
        config = tool.read_toml(root / "plugin.toml")
        config["io"]["capabilities"].append("http-request")
        write_config(root / "plugin.toml", config)
        result = self.capture(args)
        self.assertIn("io.capnp", [c["name"] for c in result["contracts"]])
        self.assertFalse(result["service_resources"])


if __name__ == "__main__":
    unittest.main()
