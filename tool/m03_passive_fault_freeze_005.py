"""Freeze reviewed inputs and pure-check evidence; never starts a host or socket."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
RUNNER = ROOT / "tool/m03_passive_fault_005.py"
TEST = ROOT / "tool/tests/test_m03_passive_fault.py"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host-manifest", type=Path, required=True)
    parser.add_argument("--host-sha256", required=True)
    parser.add_argument("--guest-manifest", type=Path, required=True)
    parser.add_argument("--guest-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    loader = importlib.util.spec_from_file_location("passive005_freeze", RUNNER)
    runner = importlib.util.module_from_spec(loader); loader.loader.exec_module(runner)
    runner.verify_candidate(args.host_manifest, args.host_sha256)
    runner.verify_candidate(args.guest_manifest, args.guest_sha256, guest=True)
    assert runner.sha(runner.CAPNP) == runner.CAPNP_SHA and runner.sha(runner.SCHEMA) == runner.SCHEMA_SHA
    files = [RUNNER, TEST, Path(__file__).resolve()]
    before = {p.relative_to(ROOT).as_posix(): runner.sha(p) for p in files}
    output = args.output.resolve(); output.mkdir(parents=True, exist_ok=False)
    command = [sys.executable, "-m", "unittest", "discover", "-s", "tool/tests", "-p", TEST.name, "-v"]
    with (output / "pure-tests.log").open("xb") as log:
        test = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, timeout=30)
    after = {p.relative_to(ROOT).as_posix(): runner.sha(p) for p in files}
    receipt = {"exit_code": test.returncode, "command": command, "inputs_before": before,
               "inputs_after": after, "source_unchanged": before == after,
               "log_sha256": runner.sha(output / "pure-tests.log"), "http_requests": 0, "host_launched": False}
    runner.dump(output / "pure-check.json", receipt)
    assert test.returncode == 0 and before == after, "pure checks failed; keep attempt"
    for rel, digest in before.items():
        dest = output / "source" / rel
        dest.parent.mkdir(parents=True, exist_ok=True); shutil.copyfile(ROOT / rel, dest)
        assert runner.sha(dest) == digest
    manifest = {"runner_sha256": before[RUNNER.relative_to(ROOT).as_posix()], "source_files": before,
                "host_manifest": str(args.host_manifest.resolve()), "host_manifest_sha256": args.host_sha256,
                "guest_manifest": str(args.guest_manifest.resolve()), "guest_manifest_sha256": args.guest_sha256,
                "pure_check_sha256": runner.sha(output / "pure-check.json"),
                "helper_sha256": runner.HELPER_SHA, "offline_decoder_sha256": runner.CAPNP_SHA,
                "schema_sha256": runner.SCHEMA_SHA, "http_cases_run": [], "sdk_frozen": False}
    runner.dump(output / "manifest.json", manifest)
    print(json.dumps({"manifest": str(output / "manifest.json"), "sha256": runner.sha(output / "manifest.json")}))


if __name__ == "__main__":
    main()
