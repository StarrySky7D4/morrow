# agent-process-control-v1

Independent experimental Cap'n Proto runtime interface for handles of actually
started processes. It does not modify the agent-session-exec-v1 revision 2 wire
or storage schemas, or Core schemas. Parsing a request, discovering capabilities or registering a process
never grants execution permission. Actual start remains inside R2's trusted
`execute_claimed` callback, protected by its durable `invocation_started` marker.

`Request` and `Reply` pin this schema's digest. Decoding is bounded, rejects
trailing data, malformed values, foreign schema identities and frames differing
from the canonical encoder. Replies correlate the exact request bytes, request
ID and generation, and check the response variant against the requested action.
Wasm clients build with `default-features = false`; no host provider or persistence
dependencies are linked into the guest.

The host registers its fresh cryptographic nonce, original connection identity,
session, operation, generation, deadline, explicit approved capability set and
the real `ProcessProvider`. Guest requests carry only that opaque handle and
generation. Every dispatch verifies the connection and generation, expiry,
revocation and a fresh trusted authorization callback. The clock is monotonic
from `Binding.created_at_ms`; rollback, expiry or lost live authorization
permanently revokes the guest binding. Discovery and replayed receipts recheck
authorization before delivery. Capability discovery is
the intersection of actual provider support and host approval. Unsupported
operations never enter the provider. In particular, `resize_pty` stays false
unless the provider implements real PTY resizing. The pinned Codex ExecProcess
trait has read/events/write/interrupt/terminate but lacks close-input and resize;
an adapter must report those unsupported unless it has an additional real API.

| Operation | Result and lifetime semantics |
| --- | --- |
| Discover | Typed supported and approved capabilities |
| Read / Events | Bounded retained event page; cursor and explicit history gap |
| Write | Accepted only means provider accepted the input request |
| CloseInput | Accepted closes subsequent guest input on this handle |
| Interrupt | Typed interrupt only; no arbitrary platform signal integers |
| Terminate | Accepted requests termination; it does not prove process exit |
| Resize | Nonzero rows/columns, explicit PTY capability required |

Output events distinguish stdout, stderr and PTY. Exit status and output EOF are
independent. Exit can be followed by buffered output and eventual `Closed`.
Unsequenced transport failures remain `OutputPage.failure`, without invented
event sequence numbers. Events pages are the bounded request/response transport
for pushed backend events; the provider must actually consume its event source.
Capability `events` must stay false for an adapter that only polls retained output.

Controls reserve an `Unknown` receipt before entering provider code. Exact
repeated requests return the saved receipt without calling the provider again;
reuse of the same ID for different bytes conflicts. An uncertain error, provider
panic, expiry or revoked authorization after the call leaves the receipt Unknown
and blocks further guest controls. Reads remain available for reconciliation while
the binding stays authorized. `EffectOutcome::Rejected` is valid only when the
provider guarantees no effect; errors with uncertain effects must return Unknown.
Input, output reservation and call budgets are charged before calls and never
refunded after failure. A replayed receipt consumes no additional budget.

Limits: 16 registrations, 128 control receipts and read calls per handle, 16
events per page, 32 KiB input or output per call, 128 KiB frame, 1 MiB cumulative
input and reserved output per handle, and 0–1000 ms wait. Lifecycle observations
cannot regress. Handles and receipts are runtime-only: restart invalidates them
all. There is no automatic recovery or replay of stdin, signals or termination.
These controls do not claim cross-restart idempotency or OS sandbox qualification.

Guest revocation and expiry block all guest calls. Trusted cleanup remains
available to the original process owner and requests termination even after
revocation. It does not forge an exit observation or an R2 execution receipt.
Providers retain ownership of timeout, process-tree cleanup, event-reader joins,
and final ExecutionFacts reporting through the existing R2 path.
`trusted_observe` allows that owner to observe terminal state after guest authority
or budget expires. `finish` requires real observed exit and EOF, drops the provider
before freeing its active slot, and retains a bounded nonce/binding tombstone to
prevent reuse. Up to 128 total bindings are admitted per Host lifetime; tombstones
are never evicted to admit a reused handle. A panic during provider teardown
retains the consumed active slot and denies further guest calls. The composed
host must recheck live authorization after reply encoding and call
`veto_delivery` for any accepted mutation whose delivery loses authority. This
marks only the exact accepted receipt Unknown and does not invoke another effect.

Tests use a deterministic provider to prove codec rejection, authorization,
no-replay/Unknown handling, cumulative budgets, input closure, lifecycle separation
and resource limits. Platform providers require their own actual-process tests;
these tests do not advertise PTY or sandbox support.
