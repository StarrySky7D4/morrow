# dev20 CardTipsEditor / TipListEditor row source audit

Scope: the full draft row editor, its complete newline TextValue and pure asynchronous formatter boundary. This report qualifies the new ETS row model/component methods on the host and their explicit isolated API26 SDK compile. It does not qualify a rendered ArkUI screen, device IME, business persistence, product HAP, installation or delivery. The overall Flutter/HMOS target remains OPEN.

## Frozen Flutter reference read

Read on 2026-10-07:

| Reference | HEAD |
| --- | --- |
| `build/io-safety-refactor` | `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` |
| `build/win-cloud-20261005` | `e83cdf1d3506b001d07ffceaaa9f98b688bcc150` |

The following files have identical source in both references after CRLF→LF normalization. `io-safety-refactor` is checked out with CRLF and `win-cloud` with LF; the normalized blob identity must not be confused with their different raw disk hashes.

| File | LF Git blob SHA1 | LF SHA256 |
| --- | --- | --- |
| `lib/card_tips_editor.dart` | `16620678b6cfd0d9a6c6615fbb62e78c7be7f5a3` | `5dffdc4dfd6f45c17e7145721815e6070976e1e687cbeedba572dbf17c8c4c0d` |
| `lib/tip_list_editor.dart` | `1a674f64827cec137aef1361529fb834a1582aff` | `fefdce765a39cd0ac78adf786a91f9267211ed7a844d298d5051874fe75104ea` |
| `lib/tip_preferences.dart` | `55c0008486b4caa4de710c633cf011bded675ff0` | `82563338a1f706d64653dde84b4b4c07bb0966872e9e73c81d6117033b5f30ee` |

## Actual reference rules

`CardTipsEditor.read` treats an empty aggregate as zero rows; otherwise it splits LF without trimming and preserves interior/trailing empty lines. IDs are local `TipItem` identities, reused by index on an external source update. They are absent from the raw journal and are never business TaskIds. A locally added empty row survives the matching empty-text controller echo; restoring the same empty raw text creates zero rows.

`CardTipsEditor.write` joins every row with LF and creates a fresh aggregate TextEditingValue. Its separate selection listener adds the preceding rows' UTF16 text lengths plus one per LF. Selection offsets are UTF16, even though the character budget uses Characters/graphemes. The row controller and focus node remain stable under reorder via GlobalKey. The reference aggregate write does not persist the row's composition; the HMOS boundary deliberately preserves the full candidate/composition in the existing raw journal instead of dropping it.

`TipListEditor` provides editable rows, explicit Add, remove including blank rows, up/down and handle drag. Maximum rows are 100; Add is enabled only while aggregate `.characters.length < 1000`. Each row receives remaining budget `clamp(1000 - sum(other row graphemes) - (rowCount - 1), 0, 1000)`. Each LF separator therefore consumes one unit. The card variant uses `daily:false`, no caption and no defaults reset, and labels its rows “提示 01”, etc.

The reference TextField has `minLines:1 / maxLines:3`. Enter is not a command to split or add a row: `FilteringTextInputFormatter.deny(RegExp(r'[\r\n]'), replacementString:' ')` replaces each CR and each LF, so CRLF becomes two spaces. With zero remaining budget, its custom no-growth Characters guard runs before that filter and still permits deletion/selection. With a positive budget, the filter runs before platform LengthLimitingTextInputFormatter. The audited Windows default is enforced. Native `todo_row` computes the complete-value chain and the EditableText formatter gate. ETS does not independently format the user's input or segment/truncate it; its receipt validator checks the expected filtered-old result at zero remaining.

`HoldReorder` admits its own scope/id/revision, distinguishes before/after at the target midpoint, and uses a handle LongPressDraggable with 380ms delay plus an edge-scroll loop every 16ms. Those gesture timing and scroll details are not established merely by a successful reorder model test.

## Legacy draft rows and versioned task business mapping

