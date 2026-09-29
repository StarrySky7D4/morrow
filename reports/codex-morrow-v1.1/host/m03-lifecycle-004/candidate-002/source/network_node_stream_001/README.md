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
