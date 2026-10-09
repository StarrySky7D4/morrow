# v25 Flutter source and Index integration review

Date: 2026-10-09 (Asia/Shanghai). Reviewer: `/root/v25_source_review`.

Scope: read actual Flutter source files and the current HMOS editor paths; review the upcoming Root-owned Index integration. This report does not build, install, mutate production code, publish Git state, or declare application parity. Source observations below are a fresh filesystem audit, not a claim about a worktree's Git cleanliness or revision.

## Current source identity

The referenced source thread `01a085bd-7a94-7f93-8a1f-1ecf417f5ee3` now reports cwd `C:\Users\Administrator\Desktop\CodeXProjext\Morron`. Its current activity concerns the C28 Rust/SDK dependency source and a separately authorized VM recovery. `Morron/lib/main.dart` does not exist. That activity is not evidence of new Flutter editor behavior.

The actual Flutter sources available for this UI audit remain:

| File | io-safety-refactor bytes / SHA-256 | win-cloud-20261005 bytes / SHA-256 |
| --- | --- | --- |
| `lib/main.dart` | 227749 / `6203B7465F320DC8463DFACB5387EA8B437EC3EC1458CCA7E215E44B8F65CB1E` | 221628 / `2CB2A519E31AC982D3A8638EB7DE95FE63D5421ED3D1B6ACDA507CD142169F06` |
| `lib/plugins/editor_session.dart` | 1247 / `D2B6BBAE3E6FA316BF4C3B9EADAEA468B8223748C312FB25E91459EC91CF1AC3` | 1211 / `9DA33A6923AE0EE40B1E4587C6674BFD6C1618F662122CB14C3C1D815797D5AB` |
| `lib/plugins/editor_draft_session.dart` | 21602 / `422A4AC7C7CACE0115CC50290E027FE63348C6B8273E89A659C24C5302015E7E` | 21017 / `107CEE0DBFE8E8FD232B66EEA92002077120ABAB0BA6FB67033FA0F3A7693AF8` |
| `lib/plugins/editor_draft_workspace.dart` | 9649 / `D68E2529D4D7E32A20B3537CB9E0908F2225CE8EA3A1DFE06624883907DDE52B` | 9367 / `5E0E18CC42CB488E0733A06C1802874F125640E49391B5B593E489A089154D7B` |
| `lib/plugins/versioned_editor_adapter.dart` | 18724 / `E1AD98C236EC877F4DB512567920F0DA72ABB8300673F5194F5DBFB90360E6C6` | 18194 / `104E82C2573C973F8C6745010E22826BD144E1B99220A51151807B42915A8D16` |
| `lib/plugins/editor_draft_handoff_coordinator.dart` | 11562 / `F9BE662D0F53BB3D024CF75ABDDBFB6496E5C39C69FF1E097776A9D4B53E7F7C` | 11201 / `95FB312D3588D7D3349D0CE204275EBDD4B8AC1B8B8A893D5F21DC681F453B4F` |
| `lib/plugins/editor_recovery.dart` | 2614 / `A5C7C76A24DBE1DD4F349C67F29C018D54740C9B3919204DC4B0574297F07D56` | 2550 / `DC145BAFAFD0ADB7BAB270F07280EF8B9EAF819205393C36C054024BA09C96DE` |
| `lib/editor_recovery_dialog.dart` | 8318 / `74C8F373C4A594CE1AB7955A7B2BA021509492922313B515300126A0E23554C2` | 8079 / `642B94066D3C42C4F1E6FED357663665A81552C83FEB311ACEFA664FFC5E0D4A` |
| `lib/content/editor_attachment_rebase.dart` | 5424 / `CD91A597094148A0DD465C0747F0E60597DFFF6D8A3B1AAAF15C9681A851C072` | 5424 / same SHA-256 |
| `lib/tip_list_editor.dart` | 15369 / `D4AF62531CBC28807C2CE0356E6B7EA26A12E81C82E979D9D470DEEB11DD9191` | 14941 / `FEFDCE765A39CD0AC78ADF786A91F9267211ED7A844D298D5051874FE75104EA` |

