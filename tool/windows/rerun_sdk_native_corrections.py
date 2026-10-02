"""Only schema-reference and MSVC UTF-8 corrections with build-bound inputs."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
from sdk_native_provenance import MODE, ROLES, Rejected, codec_recipe, digest, file_identity, json_bytes, load_verified, require

p = argparse.ArgumentParser()
p.add_argument("--repo", type=Path, required=True)
p.add_argument("--pin", required=True)
p.add_argument("--tool-commit", required=True)
p.add_argument("--mode", choices=[MODE], required=True)
p.add_argument("--prior", type=Path, required=True)
p.add_argument("--provenance-sha256", required=True)
p.add_argument("--output", type=Path, required=True)
p.add_argument("--cargo-home", type=Path, required=True)
p.add_argument("--capnp-bin", type=Path, required=True)
a = p.parse_args()
repo, prior, out = a.repo.resolve(), a.prior.resolve(), a.output.resolve()
require(not out.is_relative_to(repo) and not out.is_relative_to(prior), "Output must not modify source/prior inputs")
out.mkdir(parents=True, exist_ok=False)
commands, current = [], {"status": "not_run", "rust_passed": 0, "rust_failed": 0, "codec_passed": 0}
try:
    require(Path(__file__).resolve() == repo / "tool/windows/rerun_sdk_native_corrections.py", "Helper path does not belong to qualified tool checkout")
    value, before, historical_commands = load_verified(prior, a.provenance_sha256, repo, a.pin, a.tool_commit)
    (out / "verified-before.json").write_bytes(json_bytes({"source": before, "artifacts": value["artifacts"], "tools": value["tools"]}))
    actual_artifacts = {}
    for role, (relative, _) in ROLES.items():
        data = (prior / relative).read_bytes()
        require(digest(data) == value["artifacts"][role]["sha256"], "Artifact changed before copy: " + role)
        destination = out / Path(relative).name
        destination.write_bytes(data)
        require(file_identity(destination) == {"bytes": value["artifacts"][role]["bytes"], "sha256": value["artifacts"][role]["sha256"]}, "Copied artifact drift")
        actual_artifacts[role] = destination
    env, selected = dict(os.environ), {}
    for line in (prior / "vs-toolset-setup.log").read_bytes().decode("mbcs").splitlines():
        key, separator, item = line.partition("=")
        if separator and key.upper() in {"PATH", "INCLUDE", "LIB", "LIBPATH", "VCTOOLSVERSION", "WINDOWSSDKVERSION"}:
            if key.upper() not in selected or key == key.upper():
                selected[key.upper()] = item
    env.update(selected)
    env.update(CARGO_HOME=str(a.cargo_home.resolve()), CARGO_TARGET_DIR=str(out / "rust-target"), VSLANG="1033")
    env["PATH"] = str(a.capnp_bin.resolve()) + os.pathsep + env["PATH"]
    for variable in ("MORROW_DEPENDENCY_SMOKE_DIR", "MORROW_IO_SMOKE_DIR"):
        env.pop(variable, None)
    rustc = shutil.which("rustc", path=env["PATH"])
    require(rustc is not None, "Current Rust compiler is unavailable")
    rust_version = subprocess.run([rustc, "-Vv"], env=env, capture_output=True, text=True, check=True).stdout
    original_versions = json.loads((prior / "compiler-versions.json").read_text("utf-8"))
    require(rust_version == original_versions["rustc"], "Current Rust compiler version drift")
    current_tools = {"bound_executables": value["tools"], "rustc_version": rust_version,
        "rustc_executable": {"path": rustc, **file_identity(rustc)},
        "python_executable": {"path": sys.executable, **file_identity(sys.executable)}}
    (out / "current-tool-identity.json").write_bytes(json_bytes(current_tools))
    def verify_inputs():
        load_verified(prior, a.provenance_sha256, repo, a.pin, a.tool_commit)
        for role, path in actual_artifacts.items():
            require(file_identity(path) == {"bytes": value["artifacts"][role]["bytes"], "sha256": value["artifacts"][role]["sha256"]}, "Reused input drift: " + role)
    def run(name, command, cwd):
        verify_inputs()
        with (out / (name + ".log")).open("xb") as handle:
            result = subprocess.run(command, cwd=cwd, env=env, stdout=handle, stderr=subprocess.STDOUT, timeout=300)
        commands.append({"name": name, "command": command, "cwd": str(cwd), "exit_code": result.returncode,
            "status": "passed" if result.returncode == 0 else "failed", "log_sha256": file_identity(out / (name + ".log"))["sha256"]})
        (out / "commands.json").write_bytes(json_bytes(commands))
        verify_inputs()
        print(f"{name}: exit {result.returncode}", flush=True)
        return result.returncode
    # Fixed recipes: historical command arrays are evidence, never replayed.
    corrected_source = out / "corrected-source"
    shutil.copytree(repo / "sdk", corrected_source / "sdk")
    shutil.copytree(repo / "core/schemas", corrected_source / "core/schemas")
    for name, identity in before["files"].items():
        if name.startswith("sdk/") or name.startswith("core/schemas/"):
            require(file_identity(corrected_source / name)["sha256"] == identity["working_sha256"], "Corrected source copy drift")
    cargo = value["tools"]["rust-native-tests"]["path"]
    rust_code = run("rust-native-tests", [cargo, "test", "--offline", "--locked", "--manifest-path",
        str(corrected_source / "sdk/rust/Cargo.toml")], corrected_source)
    compiler = value["tools"]["msvc-cpp_codec-build"]["path"]
    build_code = run("msvc-cpp_codec-build-utf8", codec_recipe(compiler, repo, out, actual_artifacts, True), repo)
    codec_code = None
    if build_code == 0:
        codec_code = run("msvc-cpp_codec-run-utf8", [str(out / "cpp_codec.exe"), str(repo / "sdk/tests/fixtures")], repo)
    verified, after, _ = load_verified(prior, a.provenance_sha256, repo, a.pin, a.tool_commit)
    require(after == before, "Source identity changed before summary")
    verify_inputs()
    for name, identity in before["files"].items():
        if name.startswith("sdk/") or name.startswith("core/schemas/"):
            require(file_identity(corrected_source / name)["sha256"] == identity["working_sha256"], "Executed source changed before summary")
    counts = [tuple(map(int, pair)) for pair in re.findall(r"test result: .*? (\d+) passed; (\d+) failed",
        (out / "rust-native-tests.log").read_text("utf-8", errors="replace"))]
    current = {"status": "passed" if all(row["exit_code"] == 0 for row in commands) else "failed",
        "rust_passed": sum(row[0] for row in counts), "rust_failed": sum(row[1] for row in counts),
        "rust_command_exit": rust_code, "codec_passed": int(codec_code == 0), "codec_command_exit": codec_code}
    (out / "verified-after.json").write_bytes(json_bytes({"source": after, "original_artifacts": verified["artifacts"],
        "actually_reused_artifacts": {role: {"path": str(path), **file_identity(path)} for role, path in actual_artifacts.items()},
        "produced_outputs": {name: file_identity(out / name) for name in ["cpp_codec.obj", "cpp_codec.exe"] if (out / name).is_file()}}))
    report = {"mode": a.mode, "status": current["status"], "source_snapshot": after,
        "current_execution": current, "tools": current_tools, "commands": commands, "historical_observations": {
            "status": "historical_only_not_added_to_current", "producer_mode": value["producer_mode"],
            "documented_failures": [name for name, row in historical_commands.items() if row["exit_code"] != 0 and name != "msvc-version"]},
        "executed_command_count": len(commands), "prior_artifacts_and_source_reverified": True,
        "protected_databases_or_keys_created": 0}
except (Rejected, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
    current = {"status": "rejected", "rust_passed": 0, "rust_failed": 0, "codec_passed": 0}
    report = {"mode": a.mode, "status": "rejected", "reason": str(error), "current_execution": current,
        "commands": commands, "executed_command_count": len(commands), "historical_observations": {"status": "not_promoted"},
        "protected_databases_or_keys_created": 0}
(out / "results.json").write_bytes(json_bytes(report))
print(json.dumps({"status": report["status"], "current_execution": current,
    "executed_command_count": len(commands), "reason": report.get("reason")}, indent=2), flush=True)
raise SystemExit(0 if report["status"] == "passed" else 2)
