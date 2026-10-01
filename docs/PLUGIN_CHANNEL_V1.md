# Public channel v1 candidate contract

`channel-v1` provides bounded byte streams and event subscriptions from sources explicitly supplied by a native trusted host. The public contract is independent of the existing guest ABI v2, task v3 and runtime v7 schemas. Those schemas retain their original bytes. Channel declarations and references provide no content, filesystem, network, credential or provider authority.

The wire schema is [`core/schemas/channel.capnp`](../core/schemas/channel.capnp), version 1, SHA-256 `9517a6c51475f9e86ce0a40a6243dafba3308f473e9bcad00e9d47ac4704be46`. Guest calls use the independent `morrow_channel_v1.call(i32, i32, i32, i32) -> i32` import. A request and its response are complete bounded messages. This contract adds no HTTP, SSE or WebSocket backend; those adapters remain unsupported until separately approved and exercised. Windows qualification and SDK publication/freeze are separate decisions with their own evidence.

## Package admission and task metadata

Manifest field 21 is an optional `ChannelDeclaration` defined in [`channel_manifest.proto`](../core/schemas/channel_manifest.proto). It is absent by default in the existing package constructors. A declaration is present if and only if `required_features` contains `channel-v1`. It requires guest ABI v2, declaration and channel versions 1, the exact channel schema digest, one or two distinct kinds (`BYTE_STREAM` = 1, `EVENTS` = 2), nonempty distinct handler names, and a complete finite budget. Unknown, duplicate and nonminimal encodings within this new declaration, including the field 21 key and length, reject admission. Unrelated historical optional manifest bytes remain preserved.

Every declared channel handler refers to an existing `transform_handlers` entry that describes its ABI v2 task types and input/output sizes. For example, `channel.exercise` uses `bytes -> bytes`, with a 65-byte input ceiling and a 64-byte output ceiling. This metadata is checked by `Package::channel_handler`. The ordinary `Package::transform_handler` route rejects channel packages, so task metadata cannot route channel execution into a pure transform or pure evidence capture. Version 1 rejects composition with the earlier IO, dependency and mutation profiles.

`pack-v2` accepts `--handler NAME INPUT OUTPUT MAX_INPUT MAX_OUTPUT`, `--channel-handler NAME`, and `--channel-budget MAX_CHANNELS MAX_FRAME_BYTES MAX_BYTES MAX_MESSAGES MAX_REQUESTS MAX_DURATION_MS`. If channel names are omitted, they are taken from the supplied task metadata. The packaging tool rejects mixed content-capability, IO, service and dependency profiles. Packaging validates metadata; it creates no live source grant.

## Wire and accounting

All messages have a 128 KiB encoded ceiling. A data payload is at most 64 KiB; an opaque cursor is at most 256 bytes. Call IDs, request digests, references, source epochs and directory scope digests contain exactly 32 nonzero bytes. Decoders enforce the exact version and schema digest, finite traversal/depth limits, and a byte-for-byte re-encoding check. The encoder uses one deterministic segment at every permitted payload size. Trailing bytes, additional segments, unknown union/enum values, hidden fields, padding and alternate encodings reject decoding.

`Budget` contains these positive ceilings:

| Field | Rust/protobuf type | Maximum |
| --- | --- | --- |
| `max_channels` | `u32` / `uint32` | 8 |
| `max_frame_bytes` | `u32` / `uint32` | 65,536 |
| `max_bytes` | `u64` / `uint64` | 67,108,864, and at least `max_frame_bytes` |
| `max_messages` | `u64` / `uint64` | 1,000,000 |
| `max_requests` | `u64` / `uint64` | 1,000,000 |
| `max_duration_ms` | `u64` / `uint64` | 3,600,000 |

The runtime additionally checks the live grant against the package declaration. A managed instance shares cumulative accounting and its first monotonic deadline across binds. Closing a source or making another bind does not refund the ledger or renew the deadline. Time, requests, messages and bytes remain bounded even when no progress occurs.

`Directory` holds a host scope digest and a bounded list of endpoints. Each endpoint has a reference, source epoch, kind and finite budget. The native host grant is tied to the actual Manager, HostRuntime, ManagedInstance, package digest, instance control and connection/host bindings. The guest receives approved references; it cannot select an arbitrary local source or substitute a path, URL or SQL statement. Loading a saved directory or checkpoint does not restore a grant.

## Requests and observations

Every request contains version, schema digest, `call_id`, `reference`, `source_epoch` and one action. Every response repeats the call ID, reference and epoch and includes the SHA-256 of the exact canonical request. A frame contains sequence, source epoch, payload bytes and opaque cursor. Its digest is SHA-256 of the standalone canonical `Frame` message, including all those fields.

