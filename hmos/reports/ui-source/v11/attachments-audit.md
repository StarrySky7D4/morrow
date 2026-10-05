# Development attachment reuse audit

2026-10-05. Read-only source audit; no build, test execution, device action, production-library access, commit or push. The only new artifact from this audit is this report.

Baseline HMOS HEAD: `9564f6d5990bd74040c0dc86a06fe118fe83a5ac` (dev.10). Root's concurrent dev.11 UI work is outside this audit. References inspected: `build/io-safety-refactor` at `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420`, and `build/win-cloud-20261005` at `772466177fe589cee53bc633e69f411c34610104`.

## Concrete conclusion

The frozen core already supplies stream-to-SQLite storage, immutable byte verification, content deduplication, releasable staging owners, revision-bound reads and atomic attachment publication. These can be called through the existing development `HostRuntime`; a replacement blob database or app-path attachment store is unnecessary.

The original durable import state machine is **not included in `hmos/shared`**. It remains in the reference `workbench_host`. Current HMOS draft code explicitly rejects assets and consumption markers. Importing a file into an in-memory map, or merely calling `stage_blob`, therefore does not complete durable editor attachment support.

Recommended next complete product slice: **one selected document -> durable import -> selected draft pin -> reopen/restore -> save an existing card -> revision-bound read/export -> remove the current reference**. Limit the first UI delivery to existing cards and ordinary document/image attachments. Keep predecessor and parent origins unsupported. This uses original staging schema and original pin/accounting rules while retaining the present development-only storage boundary. It is not captured S1/S2 or production storage qualification.

If implementation must be split, the first independently valid milestone is **durable unselected imports**: choose/copy, Pending/Ready inspection after restart, verified preview/export to an app-owned temporary file, explicit abandon/discard, and cleanup reconciliation. Label these as pending imports; do not expose them as saved card attachments until the selected-pin and publication steps are complete.

## APIs that already exist in the frozen core

All paths below are relative to the repository root.

| Capability | Exact API/file | Important behavior |
| --- | --- | --- |
| Stream staging | `Store::stage_blob(reader: &mut impl Read, byte_length: u64, expected: Option<[u8;32]>, now_unix_ms: i64) -> Result<BlobInfo>`; `hmos/shared/core/src/store/blobs.rs:187` | Reads in 64 KiB storage chunks, checks exact EOF/length and optional whole SHA-256, and commits payload/chunk metadata together. |
| Durable staging owner | `Store::stage_blob_retained(reader, byte_length, expected: [u8;32], owner: &str, now_unix_ms) -> Result<BlobInfo>`; same file, line 200 | Bytes and one Snapshot retention are one transaction. Matching owner retries verify existing bytes without reading the supplied reader. Changed hash/length rejects with OperationConflict. |
| Staging lookup | `Store::retained_blob_local(owner) -> Result<Option<BlobInfo>>`; line 330 | Checks owner metadata, uniqueness, non-retirement and full bytes. It is not a terminal tombstone: after release, a late old stage call can reacquire an owner unless the higher-level journal rejects it. |
| Attachment metadata | `content::Attachment { id, display_name, media_type, byte_length, sha256 }`; `hmos/shared/core/src/content.rs` | Logical attachment identity is distinct from physical blob identity and URI. |
| Atomic business edit | `HostRuntime::edit_versioned_content(connection, &VersionedContentChange, clock) -> Result<Receipt>`; `hmos/shared/core/src/dispatch.rs:232` | `source_card` must equal the complete current Card encoding inside the transaction; set `attachments: Some(next_refs)` together with the transformed V2 body. |
| Pure body update | `cards_v2::apply(id,title,properties,&Command::Edit(Fields { ..., assets }))`; `hmos/shared/plugins/workbench/src/cards_v2.rs` | Preserves unrelated fields, tasks and unknown fields, including unknown fields in unchanged/edited existing asset entries. |
| Current, bounded read | `Store::read_attachment_chunk(&runtime::ReadAttachment) -> Result<AttachmentChunk>`; blobs.rs:385 | Exact Card revision and attachment ID; at most 32 KiB per read. Verifies every covered 64 KiB storage block before releasing bytes. |
| Full local export | `Store::export_attachment_local(card,attachment,&mut impl Write) -> Result<BlobInfo>`; blobs.rs:357 | One pinned read snapshot, verifies metadata and whole stream. Has no expected-revision argument; do not use it to silently satisfy a stale UI request. |
| Immutable blob export | `Store::export_blob_local(blob,&mut impl Write) -> Result<BlobInfo>`; blobs.rs:351 | Useful after verifying an exact staging owner. Keep physical blob IDs internal. |
| Pin/release | `retain_blob_local(id,owner,RetentionKind)` / `release_retention_local(id,owner,kind,now)`; blobs.rs:441/455 | Snapshot/Undo can release. Evidence cannot release. Release marks retirement only when no card, event or other retention refers to the bytes. |
| Retirement/collection | `retire_blob_local(id,now)` / `collect_retired_local(now,grace_ms)`; blobs.rs:484/499 | Retirement rejects retained bytes. Collection needs at least 60 seconds, checks all references again, and deletes at most 16 physical blobs per transaction. |
| Original outcome | `Store::lookup_for_card(card,operation)`; `store.rs:684`; `Store::operation_commit(card,operation)`; `store/evidence.rs:239` | Read original receipt and immutable command for exact retries. Absence alone is not proof that a concurrent in-flight request cannot commit. |

