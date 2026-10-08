# Independent real R2 proposal guest

This separately compiled Rust Wasm guest performs exactly one R2 Propose
through `morrow_agent_session_process_v1.call`. It reads the existing SDK task
envelope with handler `codex.session.propose`, input type
`codex.session.proposal.v1`, and output type
`codex.session.proposal.receipt.v1`. Input is an unchanged canonical R2
Request whose only allowed Action is Propose. The host must supply a unique
input request ID and the already fixed synthetic/native intent.

The client pins `input_request.generation()`. Its nonce is the first 16 bytes
of `input_request.digest()`. Its only nominated session, operation and execution
domain come from that fixed proposal; its declaration ceiling enables only
read and propose. Budget permits one host call and one 128KiB request/reply.
These bytes create no authority. The original host admission authorizes the
proposal. This guest performs no approval, claim, process start or control.

The actual generated request ID is:

```text
codex-<lowercase hexadecimal of the first 16 input-request SHA256 bytes>-1
```

The actual generated Request uses this ID, the unchanged input generation
and the unchanged Propose Action. The native coupling test reconstructs it
with `Request::new_for_generation`, then verifies the task completion with
the original task envelope and its output via `Reply::decode_for` against
that reconstructed request. The guest also captures and checks its actual
generated request bytes. It outputs only the actual canonical raw reply
returned by the original host after the safe client verifies it and the
phase is Proposed. It never constructs a fake reply. Non-Propose input,
denial, unknown result, wrong schema/correlation or wrong phase yields no
completion; the task runner rejects the task.

`ffi.rs` is the isolated necessary unsafe import bridge, using live owned
disjoint bounded synchronous buffers as in the existing SDK. The orchestration
module denies unsafe. Existing session/process guests, client APIs, schemas,
SDK and native execution boundaries are unchanged.

`tool/build.py` generates the separate offline lockfile, formats only this
guest, or builds this guest for wasm32-unknown-unknown. It records raw logs,
exit codes, module SHA and full dependency before/after source manifests in
a separate `proposal-guest-runs` tree, using short target directory `pg`.
Compile evidence is separate from actual native coupling evidence. The root
qualification performs real Wasm proposal, original R2 review/claim, native
Windows provider execution, and subsequent real process-control Wasm calls.

```text
python extensions/codex-session-exec-client-r2/proposal-guest/tool/build.py lock lock-001
python extensions/codex-session-exec-client-r2/proposal-guest/tool/build.py format format-001
python extensions/codex-session-exec-client-r2/proposal-guest/tool/build.py build build-001
```


## Portable helper configuration

The public helper requires `--rust-bin`, `--vs-dev-cmd` and `--capnp-bin`; append these explicit arguments to the historical command examples above. [Configuration and qualification limits](../../../tool/PORTABLE_WINDOWS_QUALIFICATION.md). Historical receipts do not qualify this changed helper or a later source overlay. No helper was run during this publication preparation.
