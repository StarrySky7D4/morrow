"""One explicit local stage per invocation; never downloads or chooses versions.

Root must first bind a reviewed complete-source receipt with both --source-receipt
and --source-receipt-sha256. Example stages: metadata, lock, build, run. Lock is
the only stage allowed to create/update the independent probe Cargo.lock and
requires --prepare-lock. Existing locks additionally require --update-probe-lock.
All stages are offline. Build/run additionally require --locked. This runner
does not alter fork patches, vendor configuration, source locks or frozen inputs.
"""
from pathlib import Path
import argparse
import datetime as dt
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
RECEIPTS = ROOT / "receipts/p02-integration-004"
PROBE = ROOT / "qualification/p02-integration-004"
OUT = ROOT / "out/p02-integration-004"
SOURCE = ROOT / "upstream/p02-integration-004/codex-work/codex-rs"
TOOLCHAIN = Path(r"C:\Users\Administrator\.rustup\toolchains\1.95.0-x86_64-pc-windows-msvc")
CAPNP = Path(r"C:\Users\Administrator\capnp-bin\capnp.exe")
HANDOFF_SHA256 = "995cfa721bae3b94a494a0ee88e9e76f9e29afeec71ee6481c9ac756446d3ca4"
CWD = Path("C:/")


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for data in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(data)
    return h.hexdigest()


def safe(path, *, local=True):
    path = Path(os.path.abspath(path))
    if local:
        path.relative_to(ROOT)
    for item in (*reversed(path.parents), path):
        try:
            info = item.lstat()
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            raise RuntimeError("Linked/reparse path refused: " + str(item))
    return path


def frozen_snapshot():
    handoff = safe(ROOT / "receipts/handoff.json")
    actual_handoff = digest(handoff)
    if actual_handoff != HANDOFF_SHA256:
        raise RuntimeError("Frozen handoff digest mismatch")
    expected = json.loads(handoff.read_text(encoding="utf-8"))["input_sha256"]
    if len(expected) != 17:
        raise RuntimeError("Expected exactly 17 frozen inputs")
    actual = {name: digest(safe(ROOT / name)) for name in expected}
    second = ROOT / "receipts/p02-native-probe-001/handoff.json"
    if digest(second) != "1fc5833e632bb3afab827a9ca3519418ab7afdf34c6e50e6cafa59ecce707020":
        raise RuntimeError("Prior network handoff changed")
    bound = json.loads(second.read_text())["input_sha256"]
    observed = {name: digest(safe(ROOT / name)) for name in bound}
    third = ROOT / "receipts/p02-exec-store-002/handoff.json"
    if digest(third) != "d3c19e26c5c04461eed188ff1d84518add5d8af711c3dfc78e6e49ad8312cd76":
        raise RuntimeError("Frozen batch002 handoff changed")
    third_bound = json.loads(third.read_text())["input_sha256"]
    third_observed = {name: digest(safe(ROOT / name)) for name in third_bound}
    fourth = ROOT / "receipts/p02-core-network-003/handoff.json"
    if digest(fourth) != "97501761be789decabc52de2a89311d1d6a877b6dd38fa8c795f649a7b91aacb":
        raise RuntimeError("Frozen batch003 handoff changed")
    fourth_bound = json.loads(fourth.read_text(encoding="utf-8"))["input_sha256"]
    fourth_observed = {name:digest(safe(ROOT/name)) for name in fourth_bound}
    return {"files":actual,"network_batch_files":observed,"exec_store_batch_files":third_observed,"core_network_files":fourth_observed,
            "matches_frozen_handoff":actual==expected and observed==bound and third_observed==third_bound and fourth_observed==fourth_bound}



def input_snapshot(source_receipt):
    paths = [Path(__file__), PROBE / "Cargo.toml", PROBE / "Cargo.lock",
             OUT / "cargo-home/config.toml", ROOT / "upstream/p02-exec-store-002/host-kit-003/manifest.json",
             source_receipt]
    paths.extend(sorted((PROBE / "src").rglob("*.rs")))
    for name in ("tokio-tungstenite", "tungstenite", "runfiles"):
        fork = ROOT / "out/p02-exec-store-002/build-forks" / name
        paths.append(fork / "Cargo.toml")
        paths.extend(sorted(path for path in fork.rglob("*") if path.is_file()))
    paths.extend(SOURCE / item for item in ("Cargo.toml", "Cargo.lock"))
    paths.extend(ROOT / item for item in json.loads((RECEIPTS / "patched-inputs.json").read_text(encoding="utf-8")))
    paths.extend(ROOT / "receipts/p02-exec-store-002" / name for name in ("mxc-verification-005.json", "nucleo-verification-002.json", "vendor-result.json"))
    result = {}
    for path in paths:
        safe(path)
        result[path.relative_to(ROOT).as_posix()] = digest(path) if path.is_file() else None
    return result


