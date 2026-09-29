# P02 Core network batch003 — ready for independent review

Actual native build and execution succeeded: **6 cases / 47 counted assertions**. Additional prerequisite guards reject inherited credential fields and credential headers before Core construction or recording. This is a producer result awaiting independent review, not completion of P02/G0/J00.

Each case closes a distinct previously untested actual Core branch:

| Case | Actual entry and observed backend order | Result |
| --- | --- | --- |
| HTTP direct | ModelClientSession::stream -> HTTP stream | One refused serialized Responses request; no WebSocket call |
| WebSocket disconnect | stream -> WebSocket connect | One refusal; no HTTP call and WebSocket remains enabled |
| Prewarm disconnect | prewarm_websocket -> WebSocket connect | One refusal; no successful prewarm connection/frame |
| stream426 fallback | stream -> WS426 -> HTTP refusal; same ModelClient's new_session -> HTTP refusal | Shared Core state remains HTTP-only |
| prewarm426 fallback | prewarm_websocket -> WS426 -> setup-only Ok; subsequent stream -> HTTP refusal | Original prewarm fallback arm, no network success |
| preconnect426 fallback | preconnect_websocket -> WS426 -> setup-only Ok; subsequent stream -> HTTP refusal | Independent preconnect fallback arm, no network success |

Every426 in this report has **local_injected_error** provenance (`synthetic_426` in runtime records). It is not a real server handshake or host003 status. The bridge does not call fallback helpers or write disable_websockets; it invokes original Core entrypoints and observes `responses_websocket_enabled`. No successful ResponseStream or WebSocket connection was manufactured. The two setup Ok results are normal Core control flow, not data-plane success.

The source patch contains 5 files (3 modified, 2 added), 391 diff lines, SHA256 `5daaa46c41e5711f696f623ad7a3df580e1daa6a11132bd038069cd3b329cafa`. A fresh 8699-file working tree is based on the unchanged fixed 8697-file Codex snapshot. Batch002's execution patch was not carried into this tree. Only the qualification bridge is feature-gated; the backend API and Core routing changes are not gated and remain unwired to production session startup.

The injection is an explicit ModelClient backend override, not a replacement HttpClientFactory. It covers the HTTP construction helper and WebSocket connection helper used by these six branches. Both HTTP execute and stream are dispatched explicitly to the injected backend without default-network fallback on error; only stream is exercised here. Realtime/unary execute callers are not covered. WebSocket interception retains Core header/telemetry/timeout construction but occurs before the downstream API client's final header merge/authentication, socket upgrade or frame operations.

Authentication precautions were applied before execution: isolated child home/profile/temp/Cargo paths; only individually read allowlisted OS/build environment fields are forwarded; API-key/refresh variables are absent and checked before ModelClient's unconditional auth-env telemetry; all provider authentication sources are checked empty; no AuthManager, configuration load, attestation, contributors, trace writer or telemetry exporter is initialized. Retries are Some(0). No current personal environment values were printed. The unavailable factory policy is only a network refusal backstop, not the basis for the credential claim.

Evidence:

- Lock: `runs/lock-20260928T133621Z-87ec8bb2a2/result.json`.
- Build: `runs/build-20260928T133753Z-6457db0b93/result.json`, actual elapsed333.88s, Rust1.95 Windows native, locked/offline.
- Run: `runs/run-20260928T134336Z-ee58937b98/result.json`, exit0, elapsed1.11s.
- Runtime: `runtime-20260928T134336Z-ee58937b98.json`, SHA256 `cadb20edfdd845efe21bb01ea62a5054de791af4693b8a3d5f2a899010eb49d1`.
- Executable: `out/p02-core-network-003/target/x86_64-pc-windows-msvc/debug/p02-core-network-probe.exe`, SHA256 `c9685f0394d3e559f351dc24e3ca8e80964c77937fce064bd28d8fcdbb839c70`.
- Patch: `patch-before-build-002.json` and `patch-after-build-001.json` match. Original source and shared host/fork copies remain unchanged.
- Dependencies: `resolved-graphs-001.json`, 1113 locked packages (1010 exact original-registry entries and103 local packages, no Git source fetch). Cargo/home/target are new; verified shared dependency sources/vendor are reused read-only.

Runtime HTTP records include exact UTF-8 body bytes, byte length and SHA256, plus method/URL/headers. WebSocket records contain grouped Core connection inputs and fixture outcome, not final wire headers/frames. Successful build/run logs and frozen input checks are retained. A later read-only inspection initially used Windows default GBK for the UTF-8 runtime JSON; it was retried with explicit UTF-8, without changing the artifact or rerunning the probe.

Host pre-review and contract gaps are documented in `preflight-addendum.md` and `contract-gaps.md`. M-03 HTTP/WebSocket semantics and M-08 account/credential/recovery authority remain missing; 003 was not edited or linked as a fake successful network adapter. Successful HTTP/SSE/WS, established-connection reuse/reconnect, generic preconnect failure, auth recovery, proxy/TLS/redirects, Realtime/unary, real host IPC, full agent loop and OS-wide bypass isolation remain unverified.

Product native/Wasm graphs stay0/2; 84 product cases stay not_run; P02/J00/G0 stay blocked. No personal account operation, real provider connection, paid model request, Morrow checkout write, commit, push, tag or release was performed. The new handoff binds this slice and verifies prior17/35/129 frozen inputs; review must not upgrade its claims beyond these branches.
