# P02 batch 002 — ready for independent review

Implemented and actually ran two native qualification executables: **9 cases / 39 assertions passed**. This is a bounded callsite slice, not completion of P-02, G0 or J-00.

| Slice | Actual entry | Result |
| --- | --- | --- |
| Storage | Original `codex_thread_store::LiveThread` | 6 cases / 12 assertions |
| Execution | Original Core `UnifiedExecProcessManager::open_session_with_prepared_exec_env` through a qualification bridge | 3 cases / 27 assertions |

Storage proves resume refusal; original history error surviving one discard attempt; nonempty append refusal before metadata; Standard persist Unsupported; flush Unsupported; and live history refusal. The complete call sequences are recorded. Only empty FakeHost history is used for setup. Simulated durableSequence is never promoted to a storage guarantee. No ThreadStore source was changed.

Execution proves that a prepared request with no shell snapshot and `is_remote() == false` reaches the explicitly injected backend exactly once. Non-TTY and TTY-shaped disconnected cases return the original backend failure through Core. The third case receives a bounded 003 Tool.Propose receipt and explicitly refuses to manufacture a process. Recorded filesystem/HTTP adapter calls are empty. These results prove the selected dispatcher branch only; they are not an OS monitor or proof that every direct spawn path is intercepted.

The actual source delta is 5 files (4 modified + 1 added), 163 diff lines, in the new working copy only. It adds explicit Environment capability injection, one dispatcher predicate, and a feature-gated bridge. The original 8697-file source snapshot remains unchanged; the new tree has 8698 files. Host kit 003 and copied fork files remain unchanged. The constructor is not wired into production session startup. Bazel was not validated.

Successful evidence:

- Store build: `receipts/p02-store-probe-002/runs/build-20260928T130311Z-808e61eb41/result.json`.
- Store run: `receipts/p02-store-probe-002/runs/run-20260928T130333Z-d07636b654/result.json`; runtime `receipts/p02-store-probe-002/runtime-20260928T130333Z-d07636b654.json`.
- Exec build: `receipts/p02-exec-probe-002/runs/build-20260928T131255Z-e637b7136f/result.json` (3m 46s compiler output).
- Exec run: `receipts/p02-exec-probe-002/runs/run-20260928T131658Z-84c272f896/result.json`; runtime `receipts/p02-exec-probe-002/runtime-20260928T131658Z-84c272f896.json`.
- Source diff and post-build validation: `patch-after-build-001.patch` / `.json`.
- Dependency review: `resolved-graphs-001.json` (store 882 packages; exec 1117; exact pinned registry checksums, no Git fetch during Cargo stages).

Executables:

- `out/p02-exec-store-002/target/x86_64-pc-windows-msvc/debug/p02-store-probe.exe`, SHA-256 `935ff209a98004d3ff97eb79319f34ccc2e615d11088e65c7835d305f36b5963`.
- `out/p02-exec-store-002/target/x86_64-pc-windows-msvc/debug/p02-exec-probe.exe`, SHA-256 `61edda878479b093fe841e5d8c974af453da50cc220e5cdc09b033a7f489933e`.

All failed attempts are preserved: missing nucleo during the first store lock; two initial store compile API mismatches subsequently fixed; public archive/metadata truncations and raw fetch failures; and a Windows default-decoding preflight error. The final mxc source is a complete 1740-file pinned snapshot reconstructed from individually verified complete archive members, raw files and connector recovery; it is not a complete downloaded archive or Git checkout. The nucleo snapshot has 42 verified blobs including a preserved license symlink.

Remaining contract and coverage gaps:

- M-04: production identity mapping, durable append/history, metadata projections, flush/persist barriers and writer release.
- M-06 (related M-02): argv/cwd/env payload retrieval, process/PTY/IO/control and terminal reconciliation. M-03 is HTTP/stream metadata, not execution.
- Core ModelClient concrete Reqwest, WebSocket, prewarm/reconnect and HTTP fallback paths remain unmodified and un-intercepted. The previous Responses endpoint probe cannot qualify them.
- `core/exec.rs:959` direct spawn and the dispatcher's ordinary local spawn branch are not independently covered. No global OS isolation claim.
- Full Core agent loop, production integration, successful model response, native/Wasm product graphs and all 84 product cases remain unverified/not_run (product graphs 0/2).

See `callsite-assessment.md` for exact original callsites, host assessment identity and test boundaries. `handoff.json` binds the evidence with hashes and verifies that the earlier 17 + 35 frozen inputs still match. No host schema edit, Morrow checkout write, personal account/config/credential read, live model request, commit, push, tag or release was performed.
