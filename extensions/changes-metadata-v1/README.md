# Experimental changes-metadata-v1 extension

Independent, opt-in payload codec and three real guests; not part of the frozen
SDK327 distribution. No Core crate, Store, SQL, filesystem or network API is
linked into the guest. `contracts/changes_metadata_v1.wire` is an exact byte copy
of the Core-owned normative specification. SHA256 is
`07fc0dc4c48eb17f85309207a441d6a07f3fb8e06aab5e2ce337a4b3c98f9089`.
Do not normalize line endings before digest checks.

The Rust decoder is bounded, borrowed and allocation-free. Its C ABI returns an
owned bounded struct and leaves it unchanged on failure. The C++ wrapper calls
that same C ABI. All three validate the exact profile, version, lengths, UTF-8
and identity rules, nonzero scope/window/revision, outer epoch and canonical
cursor. The hash is of the entire Card.encode(), never body-only content.
Scope commits to the complete host-approved fixed set; it never grants access.

The three guests consume exactly one Events endpoint from the original
`morrow.channel.directory.v1` task input. The handler is `changes.metadata`, and
output type `morrow.changes.metadata.count.v1` is a single little-endian u64
count of successfully ACKed metadata frames. They keep scope constant within
the window, enforce transport budgets and contiguous sequences, and use the
original receive/ACK transport. They may retry bounded Idle/ClosingUnconfirmed responses and
complete only at Closed. An unconfirmed close and errors do not report completion.
ACK count does not assert a business effect, producer join, or future catch-up.

The independent `core/examples/changes_metadata_package.rs` entry builds only
ABI2 transform/channel/changes-metadata packages with EVENTS-only declarations
and empty content, dependency, IO and service permissions. It requires explicit
profile name, version and digest; unknown options and profile mismatches reject.
It publishes without overwrite. Original packaging tools and gates are intact.
An old host must reject the unknown required feature before invoking new guests.

Prepare using `python3 tool/prepare_changes_metadata_guests.py --output NEW_DIR
--rust-target EXTERNAL_CACHE --c-target EXTERNAL_CACHE --wasi-sdk WASI_SDK_ROOT
--pack-executable CORE_PACK_TARGET/debug/examples/changes_metadata_package`.
Use the already available Rust/Capnp/WASI toolchain. Every Cargo invocation is
locked and offline, with no network fallback. The C/C++ link uses one combined
Rust archive containing this codec and original SDK C exports. No double Rust
runtime or new generator is introduced. Output must be new and external; the
manifest records source, contract, toolchain, archive, pack tool and guest hashes.
Build success is explicitly not runtime qualification.

Native conformance uses `tests/vectors.bin` identically in Rust tests, C and C++.
`tests/generate_vectors.py` deterministically builds the corpus with Python's
stdlib from the frozen wire spec. Native code-only tests do not establish the
Store-to-guest end-to-end source path; runtime evidence must separately use real
Core commits, receiver-specific source approval and original same-Store ACKs.

This profile covers finite host-approved fixed-card metadata invalidations only.
Fresh windows need fresh authority. No persistent watch, content grant, Store
path, global sequence, hidden upper bound or unselected IDs are exposed. Logical
Store identity can survive copies; no anti-clone claim is made. Linux guarded
SQL testing does not qualify a protected product-owner runtime path. The previously reported Windows 42 passes retain their original scope; the nine
protected tests remain NOT_RUN. This new profile has no Windows execution
qualification.