The actual main flow is `main.dart:_save` → `_editor.save(_frozenDraft,_frozenFields)`. The frozen raw fields include unmodified `todos.text` (`main.dart:5355`). The proposed legacy Idea uses LF split → trim each line → discard empty → first-order dedup (`5373–5378`), and keeps the baseline completed strings that remain in those proposed todos. `_NativeEditorSession.save` freezes the Idea and field fingerprint, uses the original captured save intent, `_writeIdea` writes the V1 `todos[]` / `completed[]`, and separately records `snapshot.todos = fields.todos` (`workbench_native.dart:3047`). Unknown business outcomes must continue to reuse/reconcile that original intent.

This editor does **not** mutate versioned TaskIds. `main.dart:5897–5898` shows CardTipsEditor only when `_baseline?.versioned == null`. `versioned_editor_adapter.dart:153–156` rejects nonempty `draft.todos`, `draft.completed` or `editor.todos` as `V2 editor source or task fields changed`; versioned task operations are handled in the separate detail task panel. `tasks_v2.rs` legacy migration synthesizes task identities as a migration operation, not as a row editor callback.

Integration must therefore not fill these draft rows from `CardView.tasks`, or call a pending single-task add for every row. The parent must supply a genuine persistence contract for the new-card/legacy draft source. A create-time V2 adaptation may normalize the complete raw rows and generate business TaskIds under the same immutable create operation; that is an explicit adaptation and requires its own backend checks. Existing V2 task details keep their independent TaskId operations. Neither the view-local row ID nor its index is a persisted task identity. This report does not declare that backend integration PASS.

## New ETS boundary and integration recipe

Sources: `entry/src/main/ets/model/EditorTodos.ets` and `entry/src/main/ets/pages/EditorTodos.ets`.

- Pass `ownerKey = attachmentEditorIdentity`, `revision = editorInputEpoch`, and the complete raw `editorValues.todos` TextValue. Supply explicit `editingEnabled/title/ink/muted/accent/surface/line/radius/family/locale`.
- `onCapture(value, owner)` writes the full candidate TextValue into the existing raw draft, with no normalization or business mutation, and returns the new parent input epoch; undefined rejects capture. A rejected capture restores source-consistent local rows and blocks confirmation.
- `isCurrent(owner, revision, value)` must check the parent editor identity, epoch and complete raw value. Owner changes, edit-away-and-back and selection changes revoke pending replies. Closing/disabling/disappearing revokes row and drag callbacks.
- `count(text, owned)` calls the parent's existing EditorFieldPolicy for `todos` and returns the complete nonnegative grapheme count even on a well-formed over-limit reply. A negative/unknown/budget response must reject. Counter success is not save qualification.
- `format(oldValue, newValue, remaining, owned)` calls the parent's EditorInputPolicy with `mode:'todo_row', field:'todos', limit:remaining`, and returns the validated native complete `value` and `action`. That adapter owns request SHA, echoed inputs, counts, Unicode16, byte limits and reply validation. At zero remaining, a retained result is compared with the shared exported `editorTodoFilteredOld` full-value receipt expectation: CR/LF→spaces, invalid selection→(-1,-1)/downstream/nondirectional and invalid/collapsed composing→(-1,-1), while legal positions and their metadata remain exact. This helper validates a native receipt; it is not a worker fallback. Positive remaining retains the exact-old requirement.
- `onStatus(owner, complete, pending, error)` blocks paste/business publication while formatting is pending or capture is incomplete. Raw retain/autosave can still retain the complete candidate/composition; it must not reuse business no-composition policy to discard raw IME. Native Unknown/invalid receipt preserves the candidate and does not silently truncate it. A confirmed native retained/truncated result is displayed with a notice.
- `onRowFocused(owner,row)` lets the parent update its last-field focus identity. `onRevealRow(owner,row,elementId)` exposes the exact owned Add target for the parent outer editor Scroll; queued focus is checked again before requestFocus.
- `onDragPosition(owner,windowY,active)` exposes only the current private drag ticket for parent outer-scroll edge handling. Ending, dropping, disabling or invalidating its revision cancels it. Foreign payloads are not admitted as reorder authority.

