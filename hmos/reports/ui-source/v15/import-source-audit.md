# dev.15 attachment selection source audit

Source inspected 2026-10-07. This report is implementation preparation; it is not picker/device acceptance.

## Flutter requirement

`build/io-safety-refactor` HEAD is `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` at inspection.

- `lib/main.dart:5439–5472`, `importFiles(List<XFile>)`, imports every selected item in original order and waits for each result. It checks the total attachment count before every item against 20. The toolbar calls `importFiles(await openFiles())` at line 5798.
- After a successful import and attachment addition, it checks the current title with `title.text.trim().isEmpty`. Only then does it insert the imported source name, truncated to 60 UTF-16 code units. This is after the import await, so an intervening non-empty user title must survive.
- `_insert` records the frozen before/selection and refuses to replace newer input. `_save` treats a still-empty title as invalid; importing a file can supply the title rather than bypassing title validation.
- `lib/attachments/clipboard_import.dart:128–141` separately caps clipboard file count at 20 and bounds aggregate in-memory bytes. Clipboard HTML/RTF/image capture is distinct from DocumentViewPicker multi-file selection and remains outside this batch change.

## Existing HMOS durable contract to reuse

- `AttachmentFiles.pick()` currently sets `DocumentSelectOptions.maxSelectNumber = 1` and prepares only that exact picker URI. It streams through the native helper to one generated private spool; no provider URI or FD is persisted.
- `AttachmentFiles.importPrepared()` writes the exact request to `request.json` before Workbench admission. An admitted request is immutable and a provider/native Unknown does not trigger automatic replay. `recover()` returns the original request for the existing explicit retry path.
- `Index.acceptPreparedImport()` validates exact `sameImport`, retains Ready records in Rust, releases only a confirmed spool, and adds a pin through the existing `EditorDraftCoordinator`. A changed generation leaves a Ready import available for explicit addition rather than overwriting current values.
- `EditorDraftCoordinator` already owns frozen save requests, Unknown, active generation, selected pin identity and exact immutable receipts. The batch coordinator should sequence this existing per-item path; it must not create a second persistent import/save journal.
- Rust staging caps each draft's import records at 20 and selected draft assets at 20. Private import spools have a separate 64 MiB aggregate quota and 20-spool bound. These retained resources must count when setting a batch's permitted size; selecting more does not grant extra storage.

## API 26 selection capability

Installed primary SDK declaration: `C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/api/@ohos.file.picker.d.ts:420–471`. `DocumentSelectOptions.maxSelectNumber` is supported since API 10. API 21+ has no platform count limit and warns about large selection performance; API 23+ also documents no limit. Product count remains 20.

Recommended picker bound is the smaller of available selected-asset slots and available per-draft import-record slots, at most 20. Do not open the picker with zero slots. Revalidate the returned full URI list and available capacity; an oversized provider return must fail before reading any URI rather than silently slice to the first item. Duplicate returned URIs are rejected before admission to avoid an accidental same-selection duplicate; different URIs remain independent items even if names or bytes coincide.

## Planned integration

1. Root splits `AttachmentFiles.pick()` into bounded URI selection and `prepareUri(uri)` while retaining `pick()` compatibility. Each preparation must retain the existing exact URI grant, stream limits, generated spool, immutable metadata and cleanup contract.
2. A new ephemeral `AttachmentImportSelection` handles one active picker/batch, original URI order, independent operation IDs, and stop-on-cancel/failure/Unknown/editor-change. It never releases or replays imported items. Existing per-item import/spool/draft coordinators remain authoritative.
3. The root adapter captures an editor boundary (owner identity, generation and edit epoch). A per-item adapter returns the updated boundary only after its own exact import and selected pin receipt are confirmed. User edits or a changed editor cannot be accepted as an internal generation advance. Any Ready item not selected remains in the existing pending import UI.
4. Empty-title filling uses the current blank title after confirmed import and preserves the captured editing boundary. The adapter must update/persist through `EditorDraftCoordinator`, and decline if the title changed during its await. No response receipt replaces current title text.

## Implemented sequencing adapter

`model/AttachmentImportSelection.ets` now provides `AttachmentImportSelection`, `attachmentSelectionCapacity()` and the synchronous proposal `attachmentImportTitle()`. There is no new Rust journal, persistent batch record, spool deleter or replay path.

The hooks are:

- `read(): AttachmentImportBoundary`: an owned UI editor token, card/draft IDs, exact decimal generation, monotonic editing epoch, selected asset count, current staging-record count, current title, and a blocked flag.
- `select(maximum): Promise<string[]>`: open one `DocumentViewPicker` with that explicit maximum. The model validates the entire returned list before confirmation or preparation; it does not truncate it to one file or slice an oversized list.
- `confirm(boundary): Promise<AttachmentImportBoundary>`: call the existing `draft.ensureConfirmed()` only after a valid non-empty selection. An unchanged source editor's cancelled picker therefore does not create a journal. Returned generation may be equal or greater than the original, and must be nonzero. Editor identity, edit epoch, title and both counts must stay identical. The model rechecks the actual current boundary after this await. A rejected confirmation is conservatively Unknown; existing draft failure/Unknown remains authoritative.
- `importOne(uri, identity, boundary): Promise<AttachmentImportReceipt>`: reuse preparation, exact `ImportCommand` and existing per-item import/pin machinery. The callback receives an independently generated operation ID and the exact expected generation for that item. It must never generate a replacement operation ID or replay on a missing reply.
- `operation()`: the existing random UUID factory.
- `changed(state)`: a copy of progress metadata for the UI. Raw picker URIs are not exposed in this state or retained after the run.

