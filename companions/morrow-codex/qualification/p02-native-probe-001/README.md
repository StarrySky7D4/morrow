# P-02 native network callsite probe 001

Windows native compilation and the two refusal cases passed on 2026-09-28.
The current result is recorded in
`receipts/p02-native-probe-001/runtime-20260928T121810Z-d60bc57149.json` and its
separate build/run receipts. Earlier preparation and failure records remain
historical evidence. This is a limited network callsite result, not P-02 completion.
This single native crate calls the unmodified fixed upstream
`codex_api::ResponsesClient::stream_request`. Its real `EndpointSession` serializes
and prepares the request, applies an explicit no-auth provider, and invokes the
concrete injected `MorrowDisconnectedTransport` through the original
`HttpTransport` trait. No new loop or copied upstream endpoint implementation is
present.

The adapter records the actual prepared request, then encodes a host-kit-003
`Stream.Open` with that exact body's length and digest. The exported
`exchange_checked` reaches a deliberately disconnected in-process `Transport`
fixture. Its refusal is mapped to an identifiable upstream error. A second real
endpoint invocation uses an unapproved `.invalid` destination and must fail
before any host exchange. Both cases require exactly one stream attempt,
preserved sentinel error and zero commit attempts.

This proves only a bounded endpoint injection/refusal when compiled and run. It
does not prove connected host IPC, generic HTTP metadata/stream support, SSE,
actual network monitoring, `ModelClientSession`, the Codex loop, exec/store
replacement, production A/B isolation, P-02 completion or G0. The host kit's
transport error enum has no disconnected variant; `Invalid` is used solely by
the explicit disconnected fixture and mapped locally, without changing Schema.
No fake host feature is enabled on the kit.

## Inputs and build boundary

- Codex: `44fe510ce3ee61c8ef623adcbf89b901c73ddd61`.
- Path dependencies expect the complete independently verified snapshot at
  `upstream/p02-source-batch-001/codex-source/codex-rs/`.
- Host-kit-003 manifest SHA-256:
  `5555751ab1cf590d68e9610dae396677b79422999ec61b7c3d3d7183e8ed4a01`.
- Rust: `1.95.0-x86_64-pc-windows-msvc`.
- Direct registry versions match the original fixed Codex lock. The required
  tokio-tungstenite/tungstenite forks are independently verified at exact Git
  revisions. Isolated build copies preserve all Rust source bytes; a recorded
  manifest-only patch redirects tokio-tungstenite's nested dependency to the
  fixed local tungstenite copy. The independent lock therefore records these
  two fork packages as local sources, not as unchanged Git source IDs.
  Provenance and the exact manifest diff are in `receipts/p02-dependencies-001`
  and `receipts/p02-dependency-cache-001/build-forks-001`.
  The unused crossterm patch is excluded because this package is absent even from
  the conservative codex-api lock dependency graph.
- The fixed `runfiles` Git dependency uses its independently verified complete
  package subtree plus the root license. This does not claim a complete
  `rules_rust` checkout. The qualification root maps it to a verified package
  copy, but the resolved graph does not use runfiles (the lock records an unused
  patch). Upstream originals remain unchanged.
- A separate probe `Cargo.lock` was prepared offline and used by the successful
  `--locked --offline` build/run. It contains 598 resolved packages, including
  17 upstream Codex path packages. This is an independent native qualification
  graph, not either product graph.

After the complete source, exact fork dependencies and probe lock are available,
use an isolated plugin `CARGO_HOME`, target and temporary directory. Start Cargo
from a config-free directory such as checked `C:\` with the absolute manifest
path, so it cannot discover a personal ancestor `.cargo/config`. Do not inherit
personal config, credentials, CODEX_HOME, RUSTFLAGS or Cargo source replacements.

```text
<exact-1.95.0-toolchain>/bin/cargo.exe run --locked --offline --manifest-path <repo>/qualification/p02-native-probe-001/Cargo.toml --target x86_64-pc-windows-msvc --target-dir <isolated-target> --bin p02-native-probe -- <repo>/receipts/p02-native-probe-001/runtime-001.json
```

The receipt output must be a new file directly inside the designated receipt
directory. Capture the command, compiler identity, current source/patch/lock
digests, exit code, stdout and stderr. A successful build alone is not a runtime
result; missing inputs and all failed attempts remain evidence.
