# m02-admission-owner-002 producer delivery

Implemented a reusable native HostAuthority extension using Core Store identity/pin
and Core HostPolicy. It adds single-use host-issued approval, full immutable binding,
cross-process ownership, durable launch-pending state and conservative crash refusal.
The old native source and all old candidates remain frozen. Guest wire stays Capnp001;
native persistent records use versioned generated Protobuf + LZ4, never JSON authority.

Current producer evidence:

- `run-20260928T202924609447Z/result.json`: offline locked build plus three focused
  API tests, including eleven individual approval-field inconsistency rejections.
- `process-20260928T202950668865Z/result.json`: twelve real process scenarios, including
  two-host concurrent claim, second host during ClosingUnconfirmed, release/reacquire,
  original deadline, external revoke, old issuer rejection, copied profile rejection,
  concurrent claim/revoke and owned-host crash with a known child still alive.
- `peer-20260928T203125991615Z/result.json`: four frozen plugin-peer scenarios: complete
  capture, byte-identical old Hello, new handshake plus byte-identical old Query, and
  inherited output holder. Both replays caused host identity denial17 and host exit2;
  the test peer's own exit0 means it observed that expected denial.

The race receipt observed revoke winning the claim/revoke competition; the separate
external-active-revoke scenario covers revocation after a completed claim. The test
suite does not claim deterministic injection of every instruction-level race.
The crash case used OpenProcess only on the actual child PID obtained from this host
and held that OS handle; no process-tree enumeration or user-process termination.
The plugin holder identity is peer-reported in producer evidence, not independently
verified as an OS identity here. All test children and owners were batch-created.

README was added after the final executable build; compiled source still matches
the recorded build snapshot. Runtime kit binds both compiled inputs and final docs.
Only this new source generation may change before sealing; no old files were edited.

Preserved failed attempts: first build generated the new Cargo.lock (and source was
still changing), so its before/after identity assertion failed despite compile success;
two process harness launches failed before spawning a host because Python inherited an
incompatible PowerShell module path. A minimal Windows PowerShell module path fixed
that setup; no persistent policy was changed. First peer run incorrectly expected the
test peer to exit17; its documented success-on-observed-denial exit0 was correct. The
new peer harness asserts host exit2, denial17 and exact historical bytes instead.
Failed directories/tools are retained and are not counted as successful receipts.

Accepted scope for review is one trusted same-user local profile/slot and read-own-
session only. Persistent records never reconstruct a live approval; production UI,
formal CLI authentication/installation, arbitrary profile copies, full crash recovery,
global uniqueness, image-load races, OS sandbox/descendants and saturated in-flight
delivery remain open. M-02 remains partial; G0/P-02/J-00 are blocked, G1 not passed,
product graphs remain 0/2 and the original 84 product scenarios remain not_run.

No commit, push, release, real user-library operation, credential access or next-stage
business capability was performed. Runtime kit is a candidate for independent joint
execution, not an independent acceptance conclusion.