`run()` shares its live promise if invoked again while selecting/importing. `stop()` only prevents later work; it does not cancel an issued provider/native operation or declare its result terminal. A late confirmed first item is retained, then the remaining URI list is stopped. Selection/preparation errors, failed/Unknown/cancelled item receipts, foreign receipt identity, or an intervening editor/generation change all stop subsequent items.

The root adapter's receipt contains `outcome`, exact `identity`, confirmed display `name`, `next` boundary and a bounded user message. Outcomes are `selected`, `retained`, `failed`, `unknown` and `cancelled`. `retained` means the original durable Ready import exists but was not added to the current draft. No cleanup is implied by any outcome. For `selected`, the next boundary must be the actual current editor, generation must strictly increase, edit epoch must be exactly old + 1, and selected/import counts must each be old + 1. The title must remain the previous non-empty title or become the confirmed filename suggestion when blank.

### Root integration recipe

1. Capture one `EditorDraftCoordinator` and one `AttachmentFiles` instance for the whole run. Create a unique `editor_id` when attaching a draft, even when card/draft IDs coincide with a replaced UI owner.
2. Track the editing epoch from a complete current `Values` snapshot. Increment when `sameValues(old, current)` becomes false, including selection/composition/assets/category/stage, rather than using generation alone. The deliberate pin addition and blank-title update must occur in one `draft.update(values)`, so they produce one epoch increment. A generation-only flush with identical current values does not increment the edit epoch.
3. `read().blocked` includes page/editor closure, disposed/current draft mismatch, conflict, Unknown, IME capture incomplete, retirement, other business writes and unsafe pending cleanup. Exclude the coordinator's own `attachmentWorking` flag so an active batch does not block itself. It may keep native text controls disabled through the existing import UI behavior.
4. Use `draft.current.assets.length` for `selected_count`. Use the complete current-scope `import_list` result length for `import_count`, **including consumed/retired entries**. Rust's `slot.entries.len()` enforces the 20-record limit before explicit reconciliation prunes entries. Counting only Ready/active entries over-promises available capacity. A remaining spool/byte quota can further restrict the picker; the existing native/private stream bounds must still be checked for every URI.
5. Split `AttachmentFiles.pick()` into `selectUris(maximum)` and `prepareUri(uri)`, preserving single-file compatibility if old callers need it. Do not open all provider files concurrently. Open each exact URI, prepare one immutable generated spool and close the provider FD, then complete that item's durable import/pin before beginning the next URI. Keep the existing 64 MiB aggregate spool budget and sidecar reserve.
6. In `confirm`, await the captured `draft.ensureConfirmed()`, then return the fresh boundary only if the editor still owns this draft and the original current editing snapshot has not changed. The coordinator independently checks this constraint. Merely waiting on confirmation does not permit adopting new user values into the batch.
7. In `importOne`, prepare the URI; before admission revalidate the captured owner/current values/generation. Construct the existing `ImportCommand` using `identity.card_id`, `identity.draft_id`, `identity.operation_id` and `identity.expected_generation`, plus the immutable spool name/length/hash. Pass those exact serialized bytes to `importPrepared`/existing `acceptPreparedImport`. Validate `sameImport` and `selectedImport`. An Unknown keeps the request/spool for the existing explicit retry control; a Ready but changed draft yields `retained` and preserves the pending import UI.
8. Before the existing `addImportedAsset` writes its values, synchronously obtain `attachmentImportTitle(values.title.text, record.request.name)`. If defined, assign that title together with the selected asset, collapse selection at its UTF-16 length, reset composition, and update `Index.title`/editor projection. Do this after exact confirmed import, while the captured editing boundary is still current; never assign a filename from a stale before-import title snapshot. Flush once through `EditorDraftCoordinator` and validate that its receipt is for the current values. Only then return `selected` plus the fresh boundary. If pin saving is Unknown, retain the original draft request and stop.
9. At editor close/replacement/page disappearance call the current selection's `stop()` before retiring/changing its owner. The issued per-item adapter must still preserve/observe its original request under existing contracts, even if its UI owner has disappeared.
10. Show selected/retained progress and the original pending retry/add/cancel controls. Finishing a failed batch must not call `import_abandon`, delete admitted spools or rerun the old URI list under new IDs. A later explicit new selection is a new user action, not an automatic remainder retry.

The title helper preserves Flutter's 60 UTF-16-unit bound and whitespace-empty check. At a surrogate pair boundary it removes a trailing high surrogate rather than producing an invalid Unicode name; an all-whitespace filename supplies no title. This is a small validity improvement over Flutter's literal `substring(0, 60)` behavior.

## Verification

`node --test hmos/tool/attachment-import-selection-model.test.cjs` executes the actual `.ets` model using the installed SDK TypeScript module. **28 / 28 passed** on 2026-10-07. Coverage includes all three selected URIs rather than first-only behavior, independent operation identity, exact large decimal generations, full-list capacity/URI validation including a sparse URI array rejected before confirmation or admission, same-name distinct URIs, cancelled-picker read-only behavior, generation-zero owner establishment, user-title changes while awaits are live, one shared handle, stop with a live operation, late confirmed preservation, failed/retained/Unknown item stopping, identity mismatch, new edits after confirmation, exact title/epoch progression and immutable state snapshots.

These tests use controlled picker and per-item adapters. They do not prove ArkUI compilation, actual provider selection/grants, native import, durable pin receipt or IME behavior. Existing import/draft/native model checks remain independently necessary. Multi-select picker runtime and device acceptance are still NOT_RUN by this subagent; root may append separate current device evidence.
