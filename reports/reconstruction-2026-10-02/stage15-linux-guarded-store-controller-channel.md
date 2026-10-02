# Stage15: guarded Store slice, stable VFS lifetime and original-controller prerequisites

## Exact scope and source

Development branch: `codex/linux-sdk-reconstruction`. Base: `d1b61b429c690f5408b272fc422628f01fa1c230`, tree `80be8df4cf0350e2868af1c8b4730a53b0dd2df9`. Final tested code: `8433f0853c4b2c179f50f9b69522ed3a5484acdd`, tree `64cdbb150b32103fd2bfda27debe89a459df97ed`. This report is a later documentation-only checkpoint. App version remains `0.1.9-test.58+62`.

This is a tested experimental Linux/shared implementation stage. Protected Linux public Store factories, native product owner, public channel binding and Flutter product launch remain unavailable. The SDK is not frozen; GUI, complete Store operations, process-tree cleanup, recovery/migration and integrated Windows protected operation are not qualified.

## Corrected VFS allocation lifetime

The stage14 assumption that closing the owned main connection proved sole VFS ownership was incorrect. A safe named `:memory:` rusqlite connection bypasses main xOpen and retains pVfs for later callbacks, creating a potential use-after-free under the previous owner-drop policy. The original report is preserved. [The explicit correction](stage15-vfs-lifetime-correction.md) records the pre-fix safe reproducer, pinned SQLite source evidence and limited impact statement.

The allocation/name and synchronized dispatch context now survive through process exit; retirement closes new opens before owned SQLite close, unregisters, and detaches live State. In-flight callbacks and actual sqlite3_file objects retain their own State Arcs. Retired contexts keep no live file authority or pins. A separate 1024 process-lifetime registration ceiling is reserved before runtime/state/file effects, never recycled after publication, and fails closed without default-VFS fallback. The existing 64-descriptor safe-retirement pool remains separate.

Actual tests cover named-memory use after owner drop on another thread, stale prepared ATTACH and private access refusal, Weak<State>/fd release, independent main reopen, exact sqlite3_file close, runtime SingleThread refusal and isolated ceiling exhaustion before creation. Send transfers ownership; the connection is still not Sync. Additional unwind exit checks preserve sticky metadata poisoning without poisoning healthy rollback. A provisional IMMEDIATE-panic hypothesis was experimentally disproved and retracted. Savepoint fixtures were corrected to valid identifiers and now explicitly prove SQLITE_AUTH, rather than syntax rejection.

## Real guarded Store SQL slice

A distinct crate-private `ProtectedStoreSlice`, constructible only by synthetic tests, supports current-schema audited cards without attachments/evidence and channel ACK history. There is no Deref, raw-connection export, generic Store conversion or unported-method fallback. Twelve internal operation methods use shared production SQL routines for card creation/rename/read/lookup/pending and checkpoint/receipt/ACK operations.

Fresh initialization uses the shared schema definitions inside a guarded transaction; current reopen requires the original matching audit binding and validates integrity without rebinding or rewriting. Old/future/partial/unrelated headers and preexisting journals reject before SQLite admission. This bounded slice does not perform hot-journal recovery; the separate existing generic VFS recovery control remains distinct.

Borrowed adapters enforce read-only reads, explicit transaction control ownership, operation/callback/post-read/pre-commit checks, typed commit versus unchanged rollback, sticky uncertainty, and owned results only. Duplicate card retries and ACKs do not commit or consume a pending sync fault. Fresh real card/ACK receipts and audit bytes match portable Store; reopen is byte-preserving. Current-schema and real process-crash/lock/unsafe-metadata controls exercise actual SQLite, not a mock backend. Portable Store keeps its prior transaction/error policy.

## Native original-controller foundation

The isolated native crate adds an exact CLONE_PIDFD-derived controller reference, sticky original-process/heartbeat/transport loss, and inherited anonymous-pipe capability transport. Frames bind role/session/capability/sequence; payload, queue, syscall work and original read/write deadlines are bounded. Partial output resumes at its exact offset, never from byte zero. IO retirement is separate from child reaping and pipe EOF.

A typed prepared child performs expensive digest/argv/cwd work before the live lease while retaining the same sealed memfd. Current object policy and original watch are checked adjacent to clone. Exec-status waiting repolls that watch; any post-clone failure returns a real retained child owner. Fixed role descriptors are staged away from stdio/executable/status and tested under closed-stdio conditions. Bootstrap seal query errors explicitly reject before reading; the prior negative-mask bug found in review is fixed.

Actual fixture qualification uses a trusted harness with sibling controller and supervisor processes, a 200ms live lease, 200ms frame deadline and separate 5s bootstrap bound. Earlier 200ms failures caused by blocking preparation, an intermediate 1s run and the eventual restored 200ms results remain separate evidence. An ordinary-file negative test initially assumed EINVAL, while tmpfs returned F_SEAL_SEAL; the corrected control accepts legitimate rejection without reading bootstrap bytes, alongside a deterministic negative-query unit test.

The existing dependent-child PDEATHSIG policy is unchanged. This does not prove GTK-parent bootstrap, independent supervisor lifetime, malicious-same-UID isolation, dynamic-loader dependency identity or full process-tree emptiness. Controller loss specifically during exec-status waiting is source-reviewed but has no deterministic dedicated process test yet; preclone refusal and ordinary failed-exec Created retention are directly tested. This is a recorded nonblocking coverage gap for the synthetic foundation.