All ten pairs compare equal after CRLF-to-LF normalization. Both copies therefore specify the same editor behavior in this audit; the different `main.dart` modification times are not evidence of a new save algorithm. The table paths are relative to `C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor` and `...\build\win-cloud-20261005` respectively. Additional TaskId UI read: win `lib/versioned_task_panel.dart`, SHA-256 `F97EC18EC42B4F6F33963EB0EB5DA15A137F2588F6E4915C7DB99F7E6D40C9A3`.

HMOS Index baseline before Root edits: `hmos/entry/src/main/ets/pages/Index.ets`, 285929 bytes, SHA-256 `AFF77EB7F1C90E53DDF5D64DD89FB3A037DAB98AC0B08AE0382090A2055EFEAF`. Existing session module: `EditorBusinessSession.ets`, SHA-256 `792E95BC45D5D589A08810B4A3BED24FA0C6229612FB38FDA18ABD12A9821FBD`; business model: `EditorBusiness.ets`, SHA-256 `6469D1F2AF4E2DA6B9066B52917E658BC13ECECB4EF2A4E6B92FBB4AD0D996CF`. These identities are baseline observations, not the future integration freeze.

## Actual Flutter requirements

Line references below use the freshly read win copy; normalized io content has the same relevant methods.

1. `main.dart:5210` observes the complete `TextEditingValue`, increasing `_editGeneration` for text, selection, affinity, direction, and composing state. A later value equal in text still cannot let an S1 reply consume S2. Controller initialization at `5678-5709` precedes listener registration, so construction is not recorded as a user edit.
2. `_save()` at `5325-5429` freezes raw `EditorFields`, normalized business `Idea`, and submitted generation once. Repeated calls retain that frozen S1. The draft includes category, stage, attachments, existing favorite and source metadata; create todo input follows the existing LF/trim/nonempty/deduplicate rule. Preparation rejection releases an unsubmitted proposal, unknown save keeps the frozen original, and committed-but-refresh-failed receives a distinct message.
3. Text editors remain available while a business save is pending (`5503`, `5873`, `5887`, `5901` gate importing, not saving). Paste/import cannot mint a second proposal while `_submitFrozen`. The save button preserves focus and guards duplicate activation internally (`5946-5960`); its label distinguishes saving, continue draft, original retry, and new save. Disabling a focused button can itself produce a selection-only successor, so native UI focus effects need actual validation.
4. On an S1 result, `_editGeneration != _submittedGeneration` invokes `_continueDraft()` (`5400-5405`) rather than closing the dialog. `_continueDraft()` (`5224-5277`) preserves current raw values, rebases selected attachments through a verified mapping, installs an actual successor editor at the accepted version, clears only the previous frozen proposal after success, and keeps `_hasUnsavedSuccessor`. It retains a prepared successor if attachment correction is needed. Failure leaves a retryable continuation state.
5. `VersionedWorkbenchEditorAdapter.save()` (`versioned_editor_adapter.dart:204-228`) refuses a changed retry signature and freezes once. Actual attachment staging and `_session.save()` are at `277-324`; result presentation/proof checks can fail after commit. `continueAfterCommit()` at `457-507` requires its own accepted version and a distinct same-target successor before retiring the old editor. `_openConfirmedVersionedEditor()` (`main.dart:4388-4431`) checks source revision, format, deletion, presentation, workspace freshness, and opens at that exact revision. Refresh can be separately retried; it is not replacement evidence for the original commit.
6. Durable editor recovery has a read-only observed identity (`plugins/editor_recovery.dart:5-33`); handoff needs exact committed source revision, while historical acknowledgement may coexist with later card state. `EditorRecoveryDialog._load()` (`editor_recovery_dialog.dart:37-55`) reads only. `_resolve()` (`58-120`) freshly re-reads and checks operation/source/digest before an explicit action; committed recovery cannot be abandoned. Handoff coordinator comments (`editor_draft_handoff_coordinator.dart:8-9`) similarly separate reads from explicit child creation/parent retirement.
7. Raw draft capture is independent of business success. `EditorDraftSession.markCaptureIncomplete()` (`editor_draft_session.dart:271-281`) invalidates completeness without altering a previously sent operation or last captured snapshot. Only a complete observation clears it (`295-307`). Unknown retry preserves the original request (`330-407`); original retry does not automatically schedule a newer save. `flushLatestVisible()` (`495-533`) waits for the real pending write, rejects unknown/conflict/incomplete capture, freezes the local generation and fails if input changes while persisting it.
8. V2 tasks use TaskId commands, not label rewriting. `VersionedTaskPanel` gates edits while an original command or accepted view is unresolved (`76-92`); `_submit()` (`126-191`) preserves the exact retry command, distinguishes proved no-commit from a locally unsent retry after an earlier unknown, and waits for the accepted view before a new command. New IDs exclude active and retired IDs (`195-200`); rename uses the existing task ID (`223-230`), reorder preserves IDs (`269-279`), and completion ambiguity has explicit controls. The ordinary V2 body editor hides the LF todo editor (`main.dart:5897`).

