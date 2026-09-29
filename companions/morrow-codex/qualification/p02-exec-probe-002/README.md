# P02 execution callsite qualification

This independent native executable links actual Codex Core and enters its original `UnifiedExecProcessManager::open_session_with_prepared_exec_env` through a small feature-gated bridge. It does not run a substitute agent loop. The bridge starts after policy/orchestration preparation and cannot validate those earlier stages.

The five-file upstream patch is restricted to the new working copy and recorded by `receipts/p02-exec-store-002/review_working_patch.py`. Explicitly injected exec/filesystem/HTTP capabilities select the backend branch even when `is_remote()` is false and the prepared request contains no shell snapshot. No default local capabilities are constructed. Other paths can still access the OS; this is not a global sandbox guarantee.

Two disconnection cases exercise non-TTY and TTY-shaped requests; TTY here is a request field, not a functioning PTY. A third bounded FakeHost Tool.Propose case rejects execution explicitly. 003 has no argv/cwd/env payload retrieval or process lifecycle contract. These require M-06 (related M-02); M-03 concerns HTTP/stream. The actual serialized parameters are digest-bound into the proposal, but no successful process, claim, output, or completion is fabricated.

Use `receipts/p02-exec-probe-002/run_probe.py` with the source patch receipt and its SHA-256. Only the explicitly selected lock stage may update this independent Cargo.lock. Build/run stages use the fixed Rust 1.95 toolchain, locked offline dependencies and isolated Cargo/home/temp directories. Failed attempts remain in fresh run folders.

The feature and bridge are qualification-only Cargo changes. Bazel and the native/Wasm product graphs are not validated. Core ModelClient HTTP/WebSocket/prewarm/fallback paths remain un-intercepted; `core/exec.rs` direct spawn remains uncovered. The constructor is not integrated into production session startup.
