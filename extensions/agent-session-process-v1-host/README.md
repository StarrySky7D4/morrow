# Reviewed session and process host

C19 candidate adds an explicit single-R2-import profile through
`PreparedSessionExecPackage`, `ManagedPreparedSessionExecPackage` and
`Catalog::{inspect_session_exec,install_session_exec,connect_session_exec}`.
These APIs require `morrow_agent_session_exec_v1.call`; the existing process APIs
retain their original import gate. Neither profile falls back to the other.
The complete immutable module and wrapper digest bind the profile. Existing
container, snapshot and wire bytes are unchanged; no old approval is migrated
to a different module or digest. Installation does not select, approve or enable.

The R2 connection uses the original Manager Control table, current base selection,
bounded limits, Core connection, cancellation and wrapper approval subset.
Catalog changes/Drop/uncertainty and Manager revocation stop that same connection.
`run_owned` invokes the original owner's maintenance at every import, checks actual
R2 request/reply correlation and revalidates before returning completion.
This source candidate has not been compiled or executed by its author. It does not
qualify ProtectedSession, a VM, production sandbox execution or the full SDK.

Experimental host for a Codex Wasm session layer and a trusted native execution layer.
The single optional import is `morrow_agent_session_process_v1.call`. Legacy factories
reject it; this profile rejects mixed extra imports and WASI. It forwards the original
canonical R2 session frames and independent canonical process-control frames according
to their schema identities. It does not reinterpret serialized IDs as authority.

`AgentProcessPackage` binds the full original package, both schema digests, finite
session scope, execution domain and capability ceilings in a canonical Protobuf/LZ4
container. A trusted host approves its complete digest and a subset of the declaration.
`PreparedPackage::approve` uses the original Core connection, R2 host and owner lease.

`register_process` is a trusted native boundary. The caller supplies a real provider,
the exact started `ToolIdentity`, an `Arc` of the **original** execution connection and
its original opaque admission. `SessionExecHost::validate_started_tool` checks the
original invocation marker and execution authority; historical identity, a read-only
admission or a different execution admission is insufficient. This additive read-only
R2 method changes no wire schema, persisted state transition, permit or replay rule.

Each dispatch uses fresh clock samples, checks the original execution grant and the
Wasm caller's separate reading grant before and after provider work, and checks again
after reply encoding. Loss of authority after a control effect leaves `Unknown` and
blocks further effects. A repeated canonical control receives its existing receipt;
it does not call the provider again. Provider capabilities are intersected with the
trusted approval. An unsupported close-input or PTY resize does not become available
merely because the protocol defines the operation.

The native provider owns genuine process resources; it must not re-enter the Core
mutex while a dispatch holds it. Trusted cleanup and observation remain possible after
guest expiry, and a resource can be finished only after both exit and output EOF are
observed. Handles are bounded, cannot survive restart and cannot be reconstructed from
history. A lost stdin/control receipt is not durable cross-restart idempotency.
`owned_handles` is a bounded trusted cleanup query, including a registration whose
final authorization failed. It enables observation and `finish` without delivering
that handle to the expired guest. Finished handles remain historical map entries.

Tests in `tests/route.rs` use ordinary temporary SQLite stores. Logical providers cover
authorization/state transitions and are explicitly not OS execution qualification.
Two tests execute actual separately built Rust Wasm artifacts. Supply their absolute
paths through `MORROW_CODEX_COMBINED_GUEST` and `MORROW_CODEX_PROCESS_GUEST`; artifact hashes
are checked before execution. Run `cargo test --locked --offline` with those explicit
qualification inputs. Frozen SDK packages, Linux/platform security and full product
acceptance remain separate gates.

`ManagedPreparedPackage::connect` is an explicit trusted bridge to the original
registry-selected base and `Manager::connect_agent_session_process`. It retains
that instance's exact `Arc<Connection>` and intersects runner limits with the
manager/manifest ceiling. A base selection never substitutes for explicit review
of the complete wrapper digest, finite session scope, domain and separate session
and process capability subsets. Preparation/admission failure closes the same
original connection; it creates no replacement connection or Store.

