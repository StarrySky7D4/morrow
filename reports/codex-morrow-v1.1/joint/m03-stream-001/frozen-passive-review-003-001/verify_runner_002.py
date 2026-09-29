"""Pure extracted AST review of frozen runner 002; no producer runtime import/call."""
from pathlib import Path
import ast
import copy
import hashlib
import json

O = Path(__file__).resolve().parent
W = Path(r"C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor")
R = W / "reports/codex-morrow-v1.1/host/m03-passive-fault-005/runner-candidate-002"

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def load(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))

assert sha(R / "manifest.json") == "60e2687cd895c3639dccc7d544f420c84e508b3ec0e7f590ba0f33fbaac985ed"
m = load(R / "manifest.json")
for rel, digest in m["source_files"].items():
    assert sha(R / "source" / rel) == sha(W / rel) == digest
assert sha(R / "pure-check.json") == m["pure_check_sha256"]
p = load(R / "pure-check.json")
assert p["exit_code"] == 0 and p["inputs_before"] == p["inputs_after"] == m["source_files"] and p["source_unchanged"]
assert sha(R / "pure-tests.log") == p["log_sha256"]
assert "Ran 11 tests" in (R / "pure-tests.log").read_text() and not p["host_launched"] and p["http_requests"] == 0
path = R / "source/tool/m03_passive_fault_005.py"
tree = ast.parse(path.read_text(encoding="utf-8"))
fns = {n.name: n for n in tree.body if isinstance(n, ast.FunctionDef)}
names = ("hex64", "identity_digest", "pipe_fault_checks", "dump", "json_bytes", "checked_outcome", "qualification_exit_code", "seal_result")
env = {"Path": Path, "hashlib": hashlib, "json": json, "sha": sha}
exec(compile(ast.Module(body=[fns[name] for name in names], type_ignores=[]), str(path), "exec"), env)
handlers = [h for n in fns["run"].body if isinstance(n, ast.Try) for h in n.handlers if isinstance(h.type, ast.Name) and h.type.id == "Exception"]
assert len(handlers) == 1
probe = {"result": {"qualification_result": "expected_fault_observed"}, handlers[0].name: AssertionError("late pin drift")}
exec(compile(ast.Module(body=handlers[0].body, type_ignores=[]), str(path), "exec"), probe)
assert probe["result"]["qualification_result"] == "failed"
for stage in ("host", "guest", "runner"):
    result = {"qualification_result": "failed"}
    def fail_pin():
        raise AssertionError(stage + " pin drift")
    try:
        env["checked_outcome"](result, "expected_fault_observed", [lambda: None, fail_pin])
    except AssertionError:
        pass
    else:
        raise AssertionError("late pin failure was not raised")
    assert env["qualification_exit_code"](result) == 1
local = load(O / "local-pipe-evidence.json")["real_prefix_close"]
events = local["observations"]
bound = events[0]["detail"]["binding"]
plan = {key: bound["plan"][key] for key in ("version", "scenario", "nonce", "fixture_spec_sha256", "target", "prefix_bytes", "close_trigger")}
witness = next(e["detail"]["witness"] for e in events if e["event"] == "qualification_pipe_witness_matched")
events = [{"event": "qualification_pipe_plan_bound", "detail": bound}] + events
on_time = env["pipe_fault_checks"](events, witness, plan, bound["identity"])
assert on_time["close_before_original_deadline_proven"]
late = copy.deepcopy(events)
c = next(e["detail"] for e in late if e["event"] == "qualification_pipe_closed")
c["before_ns"] = c["after_ns"] = c["gate_sample"]["original_deadline_offset_ns"] + 1
late_check = env["pipe_fault_checks"](late, witness, plan, bound["identity"])
assert not late_check["close_before_original_deadline_proven"]
desired_nodes = [n for n in ast.walk(fns["run"]) if isinstance(n, ast.Assign) and isinstance(n.value, ast.IfExp) and "close_in_time" in ast.unparse(n)]
assert len(desired_nodes) == 1
desired_env = {"first": {"detail": {"code": 19}}, "expired_first": False, "close_in_time": False}
exec(compile(ast.Module(body=desired_nodes, type_ignores=[]), str(path), "exec"), desired_env)
assert desired_env["desired"] == "unreached"

cases = O / "runner-002-pure-seal-cases"
cases.mkdir()
observations = {}
for case in ("success", "hash-failure", "result-write-close-failure"):
    base = cases / case
    run = base / "run"; guest = base / "guest"
    run.mkdir(parents=True); guest.mkdir()
    (guest / "synthetic.json").write_text('{"synthetic":true}\n', encoding="utf-8")
    result = {"qualification_result": "expected_fault_observed", "synthetic_only": True, "first_reason": 19}
    old_sha, old_dump = env["sha"], env["dump"]
    if case == "hash-failure":
        def failed_hash(_):
            raise OSError("synthetic unreadable evidence")
        env["sha"] = failed_hash
    elif case == "result-write-close-failure":
        def write_then_error(target, value):
            old_dump(target, value)
            if target.name == "result.json":
                raise OSError("synthetic close/flush failure after result bytes written")
        env["dump"] = write_then_error
    ok = env["seal_result"](run, guest, result)
    env["sha"], env["dump"] = old_sha, old_dump
    published = load(run / "result.json")
    observations[case] = {"seal_return": ok, "in_memory_result": result, "published_result": published, "exit_code": env["qualification_exit_code"](result), "manifest_exists": (run / "evidence-manifest.json").exists()}
    if case == "success":
        assert ok and published == result and sha(run / "result.json") == load(run / "evidence-manifest.json")["files"]["result.json"]
    else:
        assert not ok and result["qualification_result"] == "unconfirmed" and env["qualification_exit_code"](result) == 1
assert observations["hash-failure"]["published_result"]["qualification_result"] == "unconfirmed"
assert observations["result-write-close-failure"]["published_result"]["qualification_result"] == "expected_fault_observed"
v = {"scope": "pure extracted AST; only synthetic local files", "manifest_sha256": sha(R / "manifest.json"), "runner_sha256": sha(path), "producer_pure_tests": 11, "late_pin_failure_fixed": True, "generic_exception_failure_latched": True, "late_physical_close_target_unreached": True, "seal_cases": observations, "remaining_blocker": "write/close error after result bytes exist leaves a published expected result, despite memory unconfirmed and exit 1", "http_requests": 0, "host_guest_launched": False}
(O / "runner-002-review.json").write_text(json.dumps(v, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
(O / "runner-002-manifest.json").write_bytes((R / "manifest.json").read_bytes())
print(json.dumps({"repairs_verified": 2, "seal_write_error_result_still_expected": True, "http_requests": 0}))