These are concrete source requirements. They do not prove HMOS runtime parity merely because a model can express them.

## Index baseline gaps and replacement points

| Baseline Index location | Actual behavior | Required strict integration |
| --- | --- | --- |
| `start():458`, `loadDrafts():472`, draft entrance `2448` | Loads cards and raw drafts only | Discover durable intent summaries with bounded pagination; reads must not auto-send business; incomplete discovery remains explicit |
| `attachDraft():1247`, `restoreDraft():1292` | Reconstructs the raw coordinator and compares frozen source to live card; no durable business context | Recover actual intent identity, original S1 publication and transport separately from the active S2/child raw record; preserve conflicts rather than overwrite raw with current cards |
| `save():1969` | Builds legacy create/edit `Command` from current values; edit sends no todo set | Switch create/edit to the strict session path; V2 pending todo text stays separately retained; native-owned create continuation may use continued-todos only with its own root/latest proof |
| `submit():1857` | Checks fields, flushes raw, appends draft tuple, saves `pending` JSON in RAM | Flush and verify exact complete raw; pause the actual writer; bind exact publication and stable session owner; prepare durable intent before business; keep original literal on every uncertainty |
| `retry():1904` | Sends RAM pending; replaces `this.cards` from reply; tests text/raw equality then cleans up or raw-forks | Qualify strict historical receipt first; keep known commit even when refresh fails; never borrow current cards as source/handoff authority; generic TaskId commands remain a separate channel |
| `closeSavedEditor():1702`, `retireDraft():1726` | Revokes view, flushes latest, retires with ordinary `draft_discard` | Strict success requires native `saved_exact` close plan and exact original session/raw/epoch; new S2 or selection-only change must preserve/remount and handoff; strict close must not fall back to ordinary discard |
| `beginRawFork():1141` | Raw-source fork with old source lineage | A committed business needs a native source-kind-0 child from its historical source and separately proven active-parent input/pins; raw fork is not business handoff |
| `closeEditor():2356`, keep/discard `1686/1775` | Pending protects RAM operation; raw keep and explicit discard have own revoke gates | Include durable session/handoff/close states. Keeping raw is not cancelling an issued operation; manual discard is not proof that business never committed |
| footer `3635-3649` | New-save button plus old RAM request check | Distinguish new save, original request inspection/retry, continue newer draft, and fixed lifecycle retry; do not let pending session be silently replaced by a new current-input save |

## Immediate wiring rules

The existing session's hooks are a real contract: `isCurrent`, `isExact`, and `parentReady` (`EditorBusinessSession.ets:26-31`, `209-211`). Root should bind `isCurrent` to a stable owned business session and permitted, verified parent-to-child lineage. Permanently using `editorDraft === oldParent` rejects valid continuation after switching the writer; using only card ID can accept a foreign view. `isExact` separately binds the original view lease, input epoch, complete values and attachment order. Old SDK callbacks remain fenced by their original view lease.

