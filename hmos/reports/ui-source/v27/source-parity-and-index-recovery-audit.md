# v27 independent Flutter source and Index recovery audit

Worker: `v25_source_review`. Scope: read-only production review and actual-source Node integration tests. This worker changed no Index/model/Rust production, invoked no Git/SDK/native build/device commands, started no emulator/service, and sent no message to either user-owned source thread. Root owns production changes, full regression/build, device observation, documentation and branch publication.

The final worker run passes **92/92**, zero failed/cancelled/skipped, across four source suites. The new recovery suite contributes 23 and the preserved business/field/todos suites contribute 69. Final duration is 17,463.7599 ms, exit 0. This is bounded source integration qualification; it is not an ArkUI render/device or full-product acceptance result. Source identities and exact log hashes are in `source-review-inputs.json` and `source-review-result.json`.

After that successful run, the SDK compile repair added four `as Error` rethrow casts in Recovery: run input `062A7A50…` / 10,756 B became `6B02C274…` / 10,792 B. Removing exactly those four casts reproduces the complete old source hash. The 92 result belongs to the earlier Recovery source and unchanged final Index `0C162087…`; this worker does not reuse its pass count as validation of the new Recovery hash. Root owns the new frozen full-model/build evidence. The manifest records run identities and post-run review identities separately. No additional worker test run followed the SDK repair.

## Actual Flutter source

Read the active references in `build/win-cloud-20261005`, not root `lib/main.dart`. The `win-cloud-20261005` and `io-safety-refactor` main.dart copies compare equal after newline normalization. Fresh file SHA256 values:

| File | SHA256 |
| --- | --- |
| `build/win-cloud-20261005/lib/main.dart` | `2CB2A519E31AC982D3A8638EB7DE95FE63D5421ED3D1B6ACDA507CD142169F06` |
| `build/io-safety-refactor/lib/main.dart` | `6203B7465F320DC8463DFACB5387EA8B437EC3EC1458CCA7E215E44B8F65CB1E` |
| `build/win-cloud-20261005/lib/plugins/versioned_editor_adapter.dart` | `104E82C2573C973F8C6745010E22826BD144E1B99220A51151807B42915A8D16` |
| `build/win-cloud-20261005/lib/versioned_task_panel.dart` | `F97EC18EC42B4F6F33963EB0EB5DA15A137F2588F6E4915C7DB99F7E6D40C9A3` |

`main.dart:4984–5002` reads actual current full content, obtains its current workspace record, and opens the versioned editor at that revision. The adapter `_prepare` at `143–162` requires the original card/revision/format 2, unchanged category/stage/favorite, and empty legacy todos/completed/editor.todos. Category choice is shown only for `_baseline?.versioned == null` (`main.dart:5825–5855`); the legacy LF todo editor has the same eligibility at `5897–5902`. Versioned task identities stay in the independent details panel and commands.

Accordingly, ordinary details reopening of either reliably classified `v2` or migrated `legacy` belongs to fresh `current_v2`: empty editor LF, current full source, and no borrowed closed create root. The old create or `continued_todos` qualification only belongs to an exactly linked, owned business child with its preserved full LF and original Session facts. A reopened current V2 card must preserve task ids, completion/order, favorite and opaque source fields through the full source; labels or editor markers cannot supply continuation authority.

The source `_continueDraft` (`5224–5277`) and `_save` (`5325–5431`) separate accepted historical successor/raw input from the committed business publication. Unknown retains the original immutable proposal. A fixed S2 parent and later S3 current child cannot be replaced with S1 or a first acknowledgement. Full TextEditingValue selection, direction/affinity and composition are part of the raw value, not reconstructed from display strings.

One existing source difference remains outside this bounded change: Flutter's new-card category callback assigns project stage `推进中`, while the current HMOS callback assigns `计划中`. This worker does not claim category/stage string parity or full UI parity.

## Final integration review

The actual methods are extracted verbatim from current Index with the SDK TypeScript parser's method-node end. This avoids swallowing interleaved controller field initializers. The production binding class is copied verbatim; page/framework state is controlled explicitly. The new harness uses actual Draft, FieldPolicy, DirectInput, InputPolicy, Todos, Business, Session, Handoff, Recovery and the real Workbench tail. Rendering/media/import-list presentation and native/provider replies are seams, with no truthy replacement of owner/lease/save/recovery predicates.

`openCardEditor` reads current cards then draft list and rechecks ownership/idle boundary before any editor attachment. `currentEditorCardComplete` accepts only declared current kind, positive revision, full hex source and complete current card/task/asset DTO fields. It uses actual `AssetView.name/kind`; `display_name` belongs to journal pins. A matched linked current draft enters `recoverLinkedBusinessDraft` rather than ordinary raw opening. Late fresh replies, incomplete/missing kind, old ownership, pending native work, file/paste/raw-fork/boundary states retain the existing input and do not send a mutation.

Editor category/stage eligibility is restricted to a mounted new-card scope. Both actual builder enabled expression and actual extracted category callback use `editorMetadataReady`. Current source0 and recovered owned source0 children cannot send metadata changes from the full-body editor. The positive create callback still changes category/stage and real Draft current values; the negative mounted source0 callback preserves every value and sends no command.

