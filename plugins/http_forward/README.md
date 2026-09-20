# Experimental Rust HTTP forward guest

This is a real, independently compiled Rust Wasm guest, not a WAT fixture and not
the workbench guest relabeled with additional permissions. It has no dependencies
on the SDK, trusted core, a network library, WASI, or credential storage.

- Package: `org.example.morrow.http-forward`, version `0.1.0`.
- Experimental IO handler: `morrow.http.forward.v1`.
- Declared IO capabilities: `HttpRequest` and `CredentialUse`.
- IO budget: 2 resources, 1 job, 1 MiB per job, 4 MiB total, 30,000 ms.
- Execution budget: 1 IO call, 4 MiB Wasm memory, 20,000,000 fuel.

## Contract

The dedicated trusted host generates one bounded `core/schemas/io.capnp` Request
frame and supplies that exact frame as task input. It owns the endpoint reference,
credential reference, operation identity, request digest, instance authorization,
and policy checks. The guest does not accept a raw URL or a secret, does not decode
or modify the frame, and does not create or impersonate those authorities.

The guest calls `morrow_task_v1.read_input` once, forwards the original bytes through
`morrow_io_v1.call` once, and passes the exact returned response bytes to
`morrow_task_v1.complete` once. Input and output capacities are each exactly 128 KiB
as required by the experimental runtime. Request and response allocations are
disjoint. `src/imports.rs` contains the entire unsafe FFI boundary; forwarding and
length checks are safe Rust.

This experimental task mode completes with the raw IO response, **not** the SDK's
ordinary TaskCompletion envelope. The trusted host must validate actual response
provenance and the response's correlation with the original request. Structured
denials and `OutcomeUnknown` responses are returned unchanged. A zero guest return
code means only that the raw response was handed back, not HTTP success. Nonzero
codes identify a read failure (`1`), IO transport failure (`2`), or completion
failure (`3`); none authorizes an automatic retry.

This handler is a single-frame forwarder. It does not poll, cancel, finish, retry,
or reconcile network jobs by itself. The host owns the later observation and
cleanup workflow. It does not enable a plugin, approve an endpoint, read stored
secrets, or grant network access. Installation, IO approval, enabling, and endpoint
approval remain separate explicit host actions. No frozen SDK API is changed.

## Build and package

Run from the repository root with the installed `wasm32-unknown-unknown` target:

```powershell
cargo build --manifest-path plugins/http_forward/Cargo.toml --release --target wasm32-unknown-unknown --target-dir build/http-forward-guest
cargo test --manifest-path plugins/http_forward/Cargo.toml --target-dir build/http-forward-guest
cargo run --manifest-path workbench_host/Cargo.toml --example package_http_forward -- build/http-forward-guest/wasm32-unknown-unknown/release/morrow_http_forward_plugin.wasm build/http-forward-guest/http-forward-0.1.0.morrow-plugin
```

The packager validates the experimental Wasm ABI and builds the manifest using the
current core IO schema digest. It creates a new archive and refuses to overwrite
an existing path. Packaging neither installs nor executes it. The output includes
the package SHA-256 needed for a later explicit approval.

The guest unit tests check exact byte forwarding, disjoint buffers, full frame
limits, invalid lengths, and failure paths with no retries. These tests and a
successful Wasm build do not prove host integration, real network execution,
credential use, recovery, or provider reconciliation; those require the dedicated
host's integration tests and evidence.
