"""One pinned passive M03 fault case, with independent business/fault outcomes.

The server uses only its fresh loopback socket. No retries, credentials, Git
writes or implicit continuation after Unknown. Pure test imports bind no port.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import secrets
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
GUEST_ROOT = Path(r"C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex")
HELPER = ROOT / "tool/m03_revoke_barriers_011.py"
HELPER_SHA = "641dece216bda441067123b288ae62565deb79478cdb951d307bdb04c2dc536e"
CASES = ("authority-deadline", "network-abort", "pipe-partial-close")
TTL_MS = 10000
DELTA = "passive-prefix"
CAPNP = Path(r"C:\Users\Administrator\capnp-bin\capnp.exe")
CAPNP_SHA = "c41ed4ec9c9d8ed6f3334fd9a8186d7196d890db2bd540bb15ed5cec8b49ef34"
SCHEMA = ROOT / "contracts/experimental/agent_host_v3_http_stream/native_http.capnp"
SCHEMA_SHA = "8da8f1455200696d8884949f415aae4edd70cee6b583889bba7906c7730cc864"


class Unreached(Exception):
    pass


class Unconfirmed(Exception):
    pass


def require_release(condition, message):
    if not condition:
        raise Unconfirmed(message)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def dump(path, value):
    with path.open("xb") as stream:
        stream.write(json_bytes(value))


def json_bytes(value):
    return (json.dumps(value, indent=2) + "\n").encode()


def checked_outcome(result, desired, checks):
    for check in checks:
        check()
    result["qualification_result"] = desired


def qualification_exit_code(result):
    return 0 if result["qualification_result"] == "expected_fault_observed" else 1


def seal_result(run_dir, evidence, result):
    """Publish no expected result before all evidence hashes and manifest exist."""
    try:
        files = {p.relative_to(run_dir).as_posix(): sha(p) for p in sorted(run_dir.rglob("*")) if p.is_file()}
        guest_files = {str(p): sha(p) for p in sorted(evidence.rglob("*")) if p.is_file()}
        files["result.json"] = hashlib.sha256(json_bytes(result)).hexdigest()
        # A post-write/flush/close error must occur before expected bytes become
        # the public result. Keep pending bytes as forensic evidence on failure.
        pending_manifest = run_dir / "evidence-manifest.pending.json"
        pending_result = run_dir / "result.pending.json"
        dump(pending_manifest, {"files": files, "guest_files": guest_files})
        dump(pending_result, result)
        pending_manifest.rename(run_dir / "evidence-manifest.json")
        pending_result.rename(run_dir / "result.json")
        return True
    except Exception as failure:
        result["evidence_seal_error"] = repr(failure)
        if result["qualification_result"] != "failed":
            result["qualification_result"] = "unconfirmed"
        # A failure marker takes precedence over any apparent bundle. Publish
        # the failure without calling the same failed dump operation again.
        failure_record = run_dir / "seal-failure.json"
        with failure_record.open("xb") as stream:
            stream.write(json_bytes({"status": "unconfirmed", "qualification_valid": False,
                                     "error": repr(failure), "publication_failed": True}))
        published = run_dir / "result.json"
        if published.exists():
            with (run_dir / "result.rejected.json").open("xb") as stream:
                stream.write(published.read_bytes())
        replacement = run_dir / "result.failure.json"
        assert replacement.parent.resolve() == published.parent.resolve() == run_dir.resolve()
        with replacement.open("xb") as stream:
            stream.write(json_bytes(result))
            stream.flush(); os.fsync(stream.fileno())
        os.replace(replacement, published)
        return False


def spec_for(scenario, nonce):
    if scenario not in CASES or not hex64(nonce) or nonce == "0" * 64:
        raise ValueError("invalid passive spec identity")
    return {
        "version": 2, "mode": "passive-observe", "scenario": scenario, "nonce": nonce,
        "request_limit": 32768, "response_limit": 65536, "consumer_events": 8,
        "credit_limit": 16384, "pipe_buffer": 1024, "max_chunk": 1024,
        "deadline_policy": "retain-original", "events_file": "fixture-events.jsonl",
        "evidence_queue_capacity": 128, "evidence_record_limit": 128,
        "evidence_record_limit_bytes": 8192, "evidence_file_limit_bytes": 1048576,
        "pipe_prefix_bytes": 12 if scenario == "pipe-partial-close" else None,
    }


def hex64(value):
    return isinstance(value, str) and len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def identity_digest(identity):
    keys = ("session", "epoch", "child_pid", "attempt", "operation_id_sha256", "host_execution_config_sha256")
    assert set(identity) == set(keys), "identity fields"
    assert all(type(identity[k]) is int and identity[k] > 0 for k in keys[:4]), "identity integer bounds"
    assert all(hex64(identity[k]) for k in keys[4:]), "identity digest fields"
    raw = json.dumps({k: identity[k] for k in keys}, separators=(",", ":")).encode()
    return hashlib.sha256(raw).hexdigest()


class Markers:
    def __init__(self, path, spec, spec_hash, identity):
        self.path, self.spec, self.spec_hash, self.identity = path, spec, spec_hash, identity
        identity_digest(identity)
        self.rows, self.raw, self.lines = [], b"", []

    def read(self):
        if not self.path.exists():
            return self.rows
        raw = self.path.read_bytes()
        assert len(raw) <= self.spec["evidence_file_limit_bytes"], "evidence size exceeded"
        assert raw[:len(self.raw)] == self.raw, "raw evidence rewritten"
        complete = raw[:raw.rfind(b"\n") + 1]
        lines = complete.splitlines()
        assert len(lines) <= self.spec["evidence_record_limit"], "evidence count exceeded"
        assert all(len(line) <= self.spec["evidence_record_limit_bytes"] for line in lines)
        rows = [json.loads(line) for line in lines]
        assert rows[:len(self.rows)] == self.rows, "evidence rewritten"
        for i, row in enumerate(rows):
            assert row["identity"] == self.identity, "foreign marker identity"
            assert row["mode"] == "passive-observe" and row["scenario"] == self.spec["scenario"]
            assert row["nonce"] == self.spec["nonce"] and row["spec_sha256"] == self.spec_hash
            assert row["clock_domain"] == "guest-fixture-monotonic"
            assert type(row["elapsed_ns"]) is int and row["elapsed_ns"] >= 0
            assert type(row["ordinal"]) is int and row["ordinal"] > 0
            assert row["ordinal"] == i + 1, "missing/duplicate marker ordinal"
        self.rows, self.raw, self.lines = rows, raw, complete.splitlines(keepends=True)
        return rows

    def one(self, kind, where=lambda _: True):
        found = [row for row in self.read() if row["kind"] == kind and where(row)]
        assert len(found) <= 1, "duplicate marker " + kind
        return found[0] if found else None

    def line_digest(self, marker):
        self.read()
        index = self.rows.index(marker)
        return hashlib.sha256(self.lines[index]).hexdigest()

    def finish(self):
        self.read()
        assert not self.raw or self.raw.endswith(b"\n"), "unfinished evidence record"
        return self.rows


def verify_candidate(path, digest, guest=False):
    assert sha(path) == digest, "candidate manifest drift"
    manifest = json.loads(path.read_text())
    executable = Path(manifest["executable"])
    assert sha(executable) == manifest["executable_sha256"], "candidate executable drift"
    if guest:
        for rel, value in manifest["input_sha256"].items():
            assert sha(GUEST_ROOT / rel) == value, rel
    else:
        assert sha(path.parent / "build-receipt.json") == manifest["build_receipt_sha256"], "build receipt drift"
        for receipt in manifest.get("test_receipts", {}).values():
            assert sha(Path(receipt["log"])) == receipt["log_sha256"], "test receipt drift"
        if "default_candidate" in manifest:
            default = manifest["default_candidate"]
            assert sha(Path(default["executable"])) == default["executable_sha256"]
        for rel, value in manifest["source_files"].items():
            assert sha(path.parent / "source" / rel) == sha(ROOT / rel) == value, rel
    return executable, manifest


def transport_prefix():
    def sse(value):
        return ("data: " + json.dumps(value, separators=(",", ":")) + "\n\n").encode()
    value = sse({"type": "response.created", "response": {"id": "resp_passive"}})
    value += sse({"type": "response.output_text.delta", "delta": DELTA})
    assert len(value) < 1024
    return value


def pipe_plan(spec, digest):
    assert spec["scenario"] == "pipe-partial-close"
    return {"version": 1, "scenario": "pipe-partial-close", "nonce": spec["nonce"],
            "fixture_spec_sha256": digest, "target": "first-response-body-chunk",
            "prefix_bytes": 12, "close_trigger": "matched-passive-partial-frame-witness"}


def partial_witness(markers, issued):
    marker = markers.one("data_partial_frame_observed")
    if marker is None:
        return None
    d = marker["detail"]
    assert d["buffered_bytes"] == 12 and d["declared_payload_bytes"] + 4 == d["expected_frame_bytes"]
    assert d["expected_frame_bytes"] == issued["original_frame_bytes"] > 12
    assert d["prefix_sha256"] == issued["prefix_sha256"]
    assert all(type(d[k]) is int and d[k] > 0 for k in ("read_id", "read_issue_count"))
    fragments = d["read_fragments"]
    assert 1 <= len(fragments) <= 12
    fragment_bytes = []
    for i, fragment in enumerate(fragments):
        assert set(fragment) == {"io_index", "read_id", "transferred_bytes", "bytes_hex"}
        assert type(fragment["io_index"]) is int and fragment["io_index"] >= 0
        assert all(type(fragment[k]) is int and fragment[k] > 0 for k in ("read_id", "transferred_bytes"))
        assert i == 0 or fragment["io_index"] > fragments[i - 1]["io_index"]
        raw = bytes.fromhex(fragment["bytes_hex"])
        assert len(raw) == fragment["transferred_bytes"] > 0
        fragment_bytes.append(raw)
    prefix = b"".join(fragment_bytes)
    assert prefix == bytes.fromhex(d["prefix_hex"]) and len(prefix) == 12
    assert hashlib.sha256(prefix).hexdigest() == d["prefix_sha256"]
    assert int.from_bytes(prefix[:4], "little") == d["declared_payload_bytes"]
    assert fragments[-1]["io_index"] == d["io_index"] and fragments[-1]["read_id"] == d["read_id"]
    return {"identity_sha256": identity_digest(marker["identity"]), "nonce": marker["nonce"],
            "fixture_spec_sha256": marker["spec_sha256"], "marker_sha256": markers.line_digest(marker),
            "marker_ordinal": marker["ordinal"],
            **{k: d[k] for k in ("read_id", "read_issue_count", "buffered_bytes", "declared_payload_bytes", "expected_frame_bytes", "prefix_sha256")}}


def deadline_binding(events, identity):
    bound = [e for e in events if e["event"] == "authority_deadline_bound"]
    samples = [e for e in events if e["event"] == "challenge_deadline_sample"]
    assert len(bound) == len(samples) == 1, "original authority witness missing/duplicated"
    b, s = bound[0]["detail"], samples[0]["detail"]
    domain = f"host-authority-monotonic:{identity['session']}:{identity['epoch']}"
    assert all(e["clock_domain"] == domain and type(e["at_ns"]) is int and e["at_ns"] >= 0 for e in events)
    assert b["created_offset_ns"] == 0 and b["lifetime_ms"] == TTL_MS
    assert b["original_deadline_offset_ns"] == TTL_MS * 1_000_000
    assert b["parent_deadline_matches_gate"] and b["expiry_not_renewed"]
    assert s["original_deadline_offset_ns"] == b["original_deadline_offset_ns"]
    assert 0 <= s["sample_offset_ns"] < b["original_deadline_offset_ns"]
    assert s["remaining_ms"] == (b["original_deadline_offset_ns"] - s["sample_offset_ns"]) // 1_000_000
    return {"bound": bound[0], "challenge_sample": samples[0], "deadline_ns": b["original_deadline_offset_ns"], "clock_domain": domain}


def first_cancellation(events):
    cancelled = [e for e in events if e["event"] == "http_cancel_applied"]
    assert cancelled, "no original cancellation evidence"
    first = cancelled[0]
    assert first["detail"]["code"] != 0
    assert all(e["detail"]["code"] == first["detail"]["code"] for e in cancelled), "first cause overwritten"
    return first


def decode_wire(events, identity, artifact_sha, authority, env):
    decoded = []
    kinds = ("control_frame_sent", "control_frame_received", "data_frame_sent", "data_frame_received")
    for event in events:
        if event["event"] in kinds:
            raw = bytes.fromhex(event["detail"]["raw_hex"])
            direction = event["event"]
        elif event["event"] == "qualification_pipe_prefix_issued":
            raw = bytes.fromhex(event["detail"]["frame"]["original_frame_raw_hex"])
            direction = "original_target_frame_not_fully_sent"
        else:
            continue
        assert int.from_bytes(raw[:4], "little") == len(raw) - 4
        conversion = subprocess.run([str(CAPNP), "convert", "binary:json", str(SCHEMA), "Frame"],
                                    input=raw[4:], capture_output=True, timeout=2, env=env)
        assert conversion.returncode == 0, conversion.stderr.decode(errors="replace")
        frame = json.loads(conversion.stdout)
        assert int(frame["session"]) == identity["session"] and int(frame["instanceEpoch"]) == identity["epoch"]
        assert frame["childPid"] == identity["child_pid"] and int(frame["attempt"]) == identity["attempt"]
        assert bytes(frame["schemaSha256"]).hex() == SCHEMA_SHA
        assert bytes(frame["artifactSha256"]).hex() == artifact_sha
        assert bytes(frame["executionConfigSha256"]).hex() == identity["host_execution_config_sha256"]
        assert hashlib.sha256(bytes(frame["operationId"])).hexdigest() == identity["operation_id_sha256"]
        decoded.append({"ordinal": event["ordinal"], "host_observed_at_ns": event["at_ns"], "direction": direction, "frame": frame})
    challenge = [d for d in decoded if d["frame"]["kind"] == "challenge"]
    assert len(challenge) == 1 and challenge[0]["direction"] == "control_frame_sent"
    assert int(challenge[0]["frame"]["remainingMs"]) == authority["challenge_sample"]["detail"]["remaining_ms"]
    assert authority["challenge_sample"]["detail"]["sample_offset_ns"] <= challenge[0]["host_observed_at_ns"] < authority["deadline_ns"]
    nonce = challenge[0]["frame"]["nonce"]
    assert all(d["frame"]["nonce"] == nonce for d in decoded)
    assert not any(d["frame"]["kind"] == "bodyEof" for d in decoded)
    heads = [d for d in decoded if d["frame"]["kind"] == "responseHead"]
    assert len(heads) == 1 and heads[0]["frame"]["payload"]["head"]["status"] == 200
    return decoded


def effect_boundary_checks(events, guest, first):
    fence = first["detail"]["effect_fence"]
    assert fence["gate_closed"] and fence["parent_deadline_matches_gate"]
    writes = [e["detail"] for e in events if e["event"] == "data_write_issued"]
    assert all(w["ordinal"] <= fence["gate_closed_after_ordinal"] for w in writes), "new OS issue passed closed gate"
    after = [f for f in guest["frames"] if f["lane"] == "guest_control_write_admitted" and f["delivery_paused"]]
    assert not any(f["kind"] == "HttpCommit" or f["kind"] == "HttpCredit" and f["credit_window_bytes"] > 0 for f in after)
    return {"gate_fence": fence, "actual_write_issues": writes, "no_new_write_issue_past_fence": True,
            "no_new_commit_or_positive_credit_after_guest_pause": True}


def pipe_fault_checks(events, witness, plan, identity):
    wanted = ("qualification_pipe_plan_bound", "qualification_pipe_prefix_issued",
              "qualification_pipe_prefix_reaped", "qualification_pipe_witness_matched", "qualification_pipe_closed")
    selected = {}
    for kind in wanted:
        found = [e for e in events if e["event"] == kind]
        assert len(found) == 1, "missing or repeated " + kind
        selected[kind] = found[0]
    bound = selected[wanted[0]]["detail"]
    assert bound["identity"] == identity and bound["identity_sha256"] == identity_digest(identity)
    assert bound["plan"] == plan and bound["generation"] == 1 and not bound["deadline_renewed"]
    assert bound["plan_sha256"] == hashlib.sha256(json.dumps(plan, separators=(",", ":")).encode()).hexdigest()
    i, r, m, c = [selected[k]["detail"] for k in wanted[1:]]
    assert all(d["binding"] == bound and d["frame"] == i["frame"] for d in (r, m, c))
    assert all(d["clock_domain"] == i["clock_domain"] and 0 <= d["before_ns"] <= d["after_ns"] for d in (i, r, m, c))
    assert i["after_ns"] <= r["before_ns"] <= r["after_ns"] <= m["before_ns"] <= m["after_ns"] <= c["before_ns"]
    f = i["frame"]
    raw = bytes.fromhex(f["original_frame_raw_hex"])
    assert len(raw) == f["original_frame_bytes"] == f["declared_payload_bytes"] + 4 > 12
    assert int.from_bytes(raw[:4], "little") == f["declared_payload_bytes"]
    assert hashlib.sha256(raw).hexdigest() == f["original_frame_sha256"]
    assert hashlib.sha256(raw[:12]).hexdigest() == f["prefix_sha256"] == witness["prefix_sha256"]
    assert f["sequence"] == 2 and f["body_offset"] == 0 and f["body_end"] > 0
    assert f["prefix_bytes"] == 12 and not f["original_frame_complete"] and not f["raw_frame_fully_sent"] and not f["tail_reissued"]
    assert i["actual_requested_bytes"] == i["outcome"]["requested"] == 12
    assert not i["outcome"]["body_end_advanced"]
    assert i["operation_id"] == i["outcome"]["id"] == r["operation_id"] == r["outcome"]["id"]
    assert r["outcome"]["bytes"] == 12 and r["outcome"]["error"] is None and r["prefix_reaped"]
    assert m["witness"] == c["witness"] == witness
    assert m["close_source"] == c["close_source"] == "matched-passive-partial-frame-witness"
    sampled = m["gate_sample"]
    assert sampled == c["gate_sample"] and sampled["clock_domain"] == m["clock_domain"]
    assert not sampled["gate_revoked"] and not sampled["original_deadline_reached"] and sampled["gate_deadline_matches_original"]
    assert sampled["sample_offset_ns"] < sampled["original_deadline_offset_ns"]
    assert sampled["remaining_ns"] == sampled["original_deadline_offset_ns"] - sampled["sample_offset_ns"] > 0
    assert m["outcome"]["close_requested"] and not m["outcome"]["data_close_proven"]
    assert c["outcome"]["data_endpoint_closed"] and c["outcome"]["all_operations_reaped"] and c["outcome"]["matched_witness"]
    assert not any(e["event"] == "data_frame_sent" and e["detail"].get("body_end") is not None for e in events)
    return {"events": selected, "raw_frame_sha256_verified": True, "prefix_sha256_verified": True,
            "capnp_full_target_decode": "independent_review_pending", "physical_close_after_matched_witness": True,
            "close_before_original_deadline_proven": c["after_ns"] < sampled["original_deadline_offset_ns"],
            "close_time_upper_bound_ns": c["after_ns"], "owner_original_deadline_ns": sampled["original_deadline_offset_ns"]}


def consumed_witness(markers, progress):
    marker = markers.one("core_event_delivered", lambda r: r["detail"]["event"]["kind"] == "OutputTextDelta")
    head = markers.one("response_head_accepted")
    parsed = markers.one("parser_progress_observed")
    if not marker or not head or not parsed:
        return None
    event = marker["detail"]["event"]
    assert event["kind"] == "OutputTextDelta" and event["bytes"] == len(DELTA)
    assert event["sha256"] == hashlib.sha256(DELTA.encode()).hexdigest()
    consumption = parsed["detail"]
    assert head["detail"]["kind"] == "ResponseHead" and head["detail"]["status"] == 200
    assert head["detail"]["generation"] == 1
    assert type(head["detail"]["frames_index"]) is int and head["detail"]["frames_index"] >= 0
    if consumption["parser_yielded_bytes"] <= 0 or consumption["consumed_offset"] <= 0:
        return None
    assert consumption["drain_discarded_bytes"] == consumption["error_body_consumed_bytes"] == 0
    assert type(consumption["io_index"]) is int and type(consumption["body_frame_index"]) is int
    assert not progress["http_eof"] and not progress["response_material_stored"]
    assert not progress["revoke_applied"] and progress["error_code"] == 0
    # Parser consumption is observed locally. The ordinary sub-8KiB credit
    # threshold stays intact; zero host offsets here are not failure/fabricated credit.
    return {"delivered": marker, "head": head, "parser": parsed, "host_progress": progress}


def guest_reference_checks(markers, guest):
    """Check diagnostics against the original arrays, rather than their summaries."""
    frames, io = guest["frames"], guest["io"]
    head = markers.one("response_head_accepted")
    if head:
        d = head["detail"]
        f = frames[d["frames_index"]]
        assert f["lane"] == "host_control" and f["kind"] == "ResponseHead"
        assert (f["sequence"], f["generation"]) == (d["sequence"], d["generation"])
    parsed = markers.one("parser_progress_observed")
    if parsed:
        d = parsed["detail"]
        f, read = frames[d["body_frame_index"]], io[d["io_index"]]
        assert f["lane"] == "host_data" and f["kind"] == "BodyChunk"
        assert read["action"] == "os_completion" and read["kind"] == "Read"
        assert read["error"] is None and read["bytes"] > 0
        assert d["consumed_offset"] >= d["parser_yielded_bytes"] > 0
    partial = markers.one("data_partial_frame_observed")
    if partial:
        d = partial["detail"]
        end = d["io_index"]
        assert type(end) is int and 0 <= end < len(io)
        read = io[end]
        assert (read["action"], read["kind"], read["id"], read["error"]) == ("os_completion", "Read", d["read_id"], None)
        assert read["bytes"] == d["transferred_bytes"] > 0
        preceding = io[:end + 1]
        assert sum(o["action"] == "os_issue" and o["kind"] == "Read" for o in preceding) == d["read_issue_count"]
        total, chain = 0, []
        for o in reversed(preceding):
            if o["action"] == "os_completion" and o["kind"] == "Read":
                assert o["error"] is None and o["bytes"] > 0
                total += o["bytes"]; chain.append(o)
                if total >= 12:
                    break
        assert total == d["buffered_bytes"] == 12, "prefix must come from real accumulated completions"
        ordered = list(reversed(chain))
        assert len(ordered) == len(d["read_fragments"])
        for fragment, completion in zip(d["read_fragments"], ordered):
            assert io[fragment["io_index"]] == completion
            assert fragment["read_id"] == completion["id"] and fragment["transferred_bytes"] == completion["bytes"]
        assert not any(f["lane"] == "host_data" and f["kind"] == "BodyChunk" for f in frames)
        return {"partial_read_chain": ordered, "partial_io_index": end, "read_fragments": d["read_fragments"]}
    return {"response_head_reference": bool(head), "parser_reference": bool(parsed)}


def run(args):
    assert os.name == "nt", "actual Windows required"
    assert sha(HELPER) == HELPER_SHA
    host_exe, host_manifest = verify_candidate(args.host_manifest, args.host_manifest_sha256)
    guest_exe, guest_manifest = verify_candidate(args.guest_manifest, args.guest_manifest_sha256, guest=True)
    assert sha(Path(__file__)) == args.runner_sha256, "runner drift"
    assert sha(CAPNP) == CAPNP_SHA and sha(SCHEMA) == SCHEMA_SHA, "offline wire decoder/schema drift"
    if args.case == "pipe-partial-close":
        assert "qualification-pipe-fault" in host_manifest.get("features", []), "test seam must be feature-bound"
    loader = importlib.util.spec_from_file_location("frozen_m03_observation_helpers", HELPER)
    helper = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(helper)

    class PassiveHost(helper.Host):
        def command(self, action, **fields):
            if action != "qualification_close_data":
                return super().command(action, **fields)
            assert self.proc.poll() is None
            command = {"action": action, **fields}
            encoded = (json.dumps(command, separators=(",", ":")) + "\n").encode()
            assert len(encoded) <= 1024, "original operator line budget"
            before = time.monotonic_ns()
            self.proc.stdin.write(encoded); self.proc.stdin.flush()
            self.commands.append({"before_write_ns": before, "after_flush_ns": time.monotonic_ns(), "value": command})
            row = self.wait(lambda v: v.get("event") == "operator_result" and v.get("action") == action)
            assert row["value"]["ok"], row
            return row["value"]["result"], row
    run_dir = args.output.resolve()
    run_dir.mkdir(parents=True, exist_ok=False)
    evidence = GUEST_ROOT / "out/m03-passive-005" / helper.stamp()
    evidence.mkdir(parents=True, exist_ok=False)
    spec = spec_for(args.case, secrets.token_hex(32))
    dump(evidence / "fixture-spec.json", spec)
    spec_hash = sha(evidence / "fixture-spec.json")
    profile, cwd = run_dir / "profile", run_dir / "cwd"
    profile.mkdir(); cwd.mkdir()
    operation = "m03-passive-" + args.case + "-" + helper.stamp()
    dump(run_dir / "expectations.json", {
        "case": args.case, "one_post_only": True, "automatic_retry": False,
        "original_ttl_ms": TTL_MS, "server_declared_body_bytes": 8192,
        "server_sent_prefix_sha256": hashlib.sha256(transport_prefix()).hexdigest(),
        "http_eof": False, "response_material_stored": False, "intent": "Unknown",
        "qualification_scope": "fault observation and real resource release, not business success",
        "host_manifest_sha256": args.host_manifest_sha256,
        "guest_manifest_sha256": args.guest_manifest_sha256,
        "runner_sha256": sha(Path(__file__)), "helper_sha256": HELPER_SHA,
    })
    body_ready = threading.Event()
    server_abort = threading.Event()
    server_stop = threading.Event()
    server_events = []
    approved_body = None

    def event(kind, **data):
        server_events.append({"kind": kind, "at_ns": time.monotonic_ns(), **data})

    class Handler(helper.http.server.BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"
        def log_message(self, *_):
            pass
        def do_POST(self):
            connection_id = f"owned-socket:{id(self.connection)}"
            try:
                self.connection.settimeout(1)
                n = int(self.headers["Content-Length"])
                assert 0 <= n <= spec["request_limit"]
                body = self.rfile.read(n)
                event("post", connection_id=connection_id, body_hex=body.hex(), path=self.path)
                assert body == approved_body and self.path == "/v1/responses"
                assert not self.headers.get("Authorization") and not self.headers.get("Cookie")
                assert sum(e["kind"] == "post" for e in server_events) == 1
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Content-Length", "8192")
                self.end_headers()
                assert body_ready.wait(1), "body release not reached"
                prefix = transport_prefix()
                self.wfile.write(prefix); self.wfile.flush()
                event("prefix_sent", connection_id=connection_id, bytes=len(prefix), sha256=hashlib.sha256(prefix).hexdigest())
                # The socket remains open until the harness observes the fault
                # trigger/expiry; the server has no independent short timeout.
                while not server_stop.wait(.01) and not server_abort.is_set():
                    pass
                event("short_body_close", method="graceful_close_requested", tcp_fin_packet_observed=False,
                      connection_id=connection_id, abort=server_abort.is_set(), sent=len(prefix), declared=8192)
            except Exception as error:
                event("server_error", error=repr(error))
            finally:
                self.close_connection = True
                event("handler_returned", connection_id=connection_id)

    class PassiveServer(helper.OwnedServer):
        def process_request(self, request, address):
            event("socket_accepted", connection_id=f"owned-socket:{id(request)}", address=list(address))
            super().process_request(request, address)

        def shutdown_request(self, request):
            connection_id = f"owned-socket:{id(request)}"
            before = time.monotonic_ns()
            error = None
            try:
                request.shutdown(helper.socket.SHUT_WR)
            except OSError as failure:
                error = repr(failure)
            event("socket_shutdown_returned", connection_id=connection_id, before_ns=before,
                  after_ns=time.monotonic_ns(), direction="SHUT_WR", error=error)
            before = time.monotonic_ns()
            request.close()
            event("accepted_socket_closed", connection_id=connection_id, before_ns=before,
                  after_ns=time.monotonic_ns(), tcp_fin_packet_observed=False)

    server = PassiveServer(Handler)
    origin = f"http://127.0.0.1:{server.server_address[1]}"
    env = {k: os.environ[k] for k in ("SYSTEMROOT", "WINDIR", "COMSPEC", "LOCALAPPDATA") if k in os.environ}
    host = None
    result = {"qualification_result": "failed", "case": args.case, "automatic_retry": False,
              "business_success_claimed": False, "product_G0": "not_run", "sdk_frozen": False}
    try:
        init = subprocess.run([str(host_exe), "init", "--profile", str(profile), "--slot", "passive-slot"],
                              cwd=ROOT, env=env, capture_output=True, timeout=10)
        dump(run_dir / "init.json", {"exit_code": init.returncode, "stdout": init.stdout.decode(), "stderr": init.stderr.decode()})
        assert init.returncode == 0
        command = [str(host_exe), "serve", "--profile", str(profile), "--client", str(guest_exe),
                   "--sha256", guest_manifest["executable_sha256"], "--work-dir", str(cwd),
                   "--plugin-id", "morrow-codex", "--role", "synthetic-http-passive", "--operation", operation,
                   "--http-origin", origin, "--ttl-ms", str(TTL_MS), "--handshake-ms", "6000", "--close-ms", "1000"]
        for arg in ("--fixture-base", origin + "/v1", "--evidence-dir", str(evidence), "--max-chunk", "1024", "--fixture-spec-sha256", spec_hash):
            command += ["--client-arg", arg]
        if args.case == "pipe-partial-close":
            command += ["--qualification-pipe-plan", json.dumps(pipe_plan(spec, spec_hash), separators=(",", ":"))]
        dump(run_dir / "launch.json", {"command": command, "env": env})
        host = PassiveHost(command, env, run_dir)
        host.wait(lambda v: v.get("event") == "proposal", 10)
        grant = host.command("approve")[0]["grant_id"]
        identity = host.command("claim", grant_id=grant)[0]
        until = time.monotonic() + 6
        while True:
            inspected = host.command("inspect_http")[0]
            if inspected.get("proposal"):
                break
            assert time.monotonic() < until, "HTTP proposal not reached"
            time.sleep(.01)
        proposal = inspected["proposal"]
        approved_body, digest = helper.frozen.request_digest(identity, operation, proposal)
        assert proposal["absolute_target"] == origin + "/v1/responses" and proposal["method"] == "POST"
        assert proposal["response_limit"] == 65536 and len(approved_body) <= 32768
        assert json.loads(approved_body)["stream"] is True and not json.loads(approved_body).get("tools")
        snapshot = host.command("inspect", grant_id=grant)[0]
        marker_identity = {"session": identity["session"], "epoch": identity["epoch"], "child_pid": identity["pid"],
                           "attempt": 1, "operation_id_sha256": hashlib.sha256(operation.encode()).hexdigest(),
                           "host_execution_config_sha256": snapshot["grant"]["config_sha256"]}
        markers = Markers(evidence / spec["events_file"], spec, spec_hash, marker_identity)
        dump(run_dir / "approval.json", {"identity": identity, "operation": operation, "proposal": proposal, "recomputed_request_sha256": digest})
        assert not server_events
        host.command("approve_http", proposal_ref=proposal["proposal_ref"], expected_hash=digest, response_limit=65536)
        body_ready.set()
        until = time.monotonic() + 4
        witness = None
        while time.monotonic() < until:
            state = host.command("inspect_http")[0]
            if args.case == "pipe-partial-close":
                reaped = [e for e in host.observations() if e["event"] == "qualification_pipe_prefix_reaped"]
                assert len(reaped) <= 1
                witness = partial_witness(markers, reaped[0]["detail"]["frame"]) if reaped else None
            else:
                witness = consumed_witness(markers, state["progress"])
            if witness:
                break
            time.sleep(.01)
        if not witness:
            raise Unreached("actual passive fault precondition not reached within the original case")
        dump(run_dir / "fault-preconditions.json", witness)
        if args.case == "network-abort":
            accepted = [e for e in server_events if e["kind"] == "socket_accepted"]
            assert len(accepted) == 1
            event("abort_requested", connection_id=accepted[0]["connection_id"], witness_sha256=sha(run_dir / "fault-preconditions.json"))
            server_abort.set()
        elif args.case == "pipe-partial-close":
            submitted, receipt = host.command("qualification_close_data", grant_id=grant, witness=witness)
            dump(run_dir / "partial-witness-accepted.json", {"submitted_witness": witness, "acceptance": submitted, "operator_row": receipt})
        # Observe through the original host expiry plus bounded teardown. This
        # wait creates no new request, credit or authorization.
        final_row = host.wait(lambda v: v.get("event") == "final", 13)
        final = final_row["value"]
        host.proc.wait(timeout=1)
        events = final["snapshot"]["events"]
        progress = final["snapshot"]["http"]["progress"]
        result.update(final=final, final_received_ns=final_row["line_received_ns"], guest_result=json.loads((evidence / "result.json").read_text()), progress=progress)
        assert progress["intent"] == "Unknown" and not progress["http_eof"] and not progress["response_material_stored"]
        require_release(final["state"]["owner"]["phase"] == "Released" and not final["state"]["owner"]["owner_retained"], "owner release unconfirmed")
        require_release(final["snapshot"]["exit_observed"] and final["snapshot"]["stdout_eof"] and final["snapshot"]["stderr_eof"], "child exit/EOF unconfirmed")
        assert final["snapshot"]["exit_code"] == 2, "fault must not become guest business success"
        joins = [e for e in events if e["event"] == "data_worker_joined"]
        require_release(len(joins) == 1 and joins[0]["detail"] is True, "actual owner join missing")
        owner = final["state"]["owner"]
        assert (owner["pid"], owner["session"], owner["epoch"]) == (marker_identity["child_pid"], marker_identity["session"], marker_identity["epoch"])
        require_release(all(progress[k] for k in ("worker_joined", "connect_reaped", "read_reaped", "write_reaped", "data_closed")), "native worker/pipe reap unconfirmed")
        guest = result["guest_result"]
        require_release(guest["local_data_thread_joined"] and guest["control_threads_joined"] and guest["fixture_writer_joined"], "guest worker joins unconfirmed")
        assert not guest["frames_overflow"] and not guest["io_overflow"] and not final["snapshot"]["event_overflow"]
        assert guest["fixture_observation"]["evidence_complete"] and guest["fixture_observation"]["failure"] is None
        require_release(all(t["finished"] and t["joined"] and not t["handle_retained"] for t in guest["control_threads"]), "retained guest control owner")
        markers.finish()
        result["guest_reference_checks"] = guest_reference_checks(markers, guest)
        authority = deadline_binding(events, marker_identity)
        first = first_cancellation(events)
        result.update(authority_binding=authority, first_cancellation=first)
        guest_authority = guest["authority_observation"]
        assert guest_authority["initial_remaining_ms"] == authority["challenge_sample"]["detail"]["remaining_ms"]
        assert guest_authority["clock_domain"] == "guest-fixture-monotonic"
        assert not guest_authority["deadline_recomputed_after_binding"]
        assert guest_authority["original_deadline_offset_ns"] == guest_authority["first_read_offset_ns"] + guest_authority["initial_remaining_ms"] * 1_000_000
        assert markers.one("authority_bound")["detail"] == guest_authority
        decoded = decode_wire(events, marker_identity, guest_manifest["executable_sha256"], authority, env)
        dump(run_dir / "decoded-wire.json", decoded)
        result["effect_boundary_checks"] = effect_boundary_checks(events, guest, first)
        core_events = [json.loads(line)["event"] for line in (evidence / "core-events.jsonl").read_text().splitlines()]
        assert not any(e["kind"] == "actual_core_completed" for e in core_events)
        deltas = [e for e in core_events if e["kind"] == "actual_core_output_text_delta"]
        assert not guest["task"]["product_success_claimed"]
        chunks = [d["frame"]["payload"]["chunk"] for d in decoded if d["frame"]["kind"] == "bodyChunk"]
        consumed_prefix = b""
        for chunk in chunks:
            assert int(chunk["offset"]) == len(consumed_prefix)
            consumed_prefix += bytes(chunk["bytes"])
        assert consumed_prefix and transport_prefix().startswith(consumed_prefix)
        if args.case == "pipe-partial-close":
            assert len(chunks) == 1
            assert not deltas and guest["task"]["audit"]["parser_yielded_bytes"] == 0
        else:
            assert consumed_prefix == transport_prefix()
            assert len(deltas) == 1 and deltas[0]["bytes"] == len(DELTA) and deltas[0]["sha256"] == hashlib.sha256(DELTA.encode()).hexdigest()
        if args.case == "authority-deadline":
            target = [e for e in events if e["event"] == "session_close_reason" and e["detail"]["reason"] == 20]
            # A guest/local cancellation that persists reason19 first is a real
            # result, but cannot qualify host-natural-expiry as the first cause.
            if not target or first["detail"]["code"] != 20 or progress["error_code"] != 20:
                desired = "unreached"
            else:
                assert first["at_ns"] >= authority["deadline_ns"], "deadline cause before original expiry"
                assert host.proc.returncode == 2, "expiry cannot be clean host success"
                desired = "expected_fault_observed"
        elif args.case == "network-abort":
            assert any(e["event"] == "http_cancel_applied" and e["detail"]["code"] == 26 for e in events)
            assert progress["error_code"] == 26
            assert first["detail"]["code"] == 26 and first["at_ns"] < authority["deadline_ns"], "network error was not first before expiry"
            assert any(e["kind"] == "short_body_close" and e["abort"] for e in server_events)
            assert any(e["kind"] == "accepted_socket_closed" for e in server_events)
            connections = {e["connection_id"] for e in server_events if "connection_id" in e}
            assert len(connections) == 1, "different accepted socket closed"
            assert any(e["kind"] == "socket_shutdown_returned" and e["error"] is None and e["direction"] == "SHUT_WR" for e in server_events)
            desired = "expected_fault_observed"
        else:
            closed = [e for e in events if e["event"] == "qualification_pipe_closed"]
            assert len(closed) == 1
            worker = guest["data_worker_observation"]
            assert worker["finished"] and worker["joined"] and not worker["handle_retained"]
            assert worker["result"]["ok"] is False and worker["first_data_failure"] is not None
            assert worker["first_data_failure"]["buffered_bytes"] == 12
            assert worker["first_data_failure"]["prefix_sha256"] == witness["prefix_sha256"]
            assert not any(e["event"] == "data_frame_sent" and e["detail"].get("body_end") is not None for e in events)
            result["pipe_fault_checks"] = pipe_fault_checks(events, witness, pipe_plan(spec, spec_hash), marker_identity)
            result["pipe_fault_checks"]["capnp_full_target_decode"] = "verified_offline_with_pinned_schema_and_decoder"
            original_error = worker["first_data_failure"]["error"]
            expired_first = "timeout" in str(original_error).lower() or "deadline" in str(original_error).lower()
            close_in_time = result["pipe_fault_checks"]["close_before_original_deadline_proven"]
            desired = "unreached" if first["detail"]["code"] == 20 or expired_first or not close_in_time else "expected_fault_observed"
        def check_runner():
            assert sha(Path(__file__)) == args.runner_sha256, "runner changed during case"
        checked_outcome(result, desired, [lambda: verify_candidate(args.host_manifest, args.host_manifest_sha256),
            lambda: verify_candidate(args.guest_manifest, args.guest_manifest_sha256, guest=True), check_runner])
    except (Unreached, Unconfirmed) as error:
        result.update(qualification_result="unreached" if isinstance(error, Unreached) else "unconfirmed", error=str(error))
    except Exception as error:
        result.update(qualification_result="failed", error=repr(error))
    finally:
        expiry_rows = [] if host is None else [r for r in host.rows if r["value"].get("event") == "host_observation"
            and r["value"]["observation"]["event"] == "http_cancel_applied"
            and r["value"]["observation"]["detail"]["code"] == 20]
        event("server_cleanup_requested", observed_host_expiry_row=expiry_rows[0] if expiry_rows else None,
              observed_host_final_ns=result.get("final_received_ns"))
        server_stop.set()
        deadline = time.monotonic() + 2
        if host:
            if host.proc.poll() is None:
                # Only cleanup the existing owner; never replay the request.
                try:
                    host.command("stop")
                    host.proc.wait(timeout=max(.001, deadline - time.monotonic()))
                except Exception as error:
                    result["cleanup_error"] = repr(error)
            result["host_exit"] = host.proc.poll()
            if result["host_exit"] is not None:
                host.proc.stdin.close()
            result["host_reader_joined"] = helper.join_observed(host.reader, deadline)
            host.err.close()
            result["reader_errors"] = host.errors
            dump(run_dir / "host-rows.json", host.rows)
            dump(run_dir / "operator-commands.json", host.commands)
        result["server_threads"] = server.close_owned(deadline)
        result["post_count"] = sum(e["kind"] == "post" for e in server_events)
        if result["post_count"] != 1 or any(e["kind"] == "server_error" for e in server_events) or result.get("reader_errors"):
            result["qualification_result"] = "failed"
        elif not all(t["joined"] for t in result["server_threads"]) or not result.get("host_reader_joined") or result.get("cleanup_error"):
            if result["qualification_result"] != "failed":
                result["qualification_result"] = "unconfirmed"
        dump(run_dir / "server-events.json", server_events)
        seal_result(run_dir, evidence, result)
    print(json.dumps({"output": str(run_dir), "qualification_result": result["qualification_result"], "error": result.get("error")}))
    return qualification_exit_code(result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", choices=CASES, required=True)
    parser.add_argument("--host-manifest", type=Path, required=True)
    parser.add_argument("--host-manifest-sha256", required=True)
    parser.add_argument("--guest-manifest", type=Path, required=True)
    parser.add_argument("--guest-manifest-sha256", required=True)
    parser.add_argument("--runner-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    return run(parser.parse_args())


if __name__ == "__main__":
    sys.exit(main())
