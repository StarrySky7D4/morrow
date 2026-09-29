# M03 native003: terminal control observations

Only native/m03-stream-003 changes. It references frozen qualification/m03-stream-002 unchanged. All 001/002 sources, candidates, receipts and the failed s503 run remain frozen. No host, guest native executable or HTTP fixture is launched by this revision's local build/test tools.

The previous wait_close and final_control_status aggregate-error semantics remain: a matching ACK cannot clear an earlier failure, including transport/cancellation Unknown. Core 503 stays Failed, process success conditions are not relaxed, and durable Observed is a separate fact. The new main success check additionally requires confirmed, clean terminal control observation.

## Observations

close_observation snapshots one native-state lock and includes close_written, close_acked, matching ACK identity/sequence/generation/code, aggregate_error, an independently sticky control failure with its source, control_eof, control_stream_ended, control_protocol_clean, and the last validated HTTP/native progress. ACK metadata is captured only after admission, response matching and progress validation. It does not expose the admission nonce.

Control validation failures, writer failures, framing/decode errors, partial reads/timeouts, reader disconnection and unexpected EOF latch the independent control failure even when aggregate_error already holds transport Unknown. A normal frame-boundary EOF after a matched Close ACK is expected. Partial/decode failures after RequestClosed or after Close ACK are never treated as normal EOF. The independent latch preserves the first control error; the existing aggregate latch still preserves the first aggregate error.

The router applies queued frames in order before recording a terminal reader event. A clean control verdict requires Close written, matching ACK, a consumed frame-boundary EOF, and no independently recorded control failure. control_stream_ended means the router applied its terminal event, not an OS thread JoinHandle proof; partial timeout or abnormal reader end is always unclean. No observation claims the guest's own exit, stderr/stdout EOF, host release or host ledger state.

Main waits for this additional terminal observation inside the SAME existing 500ms Close budget using one absolute timeout; it does not extend the native authority TTL. The result is written after this bounded observation whether the aggregate wait succeeded or failed. control_end_result=Ok only means the router terminal event was observed; inspect control_protocol_clean and sticky_control_failure for validity. ACK alone or an empty failure slot while the stream is still open does not establish a clean control conclusion.

## Local validation scope

Retain all native002 tests, including bad sequence after RequestClosed followed by a valid Close ACK remaining failed. Added synthetic state/codec tests cover transport error19/Unknown plus clean cancel/ACK/EOF; transport Unknown followed by a bad sequence and valid ACK; a pending terminal observer receiving a trailing bad frame after ACK before EOF; control I/O Unknown kept distinct by source; and partial/decode terminal errors after RequestClosed/ACK. Reader tests distinguish boundary EOF from incomplete prefix/payload, invalid framing, decode error and ordinary I/O error. Tests do not invoke real host, native pipe or HTTP traffic.

The isolated offline runner reuses the existing mutable compilation cache only. Final executable and receipt hashes are copied to a new native003 candidate path. No profile optimization or startup timeout change is included. Local compilation/tests do not qualify the new executable for OS/HTTP runtime behavior; the old failed s503 result is not regraded.
