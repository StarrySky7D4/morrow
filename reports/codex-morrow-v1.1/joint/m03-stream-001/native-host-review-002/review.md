# Fixed host002: Close repair review

Disposition: H1 is addressed in the fixed source and supported by the producer's three real synthetic-peer process cases. No new blocker found in the targeted Close revision. Joint performed read-only source/evidence review, not an independent run. First host002/plugin002 HTTP pairing still requires coordinator authorization.

Manifest SHA-256: 12b20faeee5e3bd352d8ee204ba7d2d155a9b416e5f8fa3edbf922bf5367a021.
Executable SHA-256: 43cadaf0ff7dc97400e570bea5c45b30db2c04bd03e22da1e7137ef2701f3d7d.

## Source disposition

supervisor.rs now remembers Close's sequence and appends State/code0 after existing complete or partial control output, without clearing that output on the successful Close path. It closes stdin only after that exact ACK has been completely written. Original expiry, front-frame deadline and close deadline while ACK is pending record close_ack_failed and abandon success; incomplete/error writes do the same. Extra guest bytes after Close record failure even when the ACK was already written. main rejects a session carrying close_ack_failed independently of later resource release.

For code25 after RequestClosed, cancel_http preserves the previous error code, Observed and HTTP EOF and suppresses the new failure Terminal. It still revokes live authority and performs cleanup. The success path therefore does not manufacture a terminal HTTP failure merely to close an already completed request. This specific post-RequestClosed branch is inspected source, not exercised by the three peer cases below.

Resource release remains separate: actual child exit, stdout/stderr EOF and pipe/network cleanup are required before owner release; a failed protocol case can correctly have Released resources. Existing partial output is preserved while a Close ACK remains viable; a failed/expired write is dropped with a failure receipt, not counted as successfully acknowledged. OS partial-write/error injection was not performed.

## Producer process evidence reviewed

Normal synthetic peer: matching sequence2 ACK was fully written (396 bytes), then peer control EOF and actual child exit; host0/peer0, no close failure.
Wrong-sequence peer: no ACK; Stop received; host2/peer0 and resources Released.
Post-Close extra-byte peer: ACK fully written, then one extra byte caused close_ack_failed; host2/peer0 and resources Released. Thus ACK receipt alone does not erase the host's subsequent protocol failure.

These are actual producer-launched host/peer cases, not joint-held process observations. Joint checked per-case final snapshots, host rows, peer receipts and checker assertions. All have intent Absent, HTTP EOF false, worker_started false and final request_closed false: they close directly after Hello/Welcome, do not bind/transfer a request, and do not exercise HTTP/Core/plugin or the normal completed-request branch. The five earlier API tests were not rerun for this Close-only change.

The tested host copy hashes identically to the fixed host002 executable. All test source inputs match the fixed source except the declared README-only update. Inputs and selected chronology are recorded separately; no source mutation, test rerun or HTTP execution occurred during this review. H1 is no longer a static blocker to the first authorized002/002 pairing; native HTTP/Core/full matrix and OS fault qualification remain pending.
