# Native admission and owner extension, candidate 002

This is a reusable trusted host library with a controlled operator harness. The
public entry point is `authority::HostAuthority`; direct Admission/NativeHost/Session
constructors are crate-private in this version. It reuses Core HostPolicy and Core
Store's persistent-identity authority pin. Old native_session source remains frozen.

Only read-own-session is offered. Guest pipes use unchanged Capnp kit001, major2
revision1. No guest message, `approved` field or serialized grant can create approval.
HostAuthority::approve receives a trusted host decision, stores its native record,
and keeps an opaque LiveApproval with the original Instant and full record copy.
This controlled embedding API is not a production UI, authenticated CLI, or installed
plugin-registry verification. Plugin/role/operation labels originate at the trusted
host boundary; they are bound and compared, not authenticated as end-user identities.

## Storage and scope

Each profile supports one slot and one retained process owner. `coordination.sqlite`
is a real Core Store used for identity/pin only. `native-admissions.sqlite` is the
single native authority ledger: versioned Profile, Approval, Owner Protobuf messages
are generated from schemas/native_admission.proto, bounded to 8192 raw bytes, then
LZ4 enclosed using the Core envelope layout with distinct MRNADM01 magic/checksum.
The checksum detects corruption, not authorship. SQLite indexes are checked against
decoded payload; neither indexes nor successful decode are authorization.

The profile binds its original canonical root, Core identity payload SHA and canonical
LOCALAPPDATA namespace. Existing-file opens never reinitialize missing state. Atomic
create_new init.guard admits only one initializer and is never automatically removed.
Interrupted initialization stays rejected. Profile copies/relocations, divergent
ledger aliases and reparse/symlink paths are unsupported and rejected by the original
root/identity checks; remote UNC profiles are rejected on Windows. Ordinary alternate
spellings resolving to the same canonical directory share the ledger. This is a
cooperating same-user local profile boundary, not all-machine exclusivity or protection
from a user who can modify its protected state. Tests use current-user/SYSTEM-only
directories; production profile ACL verification/installation is not integrated.

## Binding and linearization

Approval calls Admission::authorize exactly once. Its monotonic deadline begins
before image/config validation. Execution config starts with the preceding native
config digest (complete LaunchSpec, OS/arch/schema and captured allowed environment).
Then SHA256 is computed over domain `Morrow/native-admission/v1\0`, that 32-byte base,
and canonical protobuf Approval with config_sha256 empty and state=1. The final digest
is stored in Approval and sent in existing executionConfigSha. This includes the
profile, slot, plugin, role, operation, random grant/issuer, persistent generation,
artifact/schema, capability1, original TTL and diagnostic approval time. The full
record is compared to the immutable live copy during claim, not just to its digest.

Core pin acquisition precedes an IMMEDIATE transaction that verifies Approved and
atomically commits Consumed plus LaunchPending. Only after successful commit does
the code consume the in-memory handle. A second IMMEDIATE transaction rechecks the
complete Consumed record and original deadline, and covers synchronous process spawn
and PID/session/epoch registration. Revoke transactions serialize against both steps:
revoke first prevents launch; spawn first allows revoke to apply afterward. No claim,
reconnect, repeated revoke or replacement record renews the original Instant.

Revocation commits state=3 before asking a local supervisor to revoke. Local ACK is
reported only after its independent control queue acknowledges. External revoke
reports persisted/application_pending, not runtime applied. The owner polls the
ledger and then acknowledges through the same supervisor; no hard-real-time bound
is claimed for polling or database contention. Live approval is never restored in
another issuer, even when it can read the original durable record.

Persisted LaunchPending/Preparing/Active/Revoked/Closing/ClosingUnconfirmed/Unknown
owners prevent all new claims. Core's OS lock becoming free is insufficient. A
Released transaction requires actual child exit AND stdout EOF AND stderr EOF,
then and only then the pin is dropped. A failed recheck before any launch can record
NotStarted. Failed/unknown commits or ambiguous spawn/register remain conservative;
there is no automatic clear, takeover, PID kill or TTL reclamation. Crash fail-closed
was tested with a real child still alive and again after it later exited; full crash
reconciliation is not implemented, and individual commit-failure windows are not
all dynamically injected. Dropping the embedding/runtime is not a release proof.

## CLI for controlled integration tests

Initialize an existing empty private directory:
`morrow-native-owner-host.exe init --profile ABS --slot native-control`

Start a proposal without approval or child:
`morrow-native-owner-host.exe serve --profile ABS --client ABS --sha256 HEX --work-dir EMPTY --plugin-id ID --role ROLE --operation ID`

Optional: --ttl-ms, --handshake-ms, --frame-ms, --close-ms, --budget, repeated
--client-arg ARG. The client receives fixed --morrow-native-session-v2 first.
Trusted stdin JSON accepts action approve; claim plus grant_id; inspect and optional
grant_id; revoke plus grant_id; stop; quit. Other fields reject. The guest gets its
own private pipes and no operator channel. `inspect --profile ABS` is an observation
entry point, not authority creation or reconciliation. Inspect opens the existing
ledger and queries records; it does not test OS process liveness.

operator_result has action/ok/result or error. authority_observation contains
authorize_created/claim_committed/owner_registered/approval_revoked/
external_revocation_applied/owner_phase/released. These at_us values start at authority
open; native host_observation at_us values start at original admission. They must not
be directly compared as one clock origin. The final snapshot reports independent
exit/EOF observations and the durable owner status. Frame bytes are test evidence,
not credentials; keep them in the bounded private evidence directories.

Exit 0 means controlled closure, not business success. Rejected operator operations,
protocol failure and expiry yield 2 after applicable cleanup; a negative request may
be inspected before quit. A test peer can itself exit 0 for correctly observed
identity denial while the host exits 2. Unconfirmed owner never becomes a success.

## Remaining limits

No production approval UI/CLI authentication, trusted package installation, arbitrary
profile migration, global owner, complete crash recovery, OS child sandbox, inherited
handle containment, hash-to-image-load race closure, executable dependency proof,
Linux/device acceptance or business backend. In-flight partially written response
cancellation, saturated control/output paths and blocked operator stdout remain
unqualified. Original 001 product gates and 84 not_run items are not upgraded here.