| Action | Fields and meaning |
| --- | --- |
| `Receive` | `last_acked: u64`, `credit_bytes: u32` in 1..=65,536. At most one delivered frame remains unacknowledged. A delivered sequence is `last_acked + 1` and its payload fits the granted credit. |
| `Ack` | Positive `sequence`, `frame_sha256: [u8; 32]`, and `cursor: Vec<u8>`. The runtime verifies the delivered frame's sequence, full digest and exact cursor before releasing its credit. |
| `Send` | Positive `sequence` and at most 65,536 original bytes. Admission does not prove source observation or an external/business effect. A send is never automatically resent. |
| `Close` | Requests source closure. An incomplete owned worker join remains visible as `ClosingUnconfirmed`. |
| `Query` | Observes the current bounded source state without granting or renewing authority. |

`Status` is `Ready`, `Frame`, `Acked`, `Accepted`, `Idle`, `Closed`, `ClosingUnconfirmed`, `Revoked`, `Expired`, `Limit`, `Invalid`, `Unknown`, or `Unsupported`. Only `Frame` carries a frame. Only `Accepted` carries a positive `accepted_sequence`. Terminal observations may retain the last acknowledged consumption sequence. A response must correlate to the request and its action, including receive sequence and credit, ACK sequence, or accepted send sequence.

ACK is a consumption/credit acknowledgement. It is not resource reclamation or proof of a business effect. Accepted is send admission; the native producer's actual read determines observation. Missing or uncertain send observations retain uncertainty. `Unknown` is not converted into successful delivery. For a started worker, `resource_reclaimed` requires the owning runtime's actual JoinHandle to complete and be joined. A source with no started producer can instead carry the runtime's distinct `NoProducer` proof; that proof does not claim a thread was joined. Runtime cleanup distinguishes `Pending`, `NoProducer` and `Joined`, and records the producer outcome separately. Reclamation may accompany `Closed`, `Revoked`, `Expired` or `Unknown`; it is forbidden with `ClosingUnconfirmed`. Reclamation and source outcome are distinct observations.

The native continuation driver waits outside the single guest callback. A durable ACK additionally performs a synchronous local SQLite transaction. The native Store sets `busy_timeout` to zero: an occupied SQLite write lock returns `StorageBusy` without automatic waiting or retry. Commit work still contributes to host-call latency. Cleanup handles permit cleanup only, without data access. Manager supervision retains the source context and owned worker for revocation and reaping. Source references and epochs are generated by the host; a new grant gets a new resource identity. A recovered cursor cannot unlock an owner, reactivate a previous reference or silently resume a source. Host crash, UI lifetime and subscription owner supervision remain in the existing supervision chain. The private Windows Workbench route is now implemented, but SDK014 production qualification is incomplete; see the recorded successes, failures and unrun checks in the current checkpoint report.

## Durable local subscriptions

The existing Core Store schema v24 adds `channel_checkpoints` and `channel_ack_receipts` to the same SQLite database. It does not create a new database authority. Public guest APIs expose neither SQL nor storage paths. Internal payloads retain the Store's protobuf plus LZ4 envelope representation. Exact original canonical frame bytes, ACK request bytes and ACK response bytes are contained in every committed receipt.

The trusted host API is:

```rust
Store::channel_checkpoint(&subscription, &source_epoch)
Store::channel_ack_receipt(&subscription, &source_epoch, sequence)
Store::commit_channel_ack(&subscription, expected_checkpoint,
                          frame_wire, ack_request_wire, ack_response_wire)
Store::commit_channel_ack_guarded(&subscription, expected_checkpoint,
                                  frame_wire, ack_request_wire, ack_response_wire,
                                  original_authority_guard)
```

`ChannelCheckpoint` records the host subscription identity, source epoch, consumption sequence, frame digest, cursor and revision. `ChannelCommit` distinguishes `Committed` and `Duplicate`. The commit method accepts only a canonical matching frame, ACK request and correlated `Acked` response. It does not commit an `Unknown` response or an admitted send. It checks the expected durable checkpoint and the next contiguous sequence, then atomically stores the receipt and advances the checkpoint in one immediate SQLite transaction. The guarded entry point rechecks the original source authority after SQLite acquisition and immediately before commit or duplicate return; rejection rolls back the transaction. Runtime guards use the original instance control and monotonic deadline, without reentering Store or taking the already held source lock. Duplicate consumption of the same frame does not write another receipt or advance the checkpoint. A duplicate reply can have a new call ID; the first original receipt remains unchanged.

