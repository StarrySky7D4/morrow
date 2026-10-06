# Agent session and safe execution v1

This is the sole schema and Rust API for the independent
`agent-session-exec-v1` profile, version 1, revision 1. It provides generic
opaque session history and fixed-input execution coordination. It does not
change any previous host-kit, native handshake, capability bit or Core wire
schema. The package remains experimental until its production import and
transport are integrated.

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
the complete stored state is bounded to 2 MiB. These are hard admission limits,
including after compaction. A sealed parent reference can start a fresh child.
No automatic eviction, implicit replay or second writable model history exists.

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
| `inspect_tool_observation` | Trusted read of already persisted callback observations, including an unknown operation after restart, under a current session-read admission. It cannot replay or renew execution. |
| `revoke`, `revoke_tool` | Trusted revocation prevents new claims and callbacks. A consumed claim remains unknown; revocation cannot prove that an effect did not happen. |

Process exit and output closure are separate observations. `Reported` means
the supplied observed facts were accepted; check `exit_code` and
`output_closed` individually. Missing facts are never invented. Terminal
metadata space is reserved before approval using fixed-length stored records
and Core's conservative uncompressed byte accounting.

The executor callback is an explicitly trusted execution-domain adapter. It
owns artifact/handle verification, cwd resolution, environment isolation,
actual input handling, timeout and output enforcement. This SDK does not
provide an OS sandbox or automatically spawn programs. PTY, interactive stdin,
streaming output, resize, signal and terminate interfaces are deferred. No
unsupported action falls back to an upstream execution backend.

## Authority, storage and wire boundaries

`SessionExecHost::new(&mut HostRuntime)` pins the original Store's native
service-authority owner. Other Store handles can read retained history but
cannot become competing live profile owners or mutate its ledger until the
original Store closes. Unsupported memory/OPFS ownership adapters fail closed.
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
budget; there are at most 128 ledger rows across all domains.

Wire frames use the one Cap'n Proto schema in `contracts/session_exec.capnp`.
They bind profile major/revision, raw schema SHA, unchanged Core runtime/content
digests, complete raw request SHA and request identity. Only the deterministic
SDK encoding is accepted; unknown layouts/unions/capabilities, alternate
encodings, unused bytes and trailing material are rejected. Frames are at most
128 KiB; individual opaque bodies/input/checkpoints are at most 32 KiB;
event windows/batches, argv and environment lists are at most 16 entries.
Aggregation beyond the frame bound rejects the whole result. Debug views omit
opaque bodies, inputs, arguments, environment and bearer tokens.

## Freeze and validation policy

Freeze the session and safe-exec-basic scopes together only after the real
SQLite host tests, adversarial codec tests and Core ledger tests pass.
`tool/verify_session_exec_freeze.py` requires an externally supplied reviewed
pin, verifies the full new source and Core dependency closure, and checks exact
recorded test names and log bytes. It does not rerun those commands or
authenticate the reviewer. Wire/semantic changes require a new revision and
new evidence; do not edit a frozen baseline in place. This local interface
freeze does not freeze all SDK26 or qualify production transport/OS execution.

Default offline verification, from the repository root:

```sh
cargo test --locked --offline --manifest-path extensions/agent-session-exec-v1/Cargo.toml --test codec
cargo test --locked --offline --manifest-path extensions/agent-session-exec-v1/Cargo.toml --test session
cargo test --locked --offline --manifest-path extensions/agent-session-exec-v1/Cargo.toml --test safe_exec
cargo test --locked --offline --manifest-path core/Cargo.toml --test agent_ledger
```

Client codec also builds for `wasm32-unknown-unknown`; host ownership/execution
methods are native-only. Tests use disposable ordinary stores, fixed synthetic
inputs and a Linux `printf` integration callback. They do not constitute a
Windows native-owner or authenticated production-plugin receipt.
