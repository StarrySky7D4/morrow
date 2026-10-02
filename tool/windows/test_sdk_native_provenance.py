"""Negative controls for this correction helper; never builds or modifies inputs.

Creates retained isolated source/artifact copies. Each helper invocation must
reject before a functional command, with zero current passes. Git worktrees
are retained for inspection; no checkout/reset/clean of an existing directory.
"""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
from sdk_native_provenance import MODE, ROLES, digest, file_identity, json_bytes, load_verified, require

p = argparse.ArgumentParser()
p.add_argument("--repo", type=Path, required=True)
p.add_argument("--pin", required=True)
p.add_argument("--tool-commit", required=True)
p.add_argument("--prior", type=Path, required=True)
p.add_argument("--provenance-sha256", required=True)
p.add_argument("--legacy", type=Path, required=True)
p.add_argument("--full", type=Path, required=True)
p.add_argument("--wrong-pin", required=True)
p.add_argument("--output", type=Path, required=True)
p.add_argument("--cargo-home", type=Path, required=True)
p.add_argument("--capnp-bin", type=Path, required=True)
a = p.parse_args()
repo, prior, out = a.repo.resolve(), a.prior.resolve(), a.output.resolve()
require(not out.is_relative_to(repo) and not out.is_relative_to(prior), "Controls must not modify original inputs")
require(Path(__file__).resolve() == repo / "tool/windows/test_sdk_native_provenance.py", "Control path does not belong to committed checkout")
value, before, _ = load_verified(prior, a.provenance_sha256, repo, a.pin, a.tool_commit)
out.mkdir(parents=True, exist_ok=False)
rows = []

def git(*args):
    return subprocess.check_output(["git", "-c", "core.longpaths=true", "-C", str(repo), *args], stderr=subprocess.STDOUT)

def source_copy(case):
    target = case / "source"
    log = git("worktree", "add", "--detach", "--no-checkout", str(target), a.tool_commit)
    require({item.name for item in target.iterdir()} == {".git"}, "New control worktree unexpectedly populated")
    for command in [("sparse-checkout", "set", "sdk", "core/schemas", "tool/windows"), ("read-tree", "-mu", "HEAD")]:
        log += subprocess.check_output(["git", "-c", "core.longpaths=true", "-C", str(target), *command], stderr=subprocess.STDOUT)
    # Preserve exact producer working bytes, including explicit LF/CRLF identity.
    for name in before["files"]:
        if name.startswith("tool/windows/"):
            (target / name).write_bytes((repo / name).read_bytes())
    (case / "worktree.log").write_bytes(log)
    return target

def artifact_copy(case):
    target = case / "prior"
    target.mkdir()
    paths = set(value["record_files"]) | {"provenance.json", "provenance-receipt.json"}
    paths |= {relative for relative, _ in ROLES.values()}
    paths |= {"qualification-source/" + name for name in before["files"] if name.startswith("sdk/")}
    for name in sorted(paths):
        destination = target / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(prior / name, destination)
    return target

def command_change(target, kind):
    commands = json.loads((target / "commands.json").read_text("utf-8"))
    by = {row["name"]: row for row in commands}
    if kind == "command_shape":
        by["msvc-cpp_codec-build"]["command"].insert(2, "/DNDEBUG")
    elif kind == "extra_failure":
        by["channel-vectors"].update(exit_code=1, status="failed")
    else:
        by["rust-native-tests"].update(exit_code=0, status="passed")
    (target / "commands.json").write_bytes(json_bytes(commands))
    manifest = json.loads((target / "provenance.json").read_text("utf-8"))
    manifest["record_files"]["commands.json"] = file_identity(target / "commands.json")
    (target / "provenance.json").write_bytes(json_bytes(manifest))
    # Controlled new digest bypasses byte-level rejection to test semantic gates.
    return digest((target / "provenance.json").read_bytes())

