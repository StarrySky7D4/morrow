# M03 real Core consumer, revision002

Revision002 is a new source copy; frozen revision001 remains unchanged. The
native driver binds the same CancellationSignal before the operation is used.
Host cancellation updates its first reason and token synchronously, including
cancellation that happened before binding. A short shared mutex orders event
enqueue and final consumer delivery against cancellation. An event already
delivered before that boundary cannot be retracted; queued or reserved events
that lose to cancellation are suppressed. The gate never awaits or calls driver
state while locked. Existing Completed/end_turn audit facts survive cancellation.

This crate consumes the frozen integration004 Core graph as read-only path
dependencies. `request_task::start` enters actual ModelClientSession::stream;
the API/SSE/parser implementations are upstream, not reimplemented fixtures.
It uses the restricted qualification feature and no credentials/default network.

`driver::NativeHttpSession` is an in-process semantic interface, not a schema or
approval authority. The only included implementation, WireNotReady, rejects all
operations. There is no fake successful transport and no runnable HTTP mode yet.
The CLI deliberately does not start a request. A future host-owned Capnp adapter
must implement the interface and authenticate the separate data pipe.

`network` extracts Core's actual prepared bytes, waits for a separate host
approval, commits only once and exposes bounded byte chunks to the real parser.
`request_task` owns the business operation, typed events, cancellation and cleanup;
it is independent of UI. Core Completed preserves end_turn, including false from
an interrupted response. It is not a whole-turn/product success claim.

Parser detach, Core completion, user cancellation, HTTP EOF, durable Observed,
network worker exit and native-owner release remain separate facts. Post-Completed
drain consumes/ACKs only the existing operation within its original deadline and
64KiB bound. Cancel/owner Drop wins over queued events. Cleanup waiting is bounded
and cannot authorize new data. An already durable HTTP result must never regress
because parsing/IPC later failed. ACK queued is not ACK confirmed by the host.

The first observed typed completion and first cancellation reason remain in the
audit even if a later cancellation suppresses queued business events. Approval
may tighten the response cap; it cannot expand it. `wait_cleanup` is cancellation
safe: dropping one wait future does not consume the pending receipt. Request
cleanup never waits for the caller's own process exit; whole-session release is
an optional, separately observed external fact.

The future native driver must retain each pending read across waiter/parser Drop,
coalesce classified absolute ACKs, and wait for a complete final data prefix before
turning an early control terminal into byte-stream EOF. It consumes the sole host
v3 codec; those requirements are not implemented by WireNotReady.

Builds are offline with a new Cargo home/lock/target. Old source/locks/output and
all M02 kits remain unchanged. At this stage build success establishes types and
dependency integration only: no host data pipe, actual HTTP, semantic completion,
backpressure, process cleanup, no-retry runtime or product qualification is proven.
