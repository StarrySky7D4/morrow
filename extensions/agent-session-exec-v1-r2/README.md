# Agent session and safe execution v1, revision 2

This is the revision 2 schema and Rust API for the independent
`agent-session-exec-v1` profile, version 1. It provides generic
opaque session history and fixed-input execution coordination. It does not
change any previous host-kit, native handshake, capability bit or Core wire
schema. Revision 1 retains its own frozen identity and archived source; the
two revisions have distinct raw schema digests and do not accept each other's
frames. Revision 2 qualifications must be recorded against its own source and
commands. Production integration and OS execution qualifications are separate.

The `host` feature is enabled by default and links the real Core Store. Client
consumers use `default-features = false` to compile the codec without Core's
SQLite dependency. Both modes derive runtime/content identities directly from
the authoritative `../../core` schema files; the source SDK must preserve that
relative layout. There is no second schema copy. `build.rs` checks the Core
runtime protocol version and normalizes Core schema CRLF for its digests; the
profile's own schema SHA remains the exact raw file SHA.

## Session interface

| Request | Durable behavior |
| --- | --- |
| `Create` | Create a nominated session, or a child from an exact sealed parent tail. The child retains the parent's checkpoint bytes and SHA in the same Core Store; its new event history starts empty, with that baseline sealed at child tail zero. |
| `List` | List at most 16 host-nominated sessions. It discovers no other objects. |
| `Snapshot` | Read a complete bounded consecutive event window, current metadata and the sealed opaque checkpoint. `gap` explicitly identifies compacted history; a cursor beyond the tail conflicts. |
| `OpenWriter` | Strict epoch CAS. A new writer belongs to the original live admission and current host issuer. |
| `Append` | Strict writer epoch and tail CAS, one durable batch. Original complete request bytes are retained for exact retry receipts; reusing a request or event identity with changed bytes conflicts. |
| `Checkpoint` | Seal opaque state at the exact current tail. A different checkpoint cannot replace an already sealed checkpoint at the same tail. |
| `Archive` | Persist archive state at an exact writer/tail fence and refuse further new mutations. |

`compact` is a trusted host method, not a wire request. It requires a sealed
checkpoint at the exact current tail. It removes covered event bodies while
retaining identity digests and original mutation receipts. Exact duplicate
requests return historical acknowledgments; they never restore an old writer
or rewind the current state. Use `Snapshot` to reconcile current state.

Each session has at most 256 event identities and 128 mutation receipts;
ordinary requests can use at most 125 receipts. Three control receipts and a
charged byte reserve preserve the bounded writer-recovery/checkpoint/archive
closing path. The complete stored state is bounded to 2 MiB. Compaction does
not erase identity digests or retry receipts. A sealed parent reference can
start a fresh child.

Physical deletion is a trusted `retire_session` operation, bound to an exact
reviewed row revision/SHA, an explicit retirement admission, no live original
writer, and released tool records. Deletion and its generation tombstone are
one Core ledger transaction. There is no automatic eviction or implicit replay.

## Safe execution basic interface

| Request / trusted method | Behavior |
| --- | --- |
| `Propose` | Persist exact fixed program/artifact SHA, argv, cwd, ordered environment, input, execution domain, runtime limit and original proposal SHA. No process starts. |
| `review_tool` | Trusted host review of the original complete proposal, immutable inputs, session epoch and lifetime. It grants nothing. |
| `approve` | Trusted approval requires both original proposal SHA and fixed intent SHA, plus the original live proposer and executor/domain. Returns a random permit; no process starts. |
| `Claim` | Only the strict durable CAS winner receives fixed inputs and a claim token. The state becomes `DispatchUnknown` before the reply. A duplicate never wins again. |
| `execute_claimed` | Trusted callback boundary. Strictly persist `invocation_started` before invoking the callback, then retain observed facts. Any uncertain result remains unknown and cannot be replayed. |
| `Report` | Accept only facts previously observed by the trusted callback, from the original live executor. Exact original retry is idempotent; altered reports conflict. |
| `Inspect` | Read retained facts under current nominated session-read authority. Reading cannot restore a permit, claim or execution. |
| `inspect_tool_observation` | Trusted read of the exact invocation identity and persisted observations, including an unknown operation after restart, under a current session-read admission. It cannot replay or renew execution. |
| `reconcile_tool_observation` | Trusted observation update bound to that complete identity and observation CAS revision. Observations advance monotonically; this does not invoke a callback or grant execution. |
| `retire_tool` | Trusted physical release of an exact reviewed eligible terminal row, with an atomic generation tombstone. Unresolved effects cannot be discarded. |
| `revoke`, `revoke_tool` | Trusted revocation prevents new claims and callbacks. A consumed claim remains unknown; revocation cannot prove that an effect did not happen. |

Process exit and output closure are separate observations. `Reported` means
the supplied observed facts were accepted; check `exit_code` and
`output_closed` individually. Missing facts are never invented. Terminal
metadata space is reserved before approval using fixed-length stored records
and Core's conservative uncompressed byte accounting.

