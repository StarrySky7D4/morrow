"""Freeze verified local H1/H2 candidates and sources; no HTTP or Git mutation."""
import argparse
import json
import re
import shutil
import subprocess
from pathlib import Path
from m03_host_fixture_003_check import ROOT, inputs, sha


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", required=True)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    check = (ROOT / args.check).resolve()
    out = (ROOT / args.out).resolve()
    check.relative_to(ROOT)
    out.relative_to(ROOT)
    receipt = json.loads((check / "receipt.json").read_text())
    assert receipt["source_unchanged"] and receipt["sources_after"] == inputs(), "current source drift"
    commands = receipt["commands"]
    expected = {"default-build", "feature-build", "default-pipe-tests", "default-write-state-tests",
                "default-revocation-tests", "feature-qualification-tests", "feature-ordinary-pipe-tests",
                "feature-write-state-tests", "feature-revocation-tests", "default-cli-tests", "feature-cli-tests"}
    assert {c["label"] for c in commands} == expected
    assert all(c["exit_code"] == 0 for c in commands)
    out.mkdir(parents=True, exist_ok=False)
    shutil.copy2(check / "receipt.json", out / "build-receipt.json")
    logs = out / "logs"
    logs.mkdir()
    tests = {}
    candidates = {}
    for item in commands:
        label = item["label"]
        log = check / (label + ".log")
        assert sha(log) == item["log_sha256"]
        shutil.copy2(log, logs / log.name)
        if "executable" in item:
            src = check / item["executable"]
            assert sha(src) == item["executable_sha256"]
            destination = out / ("feature" if item["features"] else "default")
            destination.mkdir()
            for binary in src.parent.glob("*.exe"):
                shutil.copy2(binary, destination / binary.name)
            executable = destination / src.name
            candidates[label] = {"executable": str(executable), "executable_sha256": sha(executable), "features": item["features"]}
        else:
            text = log.read_text(encoding="utf-8", errors="replace")
            counts = re.findall(r"test result: ok\. (\d+) passed; 0 failed", text)
            assert len(counts) == 1 and int(counts[0]) > 0, "empty or ambiguous tests"
            tests[label] = {"log": str(logs / log.name), "log_sha256": sha(log),
                            "passed": int(counts[0]), "features": item["features"]}
    snapshot = out / "source"
    for relative, digest in receipt["sources_after"].items():
        src = ROOT / relative
        assert sha(src) == digest
        dst = snapshot / relative
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
        assert sha(dst) == digest
    p = {"version": 1, "scenario": "pipe-partial-close", "nonce": "ab" * 32,
         "fixture_spec_sha256": "cd" * 32, "target": "first-response-body-chunk",
         "prefix_bytes": 12, "close_trigger": "matched-passive-partial-frame-witness"}
    plan = json.dumps(p, separators=(",", ":"))
    boundary = []
    for label, mode, expected_error in [("default-build", "serve", "unknown option"),
                                       ("feature-build", "init", "qualification pipe plan requires serve mode")]:
        command = [candidates[label]["executable"], mode, "--qualification-pipe-plan", plan]
        result = subprocess.run(command, cwd=ROOT, capture_output=True, timeout=10)
        stderr = result.stderr.decode("utf-8", errors="replace")
        assert result.returncode == 2 and expected_error in stderr
        boundary.append({"command": command, "exit_code": result.returncode,
                         "stdout": result.stdout.decode("utf-8", errors="replace"), "stderr": stderr,
                         "expected_rejection_verified": True, "features": candidates[label]["features"]})
    boundary_path = out / "cli-boundary-receipt.json"
    boundary_path.write_text(json.dumps(boundary, indent=2) + "\n", encoding="utf-8")
    assert receipt["sources_after"] == inputs(), "source drift during freeze"
    manifest = {"qualification_id": "m03-fixture-003-host-001", **candidates["feature-build"],
                "default_candidate": candidates["default-build"], "source_root": "source",
                "source_files": receipt["sources_after"], "build_receipt": str(out / "build-receipt.json"),
                "build_receipt_sha256": sha(out / "build-receipt.json"), "test_receipts": tests,
                "test_invocations_passed": sum(v["passed"] for v in tests.values()),
                "cli_boundary_receipt": str(boundary_path), "cli_boundary_receipt_sha256": sha(boundary_path),
                "evidence_tools": {name: sha(ROOT / "tool" / name) for name in ["m03_host_fixture_003_check.py", "m03_host_fixture_003_freeze.py"]},
                "source_unchanged": True, "http_requests_launched": 0,
                "scope": "H1/H2 local qualification seams; no guest/Core/HTTP lifecycle acceptance or full-product qualification"}
    path = out / "manifest.json"
    path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"manifest": str(path), "manifest_sha256": sha(path),
                      "executable_sha256": manifest["executable_sha256"],
                      "default_executable_sha256": manifest["default_candidate"]["executable_sha256"],
                      "build_receipt_sha256": manifest["build_receipt_sha256"],
                      "source_files": len(manifest["source_files"]), "passed": manifest["test_invocations_passed"]}))


if __name__ == "__main__":
    main()
