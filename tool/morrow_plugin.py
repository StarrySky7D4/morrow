"""Morrow SDK project tools. Python 3.11+, trusted local developer environment."""
import argparse
import hashlib
import os
from pathlib import Path, PureWindowsPath
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import tomllib
import unicodedata

ROOT = Path(__file__).resolve().parents[1]
CAPABILITIES = ("rename", "summary", "operation", "attachment", "create-content", "edit-content", "read-content")
KINDS = ("content", "transform", "ui", "dependency")
LANGUAGES = ("rust", "c", "cpp")

class ToolError(Exception):
    pass

def only(value, keys, label):
    if not isinstance(value, dict):
        raise ToolError(f"{label}: expected a table")
    unknown = set(value) - set(keys)
    if unknown:
        raise ToolError(f"{label}: unknown field(s): {', '.join(sorted(unknown))}")
    return value

def string(value, label, maximum=256):
    if not isinstance(value, str) or not value or len(value.encode("utf-8")) > maximum or any(unicodedata.category(c) == "Cc" for c in value):
        raise ToolError(f"{label}: expected nonempty text without control characters (at most {maximum} UTF-8 bytes)")
    return value

def integer(value, low, high, label):
    if type(value) is not int or not low <= value <= high:
        raise ToolError(f"{label}: expected an integer in {low}..{high}")
    return value

def boolean(value, label):
    if type(value) is not bool:
        raise ToolError(f"{label}: expected true or false")
    return value

