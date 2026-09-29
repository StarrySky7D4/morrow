# agent-host-v1 experimental G0 slice

This directory is the sole authority for the new host schema. `agent_host.capnp`
is not a replacement for any old Wasm/UI/IO contract. Copies in an exported kit
are generated distribution inputs, not a second authority. Raw SHA-256 of this
file (without newline normalization), major **1**, revision **1** must match on
every frame. Unknown versions, schema digests and union tags are rejected. Any
wire/semantic change requires a new experimental revision and regenerated kit;
no field/tag is reused. There is no stable SDK freeze in G0.

| Family | This slice | Still required |
|---|---|---|
| native-session-v1 | Hello, fixture binding, Drain, structured ObserveExit unavailable | Real Attach/Capabilities negotiation, OS connection/launch identity and observed exit (M-02) |
| stream-io-v1 | Open/WriteChunk/CommitRequest/Read/Inspect/Cancel/Close, bounded in-memory fake | Actual network, authorization, independent control channel and budgets (M-03/M-08) |
| session-events-v1 | OpenWriter/AppendBatch/ReadAfter, writer epoch, tail CAS, original receipt fake | Durable Protobuf+LZ4 store, Snapshot/SealCheckpoint, gap/outbox/retention (M-04) |
| tool-dispatch-v1 | Propose/Claim/Report/Inspect, trusted fixture approval, single effective claim | Actual policy UI, permit signing/revocation, execution domain and outbox (M-06) |
| agent-content-v1 | Draft only: Query/ReadRef/ProposeMutation/InspectOperation | Permission intersection, exact object CAS, separate external-send approval (M-05) |
| stream-view-v1 | Draft only: Subscribe/Snapshot/DeltaBatch/Terminal/Detach | Durable subscription/projection/UI lifecycle (M-07) |
| native-package-v1 + bundle-v1 | Draft only: Inspect/PlanInstall/Stage/CommitSelection | Member/platform validation, install selection, independent approval (M-09) |

Draft families have **no wire tags, capability bits or callable stubs** in this
revision. Do not claim seven completed contracts. Only Rust bindings are generated
and compiled here; Dart/C bindings and platform UI are deferred. UInt64 wire values
are exact. A JSON adapter must accept a JSON string before `parse_json_u64`, and
emit `json_u64` as a JSON string; JSON numbers are not an acceptable input.

`Frame` is one unpacked Cap'n Proto message, maximum 65536 bytes, with positive
request ID, bounded ASCII IDs and nonzero instance epoch. This is a byte exchange
contract; socket framing and authenticated transport remain M-02. A caller must
match requestId/sessionId/instanceEpoch and response direction. `exchange_checked`
does that for the supplied transport. Deadlines use explicit clock domain and
clock ID, with milliseconds as the unit; the fake accepts only its fixture
monotonic clock. No serialized deadline is trusted across unrelated clocks.

Read carries `expectedOffset` and `maxBytes`. The last delivered chunk is
replayable with the same pair without advancing the cursor; a different stale
offset conflicts. Inspect reports the current cursor. This is a bounded local
delivery retry, not remote response resume. Drain/cancel/expiry blocks delivery
of a cached chunk. Append/propose/report retries must retain the complete original
frame bytes (including request ID); changing bytes under an existing key conflicts.

All fake replies have `qualificationOnly=true`. Simulated `durableSequence` is
not disk persistence. Fixture source payloads and responses are deliberately
opaque. EOF is a transport fact, never agent/model completion. No operation
executes a process, opens a network endpoint, logs in, accesses secrets or opens a
production store. `qualification` is opt-in and absent in default builds. The
fake's trusted constructor/approval method must never become a production API.

Tool proposals carry no authority. Fixture approval binds the original serialized
intent and current connection epoch; only the first valid Claim says `execute`.
A repeated Claim returns the original state with `execute=false`; unknown reports
remain unknown. Reported exit/output facts are plugin claims, not host-verified
process observations. Drain revokes new work, preserves inspection, and remains
ClosingUnconfirmed because this fake cannot observe OS process exit.

ResourceRef identities are fixture-scoped, require an exact namespace, id,
revision, length and digest, and do not themselves authorize reading or sending.
The fake allows only `fixture/destination` and `fixture/source`; it has no generic
path or URL resolution. Connection loss never grants a fallback backend.

Consumer entry: use this crate by a path dependency from an exported kit. Generated
types are in `agent_host_capnp`; `frame`, `encode`, `decode`, and
`exchange_checked` provide bounded framing and correlation. Build with
`cargo test --locked --offline --manifest-path <kit>/Cargo.toml --features qualification --target-dir <independent-target>`.
See kit `manifest.json`, `vectors/manifest.json` and the host evidence report for
exact input hashes and commands. The production host has no launch command for
this extension yet. No SDK completion, G0 completion or 84-case acceptance is
implied by this candidate.
