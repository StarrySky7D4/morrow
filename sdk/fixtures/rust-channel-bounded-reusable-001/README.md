# Bounded reusable Rust Wasm qualification fixture 001

New package `org.example.channel.bounded-sdk-reusable-001`, version `0.1.0-test58.1`.
The new project was generated with the channel Directory starter. Its opt-level3,
20M fuel /16MiB memory /16 metered host-call budgets are unchanged.

This is a generic, real Rust Wasm guest. It uses `channel::transport::WasmClient`
and imports the production `morrow_channel_v1.call`. It accepts the caller's
canonical single-endpoint `morrow.channel.directory.v1` input; it cannot select
or grant a source. The trusted integration caller supplies bytes or events.

The consumer handles at most four granted frames. Each frame is received once,
followed by one Query and an ACK using that original frame's sequence, canonical
frame SHA256 and cursor. Complete owned responses and independent snapshots stay
alive across subsequent calls and the final explicit Close. There are no
transport retries, source replays, budget increases or native callback substitutes.

The 64-byte business output starts `BSDK`, then little-endian kind mode (0 bytes,
2 events), Close status, flags (bit0 resource_reclaimed; bit1 ownership checks),
frame count, payload-byte count, and SHA256 of the concatenated payloads. These
are fixture business fields, not changes to the SDK or channel wire contract.
Close status is Closed or ClosingUnconfirmed. Actual original producer join is
proved separately by the host, never inferred from this output.

Run from the repository root with the existing Rust/capnp toolchain on PATH:

1. `python3 tool/morrow_plugin.py pack sdk/fixtures/rust-channel-bounded-reusable-001 --require-sdk-lock`
2. `python3 tool/morrow_plugin.py pack sdk/fixtures/rust-channel-bounded-oneshot-001 --require-sdk-lock`
3. Build the new standard002 reusable/one-shot fixtures as their README describes
4. `sdk/fixtures/run-channel-bounded-001.sh`

The script verifies fixed ARTIFACT_SHA256SUMS and explicitly runs all ignored
`plugin_runtime/tests/channel_bounded_sdk.rs` tests. Build/dist artifacts are
local and ignored; source-only deliverables retain their SHA256 rather than the
Wasm/package binaries or compiler caches.

Observed unchanged20M boundary:4x31B and1x64KiB/2x64KiB pass for bytes/events;
4x64KiB faults Limits after8 channel calls and ACK2, with no business output.
That negative boundary revokes and actually joins the original producer. It is
not reported as successful consumption. The fresh matched one-shot control also
passes the smaller cases and faults4x64KiB; old immutable packages are untouched.

`Report.host_calls` meters only channel calls here:13,4,7 for the passing cases.
Task read_input and complete are real imports but do not increment that counter.
Event ACK receipts are reopened from SQLite and checked against every original
frame, hash, sequence, cursor and correlated response. Closing a live source
leaves ProducerOutcome::Unknown even after CleanupProof::Joined; join is not EOF.

The generated Directory-template lock referenced unavailable cfg-if1.0.5. Only
this new lock was repinned offline to cached cfg-if1.0.4, matching the current SDK
lock; no old example lock, SDK pin or package was overwritten. SDK source pins
are in sdk.lock.toml.

This evidence does not qualify protected storage, a platform product owner,
Windows native lifecycle, network adapters, services or deployment.

The later standard0025×32768-byte comparable case preserves five frames,
Receive+Query+ACK, owned responses and explicit Close. It genuinely faults
Limits before Close/completion for both transports. The combined harness retains
two red standard success assertions; the001 smaller-case evidence remains valid.