`parentReady` must observe an actually paused writer with no saving/unknown/conflict and the exact confirmed active publication. The safe initial sequence is: persist complete latest raw, verify freshness, pause, freeze one immutable business/publication proposal, prepare intent, issue intent, read exact stored save/inspect transports, and save. Revalidate ownership after asynchronous field/hash/native steps. If S2 arrives before issuance, do not use its values to patch S1. If S2 arrives after issuance but before the first save, the fresh-save exact gate rejects; the existing five-part read-only restoration and explicit original S1 retry are the recovery route, never automatic replay.

`originalSave` in the session is the registered native transport; the old business model getter is a different unregistered outer. Inner Submission bytes are preserved. The stable session keeps the native prepared/issued/closed facts, qualified historical business fact, original transport and immutable publication separately from current raw input. Phase metadata `committed` is not proof of a business mutation. An issued request cannot be cancelled as a prepared one. An absent inspection, bad DTO, local no-send, list/read failure, or later live card state cannot clear an earlier unknown or known commit.

For S1 success, capture current complete S2 under the actual writer and persist a fixed native business handoff plan before dispatching it. First child, child latest, parent retirement, intent close, view revoked and view closed are separate facts. Source/pin provenance and quotas belong to native proof validation; UI cannot invent a source from current cards. After child adoption, switch the writer and raw values together, preserving Todo row ownership/formatter revision. S3 must use the child context, a new business operation, fixed root original request and latest exact historical baseline; no recursive wire nesting.

An unchanged exact success still needs view revocation with a final epoch/value/completeness check and native fixed close. A late event during revocation prevents consumption and remounts/preserves input. Neither a host model nor a view disappearance certifies that the platform has drained every input event. Attachment import/alias correction and task controls remain unavailable for a conflicting, incomplete or unresolved lineage.

## Required actual integration review and evidence

The implementation review below is pending Root-owned Index changes. Baseline source audit alone is not implementation acceptance.

The review will cover actual method paths for create and existing edit; durable prepare/issue/load/save; stable owner and exact pause gates; old-wire unknown retry; five-part read-only restart recovery; known commit plus refresh failure; selection-only and text/asset S2; close-time late input; source0 child adoption; parent-retirement fixed wire; continued S3/root-latest context; V2 tasks that cannot be replaced by LF labels; manual keep/discard; and old callback fences after writer/view replacement. Test harnesses must invoke real Index methods and model/native DTOs. SDK build and new-native product/device acceptance remain separately owned by Root and are not claimed here.

Status at baseline: `SOURCE_AUDIT_COMPLETE / ROOT_INDEX_IMPLEMENTATION_REVIEW_PENDING`. No build or device action was run by this reviewer. Overall HMOS/Windows parity remains open.

## Root implementation review freeze, 2026-10-09

Status for this branch source checkpoint: `SOURCE_AUDIT_COMPLETE / ROOT_INDEX_BOUNDED_INTEGRATION_REVIEW_COMPLETE / PRODUCT_PARITY_OPEN`. This updates the pending baseline status above; it does not close the full HMOS port or certify its rendered UI.

The final reviewed Index (`2021-2353`) connects explicit Save to the real draft/session/business/handoff coordinators. It establishes a complete actual raw publication using `ensureConfirmed()`, checks the same lease, full editing values and epoch, pauses the parent, then freezes prepare/issue/native save/inspect transports. `businessHooks():2098` synchronously admits the actual `Workbench.send(wire)` before resuming parent writes after the first business send. The unchanged existing-card source0 path also creates an actual raw publication; it does not infer publication merely from opening the editor. The ordinary TaskId command path remains separate from strict create/edit business transport.

Independent findings resolved in Root's production changes:

- `businessParentMatches():2086` checks the full card/draft/source-kind/revision/source scope. `restoreBusinessIntent():2134` does not install a consumption binding for a same-card foreign draft. Complete raw equality alone is insufficient.
- `reconcileBusiness():2152` does not reconstruct or replace a session after a known committed effect. An earlier Unknown uses the existing session and original save literal; only an issued-before-first-send proposal without a known business fact may be restored read-only and explicitly retried. `resumeBusinessEditor():2191` calls `rebindOwner()` on the same session; it does not downgrade known facts to a newly restored Unknown model. This rebind route was reviewed statically; framework remount and callback delivery are not covered by the new integration tests.
- `save():2064` releases only its current unprepared first proposal with no pending native/read, known commit, or business Unknown after a proved no-write rejection. The complete raw draft survives. A lost prepare acknowledgement, including a later no-write response to its original retry, remains unresolved and blocks a replacement save. This defect was reproduced in stage5/stage6 before Root fixed it; both positive and Unknown counterexample now pass.
- Legacy `submit():1907` rejects create/edit. Pending strict intent guards also prevent ordinary mutation, manual discard/close, raw fork/retry, keep-and-close and new import/paste proposals from replacing the unresolved session. The new test executes old submit, ordinary mutation, manual discard and close guards; remaining import/fork/keep predicates were inspected directly, not exercised as rendered controls.
- `finishBusinessInput():2228` preserves the qualified historical commit independently of list refresh. Exact S1 success uses a fixed saved-exact close after view revocation and a final unchanged check. A late S2/S3 path uses the own accepted historical full source, one fixed child plan, an actual child writer and complete latest raw confirmation before parent retirement and fixed intent close. `continueBusinessHandoff():2278` switches writer and values synchronously before later callbacks; first S2 does not overwrite newer S3.
- For a session phase other than `issued`, `finishBusinessInput()` calls `EditorBusinessHandoffCoordinator.restore()` and reads the registered handoff/retirement/close bytes. It returns to explicit reconciliation without generating a replacement plan or cleanup identity. `reconcileBusinessHandoff():2327` requires complete unchanged S1 and controlled view detach before replaying a recovered saved-exact close; S2 blocks the replay and remains visible.

## Actual Index integration test freeze

New exclusively owned files are `hmos/tool/index-business-integration.test.cjs` and `hmos/tool/index-business-test-harness.cjs`. The harness extracts fresh verbatim production Index methods and the actual `BusinessEditorBinding` class, loads actual ETS Draft/FieldPolicy/Business/Session/Handoff/Fork/Paste models, and invokes the actual Workbench queue. All model source is snapshotted once per run. The test teardown verifies Index and every used model remained unchanged during execution and prints their identities. It uses SDK TypeScript transpilation to load the code; this is not an ArkTS SDK semantic build.

Final bounded result: **23 tests / 23 PASS / 0 FAIL / 0 SKIP / 0 CANCELLED**, elapsed 3627.1202 ms. Log: `hmos/reports/ui-source/v25/index-business-stage9-tests.log`. The 17 foundation cases keep business lifecycle completion as an explicit observation seam. The final six cases remove that seam and execute actual `finishBusinessInput`, `continueBusinessHandoff`, `finishBusinessClose`, `reconcileBusinessHandoff`, and the actual Handoff/Session/Draft models.

The complete coverage in this bounded run comprises complete-raw-before-pause, unchanged existing source0 admission, synchronous Workbench admission-before-resume, real queue order for later raw persistence, late S2 before issue/first-send, original issued/Unknown retry, selection-only S2, known-commit retention after invalid DTO/failed inspection, read-only discovery/five-part restoration, foreign same-card draft rejection, incomplete restore reference retention, old create/edit rejection, pending manual controls, ordinary task command separation, fixed planned handoff and exact-close read-only restoration, S2 rejection of recovered close, explicit recovered close ordering, fresh exact S1 close, and fresh S2 then late S3 into a real child writer before parent retirement/intent close. The cross-layer sample uses the unchanged v24 actual-Store DTO fixture (`9c8bd71ee6c42ad965a129973c5943f62a38261eef99310fa3d12af99a9999bb`) through actual Index restore/inspect; Store was not rerun by this reviewer.

