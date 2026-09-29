# Host004: first durable revocation provenance

The independent positive run exposed owner Close state3 becoming visible before shared
generation2. The old poll misidentified this local write as external revoke and queued
reason19 after an error0 Close ACK snapshot.

Native ledger004 uses envelope MRNADM04 and SQLite user_version4. Existing experimental
003 profiles are rejected, not migrated or reopened as live authority. The frozen v3
Capnp contract and original Core Store/IO contracts are unchanged.

Approval.revocation_source (0 none,1 operator,2 owner runtime) and revocation_reason are
committed atomically with state3 in the existing IMMEDIATE transaction. Source1 requires
reason19; source2 requires an existing defined reason16..30. Other states require both0.
Unknown/incomplete combinations fail decoding. Parent compares its original full tuple,
normalizing only validated revocation fields for historical cleanup.

The first durable revocation result is immutable. Owner Close cannot replace operator19
that already committed. Poll applies only source1 while generation remains1. Source2 in
that same publication window is not an external event. cancel_http always reads the durable
result and only applies it once; later calls cannot overwrite error/Observed/ACK facts with
their local requested reason. A late redundant operator revoke returns the original source.
Repeated Control::Revoke during Closing also preserves the Closing owner phase.

Five deterministic regressions passed: owner commit before generation publication with
real poll; true external commit through real poll/control delivery; external commit before
Close selects19; owner Close then redundant operator preserves0 and ACK snapshot; invalid
provenance/full-tuple drift/old-profile version fail closed. Real new ledger and native pipe
thread reap/join are exercised. Completed HTTP progress is explicitly supplied test input;
there is no child, HTTP request or proof of actual response material in these tests.

First regression run retained a fixture failure: two tests marked RequestClosed before pipe
cleanup, rejected by the frozen codec. The setup now actually cancels/reaps/joins the data
thread before using RequestClosed. Final5/5 and exact-source build pass. No old component/API
suite was rerun. Commit-uncertainty fault injection is not claimed; receipts are only returned
after successful commit and uncertain errors propagate without creating send authority.

Source README describes the preceding revisions; this note is the004 delta. No new HTTP run
has occurred. Positive harness must additionally require final error_code0 and agreement with
the plugin's final control/cleanup receipt. Original successful and failed scripts stay frozen.
