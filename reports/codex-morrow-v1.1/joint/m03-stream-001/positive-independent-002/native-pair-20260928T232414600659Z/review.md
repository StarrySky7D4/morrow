# Independent positive002: one real synthetic Core/HTTP run passed

Fixed host004 c9c043c3104573146da1fe2a12a1f009a0a9b71ceb517f8bb6fffca1a4045224 paired with fixed plugin002 f59665f639f3f6f9157111e4811cccc32569ec47cc63850fe936c13dddd5d32e. Exactly one fresh authorized batch/profile/cwd/operation/grant was executed; no retry. The previous positive001 failed batch remains unchanged.

## Independent observations

Exactly1 POST of22025 actual Core-prepared body bytes. Before explicit HTTP approval, the harness reconstructed the entire proposal-domain preimage, including session/epoch/operation/attempt/method/target/raw headers/cap/body. SHA256:1e5e70dd2c3b85c4505b886cabcfad6a597b3dfb77419e9bb9d53ae7148ad9f4. The approved body, actual Core preparation hash and server-captured body agree. There was no server request before approval.

The fixture only released completion/EOF after parsing a flushed real Core OutputTextDelta record and checking its19-byte content hash. That observation preceded HTTP EOF by30.5845ms on the harness monotonic clock. Core produced4 events including Completed(end_turn=null). The fixture appended a14-byte transport tail; HTTP response length281 equals267 parser bytes+14 drain bytes. Received/reserved/issued/OS-completed/peer-consumed offsets and guest received/delivered/acknowledged offsets all equal281. Error/cancel-discard classifications are0.

Core terminal, transport drain, final Close/control and cleanup all meet the strict positive assertions. Host final error_code is0; durable response material and Observed remain recorded. Normal Close applies exactly one durable owner/source2 reason25 decision. No false external_revocation_applied or extra control Revoke was observed.

Joint held host PID23724 through its Popen process handle and guest PID12520 through an independently opened handle from this run's claim reply. Guest initial wait was258(alive), then signaled with exit0; host OS exit0. Both stdio EOFs, network worker join, data operation reap/close, RequestClosed and guest data-thread join were verified separately. The owned fixture server was shut down and joined. No event overflow or input change occurred.

After process exit, an immutable read-only SQLite check independently verified ledger user_version4 and the bounded LZ4/SHA256 record envelopes: persisted owner Released/PID12520/session/epoch and all three exit/EOF flags; parent state3/source2/reason25/originalTTL10000; HTTP grant revoked with send_budget1 and the approved request/body hashes. The ledger file hash was unchanged by that read. No disk state was restored as live authority.

## Scope and preserved limits

Handshake6000ms remained within the original approval-based TTL10000ms; no deadline renewal. The controlled server is literal127.0.0.1 only. Fresh plugin evidence lives under the explicitly authorized out/m03-joint-001/positive-independent-002 directory. Script and adaptation diff, original request preimage/body, response bytes, operator commands, server events, host/guest receipts, process-handle observations and durable-ledger read result are sealed by evidence-manifest.json.

This qualifies one independent synthetic positive, including successful completed-request Close with the004 race repair. It is not the full M03 matrix or a product gate pass. Native credit/backpressure pressure cases, real-host revocation/late-event races, negative HTTP/limits/disconnects, and OS partial-write faults remain pending. A267/14 split in this run is an observed byte classification, not a guarantee for arbitrary transport chunking. No additional scenario, old component test or build was run.