Native receipt/metadata/error responses and lease detach are **controlled**. The field worker uses the existing controlled Unicode responder. Timers are controlled and explicitly driven where needed. Fresh lifecycle cases have no assets. These tests prove method/model/queue integration under their stated inputs; they do not prove native Store execution, ArkUI IME event draining, real attachment correction, rendered controls, SDK build, signed HAP, or device runtime.

Earlier scoped logs are retained as evidence: stage1/2 exposed missing actual helper extraction in the harness; stage3/4 passed the 14-case foundation; stage5/6 retained the real first-prepare UI lock failure; stage7/8 exposed incomplete controlled handoff fixture retirement/schema fields while the production model correctly rejected them. Those harness fixtures were corrected without weakening production validation. Only stage9 is the final integration result.

| Frozen artifact | SHA-256 |
| --- | --- |
| `Index.ets` | `4d252d1cbf5c8c193096f0b9e61d7fe0478b0715d08d3be881559913f4a8da8a` |
| `EditorDraft.ets` | `b88a46d0371d8c6f193187d1a0e1acf41671d4991cdbd59c451b8bcf66cf3775` |
| `EditorBusinessSession.ets` | `58e992b7e8bdae01f7bc07b3dee9e66b7bcae6712a945b5509382121e5af52c6` |
| `EditorBusinessHandoff.ets` | `5454de4c88133ccd04e27be693870280f6bf9f126696b9684c3db4ae025a6a55` |
| `EditorBusiness.ets` | `6469d1f2af4e2da6b9066b52917e658bc13ececb4ef2a4e6b92fbb4ad0d996cf` |
| `EditorFieldPolicy.ets` | `10c3162e77a947335acf0c3df2e464c80195d0ac2bbc2922bbb09c0a8a626842` |
| `Workbench.ets` | `1a5bcbee38ce7267b2e178fcfeffb445aaa029ab73c9074b7593b147569269bf` |
| New integration test | `5fdf7940a24eb84169bbb29db37970b81e9ddb5f9e4729933421b07ffc677eeb` |
| New integration harness | `945d2421958eacf49d9451525940f46450991bdea8b167793bf81c3973cdbab0` |
| Stage9 integration log | `670535ee7c0772b2c2b900ca14b98c015b487c3ec00ff8265bb1c97417ebb101` |

## Remaining restart and product boundaries

`EditorBusinessHandoff.restore()` restores fixed historical literals; it does not itself prove a current child writer. `openChild()` requires the still-owned, active paused parent and the accepted first child whose current generation remains first. If the active parent is missing/retired or the child has advanced, the minimum safe extension is a separately read current child journal with exact scope/full historical source/business-link validation followed by `openCurrentChild(record, latestRaw, owner-and-epoch guard)`. The actual model has `openCurrentChild():330`; Index currently does not connect this route. **Missing active parent/current child restart remains OPEN**. A complete first child followed by failed retirement/close also remains a separately reconcilable state; historical acceptance cannot revive an inactive child.

Overall Flutter/HMOS UI equivalence, real assets and import provenance during succession, task formatter/rename/reorder runtime behavior, editor focus/selection effects during asynchronous save and detach, old ArkUI callback fencing after remount, render-frame correctness, native/store/SDK/device/full-product acceptance remain separately open unless Root supplies corresponding evidence. This reviewer changed only the new report, new test/harness and scoped new logs; no production edit, Git operation, SDK build or device action was performed here.

## Final legacy-suite migration and newer source freeze

Root's first full model run found that two existing suites still called `submit(create/edit)` and expected legacy RAM pending and `draft_discard` behavior. Root explicitly delegated ownership of `index-editor-field-integration.test.cjs` and `index-editor-todos-integration.test.cjs` to this reviewer. Their original failure is retained in `index-legacy-migration-stage0-tests.log`; the old path is correctly refused by production and cannot qualify a new save.

