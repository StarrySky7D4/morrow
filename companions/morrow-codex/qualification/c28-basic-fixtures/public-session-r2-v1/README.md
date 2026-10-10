# Public session R2 fixture v1

This is a separately derived, immutable synthetic session guest: `morrow_codex_session_exec_guest_r2.wasm`, 425912 bytes, SHA256 `cca04ebb2e787f69e84ec7260aca3e93ec895ec17b68afbb660e3c6896ae2f2b`. See [provenance](provenance.json) for its source, original lock, tools and scoped evidence identities.

It is not the historical 427258-byte `b1f0f44ad2bbe2abc89eee0ee971683419bf3407f370194f5557df57fa45799e` fixture. Historical SESSION_SHA and qualification evidence remain unchanged. The new public wrapper has a separate fixed selector and package ID; unknown or modified bytes are rejected.

The actual offline guest build completed with original inputs and strict new-home guards. Finite full-byte UTF-8/UTF-16 machine-path pattern checks found no hits; this is not universal secret detection. Complete static core ABI comparison found matching3 imports,64 exports/61 function signatures,memory/table limits and target features, with no start section. Layout globals changed: __data_end1066437 to1066093 and __heap_base1066448 to1066096. Use dynamic guest/runtime memory handling.

The original HostRuntime/Runner and ordinary synthetic SQLite actually ran one exact test: eight incompatible ABI factories rejected the new guest; the intended combined factory admitted it; seven R2 exchanges create/open-writer/append/checkpoint/parent-snapshot/child-create/child-snapshot and the exact48-byte completion,parent event/checkpoint and child ancestry/inheritance assertions passed. Task read_input and complete are two additional ABI calls. Actual capture, artifact, database, root terminal and postguards received independent limited readback. This does not qualify provider/Claim/native Start, OS process ownership, real disconnect, Windows sandbox, failure-path tests or the entire SDK.

## Reproduction boundary

Use the original guest source and unchanged27-package lock, wasm32-unknown-unknown, process-profile and release profile, with the selected Rust1.95.0 Windows toolchain and Capnp compiler identities in provenance.json. All23 registry archives must retain their original checksums; the exact25-package wasm-filtered metadata graph is not the full host compiler graph. Do not regenerate the lock.

The build command is `cargo build --locked --offline --manifest-path <source>/extensions/codex-session-exec-client-r2/guest/Cargo.toml --features process-profile --target wasm32-unknown-unknown --release --target-dir <fresh-target> --message-format=json-render-diagnostics`. Use explicit already-installed tools and fresh Cargo-home/target/temp roots, clear inherited compiler wrappers/flags and supply path-remapping flags for source/home/target/temp/toolchain to fixed synthetic prefixes `/morrow-source`, `/cargo-home`, `/morrow-target`, `/morrow-tmp`, `/rust-toolchain`. Match all observed slash/backslash and extended-path forms; apply the specific source mapping last. Preserve original input pins and actual captures. Never strip or patch the resulting binary to force an expected identity.

Only one such build is established. Bit-identical reproduction in a second location is not proven; a changed output requires a separately reviewed identity, not replacement of this fixture. The public tool/qualify.py alone does not implement this remapping and strict fresh-home capture contract. No new build tool or automatic execution is included here.

This integration candidate and its focused identity tests are SOURCE only until separately compiled. SDK26_G04=OPEN; release_eligible=false. The complete public Git tree build and native lifecycle remain separate gates.
