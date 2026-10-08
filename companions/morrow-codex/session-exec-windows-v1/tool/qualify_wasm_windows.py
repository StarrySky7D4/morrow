"""Offline actual Windows + sealed Rust Wasm coupling, with source/artifact receipts.

This never compiles guests, generates a lock, downloads dependencies, or runs
another test suite. Only ordinary copied current synthetic test executables run.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import time

PACKAGE = Path(__file__).resolve().parents[1]
SOURCE = PACKAGE.parents[2]
RUNS = SOURCE.parent / "wasm-windows-coupled-runs"
GUESTS = {
    "MORROW_PROPOSAL_GUEST": (SOURCE.parent / "proposal-guest-runs/pg/wasm32-unknown-unknown/release/morrow_codex_proposal_guest_r2.wasm",
        "9552e969b09c9f982914e71ea99a3c93fddc2bdd30986f5932883bcaa3ac1243"),
    "MORROW_PROCESS_GUEST": (SOURCE.parent / "process-client-runs/pw/wasm32-unknown-unknown/release/morrow_codex_process_control_guest_v1.wasm",
        "48e2dd065c0f6ec4971650dd34ec25f852469479344727bef93f5f90334e1b36"),
}
parser = argparse.ArgumentParser()
parser.add_argument("mode", choices=("build", "run"))
parser.add_argument("label")
parser.add_argument("--filter", default="", choices=("", "actual_rust_wasm_proposal_claim_windows_process_and_wasm_controls",
    "expired_or_revoked_original_executor_blocks_actual_wasm_controls", "actual_input_effect_unknown_delivery_is_not_replayed_by_rust_wasm"),
    help="Optional exact coupling scenario; no filter runs the three scenarios plus the inert child entry.")
parser.add_argument("--target-dir", type=Path, help="Owner-approved existing native target, used only after its Cargo is idle.")
cache_override = os.environ.get("MORROW_NATIVE_CARGO_HOME")
parser.add_argument("--cargo-home", type=Path, default=Path(cache_override) if cache_override else None,
    help="Required existing isolated dependency cache, or MORROW_NATIVE_CARGO_HOME; this script fetches nothing.")
parser.add_argument("--rust-bin", type=Path, required=True, help="Explicit Rust toolchain bin directory; no machine-specific default.")
parser.add_argument("--vs-dev-cmd", type=Path, required=True, help="Explicit reviewed VsDevCmd.bat path.")
parser.add_argument("--capnp-bin", type=Path, required=True, help="Explicit Capnp compiler directory.")
args = parser.parse_args()
# Public portable configuration; historical fixed-machine receipts do not qualify this invocation.
RUST = args.rust_bin.resolve(strict=True)
VS_DEV_CMD = args.vs_dev_cmd.resolve(strict=True)
CAPNP_BIN = args.capnp_bin.resolve(strict=True)
if os.name != "nt" or not RUST.is_dir() or not CAPNP_BIN.is_dir() or not VS_DEV_CMD.is_file():
    raise ValueError("Windows and explicit existing toolchain paths are required")
if any(c in str(VS_DEV_CMD) for c in '\r\n"&|<>^%!'):
    raise ValueError("VsDevCmd path contains command metacharacters")
CARGO = RUST / "cargo.exe"
for tool in (CARGO, RUST / "rustc.exe", RUST / "rustdoc.exe", CAPNP_BIN / "capnp.exe"):
    if not tool.is_file():
        raise ValueError("Required explicit compiler tool is absent")

if not args.label.replace("-", "").isalnum():
    raise ValueError("invalid label")
OUT = RUNS / args.label
OUT.mkdir(parents=True, exist_ok=False)

def sha(raw):
    return hashlib.sha256(raw).hexdigest()

def artifacts():
    result = {}
    for variable, (path, expected) in GUESTS.items():
        actual = sha(path.read_bytes())
        if actual != expected:
            raise ValueError(f"sealed guest drift: {variable}")
        result[variable] = {"path": str(path), "sha256": actual}
    return result

def pins():
    result = {}
    directories = [PACKAGE, SOURCE / "core", SOURCE / "plugin_runtime", SOURCE / "sdk/rust",
        SOURCE / "extensions/agent-session-exec-v1-r2", SOURCE / "extensions/agent-process-control-v1",
        SOURCE / "extensions/agent-session-process-v1-host", SOURCE / "extensions/codex-session-exec-client-r2",
        SOURCE / "extensions/codex-process-control-client-v1",
        SOURCE / "companions/morrow-codex/upstream/p02-integration-004/codex-work/codex-rs"]
    extensions = (".rs", ".toml", ".lock", ".proto", ".capnp", ".py", ".md", ".txt", ".json", ".yaml", ".yml", ".rc")
    ignored = {"target", ".git", "__pycache__", "node_modules"}
    for directory in directories:
        for root, dirs, files in os.walk(directory):
            dirs[:] = [d for d in dirs if d not in ignored]
            for name in files:
                path = Path(root) / name
                if path.suffix in extensions:
                    result[path.relative_to(SOURCE).as_posix()] = sha(path.read_bytes())
    return dict(sorted(result.items()))

env = os.environ.copy()
for key in ("RUSTC", "RUSTDOC", "CC", "CXX", "AR", "CARGO_TARGET_DIR", "MORROW_WASM_COUPLED_CHILD", "MORROW_FIXED"):
    env.pop(key, None)
dev = str(VS_DEV_CMD)
setup = subprocess.run(f'cmd.exe /d /s /c ""{dev}" -no_logo -arch=amd64 >nul && set"', capture_output=True, text=True, check=True)
for line in setup.stdout.splitlines():
    if "=" in line and not line.startswith("="):
        key, value = line.split("=", 1)
        env[key] = value
env["PATH"] = str(RUST) + ";" + str(CAPNP_BIN) + ";" + env["PATH"]
env["RUSTC"] = str(RUST / "rustc.exe")
env["RUSTDOC"] = str(RUST / "rustdoc.exe")
if args.cargo_home is None or not args.cargo_home.is_absolute() or not args.cargo_home.is_dir():
    raise ValueError("existing absolute Cargo cache required")
env["CARGO_HOME"] = str(args.cargo_home)
env["CARGO_BUILD_JOBS"] = "4"
env["RUST_BACKTRACE"] = "1"
for key, name in (("TMP", "tmp"), ("LOCALAPPDATA", "local-app-data")):
    path = (SOURCE.parent / "wt" / args.label) if key == "TMP" else OUT / name
    path.mkdir(parents=True, exist_ok=False)
    env[key] = str(path)
env["TEMP"] = env["TMP"]
before_artifacts = artifacts()
for variable, item in before_artifacts.items():
    env[variable] = item["path"]
before = pins()
(OUT / "source-before.json").write_text(json.dumps(before, indent=2), encoding="utf-8")
target = args.target_dir or RUNS / "wn"
if not target.is_absolute():
    raise ValueError("target must be absolute")
command = [RUST / "cargo.exe", "test", "--offline", "--locked", "--manifest-path", PACKAGE / "Cargo.toml",
    "--target-dir", target, "--test", "codex_wasm_windows"]
if args.mode == "build":
    command += ["--no-run"]
else:
    if args.filter:
        command += [args.filter]
    command += ["--", "--nocapture", "--test-threads=1"]
    if args.filter:
        command += ["--exact"]
command = [str(part) for part in command]
started = time.time()
with (OUT / "stdout.log").open("wb") as stdout, (OUT / "stderr.log").open("wb") as stderr:
    run = subprocess.run(command, cwd=SOURCE, env=env, stdout=stdout, stderr=stderr)
after = pins()
after_artifacts = artifacts()
(OUT / "source-after.json").write_text(json.dumps(after, indent=2), encoding="utf-8")
changed = [k for k in sorted(set(before) | set(after)) if before.get(k) != after.get(k)]
stdout_text = (OUT / "stdout.log").read_text(encoding="utf-8", errors="replace")
executed = args.mode == "run" and bool(re.search(r"running [1-9][0-9]* tests?", stdout_text))
test_results = re.findall(r"test result: (ok|FAILED)\. ([0-9]+) passed; ([0-9]+) failed; ([0-9]+) ignored", stdout_text)
test_executables = [{"path": str(path), "sha256": sha(path.read_bytes())}
    for path in sorted((target / "debug/deps").glob("codex_wasm_windows-*.exe"))]
receipt = {"mode": args.mode, "test_filter": args.filter, "cargo_home": str(args.cargo_home), "args": command, "exit_code": run.returncode,
    "seconds": round(time.time() - started, 2), "source_before_sha256": sha(json.dumps(before, sort_keys=True).encode()),
    "source_after_sha256": sha(json.dumps(after, sort_keys=True).encode()), "changed_paths": changed,
    "artifacts_before": before_artifacts, "artifacts_after": after_artifacts,
    "stdout_sha256": sha((OUT / "stdout.log").read_bytes()), "stderr_sha256": sha((OUT / "stderr.log").read_bytes()),
    "runtime_executed": executed, "test_results": test_results, "test_executables": test_executables,
    "execution": "coupling test executable started" if executed else "runtime NOT_RUN"}
(OUT / "receipt.json").write_text(json.dumps(receipt, indent=2), encoding="utf-8")
print(json.dumps(receipt), flush=True)
for name in ("stdout.log", "stderr.log"):
    print((OUT / name).read_text(encoding="utf-8", errors="replace")[-6500:], flush=True)
runtime_checked = args.mode == "build" or executed and bool(test_results)
raise SystemExit(0 if run.returncode == 0 and runtime_checked and not changed and before_artifacts == after_artifacts else 1)
