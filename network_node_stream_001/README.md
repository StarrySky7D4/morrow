# Experimental outbound stream transport 001

Versioned extraction of `network_node/src/client.rs`; original inputs and hashes are under
`provenance/`. It contains no server, native authority, approval UI or IPC. A trusted caller
must perform its durable one-shot dispatch claim before calling this transport.

`Client::send_stream(RawHttpRequest, SendContext)` validates the same exact origin/method,
framing headers, limits, DNS address set and actual remote address as the origin client.
Its fresh connector retains no_proxy, no redirects/retries or transparent decompression.
Raw request/response header values and duplicate entries are preserved; the legacy text
`send` response conversion fails on non-text values. A later adapter into Core's String
request representation must reject values that cannot round-trip before its dispatch claim.
This trusted transport still permits synthetic Authorization used by inherited tests; the
M03 no-credential approval policy belongs to the not-yet-built native adapter.

The absolute `SendContext` deadline is capped by this client's per-call timeout at entry,
and is never restarted after headers. Optional `StreamGuard` is checked across stages and
at 5ms intervals during waits. A guard is a trusted synchronous callback, not a guest grant.
Cancellation does not retract HTTP or OS bytes and does not implement the future native
owner's SendTicket/write fence.

`StreamLease` retains the worker, token, deadline, guard and semaphore permit. The worker
returns its permit inside its JoinHandle result: headers and even body EOF do not release
the slot before `finish()`/drop. `next_chunk()` is demand-driven and cancellation-safe;
dropping its future preserves the outstanding receiver. It yields at most 8KiB, and does
not poll body again without demand. A single reqwest Bytes may be larger and remains bounded
by the full response budget; its backing allocation and kernel/library buffers are not
represented by an IPC queue/credit bound. `collect` calls this exact streaming path.

`finish()` joins, cancelling if the stream has not completed. `cancel_and_wait()` explicitly
cancels and joins. Drop signals cancellation and detaches the still-owning worker; it provides
no join receipt. Callers needing proof must await cleanup. `Completion.worker_joined` refers
only to this layer's worker, not private reqwest/hyper tasks or remote rollback.
`delivered_bytes` counts bytes handed to the per-demand oneshot, not OS or parser consumption.
No complete response material, Core Observed or owner Released is asserted by this crate.

Producer validation uses real loopback sockets: the server cannot send EOF until the test
actually consumes the first chunk. The six stream tests cover headers/EOF permit retention,
raw request/response bytes, no-demand original deadline expiry, guard revocation, body-read
cancel, cancellation-safe demand, size limits, and head-future/lease drop cleanup. Ten adapted
origin client tests retain destination, framing, no-retry/redirect, method/status and limit
checks. They are transport tests; native pipe OS Pending, real Core/SSE consumption and
independent joint acceptance remain unrun. No public network or account credentials used.


## W12 bounded SSE framing

`SseLease::new(StreamLease, DecoderLimits).await` adds strict UTF-8 SSE framing
without changing the legacy HTTP send or public guest contracts. `next_event()`
yields one event and retains the unparsed chunk tail; cancelled reads keep decoder
and original demand state. Buffered delivery rechecks the original guard, token
and absolute deadline. `retry` and event IDs are inert metadata; no reconnection,
replay, grant renewal or provider-specific `[DONE]` convention is implemented.

Only HTTP200 with a single `text/event-stream` Content-Type (optionally one UTF-8
charset) and absent/single identity encoding is accepted. Open failures cancel and
join, returning a real cleanup receipt. `finish()` never drains buffered input and
keeps the first parser/delivery cause separate from transport cleanup. Its decoder,
HTTP EOF, truncated and worker-joined fields describe separate facts, not remote
business success. EOF never dispatches a partial block; invalid UTF-8 is an error.

The default decoder has 8KiB line, 64KiB block, 4MiB total, 4096-event, 1024-byte ID
and 20-digit u64 retry bounds. A CR-following LF counts only against total, so block
is not a complete wire-byte cap. The underlying response limit remains independent.
There is one <=8KiB delivered chunk plus decoder state; shared Bytes backing and
network-library/kernel buffers are not covered by that chunk-view bound.

Windows Release/offline/locked validation ran 55 methods: 21 new decoder, 13 new
real loopback SSE and 21 inherited transport cases. See
[the scoped W12 report](../reports/reconstruction-2026-10-03/windows-sse-transport.md).
This is trusted transport qualification; channel/guest authorization integration,
TLS, public endpoints, other platforms and full SDK freeze remain open. Framing
follows the [HTML event-stream rules](https://html.spec.whatwg.org/multipage/server-sent-events.html#event-stream-interpretation)
with the bounded strict-UTF-8 policy described above, not browser EventSource API semantics.

## W14 opt-in original-host managed source

The separate `managed-channel` native feature now provides `managed_sse::SseSource`:
immutable explicit host approval, the original Store's strict one-shot claim before
POST, complete versioned Capnp event payloads on the old channel ABI, exact durable
ACK before the next SSE event, and source-specific revoke/deadline checks through
the original final ACK guard. A channel declaration never grants HTTP authority;
old incompatible guest import combinations remain rejected.

The earlier transport-only and W12 qualifications above keep their historical
scope. The new bridge rejects credential/cookie/API-key/Last-Event-ID injection,
does not replay/reconnect, and keeps the streaming IO intent OutcomeUnknown even
on clean HTTP EOF. HTTP worker and native producer actual joins remain separate.
`send_stream_with_receipt` exposes NoWorker or real opening cleanup evidence;
the old `send_stream` keeps its Error API and original path.

Windows Release/offline/locked tests: 72 stream/codec/bridge methods and 62 original
channel/executor/frozen compatibility methods passed, zero ignored/filtered and
zero new TEMP files. NEW Rust/C/C++ channel guests were actually executed from
unchanged SDK example source; the 9 base/3 dependency/provider original artifacts
were neither rebuilt nor repacked. Two compile failures and one 48-pass/2-fail
fixture run are retained before the complete successful run. Ordinary token, TLS,
public APIs/accounts, production Workbench owner/UI, WS and other platforms/full
SDK qualification are still open.

See [the host guide](../docs/PLUGIN_SSE_CHANNEL.md) and
[the W14 scoped report](../reports/reconstruction-2026-10-03/windows-sse-channel.md).