`Store::set_attachments_local` also exists at `store.rs:731`, but it only changes the outer Card attachment list. Calling it alone for a format-2 idea would leave `Properties.assets` inconsistent. The HMOS business route should use one `VersionedContentChange` for both representations, not two independent writes.

Content-addressed behavior here means deduplication by **SHA-256 + length**. `blob.id` is a random internal ID generated by SQLite, not a hash-named filesystem path. `blobs::bind` revalidates exact bytes in the Card commit transaction and writes `card_blobs` and `event_blobs` atomically.

## Original modules to extract rather than reimplement

`build/io-safety-refactor/workbench_host/src/editor_draft_staging.rs` and `schemas/editor_draft_staging.proto` match the inspected Windows reference semantically (no source diff; line endings may differ). Retain the schema verbatim and record the source hash when extracting.

The module exposes these Workbench operations, backed by portable Store calls:

```text
begin_editor_draft_import(&ImportRequest) -> DraftImportRecord
import_editor_draft_asset_durable(&ImportRequest, &mut impl Read) -> DraftImportRecord
inspect_editor_draft_import(card,draft,import_op) -> Option<DraftImportRecord>
list_editor_draft_imports(card,draft) -> Vec<DraftImportRecord>
export_editor_draft_import(card,draft,generation,import_op) -> Vec<u8>
abandon_editor_draft_import(card,draft,generation,import_op,abandon_op) -> DraftImportRecord
reconcile_editor_draft_imports(card,draft) -> ()
```

Do not copy the export method's whole-file `Vec<u8>` transport into HMOS. Reuse its ownership validation, then stream verified bytes to an app-owned temporary file or expose bounded chunks.

Reuse the exact `ImportRequest`: schema version, card ID, draft ID, operation ID, expected draft generation, name, kind, byte length and SHA-256. The original state is Pending -> Ready -> Retired -> Pruned; the original Pending command remains in operation history after Pruned. Identity derives from card/draft/import operation, not the source path or filename.

Extract/adapt the following parts to `HostRuntime + monotonic operation clock + Unix blob clock`:

1. Canonical body/Card validation, deterministic journal/owner/asset identities, `stage_history`, `original_entry`, `write_staging_slot`, limits and transition reservation.
2. `main_draft` using current HMOS editor journal read; exact active generation checks.
3. Retained-owner verification; Ready publication; abandon and reconcile. Write Retired before release; write Pruned only after release is observed complete. Old import retries must return the original terminal identity rather than reread a URI.
4. Original `editor_draft.rs` asset selection, `StoredAsset` pins, `charge`, and pin-vs-journal verification. Initially allow origins **0 (source), 2 (durable import), 3 (previous same-draft pins)** only; explicitly reject origins 1 and 4, predecessor evidence and handoff. Do not retain the legacy in-memory importer fallback for origin 2.

The original source holds Windows/Workbench coupling (`prepare_write`, plugin availability, production lease/seal setup, handoff helpers). A development adapter must state which platform guards are absent. Do not instantiate `Workbench`, weaken `storage.rs`, or claim its production admission/seal behavior was transferred. The current database-name rejection remains in place.

The selected pin is a real attachment of the private editor journal, with original `draft-asset-{index}` pin identities and byte/accounting rules. Source/current business records keep their own logical attachment IDs. Preserve `consumed_imports` until independent import retirement/release/prune succeeds; a later draft save must not erase failed cleanup evidence.

