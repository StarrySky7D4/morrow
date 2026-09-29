# Plugin native003 targeted review

Conclusion: no new blocker found within the authorized terminal-control observation scope. Ready for coordinator review of a new layered HTTP harness. No runtime is authorized or performed by this review; old s503 failed_expectations remains unchanged.

Fixed candidate native/m03-stream-003 v0.3.2: manifest SHA-256 54f4c6cd38e1968d1dbc54d26b8bf40460ee660498f8590c7f44c10fe04396c5; executable SHA-256 bcfde17f4237bf72c1c55991c42a2656349ea7586527cd7557895fc12dc4c365 (65,700,864 bytes).

## Source conclusions

- session.rs:461-471,982-998: close_observation reads Close-written, matching ACK identity/sequence/generation/code, aggregate fatal, independent control failure, EOF/end and final progress under one state lock. ACK admission and request matching precede capture at 678-688. Later delivery validation can still reject the frame, and the independently latched failure prevents a clean verdict; matching_ack alone must not stand for protocol validity.
- session.rs:495-504,547-556,773-809: first control-origin error is latched independently of first aggregate fatal. Prior transport error19/Unknown cannot hide a later invalid sequence or control I/O Unknown. Ordinary host transport progress preserves existing aggregate failure semantics.
- workers.rs:31-66,170-207,225-226: boundary EOF, first-byte read error, incomplete prefix/payload, framing, decode, partial timeout, disconnected reader and writer encode/write/flush failures have distinct paths. All reach the control latch after admission; initial Challenge failures return admission error before a Session is exposed. Validation failures use control_validation. RequestClosed/ACK no longer converts malformed input into normal EOF.
- Router consumes its single reader channel in order. Normal EOF is clean only after matched ACK; abnormal ends set stream_ended but never control_eof/clean. After ACK the wait stays pending until router terminal application. Partial timeout ends observation as failure, not proof that the blocked reader thread joined.
- workers.rs:303-316: successful Close write/flush is recorded before the writer returns, with no subsequent writes. Close write/flush errors route to the independent latch. Thus no later writer failure follows successful Close-written in this path.
- main.rs:150-179: Close and terminal observation share one absolute 500ms deadline; no second allowance or authority renewal. Results are recorded after bounded observation. Existing aggregate checks remain and a clean terminal-control requirement is added. Session::wait_close and Session::final_control_status are byte-for-byte unchanged from native002.

## Verified producer evidence

Current hashes match all 32 candidate inputs, all 31 previous native002 inputs and both dependency intake receipts. All 9 qualification/m03-stream-002 pins match the previous candidate. Admission, delivery and lib source are unchanged. Test/build before/after input pins agree with each other and the fixed manifest. The current compiler-reported original executable hash matches the fixed copy. Build JSON records the v0.3.2 package executable and 907 unique compiler-artifact package IDs. This is a producer build chain with independent read/hash checks, not an independent rebuild.

Producer test log reports 16 passed / 0 failed, retaining 9 native002 tests and adding 7 targeted tests:

1. Transport error19 + ACK + clean EOF remains aggregate Unknown while control is clean.
2. Transport Unknown + bad sequence + valid ACK remains explicitly control-failed.
3. ACK cannot complete terminal wait before trailing bad frame/EOF.
4. Writer-origin Unknown remains distinct from transport Unknown.
5. Partial/decode failure after RequestClosed/ACK stays abnormal.
6. In-memory reader boundary EOF vs partial/framing/decode.
7. Read I/O Unknown is not EOF.

The writer-origin test injects Shared::fail_control and does not fault a real OS write. The trailing-frame test drives the state handler directly; it does not execute a live router thread. Tests/build were not rerun.

## Harness interpretation and remaining proof

Evaluate HTTP/Core result, aggregate native failure, actual matching Close handshake, clean terminal control, RequestClosed/data/network cleanup, held process exits and durable whole-session release separately. For expected 503 failure, aggregate Unknown and guest exit 2 can coexist with valid Close ACK and clean control EOF; neither Unknown nor control_end_result=Ok may be whitelisted as protocol success. Require close_written, close_acked, matching ACK binding, control_stream_ended, control_eof, null sticky_control_failure and control_protocol_clean together; independently verify host ACK bytes and process/release evidence. Runtime observation failures must remain failures even if Core failed as expected.

No OS control/pipe, HTTP request, real process exit, thread join, host release, durable ledger or full-product success is established by these local tests. Router end means terminal-event application, not thread JoinHandle completion. Native003 runtime remains unverified. No old scripts/results modified; no source edit, build, tests or executable launch performed. Initial report diff generation hit a Windows default-text-decoding error before writing the diff; using explicit UTF-8 completed this artifact only.
