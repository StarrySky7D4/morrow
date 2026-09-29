"""Read saved probe evidence only; never import or execute its runner."""
from pathlib import Path
import hashlib
import json
from datetime import datetime, timezone

ROOT = Path(r"C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor")
OUT = Path(__file__).resolve().parent
BATCH = ROOT / "reports/codex-morrow-v1.1/host/m03-partial-write-003/windows-20260930-002"

def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()

def save(name, value):
    with (OUT / name).open("x", encoding="utf-8") as f:
        json.dump(value, f, indent=2, ensure_ascii=False)
        f.write("\n")

receipt_path = BATCH / "receipt.json"
log_path = BATCH / "windows-pipe-tests.log"
receipt_hash, log_hash = sha(receipt_path), sha(log_path)
receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
assert receipt["status"] == receipt["windows_runtime"] == "passed"
assert receipt["full_host_runtime"] == receipt["product_G0"] == "not_run"
assert receipt["source_unchanged"] is True
assert receipt["sources_before"] == receipt["sources_after"]
for relative, digest in receipt["sources_after"].items():
    assert sha(ROOT / relative) == digest, relative
assert all(c["exit_code"] == 0 and c["status"] == "passed" for c in receipt["commands"])
log = log_path.read_text(encoding="utf-8")
assert "15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out" in log

def extract(marker):
    assert log.count(marker) == 1, marker
    return json.JSONDecoder().raw_decode(log.split(marker, 1)[1])[0]

cases = {"deadline": extract("deadline_prefix="), "peer_disconnect": extract("disconnected_prefix=")}
chains = {}
for name, case in cases.items():
    assert case["peer_observed_bytes"] == 1024 and case["full_frame_bytes"] == 8192
    assert case["complete_frames"] == 0 and case["owner_joined"] is True
    events = case["observations"]
    assert len({e["clock_domain"] for e in events}) == 1
    assert all(e["clock"] == "std::time::Instant" and e["duration_unit"] == "ns"
               and e["duration_units_per_second"] == 1_000_000_000
               and e["resolution"] == "not_measured"
               and 0 <= e["before_ns"] <= e["after_ns"] for e in events)
    issues = [e for e in events if e["stage"] == "write_issue"]
    samples = [e for e in events if e["stage"] == "incomplete_sample"]
    reaps = [e for e in events if e["stage"] == "write_reaped"]
    frames = [e for e in events if e["stage"] == "write_frame_state"]
    assert len(issues) == len(samples) == len(reaps) == len(frames) == 1
    issue, sample, reap, frame = issues[0], samples[0], reaps[0], frames[0]
    op = issue["operation"]
    assert op["requested_bytes"] == op["body_end"] == 8192 and op["frame_offset"] == 0
    assert op["initial_pending"] is True and issue["outcome"]["win32_error"] == 997
    for e in (sample, reap, frame):
        assert e["operation"]["id"] == op["id"]
        assert e["operation"]["issue_ordinal"] == op["issue_ordinal"]
    assert sample["outcome"] == {"kind": "incomplete", "win32_error": 996}
    assert sample["operation"]["incomplete_samples"] == 1
    assert issue["after_ns"] <= sample["before_ns"] <= sample["after_ns"] <= reap["before_ns"]
    assert reap["outcome"]["id"] == op["id"] and reap["outcome"]["bytes"] == 0
    assert frame["outcome"]["confirmed_frame_prefix"] == 0
    if name == "deadline":
        assert case["error"] is None and case["events_errors"] == []
        assert reap["outcome"]["error"] == 995
        assert frame["outcome"] == {"cancelling": True, "confirmed_frame_prefix": 0, "result": "Ok(Cancelled)"}
        request = next(e for e in events if e["stage"] == "cancel_request_observed")
        probe = next(e for e in events if e["stage"] == "cancel_probe")
        cancel = next(e for e in events if e["stage"] == "cancel_requested")
        assert request["outcome"]["gate_or_deadline"] is True
        assert probe["outcome"] == {"kind": "incomplete", "win32_error": 996}
        assert probe["operation"]["id"] == op["id"]
        assert cancel["outcome"]["completion_claimed"] is False
        assert sample["after_ns"] <= request["before_ns"] <= probe["before_ns"]
        assert probe["after_ns"] <= cancel["before_ns"] <= cancel["after_ns"] <= reap["before_ns"]
    else:
        assert case["error"] == "write frame completion: Os(109)"
        assert case["events_errors"] == [case["error"]]
        assert reap["outcome"]["error"] == 109
        assert frame["outcome"] == {"cancelling": False, "confirmed_frame_prefix": 0,
                                     "result": 'Err("write frame completion: Os(109)")'}
        request = next(e for e in events if e["stage"] == "cancel_request_observed")
        assert request["outcome"]["gate_or_deadline"] is False
        assert reap["after_ns"] <= request["before_ns"]
        assert next(e for e in events if e["stage"] == "cancel_probe")["outcome"]["kind"] == "no_operation"
    chains[name] = case

# Source confirmation: the deadline injection shortens a local test gate.
source = (ROOT / "native_session_stream_001/src/pipe_observation_tests.rs").read_text(encoding="utf-8")
assert "PrefixStop::Deadline => gate.lock().unwrap().deadline = Instant::now()" in source
assert "drop(peer);" in source and "driver.join_if_finished()" in source
probe_source = (ROOT / "tool/qualification/m03-pipe-driver-check/src/lib.rs").read_text(encoding="utf-8")
assert 'native_session_stream_001/src/pipe_driver.rs' in probe_source
assert 'native_session_stream_001/src/write_state.rs' in probe_source
assert sha(receipt_path) == receipt_hash and sha(log_path) == log_hash
for relative, digest in receipt["sources_after"].items():
    assert sha(ROOT / relative) == digest, relative

save("probe-chains.json", chains)
save("verified-inputs.json", {"receipt": str(receipt_path), "receipt_sha256": receipt_hash,
                              "log": str(log_path), "log_sha256": log_hash,
                              "source_files": receipt["sources_after"]})
save("result.json", {"status": "passed_with_scope_limits", "reviewed_utc": datetime.now(timezone.utc).isoformat(),
                     "review_mode": "saved evidence and source reads only", "source_hashes_verified": len(receipt["sources_after"]),
                     "producer_probe_tests": 15, "new_cases_reviewed": list(cases), "post_count_by_reviewer": 0,
                     "runtime_by_reviewer": False, "original_authority_deadline_qualified": False,
                     "nonzero_short_success_completion_qualified": False, "full_host_fault_chain_qualified": False,
                     "product_G0": "not_run", "sdk_frozen": False})
print(json.dumps({"status": "passed_with_scope_limits", "verified_source_files": len(receipt["sources_after"]),
                  "receipt_sha256": receipt_hash, "log_sha256": log_hash}))
