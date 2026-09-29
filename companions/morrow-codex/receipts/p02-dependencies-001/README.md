# Fixed P02 dependency preparation

Prepared public sources only; no Cargo/build/test execution or existing lock changes.

| Dependency | Fixed commit | Prepared scope |
| --- | --- | --- |
| openai-oss-forks/tokio-tungstenite | 0e5b2d73aa18dd9f0a50ee9ff199d5aef7594186 | Complete official archive, 34 blobs |
| openai-oss-forks/tungstenite-rs | 4fffad30fe373adbdcffab9545e9e9bf4f2fc19f | Complete official archive, 888 blobs |
| dzbarsky/rules_rust (redirected by GitHub to hermeticbuild/rules_rust) | b56cbaa8465e74127f1ea216f813cd377295ad81 | Complete rust/runfiles directory, 5 blobs, plus root LICENSE.txt; package subset, not complete repository |

All sources are under `upstream/p02-dependencies-001/`. The two WebSocket source roots use `repository-commit`; the runfiles root is `rules_rust-b56cbaa8465e74127f1ea216f813cd377295ad81-runfiles-package`, with Cargo.toml at `rust/runfiles/Cargo.toml`.

For both archives, each regular file matches the fixed recursive Git tree's Git blob SHA-1 and has a SHA-256; the archive has its own SHA-256. The verifier reconstructs every Git tree object and checks the root against the fixed commit API response. Exact file sets, sizes, canonical member names, regular-file types and output boundaries are checked before extraction. Git executable modes are retained in the receipts; Windows source bytes are verified after writing.

For runfiles, exact Git objects were reconstructed along commit → root → rust → runfiles → data, and all five package blobs plus root license were downloaded at the fixed commit and checked against their Git blob hashes. Cargo.toml is standalone, has no dependencies, no build script and lib path `runfiles.rs`. The Rust source has no external module/include/include_bytes/include_str references. Its `option_env!(REPOSITORY_NAME)` belongs to the exported `rlocation!` macro; compiling this library does not require that value. Bazel BUILD.bazel references other Bazel rules, so this package snapshot does not qualify Bazel builds or repository-wide tests.

Evidence: `summary.json`, each `*-verification.json`, commit/tree responses and individual attempts. The initially attempted rules_rust recursive connector batch did not return; the interrupted attempt and repository redirects/errors are retained. The successful shallow/subtree route supersedes that attempt for package preparation only. Nothing was deleted.

Root decides the explicit path patches and isolated probe lock. No fork/upstream/probe files were edited here, and no personal Git/Cargo configuration or credentials were used. Crossterm was not downloaded: it is not in the audited narrow codex-api closure, and the coordinating root can remove the unused independent-root patch explicitly rather than introducing that extra dependency.
