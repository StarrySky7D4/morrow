"""Independent read/hash and extracted pure AST checks. No producer import or runtime."""
from pathlib import Path
import ast
import copy
import hashlib
import json
import re
import subprocess

W = Path(r"C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor")
P = Path(r"C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex")
H = W / "reports/codex-morrow-v1.1/host/m03-fixture-003-host-candidate-001"
R = W / "reports/codex-morrow-v1.1/host/m03-passive-fault-005/runner-candidate-001"
O = Path(__file__).resolve().parent

def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()

def load(p):
    return json.loads(Path(p).read_text(encoding="utf-8"))

def save(name, value):
    target = O / name
    if target.exists():
        assert load(target) == value, f"preserve existing evidence: {target}"
        return
    target.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

host = load(H / "manifest.json")
guest = load(P / "receipts/m03-fixture-003/native-candidate-fixture-003.json")
build = load(H / "build-receipt.json")
assert build["sources_before"] == build["sources_after"] == host["source_files"]
assert build["source_unchanged"]
for c in build["commands"]:
    assert c["exit_code"] == 0
    assert sha(H / "logs" / (c["label"] + ".log")) == c["log_sha256"]
for rel, digest in host["evidence_tools"].items():
    assert sha(W / "tool" / rel) == digest
test_calls = 0
for label, receipt in host["test_receipts"].items():
    log = Path(receipt["log"])
    assert sha(log) == receipt["log_sha256"]
    counts = re.findall(r"test result: ok\. (\d+) passed; 0 failed", log.read_text(encoding="utf-8"))
    assert sum(map(int, counts)) == receipt["passed"], label
    test_calls += receipt["passed"]
assert test_calls == host["test_invocations_passed"] == 52
cli = load(H / "cli-boundary-receipt.json")
assert len(cli) == 2 and all(x["exit_code"] == 2 and x["expected_rejection_verified"] for x in cli)

receipts = {}
for key in ("core_tests", "native_tests", "build"):
    directory = Path(guest[key])
    receipt = load(directory / "result.json")
    assert receipt["exit_code"] == 0 and not receipt["host_runtime_launched"] and not receipt["actual_core_http_invoked"]
    assert len(receipt["inputs_before"]) == len(receipt["inputs_after"]) == 21
    assert receipt["inputs_before"] == receipt["inputs_after"] and not receipt["changed_inputs"]
    for name, digest in receipt["log_sha256"].items():
        assert sha(directory / name) == digest
    receipts[key] = receipt
assert receipts["native_tests"]["inputs_before"] == receipts["build"]["inputs_before"]
core_inputs = {k: v for k, v in receipts["core_tests"]["inputs_before"].items() if not k.startswith("native/")}
assert core_inputs == {k: v for k, v in receipts["build"]["inputs_before"].items() if not k.startswith("native/")}
for name, digest in receipts["build"]["inputs_before"].items():
    assert sha(P / name) == digest
test_counts = {}
for key, expected in (("core_tests", 15), ("native_tests", 35)):
    log = (Path(guest[key]) / "stdout.txt").read_text(encoding="utf-8")
    count = sum(map(int, re.findall(r"test result: ok\. (\d+) passed; 0 failed", log)))
    assert count == expected
    test_counts[key] = count
compiler = [json.loads(line) for line in (Path(guest["build"]) / "stdout.txt").read_text(encoding="utf-8").splitlines() if line.startswith("{")]
packages = {x["package_id"] for x in compiler if x.get("reason") == "compiler-artifact"}
assert len(packages) == guest["compiler_artifact_unique_package_ids"] == 907
assert compiler[-1] == {"reason": "build-finished", "success": True}
artifacts = [x for x in compiler if x.get("reason") == "compiler-artifact" and x.get("executable") and x["target"]["name"] == "morrow-codex-native-http-client"]
assert len(artifacts) == 1 and not artifacts[0]["fresh"]
assert sha(artifacts[0]["executable"]) == sha(guest["executable"]) == guest["executable_sha256"]
save("receipt-checks.json", {"host_source_before_after_entries": 270, "host_commands": len(build["commands"]), "host_test_invocations": test_calls, "host_cli_rejections": 2, "guest_receipt_inputs_each": 21, "guest_log_digests": 6, "guest_tests": test_counts, "guest_compiler_unique_packages": len(packages), "guest_native_test_matches_build": True, "guest_core_source_subset_matches_build": True, "guest_executable_matches_nonfresh_compiler_artifact": True, "http_requests": 0, "producer_runtime_launched": False})

parsed = {}
for line in (H / "logs/feature-qualification-tests.log").read_text(encoding="utf-8").splitlines():
    start = line.find("{")
    if start < 0:
        continue
    try:
        row = json.loads(line[start:])
    except ValueError:
        continue
    parsed[line[:start].split(" ... ")[-1].rstrip("=")] = row
