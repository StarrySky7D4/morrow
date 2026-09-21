# Actual Rust service outbound test guest

This fixture decodes the actual authenticated service request, forwards the IO
request carried in its body through `morrow_io_v1.call`, validates the actual IO
response, and encodes a service response bound to the original request digest.
When the host supplies the opt-in `service-resources-v1` directory, it decodes
that bounded context and replaces the caller's endpoint/credential fields with
the first explicitly selected resource. Tests deliberately send incorrect body
references and forged reserved headers. The same guest in a legacy package
receives no directory and retains its previous template behavior.
It reuses current core codecs for integration testing; it is not a public SDK
example or a production forwarding service. A guest request never grants access
to an endpoint: the native host must explicitly select and approve it.

From the repository root, build both guests before running the native service tests:

```powershell
cargo build --manifest-path plugins/workbench/Cargo.toml --target wasm32-unknown-unknown --release --target-dir build/service-tests
cargo build --manifest-path workbench_host/tests/fixtures/service-outbound/Cargo.toml --locked --target wasm32-unknown-unknown --release --target-dir build/service-tests
$env:MORROW_WORKBENCH_WASM = (Resolve-Path build/service-tests/wasm32-unknown-unknown/release/morrow_workbench_plugin.wasm).Path
$env:MORROW_SERVICE_OUTBOUND_WASM = (Resolve-Path build/service-tests/wasm32-unknown-unknown/release/morrow_service_outbound_test_guest.wasm).Path
cargo test --manifest-path workbench_host/Cargo.toml --lib io_tasks::service::tests --target-dir build/service-tests
```

The test environment must provide both built files; absence fails explicitly.
The three bounded unsafe imports and fixed export belong only to this Wasm test
fixture. It has no native transport, credential provider or storage access.
