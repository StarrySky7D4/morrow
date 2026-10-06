# agent-content-v1 experimental slice

This independent host-owned extension introduces bounded content requests without
changing the original agent-host-v1, native-session v2, HTTP-stream v3 or frozen SDK
inputs. Its sole schema authority is `contracts/agent_content.capnp`: version **1**,
revision **1**, exact raw SHA256, current Core runtime version and runtime/content
digests are required on every frame. Generated bindings are crate-private. This
candidate is experimental; full SDK, M05 and Codex product qualification remain OPEN.

| Action | Bounded behavior |
|---|---|
| Query | One summary/result per explicitly nominated card, in original order, at most 16 unique IDs. No SQL, wildcard, full-library enumeration or implicit body access. |
| ReadRef | One revision/length/body-digest pinned content read, at most 32 KiB. References do not grant access. |
| ProposeMutation | An exact original Core EditContent frame plus its source reference. Returns a proposal only; no content commit or approval is conveyed by this wire request. |
| InspectOperation | Current exact card/operation lookup. AbsentSnapshot is not proof that no write is in flight; Unknown never authorizes replay. |

Frames are exactly one unpacked Cap'n Proto message, at most 128 KiB, with bounded
traversal/nesting and no trailing message. Core edit commands/replies retain their
64 KiB limits; content references are capped at 8 MiB. Reply requestId and SHA256
bind the complete original request bytes. Embedded Core replies must have the
matching request ID, action, object, revision, offset, length and digest. Query
responses preserve per-card rejection and never substitute an unrelated object.

`Request::new` creates an immutable request; `decode` retains original wire bytes,
and `digest()` hashes those raw bytes rather than a re-encoding. A proposed operation
is bound to that entire request digest, including its ContentRef and outer requestId.
Retries must retain the complete original request: changing outer requestId under
the same operation is an operation conflict, even if the embedded edit is unchanged.

The native host adapter borrows the original HostRuntime/Connection and their
host monotonic clock. It creates no second Store or HostRuntime and does not expose
grant issuance. Approval/commit belongs to a separate trusted host API bound to
the fixed proposal and exact existing object grants. Read, mutation, inspection,
connection lifetime and delivery restrictions are independently rechecked. A late
denial does not undo a committed edit; the original operation must be inspected.
The retained proposal set is bounded at 32. External content sending is unsupported
in this slice: an HTTP approval does not imply content disclosure permission.

`client::read_complete` collects ordered 32 KiB reads under an explicit caller
memory budget and returns a private-constructor `VerifiedContent` only after the
whole body length and SHA256 match the supplied reference. Empty content still
requires one authorized read. Failure stops immediately without retry or partial
success. Verification grants no persistence or external-send authority.

From the repository root, run the ordinary temporary-Store example and tests:

```bash
cargo run --locked --offline --manifest-path extensions/agent-content-v1/Cargo.toml --target-dir build/agent-content-v1-target --example content_roundtrip
cargo test --locked --offline --manifest-path extensions/agent-content-v1/Cargo.toml --target-dir build/agent-content-v1-target
```

Current Linux checks passed: codec 29, original HostRuntime/ordinary Store 17,
complete-read client 14. Zero-test unit/doc runners are not included. The example
actually performs Query, ReadRef, proposal, separate trusted approval, one CAS
commit and read-only inspection; it is not a production native plugin.

No native IPC route, package feature/import negotiation, C/C++ binding, guest,
production GUI, credential scope or persistent proposal recovery is introduced
by the codec. Existing M03 HTTP and owner implementations remain separate.