## Required-only shared executor integration

Exact transferred Windows helper source is integrated with current typed Workbench owner bindings. Original Control is revoked before fallible restoration on start/private-output/maintenance failures; invocation and maintenance panics are caught separately; owner and primary TaskReport/error survive maintenance failure. Reclaim retains report/error before restoration. Optional automatic job.stop was not applied, preserving phase3 Unknown and explicit same-key Close.

Native real-Wasmi tests exercise this production helper path, original owner/control, unrelated Control isolation, actual thread joins, retained ACK effects and no replay. Four pinned Directory003 reusable/one-shot × bytes/events runs retain original Ok(0), 11 calls, ACK5, 163840 bytes and exact receipts/fuel. Unsupported Linux protected maintenance still withholds output and returns Unknown/repair, never product success.

Original Windows candidate `9180f08d0c131c65116e4832e9476c491e2e6a8d` and prerequisite ancestry through `9a0126b42a1a0bd7850e2cca76ae6b44dc1194c2` were verified exactly, including payload tree and exported hashes. Prior Windows 15 Control +9 standalone helper passes are historical evidence only. The cumulative current source requires baseline `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420`, verified present by the Windows consumer. Its first supported Library materialization failed before ZIP creation; no integrated Windows source/tree/tests were qualified at that point. The subsequent user-requested private Drive transfer reached exact remote metadata/raw fetch but failed to create the consumer attachment directory with Windows access-denied error5. No candidate bytes or integrated tests resulted; manual user materialization is pending. Neither transfer failure was bypassed by changing permissions or credentials. No DPAPI/new-key/product proof is claimed.

## Final verification

The coordinator completed 35 commands with every expected exit on the exact final code. Relevant source closure: 1983 files unchanged before/after; manifest SHA256 `ed5dc2ab1d646ed4b8ec1b40ed0c937e92d66185f001a5aba6810ac9a2a6580f`. Expected nonzero exits include explicit product refusals and retained failures; this is not an all-tests-green claim.

- SDK91 and two wasm guest/C compile profiles; runtime40; host81 pass/3 ignored by default, then all3 explicit pinned-package tests pass
- Core72 pass/1 existing ignored; owned-VFS integration14; metadata7; guarded/Linux focused31 including documented subprocess helper entrypoints; compile-fail non-Sync contract1
- Shared Store regression68; explicit fault-injection card/ACK crash suites11 pass/1 helper ignored, including actual process exits
- Native14 units +10 actual process tests +11 unchanged foundation tests
- Existing audit suites, Linux preflight7, Python56, Dart97 and nine-target analyzer checks pass; subprocess helper summaries are not additional qualification counts
- Real GTK diagnostic configure/build/policy/status and ordinary/profile/Flutter-product refusals pass; no actual GUI window or Flutter engine test
- Wasmi003 strict comparison3 passes/four real runs; original001/002 still6 pass/2 fail at unchanged limits
- Fresh repository verification36 pinned files/13 original Wasm/package pairs, schema sync, native formatting and production host wasm32 library compile pass; no browser runtime qualification

Two early aggregate attempts hit the known shared multi-manifest prost crate-identity cache collision. Separate manifest targets resolved the build-harness issue without source/limit changes. A third run was explicitly superseded after the authorized test-proof correction; the final run above has no source drift. All earlier logs remain retained. Clippy remains unavailable in the installed Rust toolchain and was not run.

### Expanded Core failures, explicitly retained

The original six evidence_chunks plus four file_content_receipt_store failures remain unchanged. An expanded full Core sweep reported739 pass,26 fail,1 ignored. The additional16 failing test names were rerun from exact stage14 source: all225 archived Core blobs match `d1b61b42`, and the complete26-name/16-target failure set is identical. These newly observed preexisting migration-fixture failures are listed separately from the prior-known10 in the evidence receipt. No fixture/verifier weakening or broad green claim was made. The full sweep preceded only the later Linux test-only SQLITE_AUTH proof change; affected final tests were independently rerun afterward.

## Review, reproducibility and next dependency

Independent storage review reran all31 focused units on final hashes, including the corrected auth test, with no remaining scoped blocking P1/P2. Coordinator independently reviewed required shared executor wiring and native state/descriptor/transport paths. Independent native cross-review also reran14 units/10 process/11 foundation tests on matching before/after hashes, with no scoped blocking P1/P2; its log SHA256 is `2a7c9c75aecf74be51acb091fbd05a7f951e6e0c5215ab039bba7108282d23b8`. Source commits, cumulative bundle/patch, exact tree manifests, failure comparisons, full command/environment logs, review receipts and retained unsuccessful attempts accompany the private stage backup. Published GitHub commit identity and Drive raw-hash verification are recorded externally after publication, not invented in this pre-publication report.

Next native slice: typed self-pidfd capture, existing-parent endpoint and independent-supervisor role tested C→S→D headlessly, retaining dependent-child death semantics; add the dedicated exec-status loss regression. GTK integration additionally requires explicit child-reaper ownership compatibility with GLib/Dart and a narrow native bridge. Flutter engine, libmpv and compatible mimalloc remain unavailable and uninstalled. Existing verified GTK sysroot is reused only; any further acquisition needs separate provenance/capacity approval. Protected channel binding still needs genuine native/audit/Store authority, not a synthetic owner marker.