`EditorBusinessRecoveryCoordinator` restores the original qualified Session/Handoff, reads the immutable fixed S2 parent by operation/generation, and reads actual current child by fixed card/draft id. It does not synthesize an active parent. A strict current child gives the actual paused writer; getters detach metadata without constructing another writer. Pending plan/parent/current reads keep exact wires and need explicit retry. Successful late read facts survive lost page ownership, while writer installation is refused. Inactive, foreign and partial current records grant no writer. Closed saved_exact has known absence of a child rather than invented Unknown; the separately owned model suite covers that no-plan boundary.

Index installation checks same coordinator, same original Session (`businessRecoverySession`), qualification, idle Session/Handoff, identical actual writer/handoff, detached full record and parent reference, complete current raw, valid active record and current card shape. It claims the same actual writer before UI side effects and leaves parent undefined. Actual page methods then attach that writer, install a real lease/binding, rebind Session hooks, choose owned LF only for original create/continued qualification, and resume the real writer. The lease starts revoked; controlled SDK mount is required. Recovered LF row readiness comes from an actual Todos model plus actual page status callback, not ordinary field completion.

Missing active parent is covered via actual intent restore then `resumeBusinessEditor`, which selects current-child recovery. The latest complete child and immutable original S1 remain distinct. A source advanced by independent metadata/TaskId changes produces `draftConflict`; raw and task identities remain intact and Save does not submit. Exact retired parent history is accepted for an already closed handoff; original close literal and closed Session stay readonly. An explicit page cleanup test sends only the fixed retirement after complete child/lease checks; controlled close rejection preserves current writer and known business facts. It does not pretend a different first close literal already has a native receipt.

The fresh current_v2 Save test consumes both complete actual v27 native card-source fixture cases after metadata/TaskId mutations and reopen. It checks actual current source, empty LF, null continuation and unchanged category/stage before prepare admission. The raw publication reply and definite prepare rejection are controlled; there is no claimed new native business commit. Native TaskId/body preservation belongs to the native agent's actual Store tests.

## Native/model read-only review

`hmos/rust/src/lib.rs` ordinary projection first validates actual card type `idea`, format 2 and full `tasks_v2::decode`; `content_kind` is then `legacy` only for validated migration origin, otherwise `v2`. Native TaskIds resembling migrated ids, empty tasks and current-editor markers do not classify a card. Invalid origin/schema/type fails closed. `CardView.content_kind` is optional with `skip_serializing_if`; strict historical receipt constructors leave it None, preserving their original 14-key contract. Read-only review found no remaining blocking classification/DTO issue.

The new Recovery's same-wire uncertainty handling, known absence, strict `openCurrentChild` and claimed-writer disposal rules were independently reviewed. Duplicate claim rejects before owner-dependent disposal, and disposing a claimed controller cannot dispose the handed-off writer. This worker found no remaining blocking recovery-model issue in the reviewed bounded route. The native four-input manifest and final fixture are hash-checked, not rebuilt by this worker.

## Preserved failures and compatibility

All diagnostics remain separate from final evidence:

| Log | Result | Explanation |
| --- | --- | --- |
| `index-business-recovery-integration-stage1.log` | 15/16 | Test used nonexistent `continuation_root`; actual DTO field is `continuation`. |
| `index-business-recovery-integration-stage2.log` | 18/20 | Real production AssetView `display_name` check was reported and corrected to `name/kind`; test also requested nonexistent summary entry from actual seven-part fixture. |
| `index-business-recovery-integration-stage3.log` | 22/23 | Test compared the retired record to boolean and outer retirement command to inner literal. Corrected exact original inner wire and actual record assertion. |
| `index-business-recovery-integration-stage4.log` | 23/23 | Previous production hash `C6EA5904…`; historical after subsequent Session ownership hardening. |
| `index-legacy-harness-regression-stage1.log` | 68/72 | Three source freeze hooks caught real concurrent Index change; one old todo message expectation. Not final qualification. |
| `index-business-recovery-integration-final.log` | 92/92 | Final Index `0C162087…`, all actual model source checks stable, zero skips. |

Before stage logs, an exploratory 7/8 run used `editorOwner` in a static assertion while actual builder uses `owner`. The final test extracts and executes the actual callback and tests both eligibility directions. That exploratory output was not saved as a final command artifact and contributes no final count.

The only old harness/test edits are `index-business-test-harness.cjs` (precise method-node extraction, actual Recovery dependencies/helpers, explicit old fixture edit/create mode) and one current complete-LF rejection-message assertion in `index-editor-todos-integration.test.cjs`. All field/IME/todo timing, immutable operation, Unknown, raw retention, task-id, zero-write and retirement assertions remain. Old fixture mode is explicitly edit/create and is not counted as new fresh source/current_v2 qualification. No old suite was skipped or relaxed.

## Limits and frozen worker files

Source harness TypeScript transpilation is not SDK ArkTS type/build proof. Grapheme providers and SDK delivery are controlled; Unicode16 native semantics, actual native new writes, Store durability, component watches/mount/render, signed installation and device workflow require their separate evidence. New tests call no emulator/device and prove no absence of the originally reported unknown frame lines. Full HMOS/Flutter product parity remains OPEN.

Worker-owned tools are the new `index-business-recovery-test-harness.cjs` and `index-business-recovery-integration.test.cjs`, plus the two narrowly migrated existing files above. Worker-owned v27 reports are this audit, six stage/final logs, `source-review-inputs.json`, and `source-review-result.json`. These tools and reports freeze on publication of the final identity result; Root's global evidence and branch push are separate.
