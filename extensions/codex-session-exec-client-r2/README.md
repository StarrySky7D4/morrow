# Codex session and execution R2 client

Independent, safe Rust client over the frozen `agent-session-exec-v1-r2`
canonical codec. It compiles for native Rust and `wasm32-unknown-unknown`
without linking Core Store into the guest. Existing profiles, schema digests,
SDK ABI and ordinary factories remain unchanged.

`Transport::exchange_once` accepts one canonical request and returns one raw
reply. The original host binds its actual connection and admission separately.
It must not retry. `Client` pins the host-supplied nonzero generation and
unique nonzero 16-byte invocation nonce; it assigns monotonically increasing
request identities and verifies complete request SHA, generation, reply shape
and canonical bytes. No failure is retried automatically. `Unknown` retains
only the request ID, generation and SHA for explicit reconciliation. A
`CommitUnknown` rejection stays explicit. Nothing in the client claims an
unknown process completed or invents its exit status or output closure.

`Scope` is a local declaration ceiling: finite nominated session/operation
IDs, capability flags and a fixed execution domain. It never creates host
authority, permits, grants or approvals. A native or Wasm host must still
validate current original admission for every request. The fixed `Budget`
limits calls, total request bytes and total reply bytes, reserving a complete
128KiB reply before an effect can be admitted.

The typed API covers create/list/snapshot, writer CAS, append/checkpoint/archive,
sealed continuation and propose/claim/report/inspect. Any failed mutation
invalidates its local writer fence. Reading history and explicitly acquiring
a new writer against observed current state remain separate operations.
Approval, process spawning and trusted observations are host APIs.

`history` bounds pages, event count and aggregate opaque bytes. It pins one
revision across pages and rejects concurrent changes. If compaction creates
a gap, the result explicitly includes the sealed checkpoint followed only by
events after that checkpoint. A current sealed parent can create a fresh
child; the client verifies the child's inherited checkpoint digest. Debug
views expose lengths and identities, never opaque state, arguments or tokens.

## Real Rust Wasm guest

`guest/` reads the existing correlated SDK task envelope and makes seven R2
calls: create, open writer, append binary event, seal binary state, snapshot,
continue to a child, read child. It verifies inherited bytes before producing
a correlated computed receipt. The transform handler is
`codex.session.continue`, input type `codex.session.config.v1`, output type
`codex.session.receipt.v1`. Input is little-endian generation (8 bytes), a
unique host-supplied nonce (16 bytes), then UTF-8 nominated session ID. The
host must nominate that session and its `-child` name in the original admission.
These task bytes carry no authority.

The default guest has the single extra import
`morrow_agent_session_exec_v1.call`. `--features process-profile` changes only
that import to `morrow_agent_session_process_v1.call` for the separate combined
factory; it still carries the same unchanged canonical R2 frames. `ffi.rs`
is the only unsafe import bridge, following the old SDK's owned, disjoint,
bounded, synchronous buffers. No native production unsafe boundary is opened.

## Local qualification

`tests/client.rs` exercises real disposable SQLite history, sealed continuation,
bounded pagination, compaction, current revision checks, local ceilings,
budget exhaustion, explicit unknown outcomes and execution proposal inspection.
`qualification/tests/real_guest.rs` actually runs the separately compiled Rust
Wasm through the original runner and host. It checks the seven calls, original
factory rejection, revocation, mismatched replies, a committed append with a
lost reply, and the runner's call budget. A task returning without completion
fails with `TaskProtocol`; an already admitted effect remains committed.

From the source root, use a working Python installation:

```text
python extensions/codex-session-exec-client-r2/tool/qualify.py lock lock-001
python extensions/codex-session-exec-client-r2/tool/qualify.py client client-001
python extensions/codex-session-exec-client-r2/tool/qualify.py guest guest-001
python extensions/codex-session-exec-client-r2/tool/qualify.py qualification qualification-001
```

Labels must be new. The runner captures raw stdout/stderr, exit codes, hashes,
dependency source before/after pins and compiled module SHA in a separate
`wasm-client-runs` tree. Every command is offline and locked after lockfile
generation. Its Windows build environment uses the installed MSVC/capnp tools.
Compilation is separate from actual Wasm execution. These checks do not qualify
provider login, actual Codex inference, Windows OS sandboxing, deployment,
installed production packaging, interactive process control or a full product.


## Portable helper configuration

The public helper requires `--rust-bin`, `--vs-dev-cmd` and `--capnp-bin`; append these explicit arguments to the historical command examples above. [Configuration and qualification limits](../../tool/PORTABLE_WINDOWS_QUALIFICATION.md). Historical receipts do not qualify this changed helper or a later source overlay. No helper was run during this publication preparation.
