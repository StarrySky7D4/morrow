# Independent positive001: failed terminal consistency, no retry

One authorized synthetic loopback run used fixed host003 (3341dbbe55c271cecfe2c69261374c1623c4ce65c2f96be3bad16b134aff5e65) and plugin002 (f59665f639f3f6f9157111e4811cccc32569ec47cc63850fe936c13dddd5d32e). Original result.json remains failed and unmodified. No second run or negative scenario was started.

Actual failure: final host progress.error_code is19, despite normal completed-request Close initially preserving0. Both processes exited0; those exit codes do not satisfy the independent terminal-consistency assertion. The harness stopped verification at its error_code==0 assertion. post-run-verification.json separately records the remaining read-only checks; it does not change the failed run into a pass.

## Observed ordering (host approval-clock microseconds)

- ordinal53,3229152: session_close_reason25.
- ordinal54,3250007: http_cancel_applied25; Observed/EOF/RequestClosed, error_code0.
- ordinal56,3250146: close_ack_pending(sequence6).
- ordinal57,3250196: another http_cancel_applied19; error_code changes to19.
- ordinal58,3250221: control_ack(action=revoke,generation2).
- ordinal60,3250834: matching Close ACK written.
- ordinal63,3266383: actual guest exit0; then ownerReleased.

The authority observation stream also contains external_revocation_applied. Operator commands contain only approve, claim, inspect_http and approve_http. No harness revoke or stop was sent. Guest frame observations contain one normal Close sequence6 and no guest HttpCancel. Guest final close_result and final_control_result are Ok; host's queued Close ACK reflects the earlier snapshot.

## Source-linked explanation (inference, not an instrumented race trace)

Fixed host003 source/src/authority.rs:733-735 treats persisted parent state3 plus shared generation1 as external revocation and sends Session::revoke. supervisor.rs:277-310 cancel_http persists parent revocation before publishing shared generation2. During normal Close the poll can observe this intermediate state and enqueue Revoke. supervisor.rs:878 processes that Revoke via cancel_http19, whose completed-close preservation applies only to code25, overwriting the terminal error. The observed second cancel/control ACK/external_revocation_applied sequence is consistent with this interleaving. Exact internal read/write scheduling was not instrumented.

Do not suppress19 or loosen acceptance. Host must distinguish its own closing revocation from external intent, preserve stable terminal facts, and test the intermediate poll/Close ordering deterministically in a new frozen candidate. Coordinator has assigned that repair; this batch is preserved.

## Facts that remain established

Exactly one POST with22025 actual Core-prepared body bytes. The complete original request preimage was independently reconstructed before explicit HTTP approval; its digest, body digest and server-captured body match. Server saw the flushed real Core19-byte delta30.7274ms before EOF. HTTP response totals281=267 parser+14 drain; all received/reserved/issued/OS-completed/peer-consumed offsets agree. CoreCompleted(end_turn=null), transport drainOk, durable-material/Observed and RequestClosed are recorded; no event overflow.

Joint held the actual host Popen handle (PID14560) and independently opened the known claimed guest handle(PID8360), recorded its creation time/alive wait, then observed signaled exit0. Both guest stdio EOFs and joined pipe/network resources are in host records; the persisted-owner inspection says Released. Fixture server was shut down and joined. Guest result independently reports final ACK/controlOk and local data thread joined. All pinned input hashes remained unchanged. This establishes completed delivery and cleanup for the run, but overall positive qualification is failed due to terminal inconsistency.

Fresh batch/profile/cwd/operation/grant and plugin out/m03-joint-001 evidence directory were used. Handshake6000ms stayed inside original approval TTL10000ms with no renewal. Script adaptation and exact script hash are retained; only this synthetic loopback was contacted. No build, component suite, real account or user-data operation was performed. Full M03/product gates remain unchanged.
