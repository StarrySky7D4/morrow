# Plugin native002: targeted repair review

Disposition: F1 and F2 from native-driver-review-001 are addressed in the reviewed source and supported by the producer's targeted local test evidence. No new blocker found in these two repaired seams. This is not independent test execution or real-host reproduction. Pairing remains on hold for fixed host002 and explicit runtime authorization; host001 Close mismatch H1 remains applicable to that frozen candidate.

Candidate receipt SHA-256: 20135ba780b86a344eeb396fd8d8f0ba85ee3600f79308140380c3fc94fbf561.
Executable SHA-256: f59665f639f3f6f9157111e4811cccc32569ec47cc63850fe936c13dddd5d32e.

## F1 disposition

qualification/m03-stream-002/src/driver.rs introduces a shared CancellationSignal with first-reason mutex and wake token. cancel and with_live use the same mutex. Operation binds the signal before use. Native Session::bind_cancellation takes its state lock and immediately replays any pre-binding reason; cancel_for closes the signal synchronously under that state lock. The router uses handle_control, which keeps application and error cancellation in the same locked path.

Core forwarding now awaits queue capacity with reserve, then sends only through Operation::deliver_if_live. RequestTask::next_event checks that same fence after receiving. Current callbacks contain only synchronous queue send or returning the event, with no driver calls or await while holding the fence. Local cancel releases the fence before calling back into the driver. Thus queued and reserved events are suppressed when cancellation wins the shared delivery decision; an event whose decision already won cannot be retracted. Previously observed Completed(end_turn=false) remains in the audit. Deadline checking remains in the operation/consumer flow; original host authority is unchanged.

The new Core test calls the real RequestTask::next_event on a synthetic queued ResponseEvent and a reserved late event; it does not obtain the event from a real HTTP/SSE parser. The native test decodes synthetic Stop/Denied/revoke frames, runs the real Session handler with a real Operation, and checks immediate token/fence closure without a subsequent read. A separate test covers cancellation before binding. Together these support the two halves of the seam locally, not a real-host end-to-end race reproduction.

## F2 disposition

native/m03-stream-002/src/session.rs wait_close requires Close write and ACK, then returns any sticky fatal; final_control_status independently checks the same terminal error and completion facts. Cleanup still proceeds. main now observes Close first, snapshots the final control result, writes result.json with close_result/final_control_result/result_snapshot, and rejects either terminal error before exit zero.

The new native negative test feeds decoded synthetic RequestClosed, wrong-sequence State, then valid matching Close ACK through the actual handler. It verifies that the ACK is consumed, wait_close and final_control_status both fail, and prior Observed/RequestClosed facts remain. It marks local Close write synthetically; it does not execute the CLI, actual pipe write or HTTP cleanup. This addresses the reported saved-fatal-before-valid-ACK path; it is not an exhaustive control-stream fault test.

## Bound evidence

Producer Core log: four tests passed, zero failed; native log: nine passed, zero failed. Four new tests across these suites directly address F1/F2; the other nine are retained candidate-local tests. Joint did not rerun them. Build and both test receipts report exit0 and unchanged before/after inputs. Current non-README inputs match those receipts; only the two declared READMEs differ between test and final build. Compiler JSON identifies the002 native source and one binary artifact plus907 unique package IDs. This is producer build association plus fixed artifact hashes, not an independent rebuild.

inputs.json pins the31 candidate files,002 receipt and reused receipts, and rechecks the prior31 reviewed001 input paths. The original001 review and this002 disposition remain separate. No host/client/HTTP was launched, no source was modified, and no old component suite was rerun.

Remaining runtime evidence for these findings: actual parsed/queued Core output across a host revoke fence with no read-dependent cancellation, plus actual Close write/ACK and a late invalid control followed by valid ACK retaining failure. First loopback success, backpressure, final offsets/ACKs, deadline and external owner release remain unqualified. Full M03 matrix remains not_run and no product success is claimed.
