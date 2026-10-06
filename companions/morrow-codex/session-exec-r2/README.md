# Codex session / fixed execution R2 adapter

This independent companion consumes the **actual upstream** `codex-thread-store`
and `codex-exec-server` traits from
`../upstream/p02-integration-004/codex-work/codex-rs`. It does not alter that
snapshot or the R1 SDK. Its client uses the R2 SDK with `default-features = false`:
Morrow Core and Codex SQLx link different SQLite versions, so the original Core
native authority lives in a separate host process.

`MorrowThreadStore` implements every required upstream method. Creation metadata
retains the complete `CreateThreadParams` and a real `SessionMetaLine` as the
first reconstructed rollout item. Rollout, literal metadata patches and the UI
archive flag use typed opaque events. Every append commits synchronously;
persist/flush seals a typed checkpoint, shutdown flushes and revokes its dedicated
original writer admission, and discard revokes while preserving durable data.
Logical archive can be undone; it does not use irreversible SDK `Archive`.
Delete delegates reviewed generation/revision/payload-digest CAS retirement after
writers have been revoked and associated tool operations safely retired.

| Actual upstream call | R2 mapping |
| --- | --- |
| `create_thread` | Create, dedicated admission, OpenWriter, typed creation Append |
| `resume_thread` | Snapshot, fresh reviewed writer admission, OpenWriter, metadata Append |
| `append_items` | Shared rollout persistence filter, bounded typed Append |
| `persist_thread`, `flush_thread` | Synchronous durable append barrier, Checkpoint |
| `shutdown_thread`, `discard_thread` | Dedicated original writer admission revocation |
| `load_history`, `load_latest_model_context`, `read_thread` | Correlated paged Snapshot and typed replay |
| `list_threads` | Scoped List, stable ordered pagination, typed metadata filters |
| `update_thread_metadata` | Prevalidated literal patch Append; explicit clears retained |
| `archive_thread`, `unarchive_thread` | Reversible typed archive flag Append |
| `delete_thread` | Trusted retire port; never a guest-supplied delete capability |
| `read_thread_by_rollout_path` | Explicit Unsupported; no local rollout fallback |
| `as_any` | Actual adapter identity |

The adapter uses the bounded legacy history contract. Paginated reference forks,
sections, projects, attachments and unsupported advanced listing filters return
the upstream typed `Unsupported` result. Each opaque event and checkpoint must
fit the reviewed 32 KiB bound. The adapter does not compact rollout events; it
rejects history gaps rather than fabricating missing history.

`CanonicalSessionControl` pins the generation of each original reviewed native
endpoint, constructs canonical requests with fresh bounded identities and checks
every reply with `Reply::decode_for`. It never retries an uncertain commit under
a new request identity. Endpoint revocation is a server obligation that affects
retained clones, not merely a local handle drop.

`MorrowExecBackend` implements the actual upstream start seam. PTY, interactive
stdin, argv0 overrides and shell snapshots are rejected before host review. The
trusted `ReviewedExecConnection` must select an admitted artifact/domain, review
the complete original sandbox/network/environment policy, and perform Propose,
approval, Claim, durable consumption, real backend start and truthful Report on
the original connection. A returned process delegates genuine retained reads and
event subscriptions; interactive write/signal/termination controls remain rejected.
No SDK fact or fabricated event receiver is converted into a started process.

**Qualification limits:** the included host fixture proves actual ThreadStore
calls against genuine Core/Connection/Admission/SQLite authority across a process
boundary. It is a qualification harness, not production package/native routing.
The ExecBackend rejection paths and real trait compilation are qualified; a
reviewed production executor returning a genuine upstream process/event stream
is still required. Complete Codex application injection and takeover are not
claimed.

Reexecute the checks and retain exact logs, commands and before/after source hashes:

```bash
python3 companions/morrow-codex/session-exec-r2/run_qualification.py \
  --output /workspace/morrow/build/codex-session-exec-r2-qualification
```

The runner checks the conservative complete upstream snapshot input set as well
as this adapter, R2 SDK and Core runtime sources. It builds the real host process,
checks the actual traits, runs ThreadStore lifecycle and ExecBackend denial tests,
and runs Clippy and formatting checks. A source change during the run makes the
qualification fail even if individual tests pass.