local = parsed["qualification-real-pipe"]
assert local["owner_joined"] and local["peer_buffered_bytes"] == 12 and not local["peer_full_frame_decoded"]
assert local["original_complete_frames"] == local["issued_body_end"] == 0 and local["platform_io_error"] is None
assert [(x["kind"], x["bytes"], x["error"]) for x in local["reaps"]] == [("Write", 12, None), ("Read", 0, 995)]
peer = parsed["qualification-peer-error"]
assert peer["owner_joined"] and peer["original_complete_frames"] == 0 and "109" in peer["retained_error"]
assert [(x["kind"], x["bytes"], x["error"]) for x in peer["reaps"]] == [("Write", 12, None), ("Read", 0, 109)]
events = local["observations"]
frame = events[0]["detail"]["frame"]
raw = bytes.fromhex(frame["original_frame_raw_hex"])
assert len(raw) == frame["original_frame_bytes"] == 1348
assert int.from_bytes(raw[:4], "little") == frame["declared_payload_bytes"] == 1344
assert hashlib.sha256(raw).hexdigest() == frame["original_frame_sha256"]
assert hashlib.sha256(raw[:12]).hexdigest() == frame["prefix_sha256"]
capnp = Path(r"C:\Users\Administrator\capnp-bin\capnp.exe")
schema = H / "source/contracts/experimental/agent_host_v3_http_stream/native_http.capnp"
assert sha(capnp) == "c41ed4ec9c9d8ed6f3334fd9a8186d7196d890db2bd540bb15ed5cec8b49ef34"
assert sha(schema) == "8da8f1455200696d8884949f415aae4edd70cee6b583889bba7906c7730cc864"
conversion = subprocess.run([str(capnp), "convert", "binary:json", str(schema), "Frame"], input=raw[4:], capture_output=True, timeout=5)
assert conversion.returncode == 0, conversion.stderr
decoded = json.loads(conversion.stdout)
assert decoded["kind"] == "bodyChunk" and int(decoded["sequence"]) == 2 and int(decoded["revocationGeneration"]) == 1
identity = events[0]["detail"]["binding"]["identity"]
assert int(decoded["session"]) == identity["session"] and int(decoded["instanceEpoch"]) == identity["epoch"]
assert decoded["childPid"] == identity["child_pid"] and int(decoded["attempt"]) == identity["attempt"]
assert bytes(decoded["schemaSha256"]).hex() == sha(schema)
assert bytes(decoded["executionConfigSha256"]).hex() == identity["host_execution_config_sha256"]
assert hashlib.sha256(bytes(decoded["operationId"])).hexdigest() == identity["operation_id_sha256"]
assert int(decoded["payload"]["chunk"]["offset"]) == 0 and len(decoded["payload"]["chunk"]["bytes"]) == frame["body_end"] == 1024
save("local-pipe-frame-decoded.json", decoded)
save("local-pipe-evidence.json", {"scope": "existing same-process Windows real pipe tests, no HTTP or fixture guest", "real_prefix_close": local, "peer_error_close": peer, "offline_decoder_returncode": conversion.returncode, "raw_frame_sha256": hashlib.sha256(raw).hexdigest(), "prefix_sha256": hashlib.sha256(raw[:12]).hexdigest()})

runner_path = R / "source/tool/m03_passive_fault_005.py"
assert sha(runner_path) == "836ba0339b53d6a018cb7dbf146ed8d5995a166e6c0257d0601587fbe7c38d32"
tree = ast.parse(runner_path.read_text(encoding="utf-8"))
functions = {n.name: n for n in tree.body if isinstance(n, ast.FunctionDef)}
pure_names = ("hex64", "identity_digest", "pipe_fault_checks")
pure_module = ast.Module(body=[functions[name] for name in pure_names], type_ignores=[])
environment = {"hashlib": hashlib, "json": json}
exec(compile(pure_module, str(runner_path), "exec"), environment)
bound = events[0]["detail"]["binding"]
plan_keys = ("version", "scenario", "nonce", "fixture_spec_sha256", "target", "prefix_bytes", "close_trigger")
plan = {key: bound["plan"][key] for key in plan_keys}
witness = next(e["detail"]["witness"] for e in events if e["event"] == "qualification_pipe_witness_matched")
check_events = [{"event": "qualification_pipe_plan_bound", "detail": bound}] + events
assert environment["pipe_fault_checks"](check_events, witness, plan, identity)["physical_close_after_matched_witness"]
late = copy.deepcopy(check_events)
closed = next(e["detail"] for e in late if e["event"] == "qualification_pipe_closed")
deadline = closed["gate_sample"]["original_deadline_offset_ns"]
closed["before_ns"] = closed["after_ns"] = deadline + 1
accepted_late = environment["pipe_fault_checks"](late, witness, plan, identity)
assert accepted_late["physical_close_after_matched_witness"]
handlers = [h for n in functions["run"].body if isinstance(n, ast.Try) for h in n.handlers if isinstance(h.type, ast.Name) and h.type.id == "Exception"]
assert len(handlers) == 1
handler = handlers[0]
probe = {"result": {"qualification_result": "expected_fault_observed"}, handler.name: AssertionError("synthetic post-qualification pin drift")}
exec(compile(ast.Module(body=handler.body, type_ignores=[]), str(runner_path), "exec"), probe)
assert probe["result"]["qualification_result"] == "expected_fault_observed"
save("runner-blocker-reproductions.json", {"scope": "extracted frozen AST pure functions/exception body only; no run() import or call, no producer launch/network", "runner_sha256": sha(runner_path), "post_qualification_exception": {"source_lines": [handler.lineno, handler.end_lineno], "start_status": "expected_fault_observed", "injected_error": "synthetic post-qualification pin drift", "after_generic_handler": probe["result"], "defect_reproduced": True}, "late_physical_close": {"source_lines": [functions["pipe_fault_checks"].lineno, functions["pipe_fault_checks"].end_lineno], "base": "existing real local-pipe records plus plan-bound event", "mutation": "only closed before_ns/after_ns moved beyond original D", "gate_sample_ns": closed["gate_sample"]["sample_offset_ns"], "original_deadline_ns": deadline, "closed_before_after_ns": deadline + 1, "still_accepted": accepted_late["physical_close_after_matched_witness"], "defect_reproduced": True}, "http_requests": 0})
print(json.dumps({"receipt_checks": "passed", "local_pipe_raw_decode": "passed", "blockers_reproduced": 2, "http_requests": 0}))
