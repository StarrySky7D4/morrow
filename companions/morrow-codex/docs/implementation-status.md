# G0 implementation status

Initial date: 2026-09-28 (Asia/Shanghai).

| Work package | Current scope | Gate |
|---|---|---|
| P-00 | Implemented build entry, source/toolchain locks, separate graph contracts and per-run status | First slice only; product manifests/locks and complete upstream sources missing; M-00 received, kit-002 acceptance suspended |
| P-01 | 209 fixed Codex and 13 fixed CC Switch files verified; 147 Codex manifests and 16 candidate closures audited | Static source review only; complete source, feature-resolved minimal graph and runtime qualification missing |
| P-02 | Actual upstream interfaces/call sites identified, no replacement probe executed | Blocked by incomplete buildable source and host withdrawal of kit-002; no substitute loop |

Morrow is read-only to this session:
`C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor`,
branch `codex/io-safety-refactor`, initial HEAD
`88557916aabf2e10619b1022110035178498898a`. Its initial tracked/untracked status was
empty when checked before plugin initialization. The root Morrow checkout is an
older branch and is not this implementation baseline.

The host session owns the only new generic schema at
`contracts/experimental/agent_host_v1/`. The formally delivered kit is
`reports/codex-morrow-v1.1/host/host-kit-002/manifest.json`, SHA-256
`bb02d3e4b0ac1efd3195b1036bdb00669036d1124733060cde3b2d279576f464`.
The host subsequently suspended acceptance of 002 pending a maximum-event-batch
traversal-budget fix. Await a new explicitly delivered kit; 002 is not a usable
accepted baseline. The earlier `host-kit/` is also only a preserved candidate. Existing Wasm
contracts are preserved. A/B remain independent; Net/IO get no private DLL.

The original plan remains read-only. Planned interfaces and example commands in
that plan are specifications, not evidence of existing implementation. G0 is not
SDK freeze, product completion, or 84 passing acceptance cases.

## Evidence conventions

- `receipts/`: source, baseline, audit, and concise test evidence suitable for review.
- `out/runs/`: fresh invocation directories and status manifests; never reused.
- `upstream/`: fixed source checkouts/cache under this repository only.
- Native build outputs and Wasm outputs must have different target directories.
- Tests use only synthetic data and plugin-local temporary directories.
- Never infer a successful stage from the presence of any old `dist` artifact.

## Current results and limitations

The entry suite has passed 43 tests on recorded input versions, with no skips.
Use the final `receipts/handoff.json` once published to identify the final frozen
round and its exact test/script/contract/source hashes; older receipts remain
historical evidence. The independent review found and helped repair implicit
tool downloads, Git environment/lazy-fetch behavior and archive-to-tree binding.

`out/runs/final-source-check-001/status.json` verifies all 222 fetched source
files and their Git blob identities, then intentionally exits 2 with
`source_incomplete`. Git/codeload acquisition failed; partial archives and a
truncated Codex Git-tree JSON are preserved as failed-download evidence and are
never used as full-source proof. The source lock names partial snapshots explicitly.

The actual fixed Codex client still constructs `ReqwestTransport`; the generic
`ResponsesClient<T: HttpTransport>` is a narrower real seam. `ThreadManager::new`
accepts `ThreadStore`, while the default setup starts local storage/background
work. `ExecBackend` exists, but `core/src/exec.rs` also directly spawns a child.
P-02 must replace/intercept these real paths and cover the bypass inventory before
claiming the boundary is closed. No Rust product build or upstream runtime has run.

`receipts/host-kit-review-002/review.json` independently verifies all 180 kit files,
19 vector pairs, one rejection vector, and the sole authority's schema digest.
This is byte/source review; the plugin did not rerun the host's Rust tests. The
host then suspended acceptance; `receipts/host-kit-review-002/superseded.json`
supersedes that review without erasing its byte-check evidence. The actual fixed
exec-server Cargo metadata readiness attempt failed with exit 101 because the
partial workspace lacks `codex-rs/utils/rustls-provider/src/lib.rs`. No substitute
source, mock Codex loop, or second host schema was introduced to bypass this gap.

## Continuation

Review the recorded P-00 negative cases and P-01 source evidence. Obtain complete
buildable fixed source and resolve the minimal graphs/locks. When the host exports a verified kit, record its manifest digest
and a local review bound to that digest, without modifying the host schema. Select
the smallest **real** upstream invocation point for the P-02 probe and record both
successful interception and disconnected-backend failure. Cover the full bypass
inventory before claiming exec/net/store replacement or G0 passed.
