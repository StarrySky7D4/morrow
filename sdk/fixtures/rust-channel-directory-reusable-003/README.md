# Fresh operation-equivalent Directory comparison003: reusable

New package org.example.channel.directory-sdk-reusable-003.
This is one fresh comparison pair, not recovered historical Wasm/package bytes.

The one-shot source is byte-identical to immutable
sdk/examples/rust-channel-directory/src/lib.rs (SHA256
137e218a93e49ffa530998c7114d3ff247745f7b9f1e1af91347066799b83fcd).
Reusable source differs by exactly construction of WasmClient and replacement of
call_wasm submission with client.call. All other operations, handler, types,
request identity domain, limit expression, SHA/transcript/ACK and Close behavior
are identical. There are no Query calls or ownership-stress equality scans.

Both new Rust channel projects use generated release opt-level3 and unchanged
20M fuel /16MiB /16 metered host calls. Fresh locks use cached cfg-if1.0.4, matching
current SDK Cargo.lock; old template/source/lock/package pins remain unchanged.

The authentic runtime caller supplies canonical Directory plus5×32768-byte
original bytes or events. All four runs pass, exactly11 channel calls and163840
bytes, with payload SHA256
12a8659000a14e107b88bc40ee759d70c4749f68c01ee54157537b71008d6d16.
Explicit Close, actual original producer join and original event ACK/cursor/hash
receipts are independently verified. Closing the live source still leaves its
ProducerOutcome Unknown; actual join does not mean EOF or source business success.

Build both fresh projects, then explicitly run the bounded ignored comparison:

- python3 tool/morrow_plugin.py pack sdk/fixtures/rust-channel-directory-reusable-003 --require-sdk-lock
- python3 tool/morrow_plugin.py pack sdk/fixtures/rust-channel-directory-oneshot-003 --require-sdk-lock
- sdk/fixtures/run-channel-directory-comparison-003.sh

ARTIFACT_SHA256SUMS independently pins Wasm/package bytes. Build/dist binaries
and caches are ignored and excluded from source-only deliverables.

The001/002 ownership-stress fixtures and their genuine Limits/red outcomes are
unchanged.002 uses the same historical byte vector but stronger operations;
its failure cannot establish immutable Directory example failure.003 is fresh
baseline-contract evidence only, not a universal-size or protected Linux/Windows
product-owner qualification. No further comparison cases are implied.
