# Windows native SDK qualification

These tools qualify Windows x64 native SDK codecs, callbacks and consumer builds.
They do not open a Store, provision an audit identity, create/load a user profile,
change security configuration, or qualify Wasm imports, the protected host or UI.

Run with Python 3.12+, Rust/Cargo, Cap'n Proto, LLVM/Clang, and the VS2022 x64
toolchain installed. Supply a fresh output directory outside the source worktree,
a published baseline commit, and an offline Cargo registry cache. Shared SDK and
the read-only schema references must match that commit. The repository tests
compare one core schema; this is a test prerequisite, not a library dependency.
For sparse checkouts include `sdk`, `core/schemas`, and `tool/windows`.

```powershell
python tool/windows/verify_sdk_native_bounded.py `
  --repo C:/work/morrow --pin <published-40-character-SHA> `
  --tool-commit <exact-committed-Windows-tool-SHA> `
  --output C:/evidence/sdk-run-001 --cargo-home C:/offline-cargo `
  --capnp-bin C:/capnp-bin
```

The runner records every command and retains failures. It executes the existing
Rust tests, then builds a real Windows SDK DLL and C11/C++17 wrappers using both
Clang and MSVC. The existing page-boundary tests use Windows VirtualAlloc and
PAGE_NOACCESS. MSVC compilation specifies `/utf-8` so Unicode codec vectors do
not depend on the machine's source codepage. Executables keep assertions enabled.

The last phase copies only SDK source into a separate consumer workspace. Rust,
C11 and C++17 consumers build and run without repository core/runtime/audit
sources. This source copy is a qualification fixture, not a frozen official SDK
distribution. Its offline crate dependencies and Cap'n Proto compiler are still
required. The returned replies are owned; synthetic native callbacks exercise
the public ABI and are not a claim of real guest/runtime import execution.

The runner verifies HEAD and all tool source against `--tool-commit`. It verifies
all SDK/schema files against the baseline Git blobs, allowing only explicitly
identified CRLF-to-LF checkout equivalence. Additional SDK/schema files, links,
missing files or changed contents reject. Its manifest retains both working-byte
and canonical hashes. DLL, import library and MSVC wrapper-object hashes are
captured immediately after their successful build or verified DLL copy, bound
to the producing command log, and rechecked before the final manifest.

`rerun_sdk_native_corrections.py` accepts only a fresh build-bound fixture with
exactly the missing-schema Rust compile failure and the default-source-codepage
MSVC Unicode codec assertion. It rejects old directories without this provenance
instead of retrospectively adding hashes. Historical command arrays are checked
against fixed shapes; the helper constructs its own two correction recipes. It
rechecks source, schema, tool executables, records and all reused artifacts before
and after every functional command. Outputs and Rust targets use a fresh directory.
Its current Rust/codec counts come only from this invocation. Historical findings
are a separate field, without inherited passing counts.

To exercise the helper, first run the committed runner with
`--qualification-fixture` and a separate output directory. This deliberately
omits the core schema from an isolated SDK test copy and compiles only the codec
without `/utf-8`. Fixture exit zero means the expected failure pair was prepared;
it awards zero SDK passes. A full successful qualification is not a retry fixture.
Then supply the receipt's externally recorded manifest SHA256:

```powershell
python tool/windows/rerun_sdk_native_corrections.py `
  --repo C:/work/morrow --pin <baseline-SHA> --tool-commit <Windows-tool-SHA> `
  --mode schema-reference-and-msvc-utf8 --prior C:/evidence/fixture-001 `
  --provenance-sha256 <receipt-SHA256> --output C:/evidence/correction-001 `
  --cargo-home C:/offline-cargo --capnp-bin C:/capnp-bin
```

`test_sdk_native_provenance.py` uses the same identity/input arguments plus
`--legacy`, `--full`, `--wrong-pin` and a fresh `--output`. It retains isolated
copies and detached sparse worktrees for SDK/schema/tool/pin and artifact drift,
command-shape changes, extra/missing failures, altered records/fixture source,
legacy unbound evidence and an ineligible full-success run. Every control must
reject with zero functional commands and zero passing-count promotion. It never
resets, cleans or modifies existing source/evidence directories.

The first 2026-10-02 observations used SDK baseline
`03606019fc4f9e279ec21c7276b83e6ff71865b8`: 92 Rust tests, 7 Clang and 7 MSVC
native executables, and three SDK-only consumer programs across the first attempt
and targeted corrections. That first attempt ran working tool changes on HEAD
`4ad8b920e04f812c4c34f35a6e4129cba4311772`; its source metadata was written before
execution, but lacked a contemporaneous hash of the dirty tool source. The later
`4b114bfb7b8226b92ea9516ef8fc53f7d939127b` runner did not execute the full pipeline.
Those logs remain historical observations, not qualification of that final runner
or reusable artifact provenance. No missing original artifact hash is invented.
Fresh committed-tool verification requires a new full run and separate manifests.
Windows ARM64/x86, Windows Wasmi
import/fuel execution, protected host/executor and product/UI paths were not run.
The SDK remains unfrozen.
