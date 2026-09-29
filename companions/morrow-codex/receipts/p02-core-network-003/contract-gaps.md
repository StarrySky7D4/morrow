# Contract and injection boundary

The host pre-review is `reports/codex-morrow-v1.1/host/p02-core-network-003-preflight-review.md`, SHA256 `a1516facef15b8d51f5602444271fb21e9dc57973320284f4faea1d95591411d`. Its eight-input manifest and unchanged kit003/canonical schema remain authoritative. This batch does not propose a competing schema or modify host files.

M-03 still needs an authority-bound destination and HTTP method, URL, permitted request/response header, status, redirect and transmission-phase semantics. If WebSockets are exposed, upgrade outcome, frame kinds, limits, control frames and close state also need explicit semantics. M-08 governs credentials/account authority and authentication challenge/recovery. The existing ResourceRef/bytes contract alone cannot express these facts. HTTP bodies, SSE and Responses model parsing stay on the plugin side; secrets must not enter ordinary diagnostic output.

The local network backend is a Core injection point, not an implementation of that future host contract. It returns only errors. A426 fixture triggers existing Core fallback logic but is not encoded as a host003 receipt. No successful stream or opaque connection is fabricated.

The injection is selected before HTTP client construction. For WebSockets, Core still builds extra headers and telemetry and retains its timeout/error handling, then selects the injected operation before constructing/calling the API WebSocket client. Consequently the refusal probe does not cover the API client's final header merge/authentication, actual network-policy enforcement at its connector, socket upgrade or frames. Its captured headers are grouped inputs, not final wire headers.

`build_api_transport` also serves two Realtime setup callsites, but those are not invoked by this batch. `connect_websocket` is shared by the tested stream/prewarm/preconnect attempts and by reconnects, but successful cached connection reuse and reconnect paths are not tested. The ordinary preconnect generic-error arm remains untested even though preconnect426 is included. Neither default concrete backend behavior nor every ModelClient/network operation is qualified by these selected branches.
