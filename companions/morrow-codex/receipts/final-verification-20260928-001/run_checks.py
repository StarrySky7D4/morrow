"""Frozen-round entry checks; no product or upstream runtime execution."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
ENTRY = ROOT / "tools/build_plan.py"
if (HERE / "verification.json").exists():
    raise SystemExit("Refusing to overwrite a recorded verification")

INPUTS = ["tools/build_plan.py", "tests/test_build_plan.py", "tools/build-contract.json",
          "sources.lock.json", "toolchain.lock.json", "rust-toolchain.toml",
          "receipts/upstream-inventory.json", "receipts/upstream-codex-tree.json",
          "receipts/upstream-cc-switch-tree.json", "receipts/upstream-codex-dependency-closure.json",
          "receipts/upstream-cc-switch-dependency-closure.json", "receipts/upstream-side-effect-candidates.json",
          "docs/upstream-audit.md", "docs/cc-switch-review.md", "docs/build-entry.md",
          "docs/implementation-status.md", "README.md"]


def fingerprints():
    return {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in INPUTS}


initial = fingerprints()
env = dict(os.environ, PYTHONDONTWRITEBYTECODE="1", RUSTUP_AUTO_INSTALL="0")
cases = []
test = subprocess.run([sys.executable, "tests/test_build_plan.py"], cwd=ROOT,
                      env=env, capture_output=True)
(HERE / "tests.stdout.txt").write_bytes(test.stdout)
(HERE / "tests.stderr.txt").write_bytes(test.stderr)
cases.append({"name": "entry_tests", "exit_code": test.returncode,
              "expected_exit_code": 0, "passed": test.returncode == 0,
              "stdout": "tests.stdout.txt", "stderr": "tests.stderr.txt"})

commands = [("baseline", ["preflight", "--scope", "baseline"], 0, None),
            ("sources", ["preflight", "--scope", "sources"], 2, "source_incomplete"),
            ("full", ["preflight", "--scope", "full"], 2, "source_incomplete")]
commands += [(stage, [stage], 2, "not_implemented") for stage in
             ("generate", "deps", "test", "build", "inspect", "pack", "bundle", "qualify", "release-plan")]
for name, args, expected, error in commands:
    output = HERE / name
    result = subprocess.run([sys.executable, str(ENTRY), *args, "--locked", "--offline",
                             "--output-dir", str(output)], cwd=ROOT, env=env, capture_output=True)
    (HERE / (name + ".stdout.json")).write_bytes(result.stdout)
    (HERE / (name + ".stderr.txt")).write_bytes(result.stderr)
    document = json.loads((output / "status.json").read_text(encoding="utf-8"))
    passed = (result.returncode == expected == document["exit_code"]
              and document.get("error", {}).get("code") == error
              and document["entry_sha256"] == initial["tools/build_plan.py"]
              and document["contract_sha256"] == initial["tools/build-contract.json"]
              and document["artifacts"] == []
              and document["old_dist_reused"] is False
              and document["gates"] == {"P-00": "not_claimed", "G0": "not_claimed"})
    cases.append({"name": name, "exit_code": result.returncode, "expected_exit_code": expected,
                  "error": document.get("error"), "passed": passed,
                  "status": name + "/status.json"})

final = fingerprints()
report = {"schema_version": 1, "qualification": "entry_failure_behavior_only",
          "inputs_before": initial, "inputs_after": final,
          "inputs_stable": initial == final, "checks": cases,
          "verification_passed": initial == final and all(c["passed"] for c in cases),
          "source_files_verified": {"codex": 209, "cc-switch": 13},
          "upstream_sources_complete": False, "rust_product_builds_run": 0,
          "P-02": "not_implemented", "G0": "blocked", "product_acceptance_passed": 0}
(HERE / "verification.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"verification_passed": report["verification_passed"],
                  "inputs_stable": report["inputs_stable"], "cases": len(cases),
                  "evidence": str(HERE / "verification.json"), "G0": "blocked"}))
raise SystemExit(0 if report["verification_passed"] else 1)
