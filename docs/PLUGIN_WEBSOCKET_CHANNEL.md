# Experimental host-approved WebSocket channel

`network_node_stream_001` provides the opt-in `managed-websocket` native feature.
It uses a trusted, explicitly approved `managed_ws::WsSource` and the existing
channel Receive/ACK/Send wire contract. The public IO `WebSocketConnect` submission
remains Unsupported. No socket permission is inferred from a channel declaration.
Production Workbench channel/owner binding and network discovery remain unavailable.

## Host approval and one attempt

The original Manager, HostRuntime, ManagedInstance and ChannelBroker must match;
package digest, revision, operation ID, approval epoch, target, budgets and absolute
deadline are bound before the source starts. A once-issued SourceGrant binds that
same owner. The original Store retains/reserves and strictly claims the operation
before opening the socket. Repeated start or a competing claim does not reconnect.

The request is GET with no body and an exact approved ws/wss endpoint. PublicWss,
LoopbackWs and LoopbackWss are separate policy selections; their existence is not
TLS or public-provider qualification. Credentials, cookies, auth/API-key headers,
redirects, compression, retry and reconnect are not provided by this adapter.
Reqwest performs the policy-checked opening; pinned tungstenite handles RFC6455.

Text, binary, Ping, Pong and Close use the separate `ws_message.capnp` envelope.
Its exact digest/version and bounded metadata are verified independently of the
old channel schema; old SDK contracts/imports/ABI and frozen originals are unchanged.
Text and Close reason must be UTF-8. Control messages and Close codes keep the
backend's protocol constraints. No payload or URL is printed by Debug implementations.

## Flow and terminal facts

- Incoming envelope bytes use the original finite channel frame and exact durable
  ACK. A frame is not consumed merely because socket bytes or an envelope arrived.
- The native pending queue has byte/message bounds and reserves capacity before
  requesting another message. A full queue pauses reads. Outgoing writes and
  already-parsed control replies can progress; controls behind unread data cannot
  bypass TCP ordering.
- Accepted is local channel send admission. `outgoing_written` counts actual
  successful backend flushes. Neither is remote business completion.
- Original context cancellation, lease-local cancellation, source-only revocation,
  owner stop and the immutable absolute deadline continue to guard delivery and
  opening/write/flush waits. Cancellation does not retract remote bytes or effects.
- Dropping an opening future or lease signals cancellation. Drop provides no join
  receipt. `cancel_and_wait`/`finish` are required to obtain a worker receipt.
- Peer Close, transport EOF, exact ACK, worker task join, producer callback return
  and broker OS-thread `CleanupProof::Joined` are separate facts. `finish()` does
  not drain unrequested input; calling it before completion cancels the worker.
- The retained business operation stays OutcomeUnknown even after clean socket
  close and both worker/producer cleanup. No Observed outcome or automatic replay
  is fabricated; reopening a Store does not restore the old grant.

## Reproducible new qualification guests

The W15 checkpoint's test expected six external artifact paths and SHA-256 values,
but did not contain their fixture source. The source under
`network_node_stream_001/tests/fixtures/ws_guest/` and
`tool/prepare_ws_channel_fixtures.py` now prepares NEW Rust/C/C++ projects in a fresh
external directory. It refuses existing output, repository-contained output,
parent traversal and symlink ancestors. It does not compile, run or reseal anything.

Each new guest uses the unchanged public SDK to send text, binary and Close,
receive the corresponding complete envelopes, verify exact bytes, ACK each frame
while its borrowed fields remain valid, and await the original channel terminal.
The 64-byte WSV1 summary includes the transcript hash and wire reclamation boolean;
the native test separately requires actual worker and broker join evidence.
The small independent envelope layout is checked against native Capnp encoding.

Build each prepared project with the existing `morrow_plugin.py build` and its
SDK lock, then package its actual module with Core's `plugin_package pack-v2` and
the prepared manifest arguments. Supply all six paths as
`MORROW_W15_WS_{RUST,C,CPP}_{WASM,PACKAGE}` and each matching `_SHA256` value to
`tests/managed_ws.rs`. Missing files or pins fail; the test does not silently skip.
These NEW artifacts are not the old SDK014 guests or the frozen compatibility
originals. Compiling and packaging are not execution results.

See [current cloud validation and capability status](../reports/reconstruction-2026-10-04/cloud-websocket-validation.md).
