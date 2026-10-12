# Failed-startup quarantine source candidate

This directory archives a review candidate. It does **not** activate these modules in the application, supply execution authority, or freeze the SDK. Do not add its overlay to a build without a separate review and validation decision.

The five files under `quarantine-runtime/overlay/` are exact source bytes from runtime candidate 350 revision 002. They include the ledger candidate from 347 revision 001 without modification; the supervisor module registration is extended to keep the trusted resource producer and private ledger confirmation inside one library crate. The binary exposes only the existing zero-argument process entry. No public arbitrary boolean/JSON closure-proof constructor is added.

The intended abnormal path consumes a single-byte kind-4 QuarantineClose, joins the original controller reader, checks original child exit, both EOFs and output-reader joins, stdin reap, and empty Job, then performs the original-owner CAS/commit and a separate committed-row readback before emitting an optional preparation extension. Preparation still means business Unknown, owner Retained, normal shutdown false, no replay, and abnormal process exit. The client must additionally observe the original supervisor exit and both EOFs. Normal v1/v2 Released semantics are not replaced.

## Known gaps

- An in-flight heartbeat can race the abnormal-closing flag after the loss signal and prematurely end the controller reader. This yields no preparation token and remains Unknown. It is unresolved in the archived candidate.
- Dart, runner, and outer receipt handling are not connected to this protocol. A healthy Released or ownerReleased indication must never substitute for failed-startup quarantine.
- The combined runtime candidate has not been compiled or executed. The original ledger test-selection file is retained as historical source metadata; it does not establish runtime or Windows acceptance.
- Real Windows child/Job/reader/stdin, timeout, commit/readback failure and original-process exit evidence remain required. Parser and in-memory ledger tests cannot establish those facts or namespace security.

`candidate.json` records these limits. `manifest.json` pins the archived files. The two test-selection files preserve their original bytes and historical statuses. No local input lists, private paths, credentials, process observations, runtime logs, binaries, databases, or operational scope files are included.

The archived Rust sources retain the workspace's AGPL-3.0-only provenance. They depend on the existing workspace graph and are not a standalone package. This archive introduces no third-party code or dependency changes.
