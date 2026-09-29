#503: business failure and Close facts are conflated

Conclusion: this failed expectation is not evidence of a missing Close ACK or a successful503 business operation. It is a harness/API semantic mismatch, with an API reporting limitation. Preserve the batch's failed_expectations; no retroactive pass, rerun or assertion edit was performed.

Observed: guest wrote Close sequence8/code0 and accepted host State sequence8/code0(generation2). Host events57/59 record close_ack_pending/close_ack_written(412bytes,sequence8). No evidence overflow. CoreTerminal is Failed(503); all59 response bytes are error-consumed, HTTP EOF/material/Observed and RequestClosed are retained. Host exits0, guest2, resourcesReleased. One POST only;307/exact/over were not run.

Exact path: qualification/m03-stream-002/src/request_task.rs:244-252 cancels Operation with CoreError after the503 error. Native Session::cancel at903-905 does not itself set fatal; its writer emits HttpCancel sequence6. Host applies requested19/source2 once, after full response receipt. This is an expected business-error cancellation, distinct from the previously fixed normal-Close poll race.

native/m03-stream-002/src/session.rs:484-492 maps progress.error_code!=0 (even with networkEof/Observed) to fail(BridgeError::Unknown). fail at327-336 stores the first fatal. wait_close at697-703 checks close_acked AND close_written, then returns that saved fatal; final_control_status at712-720 also returns it. Thus ErrUnknown is compatible with a completed matching Close handshake under the current aggregate interface. Unknown here is a local BridgeError variant, not a durable Core intent phase; the latter remains Observed.

The immediate wrong inference is tool/m03_http_matrix_007.py:112: guest_matching_close_ack is assigned from close_result==Ok. That condition tests aggregate terminal status, not solely ACK arrival. The implementation is consistent with its002 sticky-failure design, but the field cannot serve as a standalone Close-completion fact in negative business cases.

Minimum follow-up proposal: expose immutable close_written/close_acked (ideally matching sequence) facts separately from the aggregate terminal result. Keep Core/HTTP failure and nonzero guest exit for503. If a new harness must establish control-protocol cleanliness independently, add a distinct sticky protocol/IO failure channel; do not infer its absence merely from the first aggregate ErrUnknown. The first-error-only slot could otherwise hide a later protocol error behind the earlier business cancellation. Alternatively keep existing aggregate semantics and report the handshake facts separately, without calling close_result an ACK test.

Do not simply clear/ignore fatal or make all valid ACKs succeed: that would undo F2. Any new interface semantics should be bound to a new frozen candidate and targeted tests:503 business failure + completed Close remains business-failed; business error followed by bad-sequence/replayed control and then valid ACK must still expose protocol failure. If redefining close_result as handshake-only, preserve a separate aggregate failure check in main.

No change is required to reinterpret the raw facts accurately: ACK present, business failed, cleanup completed, original harness expectation failed. This review is read-only and does not qualify additional matrix cases.
