# Fixture 001 harness contract (no run authorization)

Design pins: plugin `c69823c6e9f210dcf8b9de3bc4207a0ce7feba1c45ed869bacc11244ffd5df31`; host `39bf723fd17cf96d836dde7aaf432d18fbe7d4390c59aebfba58497fe0526db6`. Sources are new qualification/m03-fixture-001 (0.1.2) and native/m03-fixture-001 (0.4.0). Old candidates and batches remain frozen.

## Binding

Host supplies `--morrow-native-http-v3`. Client args are exactly the existing six `--fixture-base URL --evidence-dir NEW_BATCH_DIRECTORY --max-chunk 1024` followed by `--fixture-spec-sha256 LOWERCASE_SHA256`, keeping the eight-argument limit. `fixture-spec.json` is a bounded 4096-byte regular, non-reparse file inside that evidence directory. SHA binds its exact bytes, not reserialized JSON. Every field in fixture-spec.example.json is required and unknown fields are rejected. Only `mode` (core-revoke/data-pending) and a fresh lowercase 64-hex `nonce` vary; all other values, limits and filenames are fixed. No additional permission or HTTP grant comes from this spec/nonce.

`fixture-events.jsonl`, existing `core-events.jsonl` and `result.json` are create-new. Evidence directory validation remains under the fixed plugin out root with no reparse ancestors. The 128-entry evidence queue uses nonblocking try_send outside native/Operation locks; ordinal assignment and queue ordering share only the short fixture lock. Owned evidence thread performs disk writes/flush and release reads. Overflow/write errors latch fixture failure, retain incomplete counts, and do not prevent cancellation cleanup. No raw event text or nonce from the native admission is dumped.

## Marker records and release

Each JSONL record contains `identity{session,epoch,child_pid,attempt,operation_id_sha256,host_execution_config_sha256}`, `mode`, `nonce`, `spec_sha256`, increasing `ordinal`, `elapsed_ns`, `clock_domain="guest-fixture-monotonic"`, `kind`, and `detail`. Clock scope is this process's fixture instance, identified by nonce+identity; times are Instant durations, not a cross-process clock or hardware frequency.

Core marker kinds are `core_event_queued`, `core_event_held`, `core_event_reserved`, `core_event_suppressed`, `core_reserved_suppressed`; each carries `detail.event{stream_ordinal,delta_ordinal,bytes,sha256,kind:"OutputTextDelta"}`. A is delta_ordinal1; B is delta_ordinal2, regardless of prior non-delta events. Runtime IDs are formed only from actual ModelClient stream values. Suppression adds `gate_closed`; B also has `permit_released=true`, recorded after the original rejected closure and captured permit were dropped. This is not synthetic runtime event injection. Existing local tests remain explicitly synthetic.

`host_revoke_received.detail` has kind/sequence/generation/code of a validated revocation-bearing native control frame. `gate_closed.detail.read_issue_count` follows the synchronous shared Operation cancellation application. These do not claim source1 or host persistence themselves: independently correlate host raw applied/persisted source1/reason19 evidence. A is held post-recv before the old final gate; B is reserved before the old producer gate. No barrier holds a cancellation/native lock. A waits only until the original deadline; evidence failure wakes it. B wakes on actual cancellation. Cleanup and control remain independent.

After A-held, B-reserved, host applied/persisted revoke and guest gate_closed, atomically rename a complete single release into `release-consumer.json` (≤2048 bytes):

```json
{"version":1,"mode":"core-revoke","nonce":"SPEC_NONCE","spec_sha256":"SPEC_SHA","identity":{"session":0,"epoch":0,"child_pid":0,"attempt":0,"operation_id_sha256":"MATCH","host_execution_config_sha256":"MATCH"},"stage":"release-consumer","event":{"stream_ordinal":0,"delta_ordinal":1,"bytes":0,"sha256":"MATCH_A","kind":"OutputTextDelta"}}
```

All identity and event fields must exactly match the bound marker. Unknown/early/stale/mismatched/replayed releases fail; release never grants IO or resets a deadline. No release is needed or supported for data-pending mode. A existing output is suppressed only by the original cancellation gate; earlier delivered output is not retracted. Invalid release/evidence cannot qualify a scenario even if its fallback cancellation prevents delivery.

## Data pause and terminal receipt

`data_read_paused.detail` contains `last_read_id`, `read_issue_count`, `framer_bytes=0`, `framer_expected=4`, `read_operation_present=false`. It is emitted by the sole data owner after a complete DataBound and a polled/reaped read, before issuing another read. The hold never resumes; cancellation enters normal cancel/reap. Request writes/polls continue. Final read_issue_count must equal this marker; correlate individual actual IO records. The marker alone does not prove host OS pending. Host005 must independently record same write ID/OVERLAPPED/ordinal/offset/length, three IO_INCOMPLETE samples ≥25ms apart and a fourth poll at cancellation still incomplete. Otherwise target was not reached. Original 16KiB credit/1024 pipe buffer/body chunk≤8KiB/wire payload≤32KiB+4 prefix stay unchanged.

Result adds `fixture_observation{mode,spec_sha256,nonce,failure,enqueued_records,written_records,evidence_complete,stages_complete,gate_closed,read_issue_count,writer_joined,runtime_qualified:false}`, `fixture_writer_joined`, `control_threads_joined`, and `control_threads[{name,joined,handle_retained,finished}]`. Three names: control_reader/control_router/control_writer. Existing aggregate Close/final errors, actual Core terminal, IO counters and independent sticky control failure remain authoritative in their separate dimensions. `stages_complete` is local stage evidence only, not host pending/revocation/HTTP acceptance. No Unknown whitelist.

Main shares one unchanged 500ms absolute Close deadline among wait_close, terminal-router observation, completed-control-thread joins and the evidence-writer stop/join; it does not renew authority TTL. Only is_finished handles are joined, not live threads on the async control path. Handles remain retained/unconfirmed on timeout. Marker writer is stopped only after Core cleanup/terminal control observation; evidence failure or incomplete threads prevents success. Guest cannot observe its own process exit/release. Host/harness must separately join their server/handler/control threads, observe child exits/dual EOF, and verify durable Released. No host/guest pairing or HTTP has been authorized by these files.
