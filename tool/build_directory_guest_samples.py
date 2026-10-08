#!/usr/bin/env python3
"""Build only new directory C/C++ Wasm samples, with pinned local inputs.

This is build/static-import evidence, not original managed-owner execution,
ProductionGUI, ProtectedSession, picker provenance, or platform qualification.
No existing guest/provider is rebuilt or repacked. Outputs must be new.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time


EXPECTED_IMPORTS = {
    ("morrow_task_v1", "read_input"): ([0x7f, 0x7f], [0x7f]),
    ("morrow_task_v1", "complete"): ([0x7f, 0x7f], [0x7f]),
    ("morrow_fs_directory_v1", "call"): ([0x7f] * 4, [0x7f]),
}


def pin(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return {"bytes": path.stat().st_size, "sha256": digest.hexdigest()}


def snapshot(paths):
    return {str(path): pin(path) for path in sorted(set(paths))}


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


class Wire:
    def __init__(self, data):
        self.data = data
        self.offset = 0

    def byte(self):
        if self.offset >= len(self.data):
            raise ValueError("truncated Wasm")
        value = self.data[self.offset]
        self.offset += 1
        return value

    def uint(self):
        value = 0
        for index in range(5):
            byte = self.byte()
            if index == 4 and byte & 0xf0:
                raise ValueError("oversized Wasm u32")
            value |= (byte & 0x7f) << (7 * index)
            if not byte & 0x80:
                return value
        raise ValueError("unterminated Wasm u32")

    def block(self, length):
        end = self.offset + length
        if end > len(self.data):
            raise ValueError("truncated Wasm block")
        value = self.data[self.offset:end]
        self.offset = end
        return value

    def name(self):
        return self.block(self.uint()).decode("utf-8")

    def done(self):
        if self.offset != len(self.data):
            raise ValueError("trailing section bytes")


def inspect_wasm(path):
    raw = path.read_bytes()
    if not 8 <= len(raw) <= 4 * 1024 * 1024 or raw[:8] != b"\0asm\x01\0\0\0":
        raise ValueError("invalid or oversized compiled Wasm")
    wire = Wire(raw[8:])
    types, imports, functions, exports = [], [], [], {}
    memories, defined_functions, code_bodies, seen_sections = [], 0, 0, set()
    while wire.offset < len(wire.data):
        kind = wire.byte()
        section = Wire(wire.block(wire.uint()))
        if kind != 0:
            if kind in seen_sections:
                raise ValueError("duplicate noncustom Wasm section")
            seen_sections.add(kind)
        if kind == 1:
            for _ in range(section.uint()):
                if section.byte() != 0x60:
                    raise ValueError("unsupported non-function Wasm type")
                params = [section.byte() for _ in range(section.uint())]
                results = [section.byte() for _ in range(section.uint())]
                types.append((params, results))
            section.done()
        elif kind == 2:
            for _ in range(section.uint()):
                module, name, category = section.name(), section.name(), section.byte()
                if category != 0:
                    raise ValueError("non-function Wasm import")
                index = section.uint()
                if index >= len(types):
                    raise ValueError("invalid import type")
                imports.append((module, name, types[index]))
                functions.append(index)
            section.done()
        elif kind == 3:
            defined_functions = section.uint()
            for _ in range(defined_functions):
                index = section.uint()
                if index >= len(types):
                    raise ValueError("invalid defined function type")
                functions.append(index)
            section.done()
        elif kind == 5:
            for _ in range(section.uint()):
                flags = section.uint()
                if flags not in (0, 1):
                    raise ValueError("unsupported shared or non-memory32 definition")
                minimum = section.uint()
                maximum = section.uint() if flags == 1 else None
                if minimum > 65536 or maximum is not None and not minimum <= maximum <= 65536:
                    raise ValueError("invalid memory32 limits")
                memories.append((minimum, maximum))
            section.done()
        elif kind == 7:
            for _ in range(section.uint()):
                name, category, index = section.name(), section.byte(), section.uint()
                if name in exports:
                    raise ValueError("duplicate Wasm export")
                exports[name] = (category, index)
            section.done()
        elif kind == 8:
            raise ValueError("Wasm start section forbidden")
        elif kind == 10:
            code_bodies = section.uint()
            for _ in range(code_bodies):
                body = section.block(section.uint())
                if not body or body[-1] != 0x0b:
                    raise ValueError("missing bounded function body/end")
            section.done()
    if code_bodies != defined_functions:
        raise ValueError("function/code section count mismatch")
    for category, index in exports.values():
        if category == 0 and index >= len(functions):
            raise ValueError("exported function index does not exist")
        if category == 2 and index >= len(memories):
            raise ValueError("exported memory index does not exist")
    actual = {(module, name): signature for module, name, signature in imports}
    if len(actual) != len(imports) or actual != EXPECTED_IMPORTS:
        raise ValueError(f"directory import set/signature mismatch: {imports!r}")
    if "memory" not in exports or exports["memory"][0] != 2 or exports["memory"][1] >= len(memories):
        raise ValueError("exported guest memory missing")
    category, function = exports.get("morrow_run", (None, None))
    if category != 0 or function >= len(functions) or types[functions[function]] != ([], [0x7f]):
        raise ValueError("morrow_run export signature mismatch")
    return {"imports": [{"module": module, "name": name, "params": signature[0], "results": signature[1]} for module, name, signature in imports], "exports": sorted(exports), "start_section": False, "defined_memories": [{"minimum_pages": minimum, "maximum_pages": maximum} for minimum, maximum in memories], "function_type_refs_checked": len(functions), "defined_functions": defined_functions, "code_bodies": code_bodies}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cargo-home", type=Path, required=True)
    parser.add_argument("--sysroot", type=Path, required=True)
    parser.add_argument("--cargo", type=Path, required=True)
    parser.add_argument("--clang", type=Path, required=True)
    parser.add_argument("--clangxx", type=Path, required=True)
    parser.add_argument("--wasm-ld", type=Path, required=True)
    parser.add_argument("--capnp", type=Path, required=True)
    parser.add_argument("--environment-record", type=Path)
    args = parser.parse_args()
    source = args.source_root.resolve(strict=True)
    output = args.output.resolve()
    if output.exists():
        raise ValueError("output must be new; prior evidence is never overwritten")
    # Prevent a mistaken output setting from modifying the source tree.
    if output == source or source in output.parents:
        raise ValueError("output/target must be external to source")
    cargo_home = args.cargo_home.resolve(strict=True)
    sysroot = args.sysroot.resolve(strict=True)
    tools = {name: getattr(args, name).resolve(strict=True) for name in ("cargo", "clang", "clangxx", "wasm_ld", "capnp")}
    tools.update(rustc=tools["cargo"].with_name("rustc.exe" if os.name == "nt" else "rustc"), rustdoc=tools["cargo"].with_name("rustdoc.exe" if os.name == "nt" else "rustdoc"), python=Path(sys.executable).resolve(strict=True))
    if tools["wasm_ld"].parent != tools["clang"].parent or tools["clangxx"].parent != tools["clang"].parent:
        raise ValueError("clang/clang++/wasm-ld must share the selected toolchain directory")
    extension = source / "extensions/fs-directory-request-v1"
    support = extension / "guests/ffi-support"
    manifest, lock = support / "Cargo.toml", support / "Cargo.lock"
    manifest.resolve(strict=True)
    fixed = [source / "tool/build_directory_guest_samples.py", source / "sdk/c/src/morrow_plugin_wasm_libc.c", source / "sdk/cpp/src/morrow_plugin_wasm_runtime.cpp"]
    for relative in ("extensions/fs-directory-request-v1", "extensions/fs-directory-v1", "sdk/rust"):
        for path in (source / relative).rglob("*"):
            if path.is_file() and not {"target", ".git"}.intersection(path.relative_to(source).parts) and path.suffix in {".rs", ".capnp", ".h", ".hpp", ".c", ".cpp", ".toml", ".lock"}:
                fixed.append(path)
    # All sysroot headers bound transitive includes; only selected link libraries.
    sysroot_files = [path for path in (sysroot / "include").rglob("*") if path.is_file()]
    for name in ("lib/wasm32-wasip1/libc.a", "lib/wasm32-wasip1/noeh/libc++.a", "lib/wasm32-wasip1/noeh/libc++abi.a"):
        sysroot_files.append((sysroot / name).resolve(strict=True))
    before = snapshot(fixed)
    tool_before = snapshot(tools.values())
    sysroot_before = snapshot(sysroot_files)
    output.mkdir(parents=True)
    temp = output / "synthetic-temp"
    for name in ("tmp", "appdata", "localappdata"):
        (temp / name).mkdir(parents=True)
    allowed = {"SYSTEMROOT", "SYSTEMDRIVE", "WINDIR", "COMSPEC", "PATHEXT", "PROCESSOR_ARCHITECTURE", "NUMBER_OF_PROCESSORS", "INCLUDE", "LIB", "LIBPATH", "PATH"}
    inherited = os.environ
    if args.environment_record:
        inherited = json.loads(args.environment_record.resolve(strict=True).read_text(encoding="utf-8-sig"))["environment_whitelist"]
    env = {key: value for key, value in inherited.items() if key.upper() in allowed}
    env.update(CARGO_HOME=str(cargo_home), CARGO_NET_OFFLINE="true", CARGO_BUILD_JOBS="2", CARGO_TARGET_DIR=str(output / "target"), RUSTC=str(tools["rustc"]), RUSTDOC=str(tools["rustdoc"]), RUST_MIN_STACK="16777216", PYTHONDONTWRITEBYTECODE="1", TEMP=str(temp / "tmp"), TMP=str(temp / "tmp"), APPDATA=str(temp / "appdata"), LOCALAPPDATA=str(temp / "localappdata"))
    env["PATH"] = os.pathsep.join([str(tools["cargo"].parent), str(tools["capnp"].parent), str(tools["clang"].parent), env.get("PATH", "")])
    result = {"status": "STARTED", "build_only": True, "managed_owner_execution": "NOT_RUN", "ProductionGUI": False, "ProtectedSession": False, "picker_provenance": False, "other_platforms": "NOT_RUN", "source_root": str(source), "output": str(output), "source_before": before, "tool_before": tool_before, "sysroot_before": sysroot_before, "environment_whitelist": env, "commands": [], "artifacts": {}}
    save(output / "result.json", result)

    def run(label, argv):
        directory = output / "logs" / label
        directory.mkdir(parents=True)
        command = list(map(str, argv))
        record = {"label": label, "argv": command, "cwd": str(source)}
        save(directory / "started.json", {**record, "environment_whitelist": env})
        started = time.monotonic()
        with (directory / "stdout.raw").open("wb") as stdout, (directory / "stderr.raw").open("wb") as stderr:
            process = subprocess.run(command, cwd=source, env=env, stdout=stdout, stderr=stderr)
        (directory / "raw-exit.txt").write_text(str(process.returncode) + "\n", encoding="ascii")
        record.update(exit_code=process.returncode, elapsed_seconds=time.monotonic() - started, logs=snapshot(directory / name for name in ("stdout.raw", "stderr.raw", "raw-exit.txt")))
        result["commands"].append(record)
        save(directory / "result.json", record)
        save(output / "result.json", result)
        print(json.dumps({"label": label, "exit": process.returncode, "seconds": record["elapsed_seconds"]}), flush=True)
        if process.returncode:
            raise RuntimeError(f"{label} failed with exit {process.returncode}; raw logs retained in {directory}")
        return directory

    failure = None
    try:
        for name in ("cargo", "rustc", "capnp", "clang", "wasm_ld"):
            run("version-" + name, [tools[name], "--version"])
        if not lock.exists():
            run("support-lock-offline", [tools["cargo"], "generate-lockfile", "--offline", "--manifest-path", manifest])
        result["support_lock"] = {"path": str(lock), **pin(lock)}
        # New support lock is the only input that may be generated by this run.
        fixed.append(lock)
        build_before = snapshot(fixed)
        result["source_build_before"] = build_before
        save(output / "result.json", result)
        run("support-staticlib", [tools["cargo"], "build", "--manifest-path", manifest, "--package", "morrow-fs-directory-ffi-support", "--target", "wasm32-unknown-unknown", "--release", "--locked", "--offline", "--target-dir", output / "target", "-j", "2"])
        archive = output / "target/wasm32-unknown-unknown/release/libmorrow_fs_directory_ffi_support.a"
        result["combined_archive"] = {"path": str(archive), **pin(archive), "single_rust_runtime": True, "allocator": "original sdk/rust/src/wasm_alloc.rs via wasm-c"}
        common = ["--target=wasm32-wasip1", "--sysroot=" + str(sysroot), "-O2", "-Wall", "-Wextra", "-Werror", "-I" + str(extension / "include"), "-I" + str(source / "extensions/fs-directory-v1/include")]
        shim = output / "libc-shim.o"
        run("allocator-shim", [tools["clang"], *common, "-std=c11", "-c", source / "sdk/c/src/morrow_plugin_wasm_libc.c", "-o", shim])
        stdlib = sysroot / "lib/wasm32-wasip1"
        link = ["-nostdlib", "-Wl,--no-entry", "-Wl,--export=morrow_run", "-Wl,-z,stack-size=1048576", "-Wl,--max-memory=16777216", "-Wl,--strip-all"]
        cppcommon = [*common, "-std=c++17", "-nostdinc++", "-isystem", sysroot / "include/wasm32-wasip1/noeh/c++/v1", "-fno-exceptions", "-fno-rtti"]
        runtime = output / "cpp-runtime.o"
        run("cpp-runtime", [tools["clangxx"], *cppcommon, "-c", source / "sdk/cpp/src/morrow_plugin_wasm_runtime.cpp", "-o", runtime])
        for language in ("c", "cpp"):
            compiler = tools["clang"] if language == "c" else tools["clangxx"]
            obj = output / (language + "-guest.o")
            original = extension / ("guests/c/plugin.c" if language == "c" else "guests/cpp/plugin.cpp")
            compile_flags = [*common, "-std=c11"] if language == "c" else [*cppcommon, "-Dmorrow_run=mp_guest_run"]
            run(language + "-compile", [compiler, *compile_flags, "-c", original, "-o", obj])
            module = output / ("directory-" + language + ".wasm")
            objects = [obj, shim] if language == "c" else [obj, runtime, shim]
            libraries = ["-L" + str(stdlib), "-lc"] if language == "c" else ["-Wl,--export=__wasm_call_ctors", "-L" + str(stdlib / "noeh"), "-L" + str(stdlib), "-lc++", "-lc++abi", "-lc"]
            run(language + "-link", [compiler, *common, *objects, archive, *link, *libraries, "-o", module])
            result["artifacts"][language] = {"path": str(module), **pin(module), "source": {"path": str(original), **pin(original)}, "static_abi": inspect_wasm(module), "managed_owner_execution": "NOT_RUN"}
            save(output / "result.json", result)
        if snapshot(fixed) != build_before:
            raise ValueError("source/build lock changed during compilation")
        result["status"] = "PASS_NEW_C_CPP_WASM_BUILD_STATIC_ABI_ONLY"
    except Exception as error:
        failure = str(error)
        result.update(status="FAILED", failure=failure)
    finally:
        result["source_after"] = snapshot(Path(path) for path in before)
        result["tool_after"] = snapshot(tools.values())
        result["sysroot_after"] = snapshot(sysroot_files)
        result["original_inputs_unchanged"] = result["source_after"] == before
        result["tools_unchanged"] = result["tool_after"] == tool_before
        result["sysroot_unchanged"] = result["sysroot_after"] == sysroot_before
        if not all(result[name] for name in ("original_inputs_unchanged", "tools_unchanged", "sysroot_unchanged")):
            result.update(status="FAILED", failure="pinned original input/tool/sysroot changed")
            failure = result["failure"]
        save(output / "result.json", result)
        print(json.dumps({"status": result["status"], "failure": failure, "artifacts": result["artifacts"], "receipt": str(output / "result.json")}), flush=True)
    return 1 if failure else 0


if __name__ == "__main__":
    sys.exit(main())


