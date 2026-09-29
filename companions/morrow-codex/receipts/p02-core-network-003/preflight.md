# P02 network batch 003 — pre-implementation scope

Baseline is the untouched Codex `44fe510ce3ee61c8ef623adcbf89b901c73ddd61` snapshot. Work will use a fresh source copy and output directory; batch002 and host kit003 remain frozen. This batch does not carry forward the batch002 execution API patch.

Exact fixed-source callsites in `codex-rs/core/src/client.rs`:

- `ModelClient::new` line477 receives the concrete `HttpClientFactory`; `new_session` line603 retains its ModelClient clone.
- `responses_websocket_enabled` line1024 combines provider support with the session-wide disabled flag.
- `build_api_transport` line1176 constructs concrete ReqwestTransport at1197. The Responses stream path calls it at1676, then constructs `ApiResponsesClient` at1758 and calls `stream_request` at1764. Two other callers at689/741 serve Realtime setup paths and are not qualification cases here.
- `connect_websocket` line1211 builds Core headers/telemetry and calls the concrete API WebSocket client at1233. `websocket_connection` owns cache/owner/key validation and calls this connection helper. A refused connection never reaches successful frame sending.
- `stream_responses_websocket` line1835 handles handshake426 by returning FallbackToHttp; other errors are returned through provider mapping.
- `prewarm_websocket` line2157 enters the same WebSocket path with warmup=true. Handshake426 takes its own fallback arm and returns setup Ok without a stream; this must not be labeled a successful model response or prewarm exchange.
- `ModelClientSession::stream` line2218 chooses WebSocket, takes natural426 fallback, then invokes the actual HTTP path. `try_switch_fallback_transport` line2277 calls `force_http_fallback` line648, setting the shared disabled flag and clearing cached state.

Planned minimum implementation: a cloneable explicit Core network backend override, a concrete HTTP transport wrapper implementing the existing HttpTransport trait, and an injected WebSocket connection operation. Default routing remains the original concrete implementation. The override is selected before creating a real HTTP client or starting a WebSocket connection. A qualification-only bridge constructs ModelClient from a public local fixture provider, no AuthManager, no environment API-key field, disabled inference traces and no personal configuration. Its HttpClientFactory also carries an unavailable policy as an additional refusal backstop. No successful stream or WebSocket connection object will be fabricated.

Five planned actual-Core cases close distinct previously untested branches:

1. WebSocket-disabled provider -> ModelClientSession::stream -> actual Responses HTTP serialization -> refusing HTTP stream backend.
2. WebSocket-enabled provider -> stream -> connection rejection; no HTTP fallback on a generic disconnect.
3. WebSocket-enabled provider -> prewarm_websocket -> connection rejection; no successful warmup request/frame.
4. A local fixture handshake426 -> stream's natural fallback arm -> refusing HTTP backend; a subsequent new session remains HTTP-only.
5. A local fixture handshake426 -> prewarm's separate fallback arm (setup Ok, no network success) -> subsequent stream rejected by HTTP backend.

The 426 is a synthetic error at the injected connection boundary to exercise existing Core control flow, not a real HTTP exchange or host003 status receipt. Each case will record its ordered backend calls and assert the selected branch and failure propagation. No test will call a replacement loop or demonstrate only an isolated trait.

003 contract gaps: StreamOpen has only a ResourceRef destination, byte count/digest and deadline. StreamReceipt lacks HTTP method/URL/header/status/response-header semantics, redirect/auth/challenge metadata, WebSocket upgrade and frame/control/close semantics. M-03 must define these and their authority/credential handling before successful production network adaptation can be claimed. The batch will not edit003 or manufacture a successful host stream to hide these gaps.

Remaining paths even if these cases pass: successful HTTP/SSE and WebSocket frames, established-connection reconnect/cache reuse, auth ownership/recovery, actual host IPC/disconnection, redirect/proxy/TLS behavior, Realtime/unary endpoints, full Core session loop and OS-wide bypass isolation. Qualification is not product native/Wasm or complete P02/G0/J00 acceptance.