def clean_environment(run_dir):
    allowed = {"SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT", "PATH", "PROGRAMFILES", "PROGRAMFILES(X86)",
               "PROGRAMW6432", "PROGRAMDATA", "PROCESSOR_ARCHITECTURE", "NUMBER_OF_PROCESSORS", "LIB", "LIBPATH",
               "INCLUDE", "VCTOOLSINSTALLDIR", "VCTOOLSVERSION", "VSINSTALLDIR", "VCINSTALLDIR", "WINDOWSSDKDIR",
               "WINDOWSSDKVERSION", "UNIVERSALCRTSDKDIR", "UCRTVERSION", "VSCMD_ARG_TGT_ARCH", "VSCMD_ARG_HOST_ARCH"}
    env = {key: os.environ[key] for key in allowed if key in os.environ}
    profile, temp = safe(OUT / "isolated-userprofile"), safe(run_dir / "tmp")
    for path in (profile, profile / "AppData/Local", profile / "AppData/Roaming", temp):
        path.mkdir(parents=True, exist_ok=True)
    env.update({"CARGO_HOME": str(OUT / "cargo-home"), "CARGO_TARGET_DIR": str(OUT / "target"),
                "CARGO_NET_OFFLINE": "true", "CARGO_TERM_COLOR": "never", "CARGO_INCREMENTAL": "0",
                "RUSTC": str(TOOLCHAIN / "bin/rustc.exe"), "RUSTDOC": str(TOOLCHAIN / "bin/rustdoc.exe"),
                "RUSTUP_AUTO_INSTALL": "0", "HOME": str(profile), "USERPROFILE": str(profile),
                "LOCALAPPDATA": str(profile / "AppData/Local"), "APPDATA": str(profile / "AppData/Roaming"),
                "TEMP": str(temp), "TMP": str(temp), "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_SYSTEM": os.devnull,
                "GIT_CONFIG_GLOBAL": os.devnull, "GIT_TERMINAL_PROMPT": "0", "GIT_NO_LAZY_FETCH": "1"})
    env["PATH"] = str(CAPNP.parent) + os.pathsep + str(TOOLCHAIN / "bin") + os.pathsep + env.get("PATH", "")
    return env


