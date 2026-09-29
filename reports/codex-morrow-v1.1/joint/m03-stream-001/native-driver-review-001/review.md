# Native candidate001: targeted driver/Core review

Result: two source-level findings block the corresponding cancellation and fail-closed qualifications. This is read-only static review, not a runtime reproduction. No host/client invocation, build, old suite rerun, source edit, or release operation was performed.

Reviewed plugin root: `C:/Users/Administrator/Desktop/CodeXProjext/morrow-codex`.
Candidate receipt SHA-256: `7d2dc5319e4adeda829c8c80579fc09f56b84ba68eb6d746fbc9f6700b5514e1`.
Candidate executable SHA-256: `b3dcb18ba3d516ffb80b3c395061b434548611a53157146059593edff8562dbd`.
Source positions below are one-based, relative to the plugin root. Exact reviewed Rust sources are preserved in source-snapshots.json; input checks are in inputs.json.

## F1: host cancellation does not directly suppress queued Core events

`native/m03-stream-001/src/session.rs:136-183` cancels Admission, Delivery and queued outbound work when Stop/Denied/revocation is applied. It does not notify Operation's independent token, constructed at `qualification/m03-stream-001/src/network.rs:97` and exposed at lines 113-114. That token is cancelled by Operation::cancel at lines 152-162. `qualification/m03-stream-001/src/driver.rs:126-167` exposes no host-cancellation notification or shared event-delivery fence.

`qualification/m03-stream-001/src/request_task.rs:60-67` delivers events using the Operation token, deadline and receiver. Its Core forwarding loop at lines 257-260 and 297-301 similarly knows only that token/deadline. Therefore this ordering remains possible: Core has parsed/queued an event from permitted bytes; the control router applies host revocation; before a further driver read notices cancellation, the consumer receives that queued event. Rejecting later data chunks does not close this already-parsed-event path. This is more than a simultaneous select race: the consumer token can remain unset after the Session cancellation has already completed.

Required correction: propagate host cancellation/fatal state into the operation/event-consumer seam, preserve the first cancellation reason and already observed Core terminal fact, and define the delivery/cancellation ordering. Avoid recursive cancel propagation. Keep frozen001 sources intact and bind a new candidate/seam.

Targeted remaining runtime check: arrange an actual parsed Core event waiting for a paused business consumer, apply host revocation and observe its application, then resume the consumer without requiring another driver read. Verify no new business event is delivered after that fence. Also verify cleanup and classified ACK accounting remain independently observable. The existing owner-initiated cancellation test does not cover this direction.

## F2: a saved late control protocol error can be ignored by Close success

`native/m03-stream-001/src/workers.rs:95-103` stores accept_control errors through routed.fail and continues routing. `native/m03-stream-001/src/session.rs:148-152` preserves the first fatal error. However wait(..., allow_cleanup=true) at lines 451-480 skips the fatal check, and wait_close at lines 503-508 only tests control EOF/close_acked. Cleanup permission is consequently also used as final success eligibility.

Concrete static sequence: real Core completion, HTTP EOF/drain and RequestClosed/local I/O join have completed; a codec-valid control frame with an invalid sequence or direction sets fatal; cleanup is still permitted and a valid Close ACK follows. wait_close can return Ok. `native/m03-stream-001/src/main.rs:149-163` has already snapshotted/written the result before Close and checks only the old completion flags after its ACK, allowing exit zero despite the stored protocol error.

Required correction: preserve cleanup permission while making final success conditional on terminal protocol state. Include final Close/control status in the receipt or a separately sealed final receipt; do not erase prior completion/cleanup facts. Keep001 frozen.

Targeted remaining runtime check: after RequestClosed and local I/O join, inject a codec-valid invalid sequence/direction frame, then a valid Close ACK. Require explicit retained protocol failure and nonzero completion while cleanup still finishes. This finding does not claim all truncated-frame cases pass: control EOF before Close ACK is already rejected, and normal data-lane closure checks buffered partial frames.

## Other requested seam checks

Static inspection found session-owned read queues, contiguous final-offset gating across control/data lanes, actual CLI -> request_task::start -> ModelClient session.stream wiring, classified cancel-tail accounting, normal data-close partial-frame rejection, and use of the original native deadline. These are source observations, not full native acceptance. The first loopback pairing still needs to establish actual Core delta before fixture EOF, final offset/ACK agreement, normal cleanup, Close and externally observed guest exit. The full M03 matrix remains not_run; no product or whole-session-release success is claimed.

The newly authorized plugin/out/m03-joint-001 fresh batch/case output location resolves the earlier output-scope conflict. It does not authorize starting an unREADY host or change this review's read-only execution scope.
