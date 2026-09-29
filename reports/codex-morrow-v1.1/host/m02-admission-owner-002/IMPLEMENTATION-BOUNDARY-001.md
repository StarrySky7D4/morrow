# m02-admission-owner-002 implementation boundary

New source: `native_session_owner_002`. All prior source/kit/tool/report bytes remain frozen.
Guest IPC continues frozen Capnp major2/revision1; ordinary client001 is unchanged.
Only read-own-session is admitted. No business executor, network or user-content write.

Existing `service_authority::Record` has only Authentication/Publication and cannot
represent native launch. This extension uses a versioned native-domain Protobuf
schema with bounded LZ4 envelope, in `profile/native-admissions.sqlite`. That file
alone is native approval/owner authority. `profile/coordination.sqlite` is a real
Core Store used only for its persistent identity and `pin_service_authority` lock;
Core HostPolicy and the established native supervisor provide lifecycle enforcement.
The envelope checksum detects corruption; trusted local profile custody is required.

One trusted local profile is one slot, conservatively one concurrent child. Core's
OS file lock uses persisted store identity, not path/PID/TTL. `init.guard` is created
atomically before initialization and is never automatically removed or recreated.
Profile metadata binds the Core identity payload and original LOCALAPPDATA namespace.
Serve requires existing files; reparse/symlink paths and remote UNC profiles reject.
Cooperating same-user hosts in this profile are the scope. Arbitrary profile copies,
hostile same-user replacement, ACL/installer hardening and global uniqueness remain gaps.

`approve` is an explicit trusted host API call, which creates non-deserializable
in-memory Admission once. Its monotonic deadline starts before image validation.
The canonical native Approval context binds profile/slot/plugin/role/operation/
grant/generation/capability1/schema/artifact plus the fixed execution config into
the wire's existing executionConfigSha field. Durable data/IDs alone cannot create
or restore live authority, including in another issuer or after restart.

Claim first holds the Core pin, then one IMMEDIATE transaction changes Approved to
Consumed and writes LaunchPending. Commit must succeed before any spawn. A second
IMMEDIATE transaction rechecks revoke/deadline and covers synchronous spawn and PID
registration, serializing that window against revoke. Revocation commits its durable
state, then local runtime acknowledgment confirms application; external revoke is
reported persisted/application-pending until the owner polls and applies it.
Unknown commits or launch/register failures retain conservative owner state/pin.
Exit plus stdout/stderr EOF is required before a Released commit then pin drop.
If a pre-spawn recheck rejects, NotStarted records the no-spawn path explicitly.

Crash before claim leaves no recoverable live approval. Crash after pending commit,
before spawn, after spawn or before registration leaves nonterminal owner state that
all subsequent hosts reject even if the OS releases its file lock. Inspection exposes
the last durable phase and known PID; no steal/clear/TTL-recovery API exists. This is
fail-closed retention, not complete crash reconciliation or child containment.

Trusted harness CLI:

- `init --profile ABS_EMPTY_DIRECTORY --slot NAME`
- `inspect --profile ABS_DIRECTORY`
- `serve --profile ABS --client ABS --sha256 HEX --work-dir EMPTY --plugin-id ID --role ROLE --operation ID`
- Serve supports the preceding native TTL/handshake/frame/close/budget/client-arg flags.
- stdin actions: approve; claim/grant_id; inspect/optional grant_id; revoke/grant_id; stop; quit.
- Serve starts as proposal only. Guest receives separate process pipes and no approval API.
- JSON is harness command/diagnostic data, never guest IPC or durable authority format.

No production approval UI or authenticated end-user CLI is claimed. Only controlled
host-side test inputs stand in for the embedding application's approval call. Runtime
kit and independent acceptance are pending until implementation and qualification finish.

Platform references: https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock
and https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex .
Locks release with host termination; the durable unconfirmed state is required separately.