def discover_vs(env, commands):
    vswhere = Path(r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe")
    safe(vswhere, local=False)
    argv = [str(vswhere), "-latest", "-products", "*", "-requires", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath"]
    result = subprocess.run(argv, cwd=CWD, env=env, capture_output=True, timeout=30, check=False)
    commands.append({"argv": argv, "exit_code": result.returncode, "purpose": "standard VS installation discovery"})
    if result.returncode:
        raise RuntimeError("vswhere failed")
    install = result.stdout.decode("utf-8-sig").strip()
    script = safe(Path(install) / "Common7/Tools/VsDevCmd.bat", local=False)
    if not install or not script.is_file() or any(ch in str(script) for ch in '\"&|<>^%\r\n'):
        raise RuntimeError("Invalid standard VS developer environment path")
    command = f'""{script}" -no_logo -arch=x64 -host_arch=x64 && set"'
    argv = [str(Path(env["SYSTEMROOT"]) / "System32/cmd.exe"), "/d", "/s", "/c", command]
    result = subprocess.run(argv, cwd=CWD, env=env, capture_output=True, timeout=60, check=False)
    commands.append({"argv": argv, "exit_code": result.returncode, "purpose": "standard VS environment, values deliberately not logged"})
    if result.returncode:
        raise RuntimeError("VsDevCmd failed")
    permitted = {"PATH", "LIB", "LIBPATH", "INCLUDE", "VCTOOLSINSTALLDIR", "VCTOOLSVERSION", "VSINSTALLDIR", "VCINSTALLDIR",
                 "WINDOWSSDKDIR", "WINDOWSSDKVERSION", "UNIVERSALCRTSDKDIR", "UCRTVERSION", "VSCMD_ARG_TGT_ARCH", "VSCMD_ARG_HOST_ARCH"}
    for line in result.stdout.decode("utf-8", errors="replace").splitlines():
        key, sep, value = line.partition("=")
        if sep and key.upper() in permitted:
            for old in list(env):
                if old.upper() == key.upper():
                    del env[old]
            env[key.upper()] = value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", choices=("metadata", "lock", "build", "run"))
    parser.add_argument("--source-receipt", required=True)
    parser.add_argument("--source-receipt-sha256", required=True)
    parser.add_argument("--prepare-lock", action="store_true")
    parser.add_argument("--update-probe-lock", action="store_true")
    parser.add_argument("--discover-vs", action="store_true")
    parser.add_argument("--timeout-seconds", type=int, default=1200)
    args = parser.parse_args()
    if not 1 <= args.timeout_seconds <= 3600:
        parser.error("timeout must be between 1 and 3600 seconds")
    run_id = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ") + "-" + uuid.uuid4().hex[:10]
    receipt_dir, run_dir = safe(RECEIPTS / "runs" / (args.stage + "-" + run_id)), safe(OUT / "runs" / run_id)
    receipt_dir.mkdir(parents=True, exist_ok=False)
    run_dir.mkdir(parents=True, exist_ok=False)
    result = {"stage": args.stage, "run_id": run_id, "status": "blocked", "cwd": str(CWD), "commands": [],
              "P-02": "not_complete", "G0": "not_claimed", "network_policy": "offline_no_download",
              "source_review": "caller-selected complete-source receipt; hash-bound, not a repeat of source reconstruction"}
    before_frozen = before_inputs = None
    source_receipt = ROOT / args.source_receipt
    try:
        source_receipt = safe(source_receipt)
        for config in (CWD / ".cargo/config", CWD / ".cargo/config.toml"):
            if config.exists():
                raise RuntimeError("Root Cargo config exists; refusing inherited configuration")
        before_frozen = frozen_snapshot()
        if not before_frozen["matches_frozen_handoff"]:
            raise RuntimeError("Frozen inputs differ from handoff")
        if not re.fullmatch(r"[a-f0-9]{64}", args.source_receipt_sha256) or digest(source_receipt) != args.source_receipt_sha256:
            raise RuntimeError("Complete-source receipt digest mismatch")
        before_inputs = input_snapshot(source_receipt)
        missing = [name for name, value in before_inputs.items() if value is None and name != "qualification/p02-integration-004/Cargo.lock"]
        if missing:
            raise RuntimeError("Missing required source/build inputs: " + ", ".join(missing))
        probe_lock = PROBE / "Cargo.lock"
        if args.stage == "lock":
            if not args.prepare_lock or (probe_lock.exists() and not args.update_probe_lock):
                raise RuntimeError("Lock preparation/update must be explicitly selected")
            if probe_lock.exists():
                shutil.copyfile(probe_lock, receipt_dir / "probe-lock-before.txt")
        elif args.prepare_lock or args.update_probe_lock:
            raise RuntimeError("Lock mutation flags only apply to lock stage")
        if args.stage in ("build", "run") and not probe_lock.is_file():
            raise RuntimeError("A separately prepared probe Cargo.lock is required")
        env = clean_environment(run_dir)
        if args.discover_vs:
            discover_vs(env, result["commands"])
        result.update({"environment_keys": sorted(env), "cargo_home": env["CARGO_HOME"],
                       "target_dir": env["CARGO_TARGET_DIR"], "isolated_userprofile": env["USERPROFILE"]})
        for label, tool, flags in (("rustc", TOOLCHAIN / "bin/rustc.exe", ["-vV"]), ("cargo", TOOLCHAIN / "bin/cargo.exe", ["--version"])):
            safe(tool, local=False)
            version = subprocess.run([str(tool), *flags], cwd=CWD, env=env, capture_output=True, timeout=30, check=False)
            (receipt_dir / (label + "-version.stdout.txt")).write_bytes(version.stdout)
            (receipt_dir / (label + "-version.stderr.txt")).write_bytes(version.stderr)
            result["commands"].append({"argv": [str(tool), *flags], "exit_code": version.returncode})
            text = version.stdout.decode("utf-8", errors="replace")
            expected = "release: 1.95.0" if label == "rustc" else "cargo 1.95.0 "
            if version.returncode or expected not in text or (label == "rustc" and "host: x86_64-pc-windows-msvc" not in text):
                raise RuntimeError("Exact compiler/tool identity mismatch")
            result[label + "_version"] = text.strip()
        base = [str(TOOLCHAIN / "bin/cargo.exe")]
        manifest = ["--manifest-path", str(PROBE / "Cargo.toml")]
        target = ["--target", "x86_64-pc-windows-msvc", "--target-dir", str(OUT / "target")]
        runtime_receipt = RECEIPTS / ("runtime-" + run_id + ".json")
        if args.stage == "metadata":
            argv = base + ["metadata", "--no-deps", "--format-version", "1", "--locked", "--offline"] + manifest
        elif args.stage == "lock":
            # Resolve the independent graph while retaining precise Git IDs
            # from the explicitly copied upstream lock seed. Git directory
            # source replacement relies on these identities. This stage is
            # permitted to update only this probe's Cargo.lock.
            argv = base + ["metadata", "--offline", "--format-version", "1"] + manifest
        elif args.stage == "build":
            argv = base + ["build", "--locked", "--offline", "--message-format=json-render-diagnostics"] + manifest + target + ["--bin", "p02-integration-probe"]
        else:
            argv = base + ["run", "--locked", "--offline"] + manifest + target + ["--bin", "p02-integration-probe", "--", str(runtime_receipt)]
        stdout, stderr = receipt_dir / "stdout.txt", receipt_dir / "stderr.txt"
        result.update({"argv": argv, "environment_keys": sorted(env), "cargo_home": env["CARGO_HOME"],
                       "target_dir": env["CARGO_TARGET_DIR"], "isolated_userprofile": env["USERPROFILE"], "timeout_seconds": args.timeout_seconds})
        print(json.dumps({"stage": args.stage, "receipt_dir": str(receipt_dir), "state": "starting"}), flush=True)
        start = time.monotonic()
        with stdout.open("wb") as out_stream, stderr.open("wb") as err_stream:
            process = subprocess.Popen(argv, cwd=CWD, env=env, stdout=out_stream, stderr=err_stream, stdin=subprocess.DEVNULL)
            result["pid"] = process.pid
            try:
                exit_code = process.wait(timeout=args.timeout_seconds)
            except subprocess.TimeoutExpired:
                result["timed_out"], result["exit_code"] = True, None
                kill = [str(Path(env["SYSTEMROOT"]) / "System32/taskkill.exe"), "/PID", str(process.pid), "/T", "/F"]
                stopped = subprocess.run(kill, cwd=CWD, env=env, capture_output=True, timeout=30, check=False)
                result["commands"].append({"argv": kill, "exit_code": stopped.returncode, "purpose": "timeout: terminate only this runner-owned process tree"})
                raise RuntimeError("Cargo stage exceeded its bounded timeout")
        result.update({"exit_code": exit_code, "elapsed_seconds": time.monotonic() - start,
                       "stdout_sha256": digest(stdout), "stderr_sha256": digest(stderr)})
        if exit_code:
            raise RuntimeError("Cargo stage returned nonzero; see preserved logs")
        if args.stage == "lock" and not probe_lock.is_file():
            raise RuntimeError("Lock stage returned zero without producing Cargo.lock")
        if args.stage == "build":
            artifacts = []
            for line in stdout.read_text(encoding="utf-8").splitlines():
                item = json.loads(line)
                if item.get("reason") == "compiler-artifact" and item.get("target", {}).get("name") == "p02-integration-probe" and item.get("executable"):
                    executable = safe(Path(item["executable"]))
                    executable.relative_to(OUT / "target")
                    artifacts.append({"path": str(executable), "sha256": digest(executable)})
            if len(artifacts) != 1:
                raise RuntimeError("Expected one compiler-reported probe executable")
            result["artifacts"] = artifacts
        if args.stage == "run":
            runtime = json.loads(runtime_receipt.read_text(encoding="utf-8"))
            result["runtime_receipt"], result["runtime_sha256"] = str(runtime_receipt), digest(runtime_receipt)
            if runtime.get("status") != "passed_limited_integration_probe" or runtime.get("P-02") != "not_complete" or runtime.get("G0") != "not_claimed":
                raise RuntimeError("Runtime receipt did not establish the limited probe result")
            executable = safe(OUT / "target/x86_64-pc-windows-msvc/debug/p02-integration-probe.exe")
            result["expected_cargo_bin_artifact"] = {"path": str(executable), "sha256": digest(executable)}
        result["status"] = {"metadata": "passed_manifest_metadata_only", "lock": "probe_lock_prepared_not_build_proof",
                            "build": "compiled_not_runtime_proof", "run": "passed_limited_integration_probe"}[args.stage]
    except Exception as error:
        result["error"] = {"type": type(error).__name__, "message": str(error)}
    finally:
        try:
            after_frozen = frozen_snapshot()
            result["frozen_before"], result["frozen_after"] = before_frozen, after_frozen
            if not after_frozen["matches_frozen_handoff"] or (before_frozen is not None and before_frozen != after_frozen):
                raise RuntimeError("Frozen inputs changed")
            after_inputs = input_snapshot(source_receipt)
            result["inputs_before"], result["inputs_after"] = before_inputs, after_inputs
            if before_inputs is not None:
                changed = [name for name in before_inputs if before_inputs[name] != after_inputs[name]]
                result["changed_inputs"] = changed
                allowed_changes = {"qualification/p02-integration-004/Cargo.lock"} if args.stage == "lock" else set()
                if set(changed) - allowed_changes:
                    raise RuntimeError("Unexpected immutable build input change: " + ", ".join(changed))
        except Exception as error:
            result["status"] = "blocked"
            result["verification_error"] = {"type": type(error).__name__, "message": str(error)}
        result["completed_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
        (receipt_dir / "result.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"status": result["status"], "receipt": str(receipt_dir / "result.json"), "error": result.get("error")}), flush=True)
    return 2 if result["status"] == "blocked" else 0


if __name__ == "__main__":
    sys.exit(main())
