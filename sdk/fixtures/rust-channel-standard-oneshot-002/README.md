# Original five-frame standard Rust Wasmi fixture 002

Package: org.example.channel.standard-sdk-oneshot-002, version 0.1.0-test58.4.
Fresh opt-level3 Rust channel project with canonical caller-supplied Directory.
Execution stays at 20M fuel /16MiB memory /16 metered channel calls.

This source is matched to the bounded001 oneshot guest except the explicit maximum
of five granted frames. It retains Receive+Query+ACK for every original frame,
owns responses across subsequent calls, ACKs original sequence/hash/cursor, and
makes one explicit Close. No semantics are removed to meet the budget.
The reusable variant uses real channel::transport::WasmClient; the matched
one-shot variant uses real channel::transport::call_wasm.

The integration caller supplies5×32768-byte original frames (163840 bytes) for
both byte streams and events. It expects16 metered channel calls. The host
independently proves actual original producer join and reopens event ACK receipts.
The prior0014×64KiB real fuel-boundary negative remains unchanged and included.

Build from repository root with the existing Rust/capnp toolchain:

- python3 tool/morrow_plugin.py pack sdk/fixtures/rust-channel-standard-reusable-002 --require-sdk-lock
- python3 tool/morrow_plugin.py pack sdk/fixtures/rust-channel-standard-oneshot-002 --require-sdk-lock
- sdk/fixtures/run-channel-bounded-001.sh

The script also needs the bounded001 fixtures built, as their README describes.
ARTIFACT_SHA256SUMS independently pins Wasm and tool-packed archive bytes.
Cargo source and lock, plugin manifest and SDK source lock are retained. Only
this new lock selects cached cfg-if1.0.4 instead of the old template's offline-
unavailable1.0.5; the original example lock and production pins remain untouched.

Generic dot-Linux Wasmi evidence only. No protected-platform/product-owner,
Windows native lifecycle, network/service adapter or universal-size qualification.
Build/dist binaries and caches are ignored and excluded from source-only bundles.

Observed standard result: both bytes/events genuinely fault Limits after15
channel calls and ACK5. All five original frames total163840 bytes and have
SHA25612a8659000a14e107b88bc40ee759d70c4749f68c01ee54157537b71008d6d16.
Explicit Close/completion is not reached; output is absent. Original Control is
Revoked and original producer actually Joined. These are preserved red
qualification results, not reported as completed business consumption.
The combined eight-test run has six passes and two intentionally retained red
standard success-qualification assertions. No semantics or budgets are changed.