`run` and `register_process` validate the original manager identity, registry
revision, selection, Core connection and cancellation around host/provider work.
The final live-tool clock is followed by another managed check before an effect.
Each clock sample also promotes original cancellation/deadline failure to original
Core revocation before R2's internal authorization/commit check. The first process
host is bound by its opaque runtime identity; exchanging it cannot claim cleanup
of resources retained by another host.
Disable/remove/upgrade, manager drop and original instance stop prevent reuse.
Loss of authority after an effect retains the process host's Unknown receipt;
cancellation may prevent delivering it to Wasm and never permits replay. `close`
requests trusted provider cleanup and disconnects the original connection.
Dropping a bridge stops guest authority; a trusted owner still must drive cleanup,
observe actual exit/output EOF and finish native resources.
An invalid/replaced R2 issuer does not skip cleanup of original process handles.
Closing with a different process host still stops/disconnects the Core instance,
returns Denied and leaves cleanup to the original process host; it never reports
successful cleanup of a different owner's provider.

`catalog::Catalog` persists complete wrapper identity and independent approval ceilings.
`inspect` is pure preparation; `install` saves immutable wrapper/base bytes without
selecting, approving or enabling. `select`, `approve`, `set_enabled` and `remove`
require exact catalog/Manager revisions and the actual original selected base. They
stop original same-base instances using the Manager's revoke-only seam before
publishing a compact canonical versioned Protobuf/LZ4 decision snapshot. Approval
also checks complete wrapper SHA, both schema pins, subsets, sorted finite sessions
and the exact execution domain. It does not select/enable the base or grant any object.

`NativeCatalogStorage` retains the original exclusive/full-sync SqliteRegistryStorage
lease. The original adapter stores only the compact snapshot and original base packages;
complete MROWASP1 archives live in same-root SHA-addressed immutable files. Full archives
retain the original `MAX_ARCHIVE_BYTES` legal bound, including archives larger than
512 KiB. Files are synced and published without replacing an existing digest, then
fully decoded/hash/schema/base-byte checked. The compact raw snapshot is at most
384 KiB and its packed container stays within the original registry limit. There are
at most 64 installed wrappers; native quota scans also count immutable orphan files,
old unselected versions and crash temporaries, with a total bound of 64 legal maximum
archives. Exceeding quota rejects installation; no automatic deletion is performed.
Archive/base installation and the decision snapshot are not a cross-file/database
atomic commit. Partial immutable artifacts convey no selection or authority.

Storage publication/read-validation failure stops issued original controls and poisons
further catalog use until reopen. Reopen validates every recorded full archive, both
schema identities and actual base cache bytes. It never recreates prior live authority.
`Catalog::connect` consumes the persistent ceiling through `ManagedPreparedPackage`
with a fresh original R2 admission, original Manager revision and bounded monotonic
expiry. `CatalogManagedPackage` retains only the original bridge, Core Revocation and
Cancellation; catalog Drop/mutation/poison immediately stop that original connection.
Weak catalog identity/decision epoch is checked before/after clock samples and delivery.
Restoring approval never revives an old instance, admission, process handle, Claim or
Unknown effect. Actual process cleanup/exit/EOF/finish remain the trusted owner's duty.

This native catalog is a library interface. Workbench install/review UI, ProtectedSession
owner borrowing, sandbox/transport and complete product acceptance remain OPEN.

`tests/managed.rs` uses actual temporary Catalog/Registry/SQLite/Core objects and
explicit hash-pinned Rust Wasm artifacts. Its providers are logical; these tests
are not new Windows process qualification. The native full-wrapper catalog and
approval storage are now implemented as a separate library interface; Workbench
installation/task routing and protected Session/native owner borrowing remain OPEN.
This bridge is not a completed production install path.