The two suites now share the actual Index/Session/Business/Draft/Handoff/FieldPolicy models and the same actual Workbench queue through `index-business-test-harness.cjs`. Its narrow extension extracts the real field/count/input/lease/rename methods, keeps all original field/IME/ordinary TaskId and rename cases, and supplies complete controlled confirmed pin DTOs. Strict create/edit cases invoke `save()` and assert immutable Submission/publication and registered `editor_save` bytes. Unknown retries use `reconcileBusiness('save')`; a known committed effect cannot be retried as another save. Issued business rejection keeps its original durable operation rather than authorizing a replacement operation. Exact cleanup asserts the fixed saved-exact intent close instead of legacy draft discard. Late complete Todo input traverses the actual first-child/current-child/parent-retirement/intent-close lifecycle and preserves full S2 in the actual child writer. Revoked lease callbacks are tested through actual lease entrypoints and rejected; this does not simulate real framework event draining.

Migration uncovered a separate real regression: the new complete Save path had lost the old original-LF row-count limit. With 101 blank rows and an otherwise ready editor, stage2 dispatched `editor_save`; the new Business model's character validation and the Todo presentation readiness are not a replacement for this row budget. Root repaired `save():2043` after actual `ensureConfirmed()` and full lease/epoch/publication revalidation, before pause/intent preparation: a non-ordinary-edit raw Todo value with more than 100 LF rows is rejected while its full raw publication remains confirmed. The new checks keep the 101-blank rejection, add exact 100-row admission, and exercise both 100/101 boundaries through a real `continued_todos` session using its own original root request and exact accepted historical baseline. No injected row-readiness failure masks this regression.

**Latest final bounded result: 69/69 PASS, 0 FAIL, 0 SKIP, 0 CANCELLED, 9971.6955 ms** in `index-legacy-migration-stage4-tests.log`: original new business suite 23, migrated field suite 28, migrated Todo suite 18 (its original 16 cases plus the two boundary cases). Teardown verifies all executed Index/model sources remained unchanged within the run. The prior stage9 identities and 23-case result above are historical, superseded for the publication source checkpoint by this latest freeze:

| Latest frozen artifact | SHA-256 |
| --- | --- |
| `Index.ets` | `1ab29f6a124664231e6a3bea0ae5f745625ec4f77676ffcb55f746b50ad1184f` |
| `EditorBusinessHandoff.ets` | `8f68b983fb4747360a07b59bc80b0397abe9cff8990c7f4950f2280c71f2570f` |
| `EditorBusinessSession.ets` | `58e992b7e8bdae01f7bc07b3dee9e66b7bcae6712a945b5509382121e5af52c6` |
| Migrated field suite | `7b6f7bf9ceb321527bb025cd1d0cda6c11e0305f006359352cf796854ad02c7d` |
| Migrated Todo suite | `d51328c8cea4d5db39723554eda57a2955231cd980a91eebdfe419bcff681502` |
| Extended shared harness | `32a04f9bd26dbc346118fde35402e9621951dde754e1151b046f71f2a8d308ae` |
| New business suite, unchanged | `5fdf7940a24eb84169bbb29db37970b81e9ddb5f9e4729933421b07ffc677eeb` |
| Latest final combined log | `78990a8001798aefe4ecf8db7da9cb07f90a93f41f58d36115f89eaf8299fcba` |

Draft/Business/FieldPolicy/Workbench identities remain as in the earlier table. The new Index owner rebinding uses typed `EditorBusinessSessionOwnerHooks`; the Handoff's typed literal assignments changed its source hash without granting additional recovery/runtime authority. All executed model identities are printed in the combined log.

This latest migration also modifies the two explicitly delegated existing test files. Native DTOs/pin metadata/source bytes, field worker and framework lifecycle delivery remain controlled; the same Store/SDK/device/rendering and missing active parent/current child restart OPEN boundaries still apply. No test is skipped and production was not weakened or edited by this reviewer. Root owns the final broader model run, fresh dual-ABI/API26 SDK build and GitHub branch publication.
