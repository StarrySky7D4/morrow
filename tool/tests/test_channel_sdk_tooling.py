"""Channel tooling admission in disposable exact-source copies; no compilers.

These are metadata/source checks, not guest execution or SDK qualification.
"""
import contextlib
import hashlib
import io
from pathlib import Path
import shutil
import sys
import tempfile
from types import SimpleNamespace
import tomllib
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import morrow_plugin as tool

REPO = Path(__file__).resolve().parents[2]
DECLARATION = "pub const VERSION: u32 = 1;"


class ChannelSdkToolingTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="morrow channel tooling ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.host = self.root / "copied-host"
        self.sdk = self.root / "external-sdk"
        # Copy current source, not synthetic matching schemas or version files.
        for folder in ("rust/src", "rust/contracts", "c/include", "cpp/include", "c/src", "cpp/src"):
            shutil.copytree(REPO / "sdk" / folder, self.sdk / folder)
        for name in tool.sdk_lock.ROOT_FILES:
            self.copy(REPO / "sdk" / name, self.sdk / name)
        for language in tool.LANGUAGES:
            for profile in ("channel", "channel-directory", "transform"):
                name = f"examples/{language}-{profile}"
                shutil.copytree(REPO / "sdk" / name, self.sdk / name)
        # Host command preflight checks its manifest before mocked host_tool.
        self.copy(REPO / "plugin_runtime/Cargo.toml", self.host / "plugin_runtime/Cargo.toml")
        self.copy(REPO / "core/Cargo.toml", self.host / "core/Cargo.toml")
        shutil.copytree(REPO / "core/schemas", self.host / "core/schemas")
        for name in ("runtime", "task", "ui", "dependency_call", "channel"):
            self.copy(REPO / f"core/src/{name}.rs", self.host / f"core/src/{name}.rs")
        patch = mock.patch.object(tool, "ROOT", self.host)
        patch.start()
        self.addCleanup(patch.stop)

    @staticmethod
    def copy(source, target):
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)

    @staticmethod
    def snapshot(root):
        return {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in root.rglob("*") if p.is_file()}

    def args(self, name="project", language="rust", kind="channel"):
        return SimpleNamespace(path=str(self.root / name), sdk_root=str(self.sdk),
                               language=language, kind=kind, id="channel.tooling.fixture",
                               version="0.1.0", name=None, allow_network=False)

    def new(self, language="rust", kind="channel", profile=None):
        args = self.args(f"{language}-{kind}-{profile}", language, kind)
        args.channel_input = profile
        with contextlib.redirect_stdout(io.StringIO()):
            tool.new_project(args)
        return args

    def version(self, source, declaration):
        saved = source.read_bytes()
        text = saved.decode("utf-8").replace("\r\n", "\n")
        self.assertEqual(text.count(DECLARATION), 1)
        source.write_text(text.replace(DECLARATION, declaration), encoding="utf-8")
        return saved

    def test_original_external_sdk_all_languages_and_profiles_validate_read_only(self):
        for language in tool.LANGUAGES:
            for profile in (None, "directory"):
                with self.subTest(language=language, profile=profile):
                    args = self.new(language, profile=profile)
                    # validate/build/pack derive kind from metadata, not args.kind.
                    del args.kind
                    root = Path(args.path)
                    before = self.snapshot(root)
                    output = io.StringIO()
                    with contextlib.redirect_stdout(output), \
                         mock.patch.object(tool, "run", side_effect=AssertionError("compiler ran")), \
                         mock.patch.object(tool, "executable", side_effect=AssertionError("tool queried")):
                        tool.validate_project(args)
                    result = tomllib.loads(output.getvalue())
                    self.assertEqual(result["kind"], "channel")
                    self.assertFalse(result["plugin_executed"])
                    self.assertFalse(result["permissions_granted"])
                    self.assertIn("channel.capnp", [c["name"] for c in result["contracts"]])
                    self.assertEqual(self.snapshot(root), before)
                    self.assertFalse((root / "build").exists())

    def test_duplicate_versions_fail_closed_for_both_host_and_sdk(self):
        for source in (self.sdk / "rust/src/channel.rs", self.host / "core/src/channel.rs"):
            for extra in (DECLARATION, "pub const VERSION: u16 = 1;", "const VERSION: u32 = 1;"):
                with self.subTest(source=source, extra=extra):
                    saved = self.version(source, DECLARATION + "\n" + extra)
                    try:
                        with self.assertRaises(tool.ToolError):
                            tool.sdk_channel(self.sdk)
                    finally:
                        source.write_bytes(saved)

    def test_missing_wrong_type_expression_and_malformed_versions_fail_closed(self):
        declarations = ("", "pub const VERSION: u16 = 1;", "pub const VERSION: u64 = 1;",
                        "pub const VERSION: u32 = 1 + 0;", "pub const VERSION: u32 = -1;",
                        "pub const VERSION: u32 = 1", "// " + DECLARATION,
                        "/*\n" + DECLARATION + "\n*/",
                        "/* outer /* inner */\n" + DECLARATION + "\n*/")
        for source in (self.sdk / "rust/src/channel.rs", self.host / "core/src/channel.rs"):
            for declaration in declarations:
                with self.subTest(source=source, declaration=declaration):
                    saved = self.version(source, declaration)
                    try:
                        with self.assertRaises(tool.ToolError):
                            tool.sdk_channel(self.sdk)
                    finally:
                        source.write_bytes(saved)

    def test_matching_u32_overflow_is_not_accepted(self):
        for number in ("4294967296", "9" * 80, "9" * 5000):
            with self.subTest(digits=len(number)):
                sources = (self.sdk / "rust/src/channel.rs", self.host / "core/src/channel.rs")
                saved = [self.version(p, "pub const VERSION: u32 = " + number + ";") for p in sources]
                try:
                    with self.assertRaises(tool.ToolError):
                        tool.sdk_channel(self.sdk)
                finally:
                    for p, raw in zip(sources, saved):
                        p.write_bytes(raw)

    def test_matching_full_u32_value_is_not_artificially_limited_to_u16(self):
        sources = (self.sdk / "rust/src/channel.rs", self.host / "core/src/channel.rs")
        for p in sources:
            self.version(p, "pub const VERSION: u32 = 4294967295;")
        tool.sdk_channel(self.sdk)  # source-shape observation only; never compiled

    def test_channel_schema_normalizes_crlf_only(self):
        path = self.sdk / "rust/contracts/channel.capnp"
        saved = path.read_bytes()
        path.write_bytes(saved.replace(b"\r\n", b"\n").replace(b"\n", b"\r\n"))
        tool.sdk_channel(self.sdk)
        path.write_bytes(saved + b"\n# actual schema drift\n")
        with self.assertRaisesRegex(tool.ToolError, "channel contracts differ"):
            tool.sdk_channel(self.sdk)
        path.write_bytes(saved.replace(b"\n", b"\r"))
        with self.assertRaisesRegex(tool.ToolError, "channel contracts differ"):
            tool.sdk_channel(self.sdk)

    def test_missing_channel_schema_or_source_fails_closed(self):
        for path in (self.sdk / "rust/contracts/channel.capnp", self.sdk / "rust/src/channel.rs"):
            with self.subTest(path=path):
                saved = path.read_bytes()
                path.unlink()
                try:
                    with self.assertRaises((tool.ToolError, OSError)):
                        tool.sdk_channel(self.sdk)
                finally:
                    path.write_bytes(saved)

    def test_channel_wire_version_mismatch_fails_closed(self):
        self.version(self.sdk / "rust/src/channel.rs", "pub const VERSION: u32 = 2;")
        with self.assertRaisesRegex(tool.ToolError, "channel codec versions differ"):
            tool.sdk_channel(self.sdk)

    def test_string_and_raw_string_version_lookalikes_are_not_declarations(self):
        replacements = (f'pub const DOC: &str = "\n{DECLARATION}\n";',
                        f'pub const DOC: &str = r#"\n{DECLARATION}\n"#;',
                        f'pub const DOC: &[u8] = br##"\n{DECLARATION}\n"##;',
                        f'pub const DOC: &str = r#"\n{DECLARATION}\n";',
                        f'pub const DOC: &str = "\n{DECLARATION}\n')
        for source in (self.sdk / "rust/src/channel.rs", self.host / "core/src/channel.rs"):
            for replacement in replacements:
                with self.subTest(source=source, replacement=replacement):
                    saved = self.version(source, replacement)
                    try:
                        with self.assertRaises(tool.ToolError):
                            tool.sdk_channel(self.sdk)
                    finally:
                        source.write_bytes(saved)

    def test_comment_or_literal_lookalike_cannot_mask_a_mismatching_real_version(self):
        for lookalike in ("// " + DECLARATION,
                          'pub const DOC: &str = r#"\n' + DECLARATION + '\n"#;'):
            source = self.sdk / "rust/src/channel.rs"
            saved = self.version(source, lookalike + "\npub const VERSION: u32 = 2;")
            try:
                with self.assertRaisesRegex(tool.ToolError, "channel codec versions differ"):
                    tool.sdk_channel(self.sdk)
            finally:
                source.write_bytes(saved)

    def test_nested_module_function_or_uninvoked_macro_is_not_a_top_level_version(self):
        replacements = ("mod nested {\n" + DECLARATION + "\n}",
                        "fn unused() {\n" + DECLARATION + "\n}",
                        "macro_rules! unused { () => {\n" + DECLARATION + "\n}; }",
                        "macro_rules! unused { () => [\n" + DECLARATION + "\n]; }")
        for source in (self.sdk / "rust/src/channel.rs", self.host / "core/src/channel.rs"):
            for replacement in replacements:
                with self.subTest(source=source, replacement=replacement):
                    saved = self.version(source, replacement)
                    try:
                        with self.assertRaises(tool.ToolError):
                            tool.sdk_channel(self.sdk)
                    finally:
                        source.write_bytes(saved)

    def test_valid_top_level_version_ignores_literal_comments_and_lifetime_delimiters(self):
        prefix = ('pub const DOC: &str = r#"\n{ ' + DECLARATION + ' }\n"#;\n'
                  "/* braces { [ ( and nested /* comment */ are masked */\n"
                  "fn unused<'a>(value: &'a str) -> &'a str { let c = '\"'; value }\n")
        for source in (self.sdk / "rust/src/channel.rs", self.host / "core/src/channel.rs"):
            source.write_bytes(prefix.encode("utf-8") + source.read_bytes())
        tool.sdk_channel(self.sdk)

    def test_channel_header_presence_follows_actual_language_route(self):
        cpp = self.sdk / "cpp/include/morrow_channel_v1.hpp"
        c = self.sdk / "c/include/morrow_channel_v1.h"
        cpp.unlink()
        self.new("rust")
        self.new("c")
        with self.assertRaisesRegex(tool.ToolError, "incomplete channel SDK root"):
            self.new("cpp")
        c.unlink()
        tool.sdk_channel(self.sdk, "rust")
        with self.assertRaisesRegex(tool.ToolError, "incomplete channel SDK root"):
            tool.sdk_channel(self.sdk, "c")

    def test_failed_channel_lock_update_preserves_existing_exact_sdk_lock(self):
        args = self.new()
        root = Path(args.path)
        result = tool.sdk_lock.write(root, self.sdk)
        self.assertEqual(result["files"], 62)
        self.version(self.sdk / "rust/src/channel.rs", DECLARATION + "\n" + DECLARATION)
        before = self.snapshot(root)
        for option in ([], ["--update"]):
            with self.subTest(option=option), contextlib.redirect_stderr(io.StringIO()), \
                 mock.patch.object(tool.sdk_lock, "write", side_effect=AssertionError("lock write attempted")), \
                 mock.patch.object(tool, "executable", side_effect=AssertionError("tool queried")):
                self.assertEqual(tool.main(["lock-sdk", args.path, "--sdk-root", str(self.sdk), *option]), 1)
            self.assertEqual(self.snapshot(root), before)
            self.assertFalse((root / "build").exists())

    def test_invalid_channel_new_validate_build_and_pack_fail_before_side_effects(self):
        for language in tool.LANGUAGES:
            args = self.new(language)
            del args.kind
            source = self.sdk / "rust/src/channel.rs"
            saved = self.version(source, DECLARATION + "\n" + DECLARATION)
            try:
                for command in ("validate", "build", "pack", "lock-sdk"):
                    with self.subTest(language=language, command=command):
                        before = self.snapshot(Path(args.path))
                        out, err = io.StringIO(), io.StringIO()
                        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err), \
                             mock.patch.object(tool, "run", side_effect=AssertionError("compiler ran")), \
                             mock.patch.object(tool, "executable", side_effect=AssertionError("tool queried")), \
                             mock.patch.object(tool, "host_tool", side_effect=AssertionError("host ran")):
                            status = tool.main([command, args.path, "--sdk-root", str(self.sdk)])
                        self.assertEqual(status, 1)
                        self.assertIn("channel", err.getvalue())
                        self.assertEqual(out.getvalue(), "")
                        self.assertEqual(self.snapshot(Path(args.path)), before)
                        self.assertFalse((Path(args.path) / "build").exists())
                        self.assertFalse((Path(args.path) / "dist").exists())
                target = self.root / f"uncreated/{language}"
                with contextlib.redirect_stderr(io.StringIO()), \
                     mock.patch.object(tool, "run", side_effect=AssertionError("compiler ran")):
                    self.assertEqual(tool.main(["new", str(target), "--language", language,
                        "--kind", "channel", "--sdk-root", str(self.sdk), "--id", "channel.invalid"]), 1)
                self.assertFalse(target.parent.exists())
            finally:
                source.write_bytes(saved)

    def test_non_channel_legacy_roots_and_no_kind_sdk_call_remain_accepted(self):
        for name in ("rust/src/channel.rs", "rust/contracts/channel.capnp",
                     "c/include/morrow_channel_v1.h", "cpp/include/morrow_channel_v1.hpp"):
            (self.sdk / name).unlink()
        tool.sdk(SimpleNamespace(sdk_root=str(self.sdk)))
        for language in tool.LANGUAGES:
            args = self.new(language, "transform")
            del args.kind
            with contextlib.redirect_stdout(io.StringIO()), \
                 mock.patch.object(tool, "executable", side_effect=AssertionError("tool queried")):
                tool.validate_project(args)

    def test_package_check_is_a_host_route_without_sdk_or_kind_selection(self):
        package = self.root / "fixture.mplugin"
        package.write_bytes(b"host-only test input")
        with contextlib.redirect_stdout(io.StringIO()), \
             mock.patch.object(tool, "sdk", side_effect=AssertionError("SDK inspected")), \
             mock.patch.object(tool, "host_tool", return_value="host preparation fixture") as host:
            self.assertEqual(tool.main(["check", str(package), "--sdk-root", str(self.root / "absent-sdk")]), 0)
        self.assertEqual(host.call_args.args[0], "plugin_check")
        self.assertEqual(host.call_args.args[1], ["check", package.resolve()])


if __name__ == "__main__":
    unittest.main()