cases = [
    ("sdk_source_drift", "Source drift: sdk/"),
    ("schema_source_drift", "Source drift: core/schemas/"),
    ("tool_source_drift", "Source drift: tool/windows/"),
    ("extra_sdk_source", "Extra/missing source inventory: sdk"),
    ("pin_drift", "Current source/schema/tool snapshot drift"),
    *[(role + "_drift", "Artifact drift: " + role) for role in ROLES],
    ("command_shape", "Unknown codec build shape"),
    ("extra_failure", "Only the two documented failures"),
    ("missing_failure", "Only the two documented failures"),
    ("manifest_drift", "Provenance digest drift"),
    ("record_drift", "Record drift: rust-native-tests.log"),
    ("fixture_source_drift", "Historical SDK fixture source drift"),
    ("legacy_unbound", "Legacy directory has no build-bound artifact provenance"),
    ("full_success_is_not_retry_fixture", "Unsupported producer/correction mode"),
]
for name, expected_reason in cases:
    case = out / name; case.mkdir()
    target_repo, target_prior, pin, fingerprint = repo, prior, a.pin, a.provenance_sha256
    if name in {"sdk_source_drift", "schema_source_drift", "tool_source_drift", "extra_sdk_source"}:
        target_repo = source_copy(case)
        paths = {"sdk_source_drift": "sdk/rust/src/lib.rs", "schema_source_drift": "core/schemas/dependency_call.capnp",
            "tool_source_drift": "tool/windows/README.md", "extra_sdk_source": "sdk/untracked-control.rs"}
        changed = target_repo / paths[name]
        with changed.open("ab") as handle:
            handle.write(b"\n// isolated negative-control drift\n")
        (case / "mutation.json").write_bytes(json_bytes({"path": paths[name], **file_identity(changed)}))
    elif name == "pin_drift":
        pin = a.wrong_pin
    elif name == "legacy_unbound":
        target_prior = a.legacy.resolve()
    elif name == "full_success_is_not_retry_fixture":
        target_prior = a.full.resolve()
        fingerprint = digest((target_prior / "provenance.json").read_bytes())
    else:
        target_prior = artifact_copy(case)
        if name in {role + "_drift" for role in ROLES}:
            role = name.removesuffix("_drift")
            with (target_prior / ROLES[role][0]).open("ab") as handle:
                handle.write(b"controlled artifact drift")
        elif name in {"command_shape", "extra_failure", "missing_failure"}:
            fingerprint = command_change(target_prior, name)
        elif name == "manifest_drift":
            with (target_prior / "provenance.json").open("ab") as handle: handle.write(b" ")
        elif name == "record_drift":
            with (target_prior / "rust-native-tests.log").open("ab") as handle: handle.write(b"changed log")
        elif name == "fixture_source_drift":
            with (target_prior / "qualification-source/sdk/rust/src/lib.rs").open("ab") as handle: handle.write(b"\n// changed fixture")
    command = [sys.executable, str(target_repo / "tool/windows/rerun_sdk_native_corrections.py"),
        "--repo", str(target_repo), "--pin", pin, "--tool-commit", a.tool_commit, "--mode", MODE,
        "--prior", str(target_prior), "--provenance-sha256", fingerprint, "--output", str(case / "result"),
        "--cargo-home", str(a.cargo_home.resolve()), "--capnp-bin", str(a.capnp_bin.resolve())]
    with (case / "helper-driver.log").open("xb") as log:
        result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=60)
    report = json.loads((case / "result/results.json").read_text("utf-8"))
    passed = (result.returncode == 2 and report["status"] == "rejected" and report["executed_command_count"] == 0
        and report["commands"] == [] and report["current_execution"]["rust_passed"] == 0
        and report["current_execution"]["codec_passed"] == 0 and expected_reason in report["reason"])
    rows.append({"control": name, "passed": passed, "helper_exit": result.returncode,
        "reason": report.get("reason"), "expected_reason": expected_reason, "command": command,
        "executed_command_count": report["executed_command_count"], "current_execution": report["current_execution"]})
    (out / "results.json").write_bytes(json_bytes({"status": "in_progress", "controls": rows}))
    print(f"{name}: {'passed' if passed else 'FAILED'}; functional commands={report['executed_command_count']}", flush=True)
    require(passed, "Negative control failed: " + name)
load_verified(prior, a.provenance_sha256, repo, a.pin, a.tool_commit)
summary = {"status": "passed", "controls_passed": len(rows), "controls": rows,
    "original_inputs_unchanged": True, "functional_commands_executed": 0, "current_sdk_passes_promoted": 0,
    "protected_databases_or_keys_created": 0}
(out / "results.json").write_bytes(json_bytes(summary))
print(json.dumps({key: value for key, value in summary.items() if key != "controls"}, indent=2), flush=True)
