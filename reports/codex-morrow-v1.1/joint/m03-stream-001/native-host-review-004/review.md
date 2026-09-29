# Host004 targeted durable-revocation review

Disposition: no new definite blocker found in the reviewed race repair. The source addresses the independently observed owner-Close/external-poll misclassification, and the producer's five deterministic tests support its decision paths locally. This is a limited source/evidence approval, not a new HTTP pass or an independent rerun.

Fixed manifest: a1c46e448450f1066177e8f51f655f202757517f1cdb83451e7545ac68bbf6f2.
Fixed executable: c9c043c3104573146da1fe2a12a1f009a0a9b71ceb517f8bb6fffca1a4045224.

## Decision-path review

Parent::revoke_native and operator HostAuthority::revoke both use an IMMEDIATE native-ledger transaction. record_revocation writes state3/source/reason together only when not already revoked; the receipt is returned after commit. Once committed, callers reload and retain that first result. Parent validates the complete original grant/owner tuple, normalizing only validated revocation fields for history.

HostAuthority::poll now supplements only persisted source1/operator revocation while shared generation remains1. Thus source2/owner state3 during the original publication window cannot enqueue an external Revoke. Http::cancel_http uses the durable receipt.reason, not its requested argument, and returns without rewriting terminal facts on repeated application. An operator19 committed before Close25 therefore stays19; owner25 committed first remains25 on later operator requests. The Control::Revoke branch no longer changes owner phase from Closing to Revoked during closing.

Source/reason validation rejects absent/unknown/inconsistent provenance; operator source requires19, owner source requires a defined16..30 reason, other states require zero provenance. Native ledger envelope MRNADM04 and user_version4 reject old profiles. Frozen v3 wire and existing Core contracts are unchanged.

## Five producer tests reviewed

1. Owner state3 commit with shared generation still1 -> actual poll: no queued Revoke; later cancellation retains error0.
2. External operator commit -> actual poll and control delivery -> cancel_http19: error19 and gate revoked, no repeated application.
3. External commit before scheduled Close: durable19 wins over requested25; later calls preserve it.
4. Owner Close first, then late operator: first owner25 and error0 persist; ACK progress snapshot remains equal.
5. Invalid provenance, original tuple drift and old SQLite version fail closed.

The tests call actual HostAuthority::poll and Http::cancel_http against fresh ledgers. Their setup really cancels/reaps/joins a native pipe thread before setting RequestClosed. HTTP EOF/Observed/material-present are explicitly synthetic state inputs, not actual HTTP/Core evidence. The real supervisor event loop's Closing-phase guard is reviewed source; these unit tests invoke the cancellation method directly rather than running a real child through that loop. No commit-uncertainty or OS partial-write fault was injected.

Reviewed log:5 passed,0 failed,8 filtered out. Test/build before-after source maps match the fixed004 source exactly. The test receipt's generic artifact field still points at the preexisting003 host executable because cargo test --lib did not build the host binary; it is not used to bind004. Its stderr identifies the newly compiled library test executable. The subsequent exact-source build binds the fixed004 host hash separately. No claim of independent rebuilding or test execution is made.

Earlier failed setup and the independent positive001 failed batch remain untouched. No new HTTP, child, build or component suite was run by Joint during this review. The next authorized fresh positive must still require final error0, agreement with guest final control/cleanup, and all byte/resource facts; it must not relax the failed assertion. Broader negative/runtime gates remain pending.
