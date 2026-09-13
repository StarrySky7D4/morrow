"""Trusted developer tool regressions: disposable projects and no compiler execution."""
import contextlib
import copy
import io
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import tomllib
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import morrow_plugin as tool


def toml_value(value):
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if type(value) is bool:
        return str(value).lower()
    if type(value) is int:
        return str(value)
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{key} = {toml_value(item)}" for key, item in value.items()) + " }"
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(item) for item in value) + "]"
    raise AssertionError(f"unsupported test TOML value {value!r}")


def write_config(path, config):
    lines = []
    for key, value in config.items():
        if not isinstance(value, dict) and not (isinstance(value, list) and value and isinstance(value[0], dict)):
            lines.append(f"{key} = {toml_value(value)}")
    for key, value in config.items():
        tables = [value] if isinstance(value, dict) else value if isinstance(value, list) and value and isinstance(value[0], dict) else []
        for table in tables:
            lines.append(f"[{key}]" if isinstance(value, dict) else f"[[{key}]]")
            lines.extend(f"{field} = {toml_value(item)}" for field, item in table.items())
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


class ProjectTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.sdk = tool.ROOT / "sdk"

    def args(self, path, language="rust", kind="dependency"):
        return SimpleNamespace(path=str(path), sdk_root=str(self.sdk), language=language,
                               kind=kind, id="org.example.fixture", version="1.2.3-test.1",
                               name="测试 plugin", allow_network=False, sysroot=str(self.root / "sysroot"))

    def new(self, name="project", language="rust", kind="dependency"):
        args = self.args(self.root / name, language, kind)
        with contextlib.redirect_stdout(io.StringIO()):
            tool.new_project(args)
        return args, tool.project(args.path)

    def test_all_three_languages_and_four_starters_have_expected_sources_and_locks(self):
        for language in tool.LANGUAGES:
            for kind in tool.KINDS:
                with self.subTest(language=language, kind=kind):
                    args, (root, config, source) = self.new(f"{language} {kind} 空间", language, kind)
                    profile = "task" if kind == "content" else "dependency-caller" if kind == "dependency" else kind
                    name = "src/lib.rs" if language == "rust" else "plugin.cpp" if language == "cpp" else "plugin.c"
                    self.assertEqual(source.read_bytes(), (self.sdk / "examples" / f"{language}-{profile}" / name).read_bytes())
                    self.assertEqual(config["build"]["language"], language)
                    self.assertEqual(config["plugin"]["dependency_calls"], kind == "dependency")
                    self.assertEqual(len(config.get("handlers", [])), {"content": 0, "transform": 3, "ui": 2, "dependency": 1}[kind])
                    self.assertEqual(config["plugin"]["capabilities"], list(tool.CAPABILITIES) if kind == "content" else ["read-content", "edit-content"] if kind == "dependency" else [])
                    self.assertEqual((root / "LICENSE").read_bytes(), (self.sdk / "LICENSE").read_bytes())
                    self.assertFalse((root / "build").exists())
                    self.assertFalse((root / "dist").exists())
                    if language == "rust":
                        cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
                        self.assertEqual(Path(cargo["dependencies"]["morrow-plugin-sdk"]["path"]).resolve(), (self.sdk / "rust").resolve())
                        self.assertEqual(cargo["dependencies"]["morrow-plugin-sdk"]["features"], ["wasm-guest"])
                        self.assertEqual(cargo["lib"]["name"], "morrow_plugin")
                        lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
                        own = [package for package in lock["package"] if package["name"] == cargo["package"]["name"]]
                        self.assertEqual(len(own), 1)
                        self.assertEqual(own[0]["version"], args.version)
                        self.assertIn("morrow-plugin-sdk", own[0]["dependencies"])
                        self.assertEqual(tool.project(root / "plugin.toml")[0], root)
                    else:
                        self.assertFalse((root / "Cargo.toml").exists())

    def test_existing_directory_is_never_overwritten(self):
        target = self.root / "existing"
        target.mkdir()
        marker = target / "keep.txt"
        marker.write_bytes(b"keep")
        with self.assertRaisesRegex(tool.ToolError, "refuses an existing"):
            tool.new_project(self.args(target))
        self.assertEqual(marker.read_bytes(), b"keep")
        self.assertEqual(list(target.iterdir()), [marker])

    def test_unknown_keys_in_all_tables_are_rejected(self):
        args, (root, original, _) = self.new()
        for table in [None, "plugin", "build", "budget", "handlers", "dependencies"]:
            with self.subTest(table=table):
                config = copy.deepcopy(original)
                destination = config if table is None else config[table][0] if table in ("handlers", "dependencies") else config[table]
                destination["misspelled"] = 1
                write_config(root / "plugin.toml", config)
                with self.assertRaisesRegex(tool.ToolError, "unknown field"):
                    tool.project(args.path)

    def test_boolean_is_not_an_integer_and_wrong_container_types_are_rejected(self):
        args, (root, original, _) = self.new()
        cases = [("schema", None, True), ("budget", "fuel", True), ("budget", "memory_bytes", True),
                 ("budget", "host_calls", False), ("handlers", "max_input_bytes", True),
                 ("handlers", "max_output_bytes", "32"), ("dependencies", "optional", 1),
                 ("plugin", "dependency_calls", 1), ("plugin", "capabilities", "rename"),
                 ("handlers", None, "not a list"), ("build", None, "not a table")]
        for table, field, value in cases:
            with self.subTest(table=table, field=field, value=value):
                config = copy.deepcopy(original)
                if field is None:
                    config[table] = value
                else:
                    destination = config[table][0] if table in ("handlers", "dependencies") else config[table]
                    destination[field] = value
                write_config(root / "plugin.toml", config)
                with self.assertRaises(tool.ToolError):
                    tool.project(args.path)

    def test_duplicate_capabilities_handlers_and_dependency_slots_are_rejected(self):
        args, (root, original, _) = self.new()
        for table in ("capabilities", "handlers", "dependencies"):
            with self.subTest(table=table):
                config = copy.deepcopy(original)
                entries = config["plugin"][table] if table == "capabilities" else config[table]
                entries.append(copy.deepcopy(entries[0]))
                write_config(root / "plugin.toml", config)
                with self.assertRaisesRegex(tool.ToolError, "unique|duplicate"):
                    tool.project(args.path)

    def test_missing_required_fields_are_rejected(self):
        args, (root, original, _) = self.new()
        for table, fields in [(None, ["schema", "plugin", "build"]), ("plugin", ["id", "version"]),
                              ("build", ["language", "source"]), ("handlers", ["name", "input_type", "output_type", "max_input_bytes", "max_output_bytes"]),
                              ("dependencies", ["slot", "handler", "input_type", "output_type", "provider_version"])]:
            for field in fields:
                with self.subTest(table=table, field=field):
                    config = copy.deepcopy(original)
                    destination = config if table is None else config[table][0] if table in ("handlers", "dependencies") else config[table]
                    del destination[field]
                    write_config(root / "plugin.toml", config)
                    with self.assertRaises(tool.ToolError):
                        tool.project(args.path)

    def test_relative_escape_and_windows_absolute_paths_are_rejected(self):
        args, (root, original, _) = self.new()
        (self.root / "outside.rs").write_text("outside", encoding="utf-8")
        for source in ["../outside.rs", "..\\outside.rs", "C:\\outside.rs", "C:outside.rs", "\\\\server\\share\\outside.rs", str(self.root / "outside.rs")]:
            with self.subTest(source=source):
                config = copy.deepcopy(original)
                config["build"]["source"] = source
                write_config(root / "plugin.toml", config)
                with self.assertRaisesRegex(tool.ToolError, "relative|escapes"):
                    tool.project(args.path)

    def symlink(self, link, destination, directory=False):
        try:
            link.symlink_to(destination, target_is_directory=directory)
        except (OSError, NotImplementedError) as error:
            self.skipTest(f"symlink unavailable on this host: {error}")

    def test_source_symlink_cannot_escape_project(self):
        args, (root, config, _) = self.new()
        outside = self.root / "outside.rs"
        outside.write_text("outside", encoding="utf-8")
        self.symlink(root / "source-link.rs", outside)
        config["build"]["source"] = "source-link.rs"
        write_config(root / "plugin.toml", config)
        with self.assertRaisesRegex(tool.ToolError, "escapes"):
            tool.project(args.path)

    def test_build_and_dist_symlinks_are_rejected_before_compilation(self):
        for name in ("build", "dist"):
            with self.subTest(output=name):
                args, (root, _, _) = self.new(name)
                destination = self.root / f"outside-{name}"
                destination.mkdir()
                marker = destination / "keep"
                marker.write_bytes(b"outside remains intact")
                self.symlink(root / name, destination, True)
                with mock.patch.object(tool, "compile_project") as compile_mock:
                    with self.assertRaisesRegex(tool.ToolError, "escapes|symlinks|junctions"):
                        tool.pack_project(args)
                    compile_mock.assert_not_called()
                self.assertEqual(marker.read_bytes(), b"outside remains intact")

    def test_internal_output_symlinks_are_also_rejected(self):
        args, (root, _, _) = self.new()
        destination = root / "internal-output"
        destination.mkdir()
        self.symlink(root / "build", destination, True)
        with self.assertRaisesRegex(tool.ToolError, "symlinks|junctions"):
            tool.project(args.path)

    def test_compile_failure_never_reads_old_module_or_calls_packager(self):
        args, (root, _, _) = self.new()
        (root / "build").mkdir()
        stale = root / "build/plugin.wasm"
        stale.write_bytes(b"old stale module")
        with mock.patch.object(tool, "compile_project", side_effect=tool.ToolError("compiler failed")), \
             mock.patch.object(tool, "host_tool") as publish, \
             mock.patch.object(Path, "read_bytes", side_effect=AssertionError("stale module read")):
            with self.assertRaisesRegex(tool.ToolError, "compiler failed"):
                tool.pack_project(args)
            publish.assert_not_called()
        self.assertEqual(stale.read_bytes(), b"old stale module")
        self.assertFalse((root / "dist").exists())

    def test_configuration_changed_during_build_stops_packaging(self):
        args, (root, original, _) = self.new()
        def changed_build(*_args):
            changed = copy.deepcopy(original)
            changed["plugin"]["version"] = "9.9.9"
            write_config(root / "plugin.toml", changed)
            return root / "build/plugin.wasm"
        with mock.patch.object(tool, "compile_project", side_effect=changed_build), mock.patch.object(tool, "host_tool") as publish:
            with self.assertRaisesRegex(tool.ToolError, "metadata changed during build"):
                tool.pack_project(args)
            publish.assert_not_called()
        self.assertFalse((root / "dist").exists())

    def test_subprocess_uses_argument_vector_and_no_shell_for_unicode_paths(self):
        arguments = [self.root / "tool with 空格.exe", "--input", self.root / "space ünicode.wasm", ">=1.0, <2.0", 'quoted"argument']
        result = SimpleNamespace(returncode=0, stdout="success")
        with mock.patch.object(tool.subprocess, "run", return_value=result) as process:
            self.assertEqual(tool.run(arguments, cwd=self.root, capture=True), "success")
        positional, keywords = process.call_args
        self.assertEqual(positional, ([str(argument) for argument in arguments],))
        self.assertIs(keywords["shell"], False)
        self.assertEqual(keywords["cwd"], self.root)
        self.assertFalse(keywords["check"])
        self.assertEqual(keywords["stdout"], subprocess.PIPE)

    def test_toml_quote_preserves_paths_and_dependency_version_is_one_argument(self):
        for value in ['C:\\Some space\\SDK "quoted"\\插件', '/some path/sdk "quoted"/插件']:
            self.assertEqual(tomllib.loads("path = " + tool.quote(value))["path"], value)
        _, (_, config, _) = self.new()
        config["dependencies"][0]["provider_version"] = ">=1.0, <2.0"
        arguments = tool.package_arguments(config)
        index = arguments.index("--dependency")
        self.assertEqual(arguments[index + 5], ">=1.0, <2.0")
        self.assertEqual(arguments[index + 6], "required")


    def test_metadata_limits_count_utf8_bytes_and_new_rejects_before_creating(self):
        args, (root, original, _) = self.new()
        for field, valid, invalid in [("name", "🌈" * 32, "🌈" * 33),
                                      ("version", "1.2.3+" + "v" * 122, "1.2.3+" + "v" * 123)]:
            with self.subTest(field=field):
                config = copy.deepcopy(original)
                config["plugin"][field] = valid
                write_config(root / "plugin.toml", config)
                self.assertEqual(tool.project(args.path)[1]["plugin"][field], valid)
                config["plugin"][field] = invalid
                write_config(root / "plugin.toml", config)
                with self.assertRaises(tool.ToolError):
                    tool.project(args.path)
                target = self.root / f"invalid-{field}"
                bad = self.args(target)
                setattr(bad, field, invalid)
                with self.assertRaises(tool.ToolError):
                    tool.new_project(bad)
                self.assertFalse(target.exists())

    def test_deep_build_reparse_is_rejected_before_any_compiler_runs(self):
        for language in ("rust", "c", "cpp"):
            with self.subTest(language=language):
                args, (root, _, _) = self.new(f"deep-{language}", language)
                parent = root / "build/cache/target/release/build/dependency-id"
                parent.mkdir(parents=True)
                outside = self.root / f"external-{language}"
                outside.mkdir()
                marker = outside / "keep"
                marker.write_bytes(b"not compiler output")
                self.symlink(parent / "out", outside, True)
                with mock.patch.object(tool, "executable", side_effect=lambda name: name), \
                     mock.patch.object(tool, "run") as compiler:
                    with self.assertRaisesRegex(tool.ToolError, "symlink|junction|reparse"):
                        tool.compile_project(args)
                    compiler.assert_not_called()
                self.assertEqual(marker.read_bytes(), b"not compiler output")

    def test_build_tree_scan_has_a_real_entry_budget(self):
        _, (root, _, _) = self.new()
        build = root / "build"
        build.mkdir()
        for index in range(4):
            (build / f"entry-{index}").write_bytes(b"data")
        with self.assertRaises(tool.ToolError):
            tool.validate_build_tree(root, maximum_entries=2)
        tool.validate_build_tree(root, maximum_entries=20)
        self.assertEqual(len(list(build.iterdir())), 4)

    def test_malformed_cargo_tables_and_fields_raise_toolerror_before_build(self):
        args, (root, _, _) = self.new()
        path = root / "Cargo.toml"
        original = tomllib.loads(path.read_text(encoding="utf-8"))
        cases = [("lib", None, 1), ("dependencies", None, "not a table"),
                 ("lib", "path", False), ("lib", "crate-type", "cdylib"),
                 ("lib", "crate-type", ["cdylib", False]),
                 ("dependencies", "morrow-plugin-sdk", "version string"),
                 ("sdk", "path", 17), ("sdk", "path", []),
                 ("sdk", "features", "wasm-guest"), ("sdk", "features", ["wasm-guest", 1]),
                 ("sdk", "features", False)]
        for table, field, value in cases:
            with self.subTest(table=table, field=field, value=value):
                config = copy.deepcopy(original)
                if field is None:
                    config[table] = value
                else:
                    destination = config["dependencies"]["morrow-plugin-sdk"] if table == "sdk" else config[table]
                    destination[field] = value
                write_config(path, config)
                with mock.patch.object(tool, "executable", return_value="cargo"), \
                     mock.patch.object(tool, "run") as compiler:
                    with self.assertRaises(tool.ToolError):
                        tool.compile_project(args)
                    compiler.assert_not_called()

    def test_c_and_cpp_linker_use_configured_32mib_memory_budget(self):
        for language in ("c", "cpp"):
            with self.subTest(language=language):
                args, (root, config, _) = self.new(f"memory-{language}", language)
                config["budget"]["memory_bytes"] = 32 * 1024 * 1024
                write_config(root / "plugin.toml", config)
                Path(args.sysroot).mkdir(exist_ok=True)
                def fake_compiler(arguments, **_kwargs):
                    tokens = [str(argument) for argument in arguments]
                    if "-o" in tokens:
                        output = Path(tokens[tokens.index("-o") + 1])
                        if output.name == "plugin.wasm":
                            output.write_bytes(b"\0asm\1\0\0\0")
                    return ""
                with mock.patch.object(tool, "executable", side_effect=lambda name: name), \
                     mock.patch.object(tool, "run", side_effect=fake_compiler) as compiler, \
                     contextlib.redirect_stdout(io.StringIO()):
                    module = tool.compile_project(args)
                self.assertEqual(module, root / "build/plugin.wasm")
                links = [[str(value) for value in call.args[0]] for call in compiler.call_args_list
                         if "-Wl,--no-entry" in [str(value) for value in call.args[0]]]
                self.assertEqual(len(links), 1)
                self.assertIn("-Wl,--max-memory=33554432", links[0])
                self.assertNotIn("-Wl,--max-memory=16777216", links[0])



    def test_external_sdk_version_drift_with_unchanged_schemas_stops_before_compile(self):
        args, (_, _, _) = self.new()
        external = self.root / "SDK copy 空间"
        files = ["rust/Cargo.toml", "c/include/morrow_plugin_task.h", "cpp/include/morrow_plugin_task.hpp",
                 "rust/src/dependency_call.rs"]
        schemas = ["runtime.capnp", "content.proto", "task.capnp", "ui.capnp", "dependency_call.capnp"]
        versions = ["version.txt", "task-version.txt", "ui-version.txt"]
        files += ["rust/contracts/" + name for name in schemas + versions]
        for name in files:
            target = external / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(self.sdk / name, target)
        args.sdk_root = str(external)
        original_schemas = {name: (external / "rust/contracts" / name).read_bytes() for name in schemas}
        self.assertEqual(tool.sdk(args), external)
        for name in ["rust/contracts/" + version for version in versions] + ["rust/src/dependency_call.rs"]:
            with self.subTest(version_file=name):
                target = external / name
                original = target.read_bytes()
                if name.endswith(".rs"):
                    text = original.decode("utf-8")
                    import re
                    changed = re.sub(r"pub const VERSION: u16 = [0-9]+;", "pub const VERSION: u16 = 65535;", text)
                    self.assertNotEqual(changed, text)
                    target.write_text(changed, encoding="utf-8")
                else:
                    target.write_text("65535\n", encoding="ascii")
                with mock.patch.object(tool, "run") as compiler, mock.patch.object(tool, "host_tool") as packager:
                    with self.assertRaisesRegex(tool.ToolError, "versions differ"):
                        tool.compile_project(args)
                    compiler.assert_not_called()
                    packager.assert_not_called()
                self.assertEqual({key: (external / "rust/contracts" / key).read_bytes() for key in schemas}, original_schemas)
                target.write_bytes(original)
                self.assertEqual(tool.sdk(args), external)

    def test_semver_components_fit_u64_and_prerelease_numeric_ids_are_canonical(self):
        args, (root, original, _) = self.new()
        for component in range(3):
            with self.subTest(component=component):
                values = ["0", "0", "0"]
                values[component] = str(2**64 - 1)
                config = copy.deepcopy(original)
                config["plugin"]["version"] = ".".join(values)
                write_config(root / "plugin.toml", config)
                self.assertEqual(tool.project(args.path)[1]["plugin"]["version"], ".".join(values))
                values[component] = str(2**64)
                config["plugin"]["version"] = ".".join(values)
                write_config(root / "plugin.toml", config)
                with self.assertRaisesRegex(tool.ToolError, "UInt64"):
                    tool.project(args.path)
        for version in ["1.0.0-01", "1.0.0-rc.01", "01.0.0"]:
            with self.subTest(version=version):
                config = copy.deepcopy(original)
                config["plugin"]["version"] = version
                write_config(root / "plugin.toml", config)
                with self.assertRaises(tool.ToolError):
                    tool.project(args.path)

    def test_provider_version_utf8_limit_and_unicode_controls_fail_preflight(self):
        args, (root, original, _) = self.new()
        config = copy.deepcopy(original)
        config["dependencies"][0]["provider_version"] = "*" + " " * 127
        write_config(root / "plugin.toml", config)
        tool.project(args.path)
        for value in ["*" + " " * 128, "汉" * 43]:
            with self.subTest(provider=value):
                config["dependencies"][0]["provider_version"] = value
                write_config(root / "plugin.toml", config)
                with self.assertRaisesRegex(tool.ToolError, "128 UTF-8 bytes"):
                    tool.project(args.path)
        for table, field in [("plugin", "name"), ("handlers", "name"), ("dependencies", "slot"), ("dependencies", "provider_version")]:
            for control in ["\u0085", "\u009f"]:
                with self.subTest(table=table, field=field, control=repr(control)):
                    config = copy.deepcopy(original)
                    target = config[table] if table == "plugin" else config[table][0]
                    target[field] = "valid" + control
                    write_config(root / "plugin.toml", config)
                    with self.assertRaisesRegex(tool.ToolError, "control characters"):
                        tool.project(args.path)

    def test_handler_type_and_slot_identifiers_reject_path_syntax_before_compile(self):
        args, (root, original, _) = self.new()
        for table, fields in [("handlers", ["name", "input_type", "output_type"]),
                              ("dependencies", ["slot", "handler", "input_type", "output_type"])]:
            for field in fields:
                for separator in ["/", "\\", ":"]:
                    with self.subTest(table=table, field=field, separator=separator):
                        config = copy.deepcopy(original)
                        config[table][0][field] = "first" + separator + "second"
                        write_config(root / "plugin.toml", config)
                        with mock.patch.object(tool, "compile_project") as compiler:
                            with self.assertRaisesRegex(tool.ToolError, "path separators or colon"):
                                tool.pack_project(args)
                            compiler.assert_not_called()


if __name__ == "__main__":
    unittest.main()
