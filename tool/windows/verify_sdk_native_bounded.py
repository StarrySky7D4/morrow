"""Windows x64 SDK-only qualification. No Store, audit, profile or key operations.

Uses existing SDK vectors/tests, then fresh consumers of an SDK-only source copy.
All commands/logs and failures are retained; output must be a new directory.
This qualifies native callbacks/codecs, not Wasm import execution or product UI.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
from sdk_native_provenance import MODE, ROLES, emit, file_identity, require, snapshot

p = argparse.ArgumentParser()
p.add_argument("--repo", type=Path, required=True)
p.add_argument("--pin", required=True, help="Published 40-character baseline commit")
p.add_argument("--tool-commit", required=True, help="Exact committed Windows runner revision")
p.add_argument("--qualification-fixture", action="store_true", help="Produce only the two documented failures with fresh bound artifacts")
p.add_argument("--output", type=Path, required=True)
p.add_argument("--cargo-home", type=Path, required=True)
p.add_argument("--capnp-bin", type=Path, required=True)
p.add_argument("--vsdev", type=Path, default=Path(r"C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\Tools\VsDevCmd.bat"))
a = p.parse_args()
assert os.name == "nt", "Windows native qualification only"
repo, out = a.repo.resolve(), a.output.resolve()
assert not out.is_relative_to(repo), "Keep all build/evidence output outside the source worktree"
assert re.fullmatch(r"[0-9a-f]{40}", a.pin)
subprocess.run(["git", "-C", str(repo), "diff", "--quiet", a.pin, "--", "sdk", "core/schemas"], check=True)
def git_value(expression):
    return subprocess.check_output(["git", "-C", str(repo), "rev-parse", expression], text=True).strip()
source_metadata = {"baseline_commit": a.pin, "baseline_tree": git_value(a.pin + "^{tree}"),
    "sdk_tree": git_value(a.pin + ":sdk"), "windows_tool_commit": git_value("HEAD")}
source_before = snapshot(repo, a.pin, a.tool_commit)
require(Path(__file__).resolve() == repo / "tool/windows/verify_sdk_native_bounded.py", "Runner path does not belong to qualified tool checkout")
source_metadata["actual_tool_files"] = {name: value for name, value in source_before["files"].items() if name.startswith("tool/windows/")}
source_metadata["sdk_normalization_counts"] = source_before["sdk_normalization_counts"]
assert (repo / "core/schemas/dependency_call.capnp").is_file(), (
    "Repository contract-comparison tests require read-only core/schemas; "
    "the independent SDK consumer bundle itself does not require core.")
out.mkdir(parents=True, exist_ok=False)
(out / "source-metadata.json").write_text(json.dumps(source_metadata, indent=2) + "\n", encoding="utf-8")
env = dict(os.environ)
for variable in ("MORROW_DEPENDENCY_SMOKE_DIR", "MORROW_IO_SMOKE_DIR"):
    env.pop(variable, None)  # Do not write caller-selected fixture destinations.
env.update(CARGO_HOME=str(a.cargo_home.resolve()), CARGO_TARGET_DIR=str(out / "sdk-target"), VSLANG="1033")
assert not any(c in str(a.vsdev) for c in '&|<>^%"'), "Unsupported cmd metacharacter in VS path"
setup_command = f'call "{a.vsdev}" -no_logo -arch=x64 -host_arch=x64 >nul && set PATH && set INCLUDE && set LIB && set VCToolsVersion && set WindowsSDKVersion'
# A raw CreateProcess command line preserves cmd's nested quotes; list2cmdline
# would add C-style backslashes that cmd does not interpret as quote escapes.
# The only substituted command value is the metacharacter-checked VS path.
setup = subprocess.run('cmd.exe /d /s /c "' + setup_command + '"', capture_output=True, env=env)
(out / "vs-toolset-setup.log").write_bytes(setup.stdout + setup.stderr)
setup.check_returncode()
selected = {}
for line in setup.stdout.decode("mbcs").splitlines():
    key, separator, value = line.partition("=")
    if separator and key.upper() in {"PATH", "INCLUDE", "LIB", "LIBPATH", "VCTOOLSVERSION", "WINDOWSSDKVERSION"}:
        # Prefer the canonical uppercase key if an inherited environment contains
        # both Path and PATH; do not discard the freshly added MSVC directories.
        if key.upper() not in selected or key == key.upper():
            selected[key.upper()] = value
env.update(selected)
env["PATH"] = str(a.capnp_bin.resolve()) + os.pathsep + env["PATH"]
records = []
built_artifacts = {}

def bind_artifact(role):
    relative, command = ROLES[role]
    record = next(row for row in records if row["name"] == command)
    require(record["exit_code"] == 0, "Cannot bind failed artifact build")
    built_artifacts[role] = {"relative_path": relative, "build_command": command,
        "capture_phase": "immediately_after_build_or_verified_copy",
        "producer_log_sha256": file_identity(out / record["log"])["sha256"],
        **file_identity(out / relative)}

def save_results():
    (out / "commands.json").write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")

def run(name, command, cwd=repo, custom_env=None, required=False):
    log = out / (name + ".log")
    assert not log.exists(), "Never overwrite a previous attempt"
    started = time.monotonic()
    active_env = custom_env or env
    actual_command = [str(part) for part in command]
    # Windows CreateProcess does not use a supplied child PATH to locate the
    # executable itself. Resolve cl/dumpbin/cargo against the configured PATH.
    actual_command[0] = shutil.which(actual_command[0], path=active_env["PATH"]) or actual_command[0]
    try:
        with log.open("wb") as handle:
            result = subprocess.run(actual_command, cwd=cwd,
                env=active_env, stdout=handle, stderr=subprocess.STDOUT, timeout=300)
        code, status = result.returncode, "passed" if result.returncode == 0 else "failed"
    except subprocess.TimeoutExpired:
        code, status = None, "timed_out"
    records.append({"name": name, "command": actual_command,
        "cwd": str(cwd), "exit_code": code, "status": status,
        "seconds": round(time.monotonic() - started, 3), "log": log.name})
    save_results()
    print(f"{name}: {status} (exit {code})", flush=True)
    if required and code != 0:
        raise RuntimeError(f"Required setup/build failed: {name}; see retained log")
    return code

def cargo(name, args, manifest=None, cwd=repo, custom_env=None, required=False):
    manifest = manifest or repo / "sdk/rust/Cargo.toml"
    return run(name, ["cargo", *args, "--offline", "--locked", "--manifest-path", manifest],
               cwd=cwd, custom_env=custom_env, required=required)

def source_hashes(root):
    return {str(path.relative_to(root)).replace("\\", "/"): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(root.rglob("*")) if path.is_file()}

before = source_hashes(repo / "sdk")
(out / "sdk-source-hashes-before.json").write_text(json.dumps(before, indent=2) + "\n", encoding="utf-8")
versions = {"msvc_toolset": selected.get("VCTOOLSVERSION"), "windows_sdk": selected.get("WINDOWSSDKVERSION"),
    "rustc": subprocess.run(["rustc", "-Vv"], env=env, capture_output=True, text=True).stdout,
    "clang": subprocess.run(["clang", "--version"], env=env, capture_output=True, text=True).stdout}
(out / "compiler-versions.json").write_text(json.dumps(versions, indent=2) + "\n", encoding="utf-8")
run("msvc-version", ["cl", "/Bv"])
if a.qualification_fixture:
    qualification = out / "qualification-source"
    shutil.copytree(repo / "sdk", qualification / "sdk")
    cargo("rust-native-tests", ["test"], manifest=qualification / "sdk/rust/Cargo.toml", cwd=qualification)
else:
    cargo("rust-native-tests", ["test"])
cargo("sdk-native-dll-build", ["build"], required=True)
bind_artifact("dll")
bind_artifact("import_lib")
vectors = out / "channel-vectors"
run("channel-vectors", ["cargo", "run", "--offline", "--locked", "--manifest-path", repo / "sdk/rust/Cargo.toml",
    "--example", "channel_vectors", "--", vectors], required=True)
dll = out / "sdk-target/debug/morrow_plugin_sdk.dll"
import_lib = out / "sdk-target/debug/morrow_plugin_sdk.dll.lib"
run("dll-exports", ["dumpbin", "/exports", dll], required=True)

def compile_native(name, source, destination, objects, library, include_sdk, compiler, utf8=True):
    cpp = source.suffix == ".cpp"
    includes = [include_sdk / "c/include", include_sdk / "cpp/include"]
    if compiler == "clang":
        program = "clang++" if cpp else "clang"
        flags = ["-std=c++17" if cpp else "-std=c11", "-Wall", "-Wextra", "-Werror"]
        args = [program, *flags, *("-I" + str(path) for path in includes), source, *objects, library, "-o", destination]
    else:
        flags = ["/nologo", *(["/utf-8"] if utf8 else []), "/std:c++17" if cpp else "/std:c11", "/W4", "/WX", "/MD"]
        if cpp: flags.append("/EHsc")
        args = ["cl", *flags, *("/I" + str(path) for path in includes), source, *objects, library,
                "/Fo" + str(destination.with_suffix(".obj")), "/Fe" + str(destination)]
    return run(name, args)

def wrappers(directory, sdk, compiler):
    objects = []
    for source_name in ("morrow_plugin_sdk", "morrow_channel_v1"):
        source = sdk / f"c/src/{source_name}.c"
        obj = directory / (source_name + ".obj")
        if compiler == "clang":
            args = ["clang", "-std=c11", "-Wall", "-Wextra", "-Werror", "-I" + str(sdk / "c/include"), "-c", source, "-o", obj]
        else:
            args = ["cl", "/nologo", "/utf-8", "/std:c11", "/W4", "/WX", "/MD", "/I" + str(sdk / "c/include"), "/c", source, "/Fo" + str(obj)]
        run(directory.name + "-" + source_name, args, required=True)
        if directory.name == "native-msvc":
            bind_artifact("sdk_wrapper" if source_name == "morrow_plugin_sdk" else "channel_wrapper")
        objects.append(obj)
    return objects

if a.qualification_fixture:
    directory = out / "native-msvc"; directory.mkdir()
    shutil.copy2(dll, directory / dll.name)
    bind_artifact("runtime_dll")
    require(built_artifacts["dll"]["sha256"] == built_artifacts["runtime_dll"]["sha256"], "Runtime DLL copy drift")
    objects = wrappers(directory, repo / "sdk", "msvc")
    source = repo / "sdk/tests/cpp_codec.cpp"
    exe = directory / "cpp_codec.exe"
    compile_native("msvc-cpp_codec-build", source, exe, objects, import_lib, repo / "sdk", "msvc", utf8=False)
    run("msvc-cpp_codec-run", [exe, repo / "sdk/tests/fixtures"])
    observed = {row["name"] for row in records if row["exit_code"] != 0 and row["name"] != "msvc-version"}
    require(observed == {"rust-native-tests", "msvc-cpp_codec-run"}, "Documented qualification failure pair not observed")
    require(snapshot(repo, a.pin, a.tool_commit) == source_before, "Source/tool drift during qualification fixture")
    summary = {"scope": "Deliberate missing-schema/default-codepage fixture; not SDK passing qualification",
        "producer_mode": MODE, "source_metadata": source_metadata, "observed_failures": sorted(observed),
        "current_passed_tests": 0, "commands": records, "protected_databases_or_keys_created": 0}
    (out / "results.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    receipt = emit(out, repo, source_before, records, MODE, built_artifacts)
    print(json.dumps(receipt, indent=2), flush=True)
    raise SystemExit(0)

native_runs = []
cases = [("c_channel.c", vectors), ("cpp_channel.cpp", vectors),
    ("c_channel_guardpage.c", vectors), ("cpp_channel_guardpage.cpp", vectors),
    ("c_transport.c", None), ("cpp_transport.cpp", None), ("cpp_codec.cpp", repo / "sdk/tests/fixtures")]
for compiler in ("clang", "msvc"):
    directory = out / ("native-" + compiler); directory.mkdir()
    shutil.copy2(dll, directory / dll.name)
    if compiler == "msvc":
        bind_artifact("runtime_dll")
        require(built_artifacts["dll"]["sha256"] == built_artifacts["runtime_dll"]["sha256"], "Runtime DLL copy drift")
    objects = wrappers(directory, repo / "sdk", compiler)
    for filename, data in cases:
        source = repo / "sdk/tests" / filename
        name = compiler + "-" + source.stem
        exe = directory / (source.stem + ".exe")
        built = compile_native(name + "-build", source, exe, objects, import_lib, repo / "sdk", compiler)
        if built == 0:
            code = run(name + "-run", [exe, *([data] if data else [])])
            native_runs.append({"name": name, "status": "passed" if code == 0 else "failed"})
        else: native_runs.append({"name": name, "status": "not_run", "reason": "compile failed"})

# SDK-only source copy: no repository core/runtime/audit/tool dependency or cache.
bundle = out / "standalone-source"; sdk = bundle / "sdk"; sdk.mkdir(parents=True)
for folder in ("c", "cpp", "rust"):
    shutil.copytree(repo / "sdk" / folder, sdk / folder)
for filename in ("LICENSE", "README.md", "CHANNEL_API.md"):
    shutil.copy2(repo / "sdk" / filename, sdk / filename)
consumer = bundle / "consumer"; (consumer / "src").mkdir(parents=True)
fixtures = Path(__file__).resolve().parent / "fixtures"
shutil.copy2(fixtures / "sdk_consumer.rs", consumer / "src/main.rs")
(consumer / "Cargo.toml").write_text('[workspace]\n[package]\nname="morrow-windows-sdk-consumer"\nversion="0.0.0"\nedition="2024"\n[dependencies]\nmorrow-plugin-sdk={path="../sdk/rust"}\n', encoding="utf-8")
bundle_env = dict(env); bundle_env["CARGO_TARGET_DIR"] = str(out / "standalone-target")
consumer_env = dict(bundle_env); consumer_env["CARGO_TARGET_DIR"] = str(out / "consumer-target")
run("consumer-lock", ["cargo", "generate-lockfile", "--offline", "--manifest-path", consumer / "Cargo.toml"],
    cwd=consumer, custom_env=consumer_env, required=True)
cargo("standalone-rust-consumer", ["run"], manifest=consumer / "Cargo.toml", cwd=consumer, custom_env=consumer_env)
cargo("standalone-sdk-dll-build", ["build"], manifest=sdk / "rust/Cargo.toml", cwd=bundle, custom_env=bundle_env, required=True)
standalone_dll = out / "standalone-target/debug/morrow_plugin_sdk.dll"
standalone_lib = out / "standalone-target/debug/morrow_plugin_sdk.dll.lib"
directory = out / "standalone-native-msvc"; directory.mkdir()
shutil.copy2(standalone_dll, directory / standalone_dll.name)
objects = wrappers(directory, sdk, "msvc")
for extension in ("c", "cpp"):
    source = consumer / ("sdk_consumer." + extension)
    shutil.copy2(fixtures / source.name, source)
    name = "standalone-" + extension + "-consumer"
    exe = directory / (source.stem + "-" + extension + ".exe")
    if compile_native(name + "-build", source, exe, objects, standalone_lib, sdk, "msvc") == 0:
        run(name + "-run", [exe, vectors], cwd=bundle)
metadata = subprocess.run(["cargo", "metadata", "--offline", "--locked", "--format-version", "1", "--no-deps",
    "--manifest-path", consumer / "Cargo.toml"], cwd=consumer, env=consumer_env, capture_output=True, check=True)
parsed = json.loads(metadata.stdout)
dependency_paths = [d["path"] for package in parsed["packages"] for d in package["dependencies"] if "path" in d]
assert dependency_paths and all(Path(path).is_relative_to(bundle) for path in dependency_paths)
after = source_hashes(repo / "sdk")
assert before == after, "Shared SDK source changed during qualification"
require(snapshot(repo, a.pin, a.tool_commit) == source_before, "Source/schema/tool drift during full runner")
test_log = (out / "rust-native-tests.log").read_text("utf-8", errors="replace")
counts = [tuple(map(int, pair)) for pair in re.findall(r"test result: .*? (\d+) passed; (\d+) failed", test_log)]
summary = {"scope": "Windows x64 native SDK tests and SDK-only consumers; not Wasm/product qualification",
    "source_metadata": source_metadata,
    "rust_test_passed": sum(x[0] for x in counts), "rust_test_failed": sum(x[1] for x in counts),
    "rust_test_command_exit": next(r["exit_code"] for r in records if r["name"] == "rust-native-tests"),
    "native_executables": native_runs, "standalone_dependency_paths": dependency_paths,
    "shared_sdk_source_unchanged": True, "protected_databases_or_keys_created": 0,
    "test_commands_failed": [r["name"] for r in records if r["status"] != "passed" and r["name"] != "msvc-version"],
    "not_run": ["Windows Wasmi bounded-client import/fuel execution", "Windows protected host/executor/product/UI", "Windows ARM64/x86"],
    "versions": versions}
(out / "results.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
receipt = emit(out, repo, source_before, records, "full-native-qualification", built_artifacts)
summary["provenance_receipt"] = receipt
print(json.dumps(summary, indent=2), flush=True)
raise SystemExit(1 if summary["test_commands_failed"] else 0)