Rows keep stable local controllers/IDs under move. Rebind uses a new incarnation to revoke old controller callbacks. Restoring >100 preexisting rows preserves every source row; the Add cap does not truncate a restored draft. Full aggregate raw text remains authoritative, including cross-row selection that cannot be assigned to one row.

## Actual checks and qualification limits

[editor-todos-model-tests.log](editor-todos-model-tests.log): **55/55 PASS** (36 actual ETS row-model checks; 19 checks executing the component's actual fields/methods). The harness removes only ArkUI declarative builders/decorators and struct spelling. It tests injected synthetic count/formatter contracts, not the native Unicode segmentation or Flutter formatter implementation. Separate native/real Flutter qualification is owned by the native agent and parent. The added zero-retained regressions cover invalid-selection finalize, legal reverse positions/affinity/direction with CR replacement, collapsed composition finalize, unrelated metadata refusal and the unchanged positive-limit exact-old guard.

The native primitive and actual Flutter formatter/controller evidence is documented in [editor-input-native-audit.md](editor-input-native-audit.md) with [input-flutter-reference_test.dart](input-flutter-reference_test.dart), [full-value fixtures](input-flutter-reference.json) and [widget controller results](input-flutter-reference.json.widget.json). In particular, Flutter's controller can clear composing after a selection leaves the candidate range; a pure formatter proposal alone does not establish that controller side effect on ArkUI.

The checks cover LF/blank restoration, duplicate ID refusal, all row actions, total/remaining grapheme accounting, UTF16 selection, complete candidate capture, malformed/Unknown preservation, raw preview composition, late ownership/epoch/selection, private drag midpoint geometry, Add reveal/focus cancellation, stable controllers and lifecycle revocation. They do not execute ArkUI DSL or render pixels.

| Boundary | Current evidence / explicit difference |
| --- | --- |
| Device UI / row typing / Add / remove / reorder / restart / business readback | NOT_RUN by this worker. Parent qualification must bind exact draft/card IDs and operation identity. |
| SDK/HAP | Explicit isolated API26 SDK smoke PASS, 10.754s, documented in [sdk-smoke-audit.md](sdk-smoke-audit.md). The first compile revealed CustomComponent `enabled` / `onFocus` name collisions; the component now uses `editingEnabled` / `onRowFocused`. Product Index has not imported this component/policy, and the smoke HAP is a distinct bundle with no installation. |
| IME/composition | SDK PreviewText offset/value is captured in full when its offset is valid; invalid offsets block confirmation and keep old raw. Active preview is preserved without applying a destructive formatter result until commit. This differs from Windows enforced formatting during active composition. `TextAreaController.setTextSelection/stopEditing` does not prove arbitrary composition restoration. |
| Selection affinity/direction | Existing full raw metadata is retained; SDK onTextSelectionChange exposes base/extent only. Native formatter metadata is adopted, but live reverse-selection/affinity behavior still needs device proof. |
| Row layout | Current ArkUI TextArea uses fixed 76vp height. Flutter min1/max3 adaptive geometry, typography and full screen appearance are not pixel-qualified. |
| Drag gesture / edge-scroll | Only the handle is draggable; window coordinates and midpoint mapping use the actual SDK API. Native long-press threshold, 380ms match, parent edge-scroll hookup and device focus preservation remain unqualified. |
| Budgets | The aggregate 1000 graphemes is distinct from 100 rows, native request/reply JSON byte limits, renderer/backend string bytes, and raw journal transport limits. No over-limit source is sliced into success. |

No product Index, Rust/native bridge, product version, device or Git mutation was performed by this row implementation worker. The only HAP compiled here is the independent SDK smoke bundle; it is not product runtime qualification.