Journal payload bytes count against the original shared Store capacity. The journal has a finite 4,096-receipt limit. Storage busy/full/capacity failures, mismatched cursors, stale CAS, malformed bytes and failed transactions do not confirm cursor advancement. The stored `Acked` response bytes describe the durable committed consumption receipt; they do not prove that this reply reached the guest. A later delivery gate can produce a terminal response or fault, so the actual transport reply remains separate raw evidence. `CommitUnknown` requires querying durable history before any explicit host resume decision. Crash boundaries exist immediately before and after checkpoint commit. Integrity verification checks the exact schema, bounded receipt content, receipt/checkpoint correspondence and contiguous histories.

A saved checkpoint is historical consumption evidence. Reading it starts no subscription, grants no source, and renews no resource epoch. Resumption requires explicit native host approval and a newly bound source. Receipts, consumed cursors, live worker reclamation and business-effect observations remain separate evidence.

## Local verification scope

[`core/tests/channel.rs`](../core/tests/channel.rs) covers strict admission, canonical wire rejection, correlation, full-size payloads and the dedicated task route. [`core/tests/channel_journal.rs`](../core/tests/channel_journal.rs) covers restart durability, unchanged first receipts on duplicate ACK, stale CAS, malformed/unknown ACK outcomes, shared capacity, SQLite busy, receipt tampering and rollback when the final authority guard rejects. A native thread holds a real second-connection SQLite write lock: the first call returns `StorageBusy` without entering its guard or advancing state. After actual token cancellation or a real monotonic deadline expires and the lock is released, a separate explicit host validation call rejects the original authority. This is not automatic Store retry. Another test synchronizes native cancellation or real monotonic expiry at the final transaction guard after both rows have been written, and verifies their rollback. These Core tests verify the supplied host guard, while runtime tests separately verify its binding to the original instance control. Under the existing `fault-injection` feature, two parent tests launch four actual child processes. The production Store's selected fault boundary exits each child with code 86 without unwinding or dropping the connection. The parent reopens the same database file and checks exact checkpoint/receipt state before and after the ACK commit, and exact old/new schema state before and after journal migration commit. The ignored child entry point is invoked explicitly by those parent tests; it is not a skipped crash scenario. Tests do not automatically resend bytes, replay operations or restore source grants. Passing these tests establishes only their recorded local scope; runtime, language and Windows qualification require their separate run evidence.


## Local Workbench channel invocation

The private Workbench host protocol adds `channelPrepare`, `channelAppend`, `channelRun`, `channelStatus`, `channelClose` and `channelReadSent`. This route requires the original supervised Windows owner. Channel actions stay on that local owner and do not use service forwarding. Each IPC frame remains bounded to 128 KiB. A preparation accepts a package digest/revision, one declared channel handler and kind, a finite budget, and exact source frame and byte totals. The source is limited to 64 original frames and 1 MiB in total, intersected with stricter package and host ceilings. Appends carry sequential original bytes and opaque cursors; they stage a frozen source and do not prove guest consumption.

Preparation returns a fresh job key and the public canonical `Directory`. A caller explicitly submits task input once. The generic Workbench channel view supports handlers whose input type is `morrow.channel.directory.v1` and submits those exact Directory bytes. Older `channel.exercise` fixtures retain their explicit 65-byte mode/reference/epoch input contract; it is not inferred for other handlers. The view accepts caller-entered original hexadecimal frames and cursors. It does not read source paths, SQL, network endpoints or credentials.

Run admission returns promptly while the native executor owns the actual invocation. Status reports task completion and original output separately from consumption ACK, send admission, actual native peer observation, producer outcome, resource cleanup proof and executor join. If a source snapshot lock is occupied, `snapshotPending` preserves the cached observation rather than claiming new progress. A successful task return does not prove a business effect or executor cleanup. `workerJoined` records the separate owning executor join; producer `Joined` or `NoProducer` does not substitute for it. A native peer receipt can be read by sequence without resending bytes.

Dismissal retains the original session/key and any unconfirmed result. Inspect and close act on that original session. The UI does not automatically retry appends or run, recreate a grant after a lost reply, resume an old reference from a checkpoint, or treat a cursor as owner release. Browser and unsupervised channel execution remain unsupported. HTTP/SSE/WebSocket and real cloud adapters remain outside this route.


SDK014 production qualification remains incomplete: the actual 20-case suite passed 11 and failed/errored 9; the generic catalog UI failed before invocation. Compiled discovery remains conservative until the failures are resolved. The source budget, cancellation cause and actual resource joins must be fixed or diagnosed without relaxing this public contract. This is an implementation checkpoint, not SDK freeze. See [SDK014 evidence boundary](../reports/codex-morrow-v1.1/windows-sdk-014-2026-10-01.md).