`ToolInfo.facts` retains the facts acknowledged by the original `Report`.
`observation` and `observation_revision` expose the latest persisted trusted
observation independently, including late exit/EOF facts through `Inspect`.
An accepted report is not rewritten when observations advance; exact original
report retries retain their original acknowledgment.

The executor callback is an explicitly trusted execution-domain adapter. It
owns artifact/handle verification, cwd resolution, environment isolation,
actual input handling, timeout and output enforcement. This SDK does not
provide an OS sandbox or automatically spawn programs. PTY, interactive stdin,
streaming output, resize, signal and terminate interfaces are deferred. No
unsupported action falls back to an upstream execution backend.

## Authority, storage and wire boundaries

`SessionExecHost::new(&mut HostRuntime)` acquires the Core agent-ledger owner
lease. Every profile write is checked against that live original lease;
ordinary same-Store CAS calls and another Store handle cannot mutate the owned
ledger. Other Store handles can read retained history but cannot become a
competing profile owner. Unsupported memory/OPFS ownership adapters fail closed.
Creating another host on that same Store is a trusted replacement: it atomically
invalidates the previous owner and admissions, rather than running two live
issuers. Unrelated resource/configuration revocation does not invalidate the
profile owner; global revocation, Store closure and foreign Store identity do.
Startup verifies the complete active/retired identity registry against the
business domains, exact row revision/container SHA and persisted generation.
Host admission requires a reviewed declaration ceiling, an explicitly approved
subset, finite session nominations, a fixed execution domain and a half-open
lifetime. These are trusted import methods, never wire fields. Production
package metadata/import and native transport integration remain separate work.

Opaque admissions bind the original runtime, connection and issuer. Revocation,
connection retirement, clock regression, expiry, writer handoff and host restart
cannot recreate authorization. Original proposer authorization is checked as
well as executor authorization. Profile revocation and callback invocation are
serialized; revocation does not interrupt an already running callback.

The original Core Store schema 25 adds one host-local ledger table. Business
payloads remain Protobuf + LZ4 with bounded corruption-checked envelopes;
SQLite engine pages and indices remain engine-owned. Strict CAS is deliberately
different from historical idempotent operation receipts: an already committed
claim returns conflict. Ledger rows and original content outbox share one byte
budget; there are at most 128 ledger rows across all domains. A profile metadata
row bounds the current generation to 127 session/tool identities, including
retirement tombstones. Retirement releases the business row; it does not allow
identity reuse. Trusted `rollover_generation` requires all profile identities
retired and both business domains empty. It advances the generation, invalidates
the old owner/admissions and requires a new host and fresh trusted admissions.
It cannot discard unresolved operations or bypass other domains' limits.
Metadata and business mutations commit atomically under the original owner
lease. Reserved same-size or shrinking closing updates remain possible when
the shared byte quota is later lowered; growing mutations still fail closed.

Wire frames use the one Cap'n Proto schema in `contracts/session_exec.capnp`.
They bind profile major/revision, raw schema SHA, unchanged Core runtime/content
digests, complete raw request SHA, request identity and nonzero generation.
`Request::new` chooses generation 1; `new_for_generation` explicitly names a
later generation. The host rejects stale generations, and replies must match
the complete original request and its generation. Only the deterministic
SDK encoding is accepted; unknown layouts/unions/capabilities, alternate
encodings, unused bytes and trailing material are rejected. Frames are at most
128 KiB; individual opaque bodies/input/checkpoints are at most 32 KiB;
event windows/batches, argv and environment lists are at most 16 entries.
Aggregation beyond the frame bound rejects the whole result. Debug views omit
opaque bodies, inputs, arguments, environment and bearer tokens.

## Freeze and validation policy

Freeze the session and safe-exec-basic scopes together only after the real
SQLite host tests, adversarial codec tests and Core ledger tests pass.
`tool/verify_session_exec_r2.py` requires an externally supplied reviewed
pin, verifies the full new source and Core dependency closure, and checks exact
recorded test names and log bytes. It does not rerun those commands or
authenticate the reviewer. Wire/semantic changes require a new revision and
new evidence; do not edit a frozen baseline in place. This local interface
freeze does not freeze all SDK26 or qualify production transport/OS execution.

Default offline verification, from the repository root:

```sh
cargo test --locked --offline --manifest-path extensions/agent-session-exec-v1-r2/Cargo.toml --test codec
cargo test --locked --offline --manifest-path extensions/agent-session-exec-v1-r2/Cargo.toml --test session
cargo test --locked --offline --manifest-path extensions/agent-session-exec-v1-r2/Cargo.toml --test safe_exec
cargo test --locked --offline --manifest-path core/Cargo.toml --test agent_ledger
```

Client codec builds with `--no-default-features`, including for
`wasm32-unknown-unknown --lib`; host ownership/execution
methods are native-only. Tests use disposable ordinary stores, fixed synthetic
inputs and a Linux `printf` integration callback. They do not constitute a
Windows native-owner or authenticated production-plugin receipt.
