# Matrix008 independent evidence review

No new blocker or evidence contradiction found for the four authorized producer cases on fixed host004/plugin003. All four scoped runtime outcomes are supported by original host, guest, fixture, gate and native-ledger evidence. The 124 producer checks are 30/30/32/32. This review made zero HTTP requests and is not an independent rerun. Matrix007 remains failed_expectations; no M03/product gate is upgraded.

## Inputs and authority

Verified supplied group-result, handoff and batch-manifest hashes; all 71 batch-local files and 8 external guest files; all four case manifests (64 underlying local + 8 guest entries, overlaps intentional). Fixed harness SHA 9deef213e4ea7765395fbaf5a0aede45b7cdfc7ff2d75faca662782fa3af39e1, host004 c9c043c3104573146da1fe2a12a1f009a0a9b71ceb517f8bb6fffca1a4045224 and plugin003 bcfde17f4237bf72c1c55991c42a2656349ea7586527cd7557895fc12dc4c365 match their locked bindings. All 32 plugin input pins and host004 frozen source pins match. Inputs records contain 207 hash checks including deliberate overlaps.

Reviewed the prelocked expectations and actual harness source without importing/running the harness. Independent review code reconstructed all four proposal preimages and checked each request hash, body hash and actual server POST body (22,025 bytes). Each case has one approve/claim/approve_http, one actual POST, no retry/revoke command and no Authorization header. The redirect sink recorded no hit. Distinct fresh profiles, operations and grants are bound by the retained approvals and identities.

## Original runtime outcomes

| Case | HTTP/Core and classification | Native result |
| --- | --- | --- |
| s503 | Actual 503; Core Failed with 503; 59 bytes entirely error-consumed; Observed + HTTP EOF + material | Original aggregate/close/finalcontrol Unknown; reason19; host0/guest2 |
| r307 | Actual 307; Core Failed with 307; sink0; 59 bytes entirely error-consumed; Observed + HTTP EOF + material | Original aggregate/close/finalcontrol Unknown; reason19; host0/guest2 |
| exact | All five offsets 65,536 before EOF; after actual EOF: Observed + material, error0; Core Completed, drain Ok | Host0/guest0; clean aggregate |
| over | Same 65,536 prefix, then server flushes one extra body byte; network Limit, original and final reason21; five offsets remain 65,536; Unknown, no HTTP EOF/material; earlier Core Completed retained, drain Err Cancelled | Host0/guest2; aggregate Unknown retained |

Both cap cases classify 1,024 bytes parser-yielded and 64,512 drain-discarded. This is the observed classification for this framing, not a universal parser-size promise. Negative cases remain failures in their business/aggregate dimensions; passing means the expected negative behavior and cleanup were observed.

## EOF/extra-byte gate chronology

The gate values match a real inspect_http operator reply in host-rows and an entry in pre-eof-inspections. The harness serializes this snapshot before releasing the fixture tail. Both snapshots have all five offsets at 65,536, network Streaming, intent Unknown, no HTTP EOF, no material and no RequestClosed; worker not yet joined.

- exact: inspected gate at 4,102,659,200 ns; tail opened at 4,114,142,900 ns; flushed HTTP terminator at 4,114,206,200 ns, 11.547 ms after the gate.
- over: inspected gate at 3,961,721,500 ns; tail opened at 3,969,896,700 ns; one extra byte flushed at 3,969,972,700 ns, 8.2512 ms after the gate. Server also subsequently sends a terminator, but host correctly retains http_eof=false because quota failure preceded accepted EOF. Original network_error is Limit, then first cancellation21; later requests19 and25 retain effective21.

These timestamps share the fixture/harness monotonic origin. Host event at_us and guest elapsed_us use different origins and were not directly subtracted. Core Completed retention is in original guest events/result; pre-gate drain classification and source sequencing support its relationship to draining. The check named core_completed_before_tail_outcome itself tests only final Completed, so it is not a standalone direct timestamp assertion. The qualified cap-before-tail gate is directly observed. Terminal read-poll internals are not instrumented.

## Close, cleanup and process boundaries

Every original guest report shows Close written/acked, exact session/epoch/PID/attempt/sequence/generation/code binding, EOF + router-ended + control_protocol_clean and null independent sticky_control_failure. control_end_result is checked separately. A unique guest Close and decoded State ACK match one actual host close_ack_written of 412 bytes. Each host ACK precedes host-observed stdout/stderr EOF and exit. Original host/guest progress and cleanup offsets/HTTP/worker/pipe fields agree; guest fields about its own future exit/Released are correctly excluded. Host process exit0 is recorded by its retained Popen wait; guest exits0/2 and dual EOF are recorded by its actual host owner. This review did not independently acquire a live guest process handle.

## Independent persisted native records

Read all four native-admissions.sqlite files using mode=ro&immutable=1, with no WAL present. Verified user_version4, MRNADM04 envelope/version, bounded LZ4 output and SHA-256, then parsed protobuf fields against frozen host004 schema. DB hashes are unchanged after reading.

All four Owner records persist Released, matching PID/session/epoch, exit_observed and stdout/stderr EOF. Each has exactly one parent approval and one HTTP approval; parent is revoked(state3), original TTL10,000ms, source2, first reasons respectively19/19/25/21. HTTP approvals persist revoked(state3), send_budget1, cap65,536 and exact request/body hashes. Plugin/config/operation bindings match raw run evidence. No authority was restored or application opened. This strengthens the producer's host-final-only Release evidence. Core coordination database files were hash-checked but their durable intent/material payloads were not independently decoded here.

## Remaining qualification and suggested scope

No repeat of these four HTTP fixtures is needed to resolve a contradiction found in this review. The following are still separate acceptance work: direct terminal-poll instrumentation, real host revocation while Core output is queued/late, sustained 16KiB credit/backpressure with actual OS-pending I/O and control progress/latency, and partial-control-write fault/reap behavior. If required for the larger matrix, coordinate fresh focused cases with fixed inputs and one-shot/no-resend rules; do not infer these properties from 65,536 total bytes or clean EOF.

router-ended is terminal-event application, not OS control-thread JoinHandle completion. Fixture servers use daemon handler/serve threads and shutdown/server_close; explicit fixture-thread joins are not recorded. The completed server events and application cleanup support these scoped outcomes but do not prove every fixture thread was joined. No whole-product, M03 gate, real-provider or untested-platform success is claimed.