## Harmony selected URI and stream route

The requested SAF-style route is Harmony **DocumentViewPicker + selected URI**, not Android `content://` parsing. The [official Picker reference](https://developer.huawei.com/consumer/en/doc/harmonyos-references/js-apis-file-picker) confirms user selection returns document URIs and is available through UIAbility. Local deployed SDK declarations also confirm `DocumentViewPicker(context).select(...) -> Promise<string[]>`, URI-capable `fileIo.open(uri, READ_ONLY)`, `File.fd`, `read`, `write`, `dup` and `fsync`.

Implementation sequence:

1. Freeze the editor coordinator identity/current confirmed generation before opening the picker. Begin with one selection (`maxSelectNumber: 1`). Cancel produces no import and no byte staging.
2. Open the exact returned URI read-only. Never convert it to an assumed disk path, persist it as the attachment location, or accept an arbitrary URI/path through ordinary Engine JSON.
3. Stream into a unique app-owned spool using bounded buffers, enforcing size during copy. Compute complete SHA-256 and measured length; handle short reads/writes and EOF. Close/flush the spool before creating `ImportRequest`. This snapshot is needed because original durable import admission requires the hash before the Pending intent, and provider streams need not support seek or replay.
4. Recheck scope/generation and save/confirm the draft if a main journal does not yet exist. Admit original Pending intent before staging the fixed spool bytes. The cache file alone is never Ready or recovery evidence.
5. Add a purpose-built native FD import route. Duplicate the FD synchronously before queueing native work; transfer one owned duplicate to Rust `Read`, and close exactly once. Keep descriptors out of JSON as public identities. Alternatively use a private host-issued spool handle validated under the app's own directory. The existing `request(string)` bridge has only 512 KiB JSON and cannot carry attachment files safely as one base64/hex payload.
6. Ready requires retained bytes and the original Ready journal commit. Remove the spool only after Ready is acknowledged, or after a definitive failure with no unresolved native reader/commit. A Pending import without retained bytes can require user reselection of the same hash; do not automatically reopen its old URI after restart.
7. UI shows original name/kind/length and a pending/ready state. Late callbacks are scoped to the same draft. A changed editor does not receive a selected asset; retain or abandon the old import using its original operation identity.

URI permission duration, provider behavior and Native FD ownership still need actual API26 device validation. SDK declarations do not prove those runtime properties. Restoring a Ready import must need neither URI access nor its cache file.

Blob timestamps must use Unix milliseconds (original `platform::unix_millis` equivalent), not the Engine's process-relative `Instant`. The durable blob clock rejects regression; on a clock error, keep owners/journal state and report the error. Exact retained-owner retries do not advance that clock.

## Publication and removal invariants

For existing-card save, derive body fields/tasks from the full source and selected metadata using original `cards_v2`; derive outer references from verified source/pins, never caller-supplied arbitrary hashes. Validate one-to-one ID/name/length correspondence with `Properties.assets`. Preserve metadata order and unknown fields. Commit the body and `Some(next_attachments)` through the existing EditContent grant and full-source CAS.

Removal must identify the logical attachment ID, update both representations in that same transaction, and preserve other attachments, tasks, favorite/category/stage and their unknown fields. Do not unlink the physical blob or source URI.

Current reference removal **does not reclaim committed bytes**: `event_blobs` retains the original immutable history, even after the card no longer shows the attachment. Likewise selected draft pins become journal history. Only never-selected/never-published staging owners can normally release into retirement and collection. This is a storage-safety rule, not a UI bug; avoid a "space reclaimed" promise.

The import intent, retained-byte commit, Ready commit, draft pin commit and business commit are separate observations. A Pending/Ready acknowledgment is not a business save acknowledgment. Keep phase/bytes-retained and exact operation receipt separately visible. CommitUnknown, native disconnection, lost reply and post-commit read failure preserve the original request; never change its operation/hash/generation to force progress. Retry history proof must remain committed even if current-state readback then fails, following the already fixed draft effect rule.

For preview, use revision-bound chunks and verify final length/hash in an app-owned temporary file before displaying it. For export, verify the whole local snapshot before touching the picker-selected destination, then copy through its granted write stream. A provider write/fsync failure can leave a partial external file; current core cannot promise atomic URI replacement or external rollback. Report incomplete export; do not retry as a new destination implicitly. Library metadata remains unchanged by export.

## Limits and filtering to keep intact

- Core physical blob budget: 200 MiB per blob, 2048 blobs, 2 GiB total; 64 KiB stored verification chunks, 32 KiB read replies. Deduplication does not bypass the preallocation capacity check.
- Original import budget: 20 entries per draft, 256 import-journal identities, 320 current entries and 64 MiB staged bytes; import journal revision at most 256 with Ready/Retired/Pruned revision reservation. Draft pin charge additionally includes metadata and selected bytes; exactly 64 MiB payload may exceed the final draft budget after metadata is counted.
- Existing development business cap remains 256 cards; core events remain 1024 / 64 MiB. Import transition reservation is not permission to bypass the core event budget. Exhaustion can keep cleanup pending; retain evidence/owners and stop new writes.
- Add validated import-journal recognition to Engine list/query/card-cap filtering alongside editor journals. Import journals are format 1 with `morrow-host-editor-imports-...` and `org.morrow.host.editor-draft-imports`; never skip a prefix without canonical type/body/history validation. Current public create rejects `morrow-host-` already.
- Existing draft read/restore/confirm/discard and source-conflict paths must carry selected pin metadata. Otherwise a text autosave would drop attachments, and discard/rebase could release or retire the wrong scope.

## Verification required for the next delivery

These tests exist as readable reference evidence; they were **not run by this audit**:

- `hmos/shared/core/tests/attachments.rs`: short/long/erroring reader rollback, dedup, metadata unknown fields, current-reference removal preserving history, retention/retirement and concurrent snapshot export.
- `hmos/shared/core/tests/attachment_reads.rs`: exact sibling scope/revision, revoked/expired read, chunk corruption rejected before delivery, cross-block and empty reads.
- `hmos/shared/core/tests/attachment_crash.rs`: staging/publication/removal/collection crash boundaries.
- Reference `workbench_host/tests/editor_draft_staging{,_failures,_capacity}.rs`: Pending restart, Ready exact retry without reading original source, foreign identity/hash rejection, consumed/abandoned old operations cannot reacquire owners, failed Ready/release/prune, selected pin across restart and reserved revision capacity.

Adapt meaningful tests around the actual HMOS `HostRuntime` and JSON/FD adapter: two drafts sharing physical bytes but distinct owners; Ready missing spool/source; failure between byte and Ready commits; old operation after abandonment; text autosave keeping pins; failed business CAS keeping the import; remove-then-retry returning old receipt/current state; business/source bytes unchanged during pending import; validated private-journal exclusion; UTF-16/metadata limits; descriptor closed exactly once on queue failure/cancel/error; library export bytes survive provider write failure. Native runner should exercise real staging/pin/publication/removal/reopen, not only serialize a sample Attachment DTO.

Device acceptance: select a file with a Unicode/spaced name, cancel picker, provider permission/read failure, same bytes twice, force-stop after Ready and restore without original URI, save once, preview/export exact hash, remove one shared logical reference, and old-operation replay. Do not claim this is accepted until those observations exist.

## Source hashes observed

| Source | SHA-256 (raw bytes) |
| --- | --- |
| `hmos/shared/core/src/attachment.rs` | `D8D2F3B6F057E6A2B69EF334622AFAB384B3FFD917B6D215AD4F8507205A9700` |
| `hmos/shared/core/src/store/blobs.rs` | `BE5D3C3C4548BC797DC13CB070D82E336A217326AEE5D90D49BCF8CDCE071019` |
| `hmos/shared/core/src/versioned_content_change.rs` | `3FB13BB75C5D1F5A256F98576F5D351261DC47A3D2FA306E620C05790FC93F8A` |
| `hmos/shared/plugins/workbench/src/cards_v2.rs` | `ED92F85AF871A1AF28B6BD680217DDE9F39E1A8CB5701F5DE15A0EA04AC86CED` |
| Reference `workbench_host/src/editor_draft_staging.rs` | `4DE975878809AD9FCB969693768165BC55AD0090FDD8971DAA64ABB7215B2C99` |
| Reference `workbench_host/schemas/editor_draft_staging.proto` | `5FD6183551224232AA90ADB6F7D90E26317E2B4A294F155732668A8B9EA4E981` |
| Reference `workbench_host/src/editor_draft.rs` | `B4ADAFD9EDCA7ECBA9E4DD3A23963F226E3009233914D62C16630A1C7AC0DA28` |

Frozen `blobs.rs` vs inspected reference has formatting differences only. Keep current frozen files and adapt the host layer; a wholesale core/schema version update is unnecessary for this slice.
