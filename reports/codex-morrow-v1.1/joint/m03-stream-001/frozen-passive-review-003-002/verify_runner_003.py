"""Incremental readonly pins and pure extracted sealing tests; no producer import."""
from pathlib import Path
import ast
import hashlib
import json
import os
from unittest.mock import patch

W = Path(r"C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor")
B = W / "reports/codex-morrow-v1.1/host/m03-passive-fault-005"
R = B / "runner-candidate-003"
O = Path(__file__).resolve().parent

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def load(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))

assert sha(R / "manifest.json") == "d3ab74809a7bb56472e9999a8b5280e7962c94cb8df2d69d7e912dab0ff20e53"
m = load(R / "manifest.json")
for rel, digest in m["source_files"].items():
    assert sha(R / "source" / rel) == sha(W / rel) == digest
assert sha(R / "pure-check.json") == m["pure_check_sha256"]
p = load(R / "pure-check.json")
assert p["exit_code"] == 0 and p["inputs_before"] == p["inputs_after"] == m["source_files"] and p["source_unchanged"]
assert sha(R / "pure-tests.log") == p["log_sha256"]
assert "Ran 13 tests" in (R / "pure-tests.log").read_text() and not p["host_launched"] and p["http_requests"] == 0
for path_key, digest_key in (("host_manifest", "host_manifest_sha256"), ("guest_manifest", "guest_manifest_sha256")):
    assert sha(m[path_key]) == m[digest_key]
host = load(m["host_manifest"]); host_root = Path(m["host_manifest"]).parent
for rel, digest in host["source_files"].items():
    assert sha(W / rel) == sha(host_root / "source" / rel) == digest
guest = load(m["guest_manifest"])
P = Path(m["guest_manifest"]).parents[2]
for rel, digest in guest["input_sha256"].items():
    assert sha(P / rel) == digest

def functions(path):
    tree = ast.parse(path.read_text(encoding="utf-8"))
    return {n.name: n for n in tree.body if isinstance(n, ast.FunctionDef)}

path = R / "source/tool/m03_passive_fault_005.py"
before = functions(B / "runner-candidate-002/source/tool/m03_passive_fault_005.py")
after = functions(path)
assert before.keys() == after.keys()
changed = [name for name in before if ast.dump(before[name], include_attributes=False) != ast.dump(after[name], include_attributes=False)]
assert changed == ["seal_result"]
names = ("dump", "json_bytes", "checked_outcome", "qualification_exit_code", "seal_result")
env = {"Path": Path, "hashlib": hashlib, "json": json, "os": os, "sha": sha}
exec(compile(ast.Module(body=[after[name] for name in names], type_ignores=[]), str(path), "exec"), env)

cases = O / "synthetic-seal-cases-v2"
cases.mkdir()
observed = {}
case_names = ("success", "hash-error", "manifest-postwrite-error", "result-postwrite-error", "manifest-prerename-error", "manifest-postrename-error", "result-prerename-error", "result-postrename-error", "already-failed-hash-error")
for case in case_names:
    run = cases / case / "run"; guest_dir = run.parent / "guest"
    run.mkdir(parents=True); guest_dir.mkdir()
    (guest_dir / "synthetic.json").write_text('{"synthetic":true}\n', encoding="utf-8")
    result = {"qualification_result": "failed" if case.startswith("already-failed") else "expected_fault_observed", "synthetic_only": True, "first_reason": 19, "actual_error_retained": "partial frame Unknown"}
    original_dump = env["dump"]; original_sha = env["sha"]; original_rename = Path.rename
    if "hash-error" in case:
        def fail_hash(_):
            raise OSError("synthetic unreadable evidence")
        env["sha"] = fail_hash
    if "postwrite" in case:
        selected_dump = "evidence-manifest.pending.json" if case.startswith("manifest") else "result.pending.json"
        def dump_then_fail(target, value):
            original_dump(target, value)
            if target.name == selected_dump:
                raise OSError("synthetic write-close error after bytes exist")
        env["dump"] = dump_then_fail
    selected = "evidence-manifest.json" if case.startswith("manifest") else "result.json"
    def renamed(path_arg, target):
        if "prerename" in case and target.name == selected:
            raise OSError("synthetic rename refused")
        value = original_rename(path_arg, target)
        if "postrename" in case and target.name == selected:
            raise OSError("synthetic error after actual rename")
        return value
    with patch.object(Path, "rename", new=renamed):
        ok = env["seal_result"](run, guest_dir, result)
    env["dump"], env["sha"] = original_dump, original_sha
    published = load(run / "result.json")
    assert published["first_reason"] == 19 and published["actual_error_retained"] == "partial frame Unknown"
    summary = {"seal_return": ok, "result": published, "exit_code": env["qualification_exit_code"](result), "files": {p.name: sha(p) for p in sorted(run.iterdir()) if p.is_file()}}
    if case == "success":
        assert ok and published["qualification_result"] == "expected_fault_observed"
        manifest = load(run / "evidence-manifest.json")
        assert sha(run / "result.json") == manifest["files"]["result.json"]
        assert all(sha(Path(p)) == d for p, d in manifest["guest_files"].items())
        assert not any("pending" in p.name for p in run.iterdir()) and not (run / "seal-failure.json").exists()
    else:
        assert not ok and published["qualification_result"] != "expected_fault_observed" and summary["exit_code"] == 1
        marker = load(run / "seal-failure.json")
        assert marker["qualification_valid"] is False and marker["publication_failed"] is True
        summary["failure_marker"] = marker
        if case == "result-postwrite-error":
            assert load(run / "result.pending.json")["qualification_result"] == "expected_fault_observed"
        if case == "result-postrename-error":
            assert load(run / "result.rejected.json")["qualification_result"] == "expected_fault_observed"
            assert sha(run / "result.rejected.json") == load(run / "evidence-manifest.json")["files"]["result.json"]
            assert sha(run / "result.json") != load(run / "evidence-manifest.json")["files"]["result.json"]
        if case.startswith("already-failed"):
            assert published["qualification_result"] == "failed"
    observed[case] = summary

v = {"status": "runner003_incremental_limited_ready_for_authorized_runtime", "manifest_sha256": sha(R / "manifest.json"), "runner_sha256": sha(path), "source_files": m["source_files"], "only_changed_runner_function": changed, "host_current_and_snapshot_sources": 270, "guest_current_inputs": 78, "producer_pure_tests": 13, "independent_pure_cases": observed, "development_injection_attempt_preserved": "synthetic-seal-cases: local test closure selected the final filename instead of pending; repaired test in v2, not a producer defect", "http_requests": 0, "host_guest_launched": False, "sdk_frozen": False, "product_accepted": False}
(O / "result.json").write_text(json.dumps(v, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
(O / "runner-manifest.json").write_bytes((R / "manifest.json").read_bytes())
(O / "pure-check.json").write_bytes((R / "pure-check.json").read_bytes())
print(json.dumps({"status": v["status"], "pure_cases": len(observed), "http_requests": 0}))
