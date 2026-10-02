# Linux reconstruction foundation: limited stage, not protected product qualification

This is independently reimplemented source from pinned test.57 commit `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` after workspace loss. It is not exact recovery of the lost Linux candidate, and no historical pass is reused.

Protected SQLite operation, native supervisor, sealed-memfd execution/pidfd owner proof, GTK runner and protected product wiring are **NOT REBUILT** in this stage. The four private Store entry points are deliberately unavailable before any filesystem or SQL effect. Generic portable Store behavior is unchanged and is not a protected fallback.

Fresh bounded evidence: metadata helper 7 integration + 1 owner-policy unit; audit production policy 8 unit + 2 integration; explicit memory fixture 10 unit + 4 integration. The production release compiles; release with the fixture feature is rejected by the intended compile-time guard. These are separate overlapping scopes, not a summed full-product pass count. The timeout test uses a pending future, not an actual SecretService socket.

The broader Core run is not green. Initial 9 existing service-authority tests failed on read-only real HOME; an explicitly isolated synthetic XDG_STATE_HOME resolved those tests. Later 6 evidence-chunk and 4 content-receipt tests failed; independently named exact pinned-base controls reproduce the same failures and schema24-vs23 expectations under the same toolchain/environment. Those failures are preserved, not silently fixed or discarded. Remaining broad stages are not claimed passed.

Two independent read-only reviews, including a narrowly authorized Max SQLite design review, required removing incomplete private-open wrappers. `SQLITE_FCNTL_HAS_MOVED` is pathname-relative and cannot prove the actual opened inode; `FILE_POINTER` does not expose a public Unix descriptor identity. A future owned/documented VFS/io_methods admission must establish the actual main/sidecar object before SQLite IO and then cover transaction/read/snapshot/backup/recovery paths. No private `unixFile` layout or `/proc/self/fd` shortcut is accepted as that proof.

Real same-UID SecretService/Unix peer/owner-churn, passwd-home write, Windows live and protected Linux product positives were not run. The previously blocked platform operations were not retried or bypassed.

## Implemented boundary and pending obligations

Bounded Linux protected-storage foundation: required future integration
Base: 925fb8ca6563dcb7de38db8fbdf6b005f9ad5420

Current boundary
- Linux key provider compiles and implements access to original same-UID running SecretService items via opaque references. The local file alone cannot restore a key onto a fresh account/machine without its original SecretService item. Failed publication may orphan a newly provisioned item; no automatic broad cleanup/delete/identity rotation is performed.
- Linux audit identity lease derives the real account's passwd home, holds a kernel file lock, and rejects unsafe metadata. Its positive tests use an explicit synthetic private directory only; no real home writes or SecretService positive qualification were retried.
- SqlitePermissions is metadata-only: O_PATH plus directory-anchored no-follow fstatat, UID/single-link/mode/inode validation for DB and sidecars. It is not proof of the inode actually opened by SQLite and must not wrap ordinary pathname opens as a protected substitute.
- Store::open_private/open_private_audited/open_private_read_only_audited/private_audit_binding_status deliberately return Invalid("protected Linux SQLite admission unavailable") before metadata, file creation, SQLite open, or SQL. There is no enabled private Store implementation. Generic portable Store paths remain unchanged and are not a fallback for protected Linux callers.

Required before any protected Linux product owner is enabled
1. Define and independently review actual SQLite-opened inode admission before all SQL, migration, binding or mutation. Do not guess sqlite unixFile layout. SQLITE_FCNTL_HAS_MOVED alone is insufficient; bundled implementation omits st_dev, follows pathname links, and permits ABA races absent independent stable object proof.
2. Wire all protected session initialization/reopen/binding paths in audit/src/session.rs to a usable strict API once (1) is implemented. Its generic Store opens and Windows-only database lease must never be selected as Linux protection fallback.
3. Wire all read-only source, original-key, recovery and restored-destination paths in audit/src/recovery.rs, snapshot.rs, backup.rs and library.rs, including copying, atomic publication, DB/WAL/SHM/journal files and original provider references. New/restored directories0700 and files0600; reject preexisting links/hardlinks/owner/mode violations rather than silently repairing.
4. Cover every Store transaction/read/write seam and before-commit verification as appropriate; reject unsafe post-open DB/sidecar mode, owner or pathname/inode changes. Avoid ordinary open+close of any SQLite inode for metadata because it drops process fcntl locks.
5. Wire card/read-archive snapshot reopen/copy and snapshot_to destinations in core/src/store/card_snapshot.rs, read_archive_cursor.rs and store.rs. Refuse unsupported protected snapshot/backup operations rather than exporting through a generic0644 path.
6. Only after (1)-(5), enable sealer/session/library/backup/recovery/snapshot/CLI/workbench Linux cfg gates, integrate the real passwd-home identity lease, and prevent owner recreation/PID disappearance from being treated as unlock proof.
7. HTTP/TLS credential codecs and core provider tags currently remain Windows-only. A later authorized Linux integration must use explicit Linux provider tags/domain-bound opaque references and preserve Windows DPAPI bytes/entropy. No plaintext secret fallback is allowed.
8. Obtain separately authorized real same-UID SecretService/Unix peer credential/owner-churn/identity-home qualification in a suitable environment. The current socket/home failures were not bypassed or rerouted.

Exact current line inventory is in protected-storage-required-callsites.raw.txt. These are pending implementation obligations, not claims that current generic routes are protected.
