# Bound auxiliary archive on Windows

Source `5747c341ec49c0c089c9bcc0729899b866bba669de8abcd3efc67cd222fb4ee3` passed 44 actual Rust host methods (31 draft and 13 handoff,
zero ignored or filtered) and four independent native Windows clients.

Version 2 archive markers bind the exact original closed import, abandon-decision
and parent-handoff journals by identity, encoded-card length and digest. Explicit
archive transfers their accounting out of the original 256-per-kind live quotas
into a shared 4096-auxiliary archive limit; the existing 64 MiB archive budget
now charges their encoded current cards too. Original journals, cancellation
outcomes, operation reservations and lineage fences are retained. Pending or
conflicting auxiliary outcomes cannot be archived, and new decisions against an
archived draft are rejected. No original outcome is synthesized or replayed.

Actual boundary cases cover 256 import journals with their main histories,
256 cancelled abandon decisions, and 256 cancelled proposals. Admissions fail
at the relevant boundary; explicit archive then permits a new independent
admission. Restart retains all original cancellations and rejects each of the
256 cancelled handoff operations. The import case exercises the joint main
and import limit, not an independently isolated import-only capacity test.
Another test rejects a conflicted original decision until its explicit cancel.

Three native clients exercise formal reselection/Save, parent history/archive
and independent restart. The selected export contains the two original closed
import/proposal journals; their canonical encoded bytes, lengths and SHA-256
are verified. This user-selected JSON file is an external history export;
the internal request and completion remain Cap'n Proto.

A fourth current client reads a private byte-identical snapshot of the original
successful version 1 archive. Exclusive read-only handles pinned the input
snapshot; the original library hashes remain unchanged. It submits no business
operation and does not replay the original owner. Version 1 remains readable,
but receives no retroactive auxiliary quota credit: safe maintenance/migration
of old version 1 accounting is still open.

Evidence: `D:\build_cache\_audit\windows-restriction-lineage-20261022-019` / `p3-auxiliary-archive-summary.json`,
`p3-archive-host-002`, `p3-committed-history-windows-004`, and
`p3-archive-legacy-windows-001`. Earlier results retain their own source pins.

The archive quota bounds current encoded cards and markers, not the whole
append-only database or transaction history. Exhaustion of the global 4096
auxiliary/64 MiB archive limits and the full combined fault matrix are not
qualified by this run. Framework input in real Windows windows is not physical
input, picker or pixel acceptance. Full P2/P3, SDK26, G04, SDK_FREEZE, source
acceptance and release eligibility remain open/false.
