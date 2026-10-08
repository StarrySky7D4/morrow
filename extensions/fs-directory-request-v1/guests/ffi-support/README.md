# Directory C/C++ Wasm build support

This independent workspace combines the existing directory request/page codecs
and the original SDK `wasm-c` allocation exports into one Rust static archive.
Its own `Cargo.lock` is separate from all existing locks. It adds no allocator,
wire protocol, host import, or runtime authority.

Use `tool/build_directory_guest_samples.py` with absolute paths:

```text
python -B tool/build_directory_guest_samples.py
  --source-root SOURCE
  --output NEW_EXTERNAL_OUTPUT
  --cargo-home EXISTING_OFFLINE_CARGO_CACHE
  --sysroot EXISTING_WASI34_SYSROOT
  --cargo RUST195_CARGO
  --clang LLVM22_CLANG
  --clangxx LLVM22_CLANGXX
  --wasm-ld LLVM22_WASM_LD
  --capnp CAPNP
  [--environment-record PRIOR_SYNTHETIC_BUILD_RECORD]
```

The output directory must be new and outside the source tree. If the support
lock is absent, only that new lock is generated offline; compilation then uses
`--locked --offline`. Rust builds `wasm32-unknown-unknown`; Clang uses the existing
`wasm32-wasip1` headers/libraries with no startup or WASI host imports. Linking
reuses `sdk/c/src/morrow_plugin_wasm_libc.c` and, for C++, the original
`sdk/cpp/src/morrow_plugin_wasm_runtime.cpp`. The unchanged C++ sample's run
symbol is renamed at compile time so the original constructor adapter can own
`morrow_run`. Only one combined Rust archive is linked.

Outputs are `directory-c.wasm`, `directory-cpp.wasm`, raw per-command logs and
exit records, and `result.json`. The receipt pins source/build inputs, the
support lock, compiler executables, sysroot headers and selected libraries,
and each artifact; original inputs are compared after building. Static checks
require exactly the three fixed task/directory function imports, their exact
signatures, exported memory and `morrow_run`, and no start section. No import
relaxation or undefined-symbol fallback is used.

A passing build receipt is not managed-owner execution evidence. Real Wasm
execution must separately use the original approved owner, selection, clock,
IoBinding, cancellation and quotas. This remains ordinary synthetic
Store/TempDir qualification, not ProductionGUI, ProtectedSession, picker,
other-platform, or complete SDK26/G04 qualification.
