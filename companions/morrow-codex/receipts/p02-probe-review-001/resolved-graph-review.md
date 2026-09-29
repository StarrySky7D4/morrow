# Resolved graph independent review

No unresolved material graph blocker found. Read-only review of the completed metadata, independent lock, manifests, checksums and build messages; no Cargo or test rerun.

The resolved graph has 598 packages: 21 local paths and 577 registry packages. Full identities, versions, checksums, enabled features and dependency conditions are listed in `resolved-graph-review.json`.

| Local package | Version | Manifest |
| --- | --- | --- |
| codex-api | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/codex-api/Cargo.toml` |
| codex-async-utils | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/async-utils/Cargo.toml` |
| codex-client | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/codex-client/Cargo.toml` |
| codex-execpolicy | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/execpolicy/Cargo.toml` |
| codex-extension-items | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/ext/items/Cargo.toml` |
| codex-http-client | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/http-client/Cargo.toml` |
| codex-network-proxy | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/network-proxy/Cargo.toml` |
| codex-protocol | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/protocol/Cargo.toml` |
| codex-utils-absolute-path | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/utils/absolute-path/Cargo.toml` |
| codex-utils-cache | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/utils/cache/Cargo.toml` |
| codex-utils-home-dir | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/utils/home-dir/Cargo.toml` |
| codex-utils-image | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/utils/image/Cargo.toml` |
| codex-utils-path-uri | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/utils/path-uri/Cargo.toml` |
| codex-utils-redacted-string | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/utils/redacted-string/Cargo.toml` |
| codex-utils-rustls-provider | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/utils/rustls-provider/Cargo.toml` |
| codex-utils-string | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/utils/string/Cargo.toml` |
| codex-websocket-client | 0.0.0 | `upstream/p02-source-batch-001/codex-source/codex-rs/websocket-client/Cargo.toml` |
| morrow-agent-host-contract | 0.1.0-experimental.1 | `sdk/host-kit-003-copy/Cargo.toml` |
| morrow-codex-p02-native-probe | 0.0.1 | `qualification/p02-native-probe-001/Cargo.toml` |
| tokio-tungstenite | 0.28.0 | `out/p02-native-probe-001/build-forks/tokio-tungstenite/Cargo.toml` |
| tungstenite | 0.27.0 | `out/p02-native-probe-001/build-forks/tungstenite/Cargo.toml` |

574 registry name/version/source/checksum tuples match the fixed Codex lock. The only registry additions are capnp 0.24.1, capnpc 0.24.0 and embedded-io 0.7.1, all with checksums matching host-kit-003 Cargo.lock. No selected upstream registry version was replaced by an unapproved version. Embedded-io 0.7.1 is a kit addition; the larger original lock also contained 0.4.0 and 0.6.1. All 577 vendor package checksum declarations match the selected lock. This does not repeat the earlier full vendor byte audit.

Both WebSocket forks resolve to explicit local paths, with versions 0.28.0 and 0.27.0 unchanged. Independent byte comparison confirms all 922 prepared files match their fixed originals except the declared tokio-tungstenite Cargo.toml edit: replace its nested tungstenite git/rev pair with `path = "../tungstenite"`. Parsed TOML has no other semantic change; every Rust source byte matches. There is no crates.io fallback for either fork. The lock deliberately uses path identities, so the separate source/patch receipts retain commit provenance. Runfiles is only an unused patch entry; crossterm is absent.

Metadata was not filtered by target, so its 598 packages include platform alternatives. The actual native build reported 476 distinct compiler-artifact package IDs and 60 build-script-executed messages (54 unique package IDs). These messages can include cached script outputs; they are not a process tracing log. The selected artifact is a Windows x86_64 MSVC debug binary. Wasm/WASI names in the all-target graph do not establish a Wasm build, sandbox or browser runtime. No Wasm/plugin IPC artifact is qualified.

Key build scripts include kit schema generation via the local capnp compiler; aws-lc-sys, ring, zstd-sys and blake3 C/assembly tooling; and starlark rustc version probing. Source/lock hashes alone do not pin every host C toolchain detail. Cargo offline mode limits dependency downloading; it is not OS network or subprocess isolation. No automatic download path was identified in the inspected key entrypoint snippets, without claiming a complete transitive build-tool audit.

Reqwest TLS/system-proxy support and Tokio fs/net/process features remain enabled through upstream dependencies. The probe constructs its refusing transport and does not construct those network clients. This qualifies only the actual ResponsesClient endpoint refusal seam; it does not close ModelClient/Core WebSocket/direct network paths, exec or store, or prove complete P02/G0.

All metadata stdout and recorded post-lock input hashes matched current files at review. The runtime chain is reviewed separately in `runtime-review-001.json`.