def semver(value, label):
    string(value, label, 128)
    numeric = r"(?:0|[1-9][0-9]*)"
    if not re.fullmatch(numeric + r"\." + numeric + r"\." + numeric + r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?", value):
        raise ToolError(label + ": expected a SemVer such as 0.1.0 or 0.1.0-test.1")
    components = value.split("+", 1)[0].split("-", 1)[0].split(".")
    if any(int(part) > 18446744073709551615 for part in components):
        raise ToolError(label + ": version components must fit UInt64")
    prerelease = value.split("+", 1)[0].partition("-")[2]
    if any(part.isdigit() and len(part) > 1 and part.startswith("0") for part in prerelease.split(".")):
        raise ToolError(label + ": numeric prerelease identifiers must not have leading zeros")
    return value

def path_text(value):
    string(str(value), "path", 32768)
    return Path(value)

def reparse(path):
    try:
        entry = path.lstat()
    except FileNotFoundError:
        return False
    return stat.S_ISLNK(entry.st_mode) or bool(getattr(entry, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400))

def child(root, relative, *, output=False):
    raw = string(relative, "project-relative path", 32768).replace("\\", "/")
    if Path(raw).is_absolute() or PureWindowsPath(raw).drive:
        raise ToolError("project path must be relative: " + relative)
    candidate = root / raw
    resolved = candidate.resolve()
    if not resolved.is_relative_to(root) or resolved == root:
        raise ToolError("project path escapes its root: " + relative)
    if output:
        current = root
        for part in Path(raw).parts:
            current = current / part
            if reparse(current):
                raise ToolError("build/output paths must not use symlinks or junctions: " + str(current))
    return resolved

def validate_build_tree(root, maximum_entries=200000):
    # Preflight existing incremental artifacts without following directory links.
    # This is not a sandbox: trusted build scripts can still execute local code.
    pending = [child(root, "build", output=True)]
    remaining = maximum_entries
    while pending:
        directory = pending.pop()
        if not directory.exists():
            continue
        if reparse(directory) or not directory.is_dir():
            raise ToolError("build tree must contain regular directories: " + str(directory))
        with os.scandir(directory) as entries:
            for entry in entries:
                remaining -= 1
                if remaining < 0:
                    raise ToolError("build tree exceeds preflight entry budget")
                metadata = entry.stat(follow_symlinks=False)
                if stat.S_ISLNK(metadata.st_mode) or bool(getattr(metadata, "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)):
                    raise ToolError("build tree must not use symlinks or junctions: " + entry.path)
                if stat.S_ISDIR(metadata.st_mode):
                    pending.append(Path(entry.path))
                elif not stat.S_ISREG(metadata.st_mode):
                    raise ToolError("build tree contains a nonregular entry: " + entry.path)


def read_toml(path):
    try:
        if path.stat().st_size > 65536:
            raise ToolError("project metadata exceeds 64 KiB: " + str(path))
        return tomllib.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as error:
        raise ToolError(f"Cannot read {path}: {error}") from error

def project(path):
    root = path_text(path).resolve(strict=True)
    if root.is_file():
        if root.name != "plugin.toml":
            raise ToolError("expected a project directory or plugin.toml")
        root = root.parent
    config = only(read_toml(root / "plugin.toml"), ("schema", "plugin", "build", "budget", "handlers", "dependencies"), "project")
    integer(config.get("schema"), 1, 1, "schema")
    plugin = only(config.get("plugin"), ("id", "version", "name", "capabilities", "dependency_calls"), "plugin")
    identifier = string(plugin.get("id"), "plugin.id")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", identifier):
        raise ToolError("plugin.id: use 1..128 ASCII letters, digits, dots, underscores or hyphens")
    semver(plugin.get("version"), "plugin.version")
    string(plugin.get("name", identifier), "plugin.name", 128)
    caps = plugin.get("capabilities", [])
    if not isinstance(caps, list) or any(c not in CAPABILITIES for c in caps) or len(caps) != len(set(caps)):
        raise ToolError("plugin.capabilities: use unique supported capability names")
    boolean(plugin.get("dependency_calls", False), "plugin.dependency_calls")
    build = only(config.get("build"), ("language", "source"), "build")
    if build.get("language") not in LANGUAGES:
        raise ToolError("build.language: expected rust, c or cpp")
    source = child(root, build.get("source"))
    if not source.is_file():
        raise ToolError("build.source does not exist: " + str(source))
    for output in ("build", "dist"):
        child(root, output, output=True)
    budget = only(config.get("budget", {}), ("fuel", "memory_bytes", "host_calls"), "budget")
    integer(budget.get("fuel", 20_000_000), 1, 100_000_000, "budget.fuel")
    memory = integer(budget.get("memory_bytes", 16 * 1024 * 1024), 65536, 64 * 1024 * 1024, "budget.memory_bytes")
    if memory % 65536:
        raise ToolError("budget.memory_bytes: must be a multiple of 65536")
    integer(budget.get("host_calls", 16), 0, 1024, "budget.host_calls")
    for table, fields, identity in (
        ("handlers", ("name", "input_type", "output_type", "max_input_bytes", "max_output_bytes"), "name"),
        ("dependencies", ("slot", "handler", "input_type", "output_type", "provider_version", "optional"), "slot"),
    ):
        entries = config.get(table, [])
        if not isinstance(entries, list) or len(entries) > 16:
            raise ToolError(f"{table}: expected at most 16 tables")
        seen = set()
        for index, entry in enumerate(entries):
            label = f"{table}[{index}]"
            only(entry, fields, label)
            for field in fields:
                if field.startswith("max_"):
                    integer(entry.get(field), 0, 65536, label + "." + field)
                elif field == "optional":
                    boolean(entry.get(field, False), label + ".optional")
                else:
                    value = string(entry.get(field), label + "." + field, 128 if field == "provider_version" else 256)
                    if field != "provider_version" and any(character in value for character in "/\\:"):
                        raise ToolError(label + "." + field + ": identifier must not contain path separators or colon")
            if entry[identity] in seen:
                raise ToolError(label + ": duplicate " + identity)
            seen.add(entry[identity])
    if plugin.get("dependency_calls", False) and not config.get("handlers"):
        raise ToolError("dependency_calls requires at least one handler")
    return root, config, source

def sdk(args):
    root = path_text(args.sdk_root).resolve(strict=True)
    for file in ("rust/Cargo.toml", "c/include/morrow_plugin_task.h", "cpp/include/morrow_plugin_task.hpp"):
        if not (root / file).is_file():
            raise ToolError("incomplete SDK root: " + str(root / file))
    for name in ("runtime.capnp", "content.proto", "task.capnp", "ui.capnp", "dependency_call.capnp"):
        if (root / "rust/contracts" / name).read_text(encoding="utf-8") != (ROOT / "core/schemas" / name).read_text(encoding="utf-8"):
            raise ToolError("SDK and host packager contracts differ: " + name)
    for name, source, constant in (("version.txt", "runtime.rs", "PROTOCOL_VERSION"), ("task-version.txt", "task.rs", "VERSION"), ("ui-version.txt", "ui.rs", "VERSION")):
        match = re.search(r"pub const " + constant + r": u16 = ([0-9]+);", (ROOT / "core/src" / source).read_text(encoding="utf-8"))
        if match is None or (root / "rust/contracts" / name).read_text(encoding="ascii").strip() != match.group(1):
            raise ToolError("SDK and host packager versions differ: " + name)
    dependency_versions = []
    for source in (root / "rust/src/dependency_call.rs", ROOT / "core/src/dependency_call.rs"):
        match = re.search(r"pub const VERSION: u16 = ([0-9]+);", source.read_text(encoding="utf-8"))
        if match is None:
            raise ToolError("missing dependency-call version: " + str(source))
        dependency_versions.append(match.group(1))
    if dependency_versions[0] != dependency_versions[1]:
        raise ToolError("SDK and host packager dependency-call versions differ")
    return root


def executable(name):
    found = shutil.which(name)
    if not found:
        raise ToolError("missing tool: " + name)
    return found

def run(arguments, *, cwd=ROOT, capture=False):
    arguments = [str(a) for a in arguments]
    result = subprocess.run(arguments, cwd=cwd, shell=False, check=False,
                            stdout=subprocess.PIPE if capture else None,
                            text=True, encoding="utf-8", timeout=900)
    if result.returncode:
        raise ToolError(f"{Path(arguments[0]).name} failed (exit {result.returncode}); subsequent steps were not run")
    return result.stdout if capture else ""

def quote(value):
    return '"' + str(value).replace("\\", "\\\\").replace('"', '\\"') + '"'

def handler(name, input_type="bytes", output_type="bytes", max_input=65536):
    return {"name": name, "input_type": input_type, "output_type": output_type, "max_input_bytes": max_input, "max_output_bytes": 65536}

def new_project(args):
    sdk_root = sdk(args)
    target = path_text(args.path).absolute()
    if target.exists() or reparse(target):
        raise ToolError("new refuses an existing project path: " + str(target))
    string(args.id, "plugin.id")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", args.id):
        raise ToolError("invalid plugin.id")
    semver(args.version, "plugin.version"); string(args.name or args.id, "plugin.name", 128)
    profile = "task" if args.kind == "content" else "dependency-caller" if args.kind == "dependency" else args.kind
    example = sdk_root / "examples" / f"{args.language}-{profile}"
    source_name = "src/lib.rs" if args.language == "rust" else "src/plugin.cpp" if args.language == "cpp" else "src/plugin.c"
    original = example / ("src/lib.rs" if args.language == "rust" else "plugin.cpp" if args.language == "cpp" else "plugin.c")
    caps = list(CAPABILITIES) if args.kind == "content" else ["read-content", "edit-content"] if args.kind == "dependency" else []
    handlers = [handler(n) for n in ("bytes.reverse", "bytes.ascii-uppercase", "bytes.require-ascii")] if args.kind == "transform" else []
    if args.kind == "ui":
        handlers = [handler("ui.form", "text.utf8", "morrow.ui.document.v1", 32), handler("ui.edit", "morrow.ui.event.v1", "morrow.ui.document.v1")]
    if args.kind == "dependency":
        handlers = [handler("bytes.dependency-wrap", max_input=65531)]
    config = ["schema = 1", "", "[plugin]", "id = " + quote(args.id), "version = " + quote(args.version),
              "name = " + quote(args.name or args.id), "capabilities = [" + ", ".join(quote(c) for c in caps) + "]",
              "dependency_calls = " + str(args.kind == "dependency").lower(), "", "[build]", "language = " + quote(args.language),
              "source = " + quote(source_name), "", "[budget]", "fuel = 20000000", "memory_bytes = 16777216", "host_calls = 16"]
    for entry in handlers:
        config.extend(["", "[[handlers]]"] + [f"{k} = {v if type(v) is int else quote(v)}" for k, v in entry.items()])
    if args.kind == "dependency":
        config.extend(["", "[[dependencies]]", 'slot = "reverse"', 'handler = "bytes.tag-reverse"', 'input_type = "bytes"',
                       'output_type = "bytes"', 'provider_version = "^1.0.0"', 'optional = false'])
    files = {source_name: original.read_bytes(), "plugin.toml": ("\n".join(config) + "\n").encode(),
             ".gitignore": b"/build/\n/dist/\n", "LICENSE": (sdk_root / "LICENSE").read_bytes()}
    if args.language == "rust":
        cargo_name = "morrow-plugin-" + hashlib.sha256(args.id.encode()).hexdigest()[:16]
        cargo = f"""[workspace]
resolver = "2"
[package]
name = {quote(cargo_name)}
version = {quote(args.version)}
edition = "2024"
publish = false
license = "AGPL-3.0-only"
[lib]
name = "morrow_plugin"
path = "src/lib.rs"
crate-type = ["cdylib"]
[dependencies]
morrow-plugin-sdk = {{path = {quote((sdk_root / 'rust').as_posix())}, features = ["wasm-guest"]}}
[profile.release]
opt-level = "s"
panic = "abort"
strip = true
"""
        origin_cargo = read_toml(example / "Cargo.toml")["package"]
        lock = (example / "Cargo.lock").read_text(encoding="utf-8")
        old = f'name = "{origin_cargo["name"]}"\nversion = "{origin_cargo["version"]}"'
        if lock.count(old) != 1:
            raise ToolError("SDK example lock provenance is ambiguous")
        lock = lock.replace(old, f'name = "{cargo_name}"\nversion = {quote(args.version)}')
        files.update({"Cargo.toml": cargo.encode(), "Cargo.lock": lock.encode()})
    files["README.md"] = f"""# {args.name or args.id}

{args.language} / {args.kind} SDK starter. Source template: Morrow SDK, AGPL-3.0-only (LICENSE).

Use Python 3.11+ and the Morrow repository's tool/morrow_plugin.py:

- `doctor --language {args.language}` checks the local toolchain.
- `build PROJECT` compiles the current source into PROJECT/build/plugin.wasm.
- `pack PROJECT` builds again, prepares the candidate and publishes an immutable hash-named package in PROJECT/dist.
- `check PACKAGE` prepares only; it does not execute, grant permissions or resolve dependencies.
- `transform PACKAGE HANDLER INPUT_TYPE OUTPUT_TYPE INPUT_FILE OUTPUT_FILE` explicitly runs one transform without content grants.

plugin.toml is compiler input. The application reads only the Protobuf+LZ4 package.
Build failures stop packaging; previous packages remain available under their own hashes.
Build runs trusted local compiler/Cargo code and is not a source-code sandbox.
Current complete qualification is Windows; other platforms need their own validation.

Content templates need host-provided task identities and per-card grants.
UI templates need a host renderer and session event validation.
Dependency templates call slot reverse, require a bytes.tag-reverse provider (^1.0.0), and need an explicitly approved host dependency lock; the simple transform command cannot supply this context.
Rust uses the selected SDK path in Cargo.toml. If relocating the SDK, update that dependency and pass the matching --sdk-root.
""".encode()
    target.parent.mkdir(parents=True, exist_ok=True)
    target.mkdir(exist_ok=False)
    for name, data in files.items():
        path = target / name; path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("xb") as stream:
            stream.write(data)
    project(target)
    print("PROJECT", target.resolve())

def compile_project(args, loaded=None):
    sdk_root = sdk(args)
    root, config, source = loaded or project(args.path)
    validate_build_tree(root)
    output = child(root, "build", output=True); output.mkdir(exist_ok=True)
    module = child(root, "build/plugin.wasm", output=True)
    cargo = executable("cargo")
    common_cargo = ["--locked"] + ([] if args.allow_network else ["--offline"])
    language = config["build"]["language"]
    if language == "rust":
        manifest = read_toml(root / "Cargo.toml")
        library = manifest.get("lib", {})
        dependencies = manifest.get("dependencies", {})
        if not isinstance(library, dict) or not isinstance(dependencies, dict):
            raise ToolError("Cargo.toml lib and dependencies must be tables")
        crate_types = library.get("crate-type", [])
        if not isinstance(crate_types, list) or any(not isinstance(item, str) for item in crate_types):
            raise ToolError("Cargo.toml lib.crate-type must be an array of strings")
        if library.get("name") != "morrow_plugin" or "cdylib" not in crate_types or child(root, library.get("path", "src/lib.rs")) != source:
            raise ToolError("Cargo.toml [lib] must name morrow_plugin, use cdylib and match build.source")
        dependency = dependencies.get("morrow-plugin-sdk", {})
        if not isinstance(dependency, dict):
            raise ToolError("Cargo.toml SDK dependency must be a table")
        dependency_path = string(dependency.get("path"), "Cargo.toml SDK path", 32768)
        features = dependency.get("features", [])
        if not isinstance(features, list) or any(not isinstance(item, str) for item in features):
            raise ToolError("Cargo.toml SDK features must be an array of strings")
        if (root / dependency_path).resolve() != (sdk_root / "rust").resolve() or "wasm-guest" not in features:
            raise ToolError("Cargo.toml must use the selected SDK path with wasm-guest")
        target = child(root, "build/rust", output=True)
        run([cargo, "build", *common_cargo, "--manifest-path", root / "Cargo.toml", "--lib", "--target", "wasm32-unknown-unknown", "--release", "--target-dir", target], cwd=root)
        produced = child(root, "build/rust/wasm32-unknown-unknown/release/morrow_plugin.wasm", output=True)
        if produced.stat().st_size > 4 * 1024 * 1024:
            raise ToolError("module exceeds 4 MiB")
        module.write_bytes(produced.read_bytes())
    else:
        sysroot = path_text(args.sysroot).resolve(strict=True)
        clang = executable("clang"); clangxx = executable("clang++") if language == "cpp" else None
        target = child(root, "build/sdk", output=True)
        run([cargo, "rustc", *common_cargo, "--manifest-path", sdk_root / "rust/Cargo.toml", "--target", "wasm32-unknown-unknown", "--features", "wasm-c", "--release", "--target-dir", target, "--crate-type", "staticlib"])
        common = ["--target=wasm32-wasip1", "--sysroot=" + str(sysroot), "-O2", "-Wall", "-Wextra", "-Werror", "-I" + str(sdk_root / "c/include")]
        objects = []
        for name in ("morrow_plugin_sdk", "morrow_plugin_wasm", "morrow_plugin_wasm_libc", "morrow_plugin_task"):
            obj = child(root, "build/" + name + ".o", output=True)
            run([clang, *common, "-std=c11", "-c", sdk_root / f"c/src/{name}.c", "-o", obj]); objects.append(obj)
        codec = child(root, "build/sdk/wasm32-unknown-unknown/release/libmorrow_plugin_sdk.a", output=True)
        stdlib = sysroot / "lib/wasm32-wasip1"
        link = ["-nostdlib", "-Wl,--no-entry", "-Wl,--export=morrow_run", "-Wl,-z,stack-size=1048576", "-Wl,--max-memory=" + str(config.get("budget", {}).get("memory_bytes", 16777216)), "-Wl,--strip-all"]
        if language == "c":
            run([clang, *common, "-std=c11", source, *objects, codec, *link, "-L" + str(stdlib), "-lc", "-o", module])
        else:
            cppcommon = [*common, "-std=c++17", "-nostdinc++", "-isystem", sysroot / "include/wasm32-wasip1/noeh/c++/v1", "-fno-exceptions", "-fno-rtti", "-I" + str(sdk_root / "cpp/include")]
            runtime = child(root, "build/morrow_plugin_wasm_runtime.o", output=True)
            run([clangxx, *cppcommon, "-c", sdk_root / "cpp/src/morrow_plugin_wasm_runtime.cpp", "-o", runtime])
            run([clangxx, *cppcommon, "-Dmorrow_run=mp_guest_run", source, runtime, *objects, codec, *link, "-Wl,--export=__wasm_call_ctors", "-L" + str(stdlib / "noeh"), "-L" + str(stdlib), "-lc++", "-lc++abi", "-lc", "-o", module])
    if not module.is_file() or not 8 <= module.stat().st_size <= 4 * 1024 * 1024 or module.read_bytes()[:8] != b"\x00asm\x01\x00\x00\x00":
        raise ToolError("compiler did not produce a supported Wasm module")
    print("MODULE", module)
    return module

def host_tool(example, arguments, args):
    crate = "core" if example == "plugin_package" else "plugin_runtime"
    command = [executable("cargo"), "run", "--locked"] + ([] if args.allow_network else ["--offline"])
    command += ["--release", "--manifest-path", ROOT / crate / "Cargo.toml", "--example", example]
    if crate == "plugin_runtime":
        command += ["--features", "packages"]
    return run([*command, "--", *arguments], capture=True)

def package_arguments(config):
    plugin = config["plugin"]
    arguments = ["--name", plugin.get("name", plugin["id"])]
    for capability in plugin.get("capabilities", []):
        arguments += ["--capability", capability]
    for entry in config.get("handlers", []):
        arguments += ["--handler", entry["name"], entry["input_type"], entry["output_type"], str(entry["max_input_bytes"]), str(entry["max_output_bytes"])]
    for entry in config.get("dependencies", []):
        arguments += ["--dependency", entry["slot"], entry["handler"], entry["input_type"], entry["output_type"], entry["provider_version"], "optional" if entry.get("optional", False) else "required"]
    if plugin.get("dependency_calls", False):
        arguments += ["--dependency-calls"]
    for key, flag in (("fuel", "--fuel"), ("memory_bytes", "--memory-bytes"), ("host_calls", "--host-calls")):
        if key in config.get("budget", {}):
            arguments += [flag, str(config["budget"][key])]
    return arguments

def pack_project(args):
    loaded = project(args.path)
    root, config, _ = loaded
    # Always require a successful build in this invocation. Never pick a previous module/package by mtime.
    module = compile_project(args, loaded)
    if project(args.path)[1] != config:
        raise ToolError("project metadata changed during build; run pack again")
    build = child(root, "build", output=True)
    with tempfile.TemporaryDirectory(prefix="package-", dir=build) as temporary:
        candidate = Path(temporary) / "candidate.mplugin"
        plugin = config["plugin"]
        print(host_tool("plugin_package", ["pack-v2", module, candidate, plugin["id"], plugin["version"], *package_arguments(config)], args), end="")
        print(host_tool("plugin_check", ["check", candidate], args), end="")
        data = candidate.read_bytes(); digest = hashlib.sha256(data).hexdigest()
        destination = child(root, "dist", output=True)
        print(host_tool("plugin_package", ["install", candidate, destination], args), end="")
        published = child(root, "dist/" + digest + ".mplugin", output=True)
        if published.read_bytes() != data:
            raise ToolError("published package differs from the prepared candidate")
    print("PACKAGE", published)
    print("SHA256", digest)
    return published

def doctor(args):
    sdk_root = sdk(args)
    names = ["cargo", "rustc", "rustup", "capnp"]
    if args.language != "rust":
        names += ["clang"] + (["clang++"] if args.language != "c" else [])
    failed = False
    for name in names:
        try:
            tool = executable(name)
            version = run([tool, "--version"], capture=True).splitlines()[0]
            print("OK", name, version)
        except (ToolError, OSError) as error:
            print("MISSING", error); failed = True
    try:
        if "wasm32-unknown-unknown" not in run([executable("rustup"), "target", "list", "--installed"], capture=True).splitlines():
            raise ToolError("install Rust target wasm32-unknown-unknown")
    except (ToolError, OSError) as error:
        print("MISSING", error); failed = True
    if args.language != "rust":
        sysroot = path_text(args.sysroot)
        for relative in ("lib/wasm32-wasip1/libc.a", "include/wasm32-wasip1/stdlib.h"):
            if not (sysroot / relative).is_file():
                print("MISSING WASI sysroot file", sysroot / relative); failed = True
        if args.language != "c" and not (sysroot / "lib/wasm32-wasip1/noeh/libc++.a").is_file():
            print("MISSING WASI no-exceptions C++ standard library"); failed = True
    print("SDK", sdk_root, "(contracts match; no plugin executed)")
    if failed:
        raise ToolError("toolchain incomplete; prepare the missing tools before building")

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("new", "build", "pack", "check", "transform", "doctor"):
        command = commands.add_parser(name)
        command.add_argument("--sdk-root", default=str(ROOT / "sdk"))
        command.add_argument("--sysroot", default=str(ROOT / "build/tools/wasi-34/wasi-sysroot-34.0"))
        command.add_argument("--allow-network", action="store_true", help="allow Cargo dependency downloads (offline by default)")
        if name in ("new", "build", "pack"):
            command.add_argument("path")
        if name == "new":
            command.add_argument("--language", choices=LANGUAGES, required=True)
            command.add_argument("--kind", choices=KINDS, default="transform")
            command.add_argument("--id", required=True)
            command.add_argument("--version", default="0.1.0")
            command.add_argument("--name")
        if name in ("check", "transform"):
            command.add_argument("package")
        if name == "transform":
            for argument in ("handler", "input_type", "output_type", "input_file", "output_file"):
                command.add_argument(argument)
        if name == "doctor":
            command.add_argument("--language", choices=LANGUAGES)
    args = parser.parse_args(argv)
    try:
        if args.command == "new":
            new_project(args)
        elif args.command == "build":
            compile_project(args)
        elif args.command == "pack":
            pack_project(args)
        elif args.command == "doctor":
            doctor(args)
        else:
            arguments = [args.command, path_text(args.package).resolve(strict=True)]
            if args.command == "transform":
                arguments += [args.handler, args.input_type, args.output_type, path_text(args.input_file).resolve(strict=True), path_text(args.output_file).absolute()]
            print(host_tool("plugin_check", arguments, args), end="")
    except (ToolError, OSError, subprocess.TimeoutExpired, UnicodeError) as error:
        print("ERROR:", error, file=sys.stderr)
        return 1
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
