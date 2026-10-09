# v26 business recovery model checkpoint

Scope: actual production ETS models and source tests. The v26 page recovery entry and `current_v2` page selection are not implemented by this checkpoint. Full Flutter/Windows UI parity remains open.

## Frozen production and tests

| File | Bytes | SHA256 |
| --- | ---: | --- |
| `entry/src/main/ets/model/EditorBusinessHandoff.ets` | 47627 | `fc2878c8f8fc2f312cf879023a1e445fe2cb3ae409755372feb9ffbc596723f0` |
| `entry/src/main/ets/model/EditorBusiness.ets` | 26878 | `bbd4de0b1e02a67e19345147eb7de4c01622c1e015e12fb0c1431a101945c4fd` |
| `tool/editor-business-handoff-model.test.cjs` | 50979 | `27aa2565dca25bfb9e9937f523395dcd1ccbc8dd6aec747701b2e85feb369ac3` |
| `tool/editor-business-model.test.cjs` | 31934 | `cafc77cfdc2dac380b0e166a48e861a546e354a4034e149566f5bf1156665b4e` |
| `tool/editor-business-session-model.test.cjs` | 45882 | `363e435d2af0eb7b5ab3ed382fdf129f67a2a5a54283fac7aca50d08599f2b59` |

Paths in the table are relative to `hmos/`. `EditorBusinessSession.ets`, `EditorDraft.ets`, `EditorFieldPolicy.ets`, and `EditorDraftFork.ets` remain unchanged from the prior frozen source. No Index, Rust, SDK, device, or Git operations were performed for this model task.

## Minimal caller API

1. Restore the actual original `EditorBusinessSessionCoordinator` from its immutable proof and registered literal parts, then explicitly inspect its original business result. A known live session can keep its existing qualified receipt and original transport through `rebindOwner`; a replacement restore must not wash known facts or Unknown.
2. `EditorBusinessHandoffCoordinator.restore(session, undefined, hooks)` restores the original handoff/retirement/close literals read-only. A parent coordinator is optional. The fixed `proposal.parent` can be later S2 and is not replaced with the business publication S1.
3. `loadParentHistory(): Promise<DraftRecord>` reads the fixed parent save operation/generation through `draft_read_history`. It requires a full repeated immutable record, exact scope/source/proof, ordered selected pins, and coherent real current flags. It stores historical evidence, never fabricates an active parent writer.
4. After an actual current child read, `openCurrentChild(record, latestRaw, guard)` validates the complete business15 lineage and exact historical result source, requires the current journal to be active and at the returned generation, and opens a real `EditorDraftCoordinator`. `guard` is the caller's current lease/epoch/input cutoff. This keeps complete latest raw; generation1 also must match the fixed first raw and persisted canonical first request digest. A successful current child observation clears only handoff reconciliation, never separate retirement/close/read Unknown.
5. For an active parent that has not retired, the caller installs the real child under `hooks.childCurrent`, confirms its latest full raw using that child, then calls `retire()`. The model needs the actual parent history above and the original native retirement literal. It does not need an active parent coordinator.
6. For an already retired parent, `readRetirementHistory(): Promise<DraftRecord>` first loads the fixed parent history if necessary, then reads the fixed retirement operation at parent generation plus one. `observeParentRetirement(actualRecord)` accepts the corresponding actual record already returned by a caller read. Both require exact business16 lineage, inactive current flags, full original values, consumed imports, pin identity/metadata, and resolve only the same retirement Unknown.
7. `closeRetired(operation)` uses a separately supplied independent operation only when no close has already been frozen and exact retirement is proven. Restored close plans are kept byte-for-byte and reconcile through `retryClose()`; a closed intent never grants another business save. Closing the intent leaves the actual current child intact.

`parentReference` exposes a detached fixed parent proof; `currentConfirmed` exposes the record accepted by the current-child restore; subsequent child saves are observed through the returned real child's `confirmed`. `pendingHistory` and `retryHistory()` retain one fixed uncertain readonly history request. Read failures never infer absence or regenerate operations.

## Body editing mode

`current_v2` is a schema1 typed body/general-field edit mode with action `edit`, full current source, a real source0 publication, empty `business.todos` and publication todos text, and null continuation. It keeps the existing field, composition, publication, owner, and exact-byte gates. It cannot become a `continued_todos` root/baseline. This does not implement LF editing of real TaskIds.

The final native fixture starts with actual current card revision5 after favorite/category/task rename/completion changes. Its `current_v2` command changes title to `current V2 edited title` and description to `current V2 edited 正文 🧪 é.`; the actual saved/reopened revision6 retains real TaskIds, task order, completion, renamed text, and favorite. Model tests consume those complete actual replies without replacing their fields.

## Actual DTO compatibility finding

The first actual v26 retirement reply exposed a production parser defect: native `Reply.intent_next_after` is an optional serialized field. Draft handoff/retirement acknowledgements omit it, whereas explicit intent read/list replies include the cursor. The final model accepts omitted or empty cursor only for a mutation summary, rejects null/nonempty cursor, and preserves the existing strict read/list cursor checks. The exact plan, source, lineage, and effect/receipt checks remain mandatory.

`editor-business-recovery-initial-diagnostics.json` records the original actual DTO rejection and a test-only restored-save call error. The complete later `editor-business-recovery-model-stage1-*` files retain the 132-check 131-PASS/1-FAIL run. Its remaining test error compared different JSON field order between a new local first close and an independently registered native fixture close. Actual fixed retirement validation and exact already-closed literal restoration are now distinct tests; existing controlled source tests still exercise fixed first close and exact Unknown retry bytes. No retry contract was relaxed.

## Final verification

- Five actual source suites: Handoff29, Business25, Session34, Draft27, Fork17: **132/132 PASS, 0 failures, 0 skips**, exit0, runner duration9378ms.
- Six production models, five source tests, and five actual Store fixtures: **16 inputs identical before/after**. `editor-business-recovery-model-inputs-before.json` and `...-inputs-after.json` enumerate full identities.
- Final log: `editor-business-recovery-model-tests.log`, SHA256 `5857ffa896e161922fbed4a3b0c056eb10303edc9041668843407c38698bba2a`.
- v26 current fixture: 38806 bytes, SHA256 `50f9037b3a235cff72e1423bf4ce5f30cf6b655c243373fa7f6b273bb926b301`.
- v26 history fixture: 75959 bytes, SHA256 `4191624d7b2894abc17a3874ea3005249b450e6ee270b9c880ea2f04530415db`.
- Existing v22/v24/v25 fixture files remain read-only and retain their previous identities. Controlled transports qualify model behavior; separate native Store-produced full DTOs qualify parsing and binding to actual records. This model run performs no new Store writes.

## Limits

Page routing, fresh SDK lease/input cutoff binding, current card UI source selection, device restart, task identity UI operations, and full product acceptance require subsequent integration/runtime evidence. Inactive/advanced historical first receipts are never mounted as a current writer. Historical read failure, external parent advancement, malformed committed replies, and close uncertainty retain explicit reconciliation. This checkpoint is suitable for branch publication as verified foundations; it is not full UI parity acceptance.
