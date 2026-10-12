# Quarantine heartbeat candidate revision 003

This is an inert source delta, not a production activation or SDK freeze. The manifest binds the original five-file revision 002 archive at commit `c1c000e6b0faeeb61a2aa28540288d2d5f08139a`. Keep its four unchanged files and replace only `workbench_host/src/workbench_supervisor.rs` with this revision.

The change keeps the controller reader available if loss precedes the abnormal-closing flag or races a rejected renewal. It never renews known loss, resets the original deadline, reopens business admission, or accepts QuarantineClose before abnormal closing.

Five original parser/heartbeat logic tests passed after exact std-only source extraction with the real in-memory LeaseSignal. The full relocated runtime has not compiled or run. Original Job/child/stdin/readers/lease/ledger closure and Dart/runner integration remain unverified. A lost or failed reader is still Unknown and cannot emit prepared closure proof.

The separate Stage353 combined-library build and nine tests validate the earlier stop/audit/ledger source graph, not this runtime delta. See [bounded validation](../../windows-integrated-validation.json). Historical revision 002 metadata remains the record of its own freeze. No private paths, credentials, runtime logs, databases, binaries, VM carriers or execution authorizations are included.

License: AGPL-3.0-only, inherited from the workspace. No dependency or lock changes.
