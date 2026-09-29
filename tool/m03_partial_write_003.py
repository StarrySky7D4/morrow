#!/usr/bin/env python3
"""Record one M03 qualification attempt; no retries, Git writes or CI calls.

Portable PASS and Windows cross-check PASS are not Windows runtime acceptance.
The Windows probe imports production driver/platform files, but excludes the
full Core/child/HTTP/product stack. Existing frozen candidates are never edited.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone


ROOT = Path(__file__).resolve().parents[1]
SOURCE_DIRS = (
    "network_node_stream_001", "native_session_stream_001",
    "native_pipe_win_001", "tool/qualification/m03-pipe-driver-check",
)


def hashes():
    paths = [Path(__file__).resolve()]
    for name in SOURCE_DIRS:
        paths.extend(p for p in (ROOT / name).rglob("*")
                     if p.is_file() and "target" not in p.relative_to(ROOT / name).parts)
    return {p.relative_to(ROOT).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(set(paths))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("portable", "windows"), required=True)
    parser.add_argument("--cross-check", action="store_true",
                        help="compile Windows driver/tests; does not run them")
    parser.add_argument("--output", type=Path, required=True,
                        help="new, non-existing evidence directory outside source modules")
    parser.add_argument("--target-dir", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    target = args.target_dir.resolve()
    for directory in [ROOT / name for name in SOURCE_DIRS]:
        if output.is_relative_to(directory) or target.is_relative_to(directory):
            parser.error("evidence and build directories must be outside source modules")
    output.mkdir(parents=True, exist_ok=False)
    receipt = {
        "candidate": "m03-partial-write-003", "mode": args.mode,
        "started_utc": datetime.now(timezone.utc).isoformat(),
        "platform": platform.platform(), "python": sys.version,
        "windows_runtime": "not_run", "full_host_runtime": "not_run",
        "product_G0": "not_run", "commands": [], "sources_before": hashes(),
    }
    def save():
        receipt["sources_after"] = hashes()
        receipt["source_unchanged"] = receipt["sources_before"] == receipt["sources_after"]
        receipt["finished_utc"] = datetime.now(timezone.utc).isoformat()
        (output / "receipt.json").write_text(
            json.dumps(receipt, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    def run(label, command, scope):
        started = time.monotonic()
        record = {"label": label, "command": command, "scope": scope}
        receipt["commands"].append(record)
        with (output / (label + ".log")).open("wb") as log:
            try:
                process = subprocess.Popen(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
                try:
                    record["exit_code"] = process.wait(timeout=600)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                    record["exit_code"] = None
                    record["error"] = "timeout; cleanup unconfirmed; do not count as PASS"
            except OSError as error:
                record["exit_code"] = None
                record["error"] = str(error)
        record["seconds"] = round(time.monotonic() - started, 3)
        record["status"] = "passed" if record["exit_code"] == 0 else "failed"
        save()
        print(label + ": " + record["status"], flush=True)
        return record["status"] == "passed"

    if args.mode == "windows" and os.name != "nt":
        receipt["status"] = "blocked"
        receipt["reason"] = "Actual Windows is required; cross-compilation is not runtime evidence."
        save()
        print(receipt["reason"], file=sys.stderr)
        return 2
    cargo, rustc = shutil.which("cargo"), shutil.which("rustc")
    if not cargo or not rustc:
        receipt["status"] = "blocked"
        receipt["reason"] = "Install Rust 1.95 or compatible stable and expose cargo/rustc in PATH."
        save()
        return 2
    target.mkdir(parents=True, exist_ok=True)
    ok = run("rustc-version", [rustc, "--version", "--verbose"], "toolchain")
    ok = run("cargo-version", [cargo, "--version"], "toolchain") and ok
    binary = output / ("write-state-tests.exe" if os.name == "nt" else "write-state-tests")
    compiled = run("write-state-build", [rustc, "--edition=2024", "--test",
                   "native_session_stream_001/src/write_state.rs", "-o", str(binary)], "pure state model")
    ok = compiled and ok
    if compiled:
        ok = run("write-state-tests", [str(binary), "--nocapture"], "pure state model") and ok
    common = ["--locked", "--target-dir", str(target)]
    ok = run("network-tests", [cargo, "test", *common, "--manifest-path",
             "network_node_stream_001/Cargo.toml"], "portable network and loopback") and ok
    probe = "tool/qualification/m03-pipe-driver-check/Cargo.toml"
    if args.cross_check:
        ok = run("windows-cross-check", [cargo, "check", *common, "--manifest-path", probe,
                 "--target", "x86_64-pc-windows-gnu", "--tests"], "compile only; no OS execution") and ok
    if args.mode == "windows":
        actual = run("windows-pipe-tests", [cargo, "test", *common, "--manifest-path", probe,
                     "--", "--nocapture", "--test-threads=1"], "actual same-process Windows pipes; not full host")
        receipt["windows_runtime"] = "passed" if actual else "failed"
        ok = actual and ok
    receipt["status"] = "passed" if ok and receipt["sources_before"] == hashes() else "failed"
    save()
    print("Receipt: " + str(output / "receipt.json"))
    return 0 if receipt["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
