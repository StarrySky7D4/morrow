"""Bind this limited, actually executed qualification slice to exact artifacts."""
from pathlib import Path
from datetime import datetime, timezone
import hashlib
import json

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).with_name("handoff.json")
FROZEN_SHA = "995cfa721bae3b94a494a0ee88e9e76f9e29afeec71ee6481c9ac756446d3ca4"
RUN_ROOT = "receipts/p02-native-probe-001/runs/"
BUILD = RUN_ROOT + "build-20260928T121551Z-457f497c21/result.json"
RUN = RUN_ROOT + "run-20260928T121810Z-d60bc57149/result.json"
LOCK = RUN_ROOT + "lock-20260928T121520Z-4abd282d33/result.json"
RUNTIME = "receipts/p02-native-probe-001/runtime-20260928T121810Z-d60bc57149.json"


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def read(relative):
    return json.loads((ROOT / relative).read_text(encoding="utf-8"))


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def main():
    require(not OUT.exists(), "Handoff already exists; do not overwrite frozen delivery")
    frozen = read("receipts/handoff.json")
    require(sha(ROOT / "receipts/handoff.json") == FROZEN_SHA, "Original handoff changed")
    require(len(frozen["input_sha256"]) == 17, "Unexpected original frozen input count")
    for name, expected in frozen["input_sha256"].items():
        require(sha(ROOT / name) == expected, "Original frozen input changed: " + name)
    inventory = read("receipts/p02-source-batch-001/source-inventory.json")
    require(inventory["status"] == "both_complete_fixed_source_trees_verified", "Source batch incomplete")
    for source in inventory["sources"]:
        require(source["complete_repository_source"], "Incomplete source entry")
        for key in ("files_manifest", "tree_manifest", "commit_api", "verification_receipt"):
            reference = source[key]
            require(sha(ROOT / reference["path"]) == reference["sha256"], "Source reference changed")
    build, run, runtime, lock = read(BUILD), read(RUN), read(RUNTIME), read(LOCK)
    require(build["status"] == "compiled_not_runtime_proof" and build["exit_code"] == 0, "Build did not pass")
    require(run["status"] == "passed_limited_net_callsite_probe" and run["exit_code"] == 0, "Run did not pass")
    require(lock["status"] == "probe_lock_prepared_not_build_proof", "Lock not prepared")
    require(runtime["status"] == "passed_limited_net_callsite_probe", "Runtime receipt failed")
    require(runtime["P-02"] == "not_complete" and runtime["G0"] == "not_claimed", "Runtime scope inflated")
    require(len(runtime["cases"]) == 2, "Unexpected case count")
    checks = [entry for case in runtime["cases"] for entry in case["assertions"]]
    require(len(checks) == 15 and all(passed is True for _, passed in checks), "Runtime assertions failed")
    require(sha(ROOT / RUNTIME) == run["runtime_sha256"], "Runtime receipt changed")
    for receipt in (build, run):
        require(receipt["changed_inputs"] == [], "Build/run input changed")
        require(receipt["frozen_before"]["matches_frozen_handoff"] and receipt["frozen_after"]["matches_frozen_handoff"], "Old input check failed")
        for name, expected in receipt["inputs_after"].items():
            require(expected is not None and sha(ROOT / name) == expected, "Current build input changed: " + name)
    artifact = build["artifacts"][0]
    executable = Path(artifact["path"])
    executable.relative_to(ROOT)
    require(sha(executable) == artifact["sha256"], "Compiler artifact changed")
    require(run["expected_cargo_bin_artifact"] == artifact, "Build/run executable identity differs")
    source_recheck = read("receipts/p02-source-batch-001/codex-source-post-build-001.json")
    require(source_recheck["status"] == "complete_fixed_source_reverified", "Post-build source check failed")
    require(all(not value for value in source_recheck["dirty_diff"].values()), "Source changed during build/run")

    names = [
        "receipts/p02-native-probe-001/make_handoff.py",
        "receipts/p02-native-probe-001/delivery.md",
        "receipts/p02-native-probe-001/run_probe.py",
        "qualification/p02-native-probe-001/Cargo.toml",
        "qualification/p02-native-probe-001/Cargo.lock",
        "qualification/p02-native-probe-001/src/main.rs",
        "qualification/p02-native-probe-001/README.md",
        "out/p02-native-probe-001/cargo-home/config.toml",
        "receipts/p02-source-batch-001/source-inventory.json",
        "receipts/p02-source-batch-001/codex-source-verification.json",
        "receipts/p02-source-batch-001/codex-source-post-build-001.json",
        "receipts/p02-source-batch-001/cc-switch-source-verification.json",
        "receipts/p02-source-batch-001/cc-switch-source-recheck-001.json",
        "receipts/p02-dependency-cache-001/result.json",
        "receipts/p02-dependency-cache-001/download-result.json",
        "receipts/p02-dependency-cache-001/vendor-result.json",
        "receipts/p02-dependency-cache-001/build-forks-001/result.json",
        "receipts/p02-dependency-cache-001/build-forks-001/tokio-tungstenite-manifest.patch",
        "receipts/p02-dependencies-001/tokio-tungstenite-verification.json",
        "receipts/p02-dependencies-001/tungstenite-rs-verification.json",
        "receipts/p02-dependencies-001/runfiles-package-verification.json",
        "receipts/p02-probe-review-001/decision.md",
        "receipts/p02-probe-review-001/resolved-graph-review.json",
        "receipts/p02-probe-review-001/resolved-graph-review.md",
        "receipts/p02-probe-review-001/runtime-review-001.json",
        BUILD, RUN, LOCK, RUNTIME,
    ]
    for stage in (Path(BUILD).parent, Path(RUN).parent, Path(LOCK).parent):
        names.extend((stage / name).as_posix() for name in ("stdout.txt", "stderr.txt"))
    hashes = {name: sha(ROOT / name) for name in names}
    result = {
        "schema_version": 1,
        "batch": "p02-native-probe-001",
        "ready_for_review": True,
        "scope": "Complete pinned source snapshots and an actually compiled/executed Windows Responses endpoint refusal probe only",
        "status": "passed_limited_net_callsite_probe",
        "source_status": "both_complete_fixed_source_trees_verified",
        "cases_passed": 2,
        "assertions_passed": 15,
        "real_callsite": runtime["real_entry"],
        "artifact": {"path": executable.relative_to(ROOT).as_posix(), "sha256": artifact["sha256"]},
        "runtime_receipt": RUNTIME,
        "runtime_sha256": run["runtime_sha256"],
        "independent_lock_sha256": sha(ROOT / "qualification/p02-native-probe-001/Cargo.lock"),
        "input_sha256": hashes,
        "original_frozen_handoff_sha256": FROZEN_SHA,
        "original_frozen_inputs_unchanged": 17,
        "P-02": "incomplete_net_callsite_only_exec_store_not_implemented",
        "G0": "blocked",
        "product_graphs_built": 0,
        "product_graphs_required": 2,
        "product_acceptance_cases": {"passed": 0, "not_run": 84},
        "source_download_block_resolved": True,
        "remaining": ["core ModelClient and full network injection/no-bypass proof", "actual core execution and storage probes", "successful HTTP/SSE/real host IPC qualification", "product native and Wasm graphs and packaging", "generic host HTTP/store/process contract capabilities"],
        "limits": runtime["limits"],
        "historical_attempts_preserved": True,
        "committed_pushed_or_released": False,
        "created_utc": datetime.now(timezone.utc).isoformat(),
    }
    with OUT.open("x", encoding="utf-8") as stream:
        json.dump(result, stream, indent=2, ensure_ascii=False)
        stream.write("\n")
    print(json.dumps({"ready_for_review": True, "handoff": str(OUT), "sha256": sha(OUT), "bound_files": len(hashes), "cases": 2, "assertions": 15, "P-02": result["P-02"], "G0": "blocked"}, indent=2))


if __name__ == "__main__":
    main()
