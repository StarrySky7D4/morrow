# Codex process control client v1

Independent safe Rust client over `agent-process-control-v1`, with its host
feature disabled. The guest does not link a provider, mint handles, approve
execution or renew authority. It uses an already started process's original
opaque 32-byte handle and fixed nonzero generation supplied by the trusted host.

`Transport::exchange_once` makes one canonical request/response exchange.
`Client::new(transport, handle, generation, unique_invocation_prefix,
declared_capabilities, budget)` pins those values. Prefixes reserve enough
space for the full monotonically increasing u64 suffix and the protocol's
128-byte request ID limit. Every reply verifies generation, complete request
SHA, ID, response variant and canonical encoding. No exchange is retried.

Call `discover` first. Actual capabilities are the intersection of the
provider's approved support and the client's declaration ceiling. Typed APIs
cover `read`, `events`, `write`, `close_input`, `interrupt`, `terminate` and
`resize`. Unsupported actions are rejected before exchange. In particular,
the pinned Codex ExecProcess lacks close-input and resize methods; discovery
from its real adapter must report those capabilities false. Protocol support
does not supply missing provider methods.

The client has finite call/request/reply byte budgets and reserves the entire
maximum reply before admitting a control. A lost, malformed or mismatched
control reply, or an explicit host Unknown response, permanently blocks
further controls through that client. `discover`, `read` and `events` remain
available within authority and budgets for observing facts. They never unlock
the uncertain effect. The client does not automatically replace itself or
reissue writes, signals or termination. A successful close-input blocks more
client input. Accepted controls do not prove process exit or output EOF;
those observations remain distinct in returned `OutputPage` values.
The Unknown latch is set before invoking a mutation transport and only cleared
by a fully correlated Accepted reply or definite Rejected reply. A native
transport panic caught by an outer caller therefore also leaves controls blocked.

`call(Action)` returns the original typed canonical host reply, including
explicit rejected/Unknown bodies. Typed control wrappers return errors for
rejected bodies. Unknown error diagnostics include only request identity,
generation and digest; no bearer handle or process input is printed.

## Real Rust Wasm guest

`guest/` has the single extra import `morrow_agent_session_process_v1.call`.
Only its isolated `ffi.rs` uses the established owned/disjoint/bounded
synchronous import bridge. The SDK's safe task read/complete wrappers remain
unchanged. The client's code denies unsafe.

The host supplies an existing transform task with handler
`codex.process.control`, input type `codex.process.request.v1` and output type
`codex.process.reply.v1`. Its input is one canonical process-control Request;
the input request ID must be a unique host-supplied invocation prefix, at most
107 bytes, valid for this protocol. The guest makes Discover with
`<input-request-id>-1`, then the supplied Action with `<input-request-id>-2`.
If the supplied action is Discover, only the first call runs. Its completion
contains the actual canonical reply for that last generated request. The host
must reconstruct that request to verify reply correlation and verify the
existing task completion correlation independently.

The guest completes with a canonical rejected host reply if the actual host
returns one. Local refusal, import failure or malformed replies return without
completion and the task runner rejects with TaskProtocol or its transport fault.
No error is synthesized into an Accepted reply. Configuration and task bytes
carry no authority; the composed host must validate the original binding and
live admission again at each import.

## Qualification

From the source root, with installed Python/MSVC/capnp:

```text
python extensions/codex-process-control-client-v1/tool/qualify.py lock lock-001
python extensions/codex-process-control-client-v1/tool/qualify.py format format-001
python extensions/codex-process-control-client-v1/tool/qualify.py client client-001
python extensions/codex-process-control-client-v1/tool/qualify.py guest guest-001
```

Each label must be new. Raw logs, exit codes, module SHA and full dependency
source before/after pins are kept under a separate `process-client-runs` tree.
Short independent `pn`/`pw` target directories avoid Windows linker path limits.
The client tests check every typed action, capability intersection, missing
close/resize provider support, budgets, ID/generation correlation, explicit
rejections and Unknown controls remaining blocked after fact reads. Guest
compilation is a separate result from actual Wasm execution through the composed
host. Real provider, process-tree, PTY and OS isolation qualifications are separate.


## Portable helper configuration

The public helper requires `--rust-bin`, `--vs-dev-cmd` and `--capnp-bin`; append these explicit arguments to the historical command examples above. [Configuration and qualification limits](../../tool/PORTABLE_WINDOWS_QUALIFICATION.md). Historical receipts do not qualify this changed helper or a later source overlay. No helper was run during this publication preparation.
