# P02 batch 002: actual callsites and limits

Fixed upstream: Codex `44fe510ce3ee61c8ef623adcbf89b901c73ddd61`. Edits exist only in the new `upstream/p02-exec-store-002/codex-work` copy. Prior source snapshots, 17 P00/P01 inputs, 35 network-probe inputs, and host kit 003 are frozen.

Host assessment received before implementation: `reports/codex-morrow-v1.1/host/p02-callsite-contract-assessment-2026-09-28.md`, SHA-256 `d44d6ea3dcec7fb867047496e08fa323f137f64b45fedc273f0e0a2cbe093a32`. Execution contract ownership is M-06 (related M-02), not M-03. M-03 concerns HTTP/stream metadata. Storage durability requires M-04.

## Store

Original `codex-rs/thread-store/src/live_thread.rs:181` owns resume -> resume_thread -> load_history (when supplied history is None) -> discard on load failure. The probe calls this original function with an explicit non-local store, Legacy mode, no rollout path, no supplied history, no cwd and disabled memory. It never constructs `LocalThreadStore` or uses the Paginated downcast/DB escape hatch. Create's pre-store Git metadata collection is not covered.

Original append at line 239 passes the actual nonempty raw item vector into `ThreadStore::append_items` before any metadata write. Six cases record complete invoked method sequences, encoded Event frame identities and refusal errors. The failed-history case records one discard and verifies the original history error survives the discard Unsupported error. The append/persist/flush cases compare their complete call arrays, which exclude metadata updates.

Only a bounded FakeHost empty-history resume is permitted. No second in-memory history store is introduced. Append encodes the actual raw batch but cannot acknowledge durable storage. Standard persist and flush return Unsupported; 003 simulated durableSequence is not accepted as a barrier. Discard also returns Unsupported because 003 has no writer close/release operation. This is qualification-only, not a production ThreadStore adapter.

## Execution

Original `core/src/unified_exec/process_manager.rs:1322` selected the backend only when `environment.is_remote()` or `request.exec_server_shell_snapshot.is_some()`. Otherwise line 1413 reached `codex_sandboxing::spawn_process`. Merely replacing a private backend would therefore miss ordinary local-shaped requests.

The new small patch adds `Environment::with_injected_capabilities`, requiring explicit exec, filesystem and HTTP objects with no local or remote defaults. `is_remote()` stays false. The dispatcher checks the injected marker as an additional condition, retaining the original parameter conversion and backend invocation. A feature-gated qualification bridge invokes the original prepared-request function; it does not implement an agent loop or a replacement dispatcher.

The probe's backend records the actual ExecParams and binds their serialized digest into a 003 Tool.Propose frame. The disconnected case refuses through `Transport::exchange`; the bounded FakeHost proposal case explicitly refuses because a proposal is not StartedExecProcess. No claim, process output, PTY, or successful process handle is fabricated. Filesystem and HTTP interfaces are explicitly refusing adapters, with call counters.

The prepared-request bridge begins after approvals, sandbox transformation and higher-level tool orchestration. Those stages are not validated. This call's early-return path can establish only its local routing behavior. The independent direct-spawn path `core/src/exec.rs:959` remains uncovered, as do other code paths which access the OS directly. No process/socket/registry OS-wide monitor or isolation claim is made. The constructor is not wired into production session creation.

## Network gap from probe 001

The previous probe enters `codex_api::ResponsesClient::stream_request` directly. It does not instantiate Core ModelClient, obtain configuration/auth, enter the core session loop, prewarm, retry or exercise fallback.

In unchanged `core/src/client.rs`, the real client stores `HttpClientFactory` (line 271); line 1197 constructs concrete ReqwestTransport after route selection; line 1233 constructs/connects ResponsesWebsocketClient with that factory; line 1758 builds the Responses HTTP client; line 2157 starts WebSocket prewarm; line 648 controls session-scoped HTTP fallback. All remain unmodified and un-intercepted in batch 002. A complete integration must inject the common HTTP and WebSocket creation paths and preserve prewarm/reconnect/fallback routing, without using personal credentials or making live model requests in qualification.

## Acceptance boundary

Native callsite probes are separate qualification graphs, not the product native graph. Wasm and all 84 product cases remain not_run. No full P-02, G0 or J-00 completion, production durability, executor authority, successful model response or OS isolation is claimed. No host schema extension, commit, push, tag, release, account operation or paid request is authorized by this slice.
