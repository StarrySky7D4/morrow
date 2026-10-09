# v28 Todo source parity and explicit pure-check recovery

This checkpoint adds an explicit **重新检查** control for a failed Todo row formatter or remaining-budget count. It preserves complete raw input and the failed check's original confirmed old value, full new value, owner, captured revision, local version, row incarnation, and full aggregate editing value. A formatter failure after budget confirmation retries the same serialized pure formatter request with the same fixed remaining limit, including zero. A failure before budget confirmation reruns only the pure counts for the same captured aggregate before the first formatter request.

## Actual Flutter reference

The inspected reference is `build/win-cloud-20261005/lib/card_tips_editor.dart` and `tip_list_editor.dart`, with fresh byte identities in `todo-source-reference-inputs.json`.

- `card_tips_editor.dart:67–73` writes all rows joined with LF; `76–89` maps a single row's selection to full UTF16 offsets. The full field limit is 1000 at line 106.
- `tip_list_editor.dart:68–83` appends an empty row, then reveals/focuses it after the frame. There is no separate pending-add textbox. `144–157` gates Add on the complete LF-joined character count and the 100-row limit.
- `tip_list_editor.dart:222–232` subtracts all other rows' grapheme counts and one LF per gap from the row's remaining budget. `390–406` applies Flutter's positive `maxLength`; at zero it accepts a new value only if it does not grow beyond the old grapheme count, then replaces CR/LF with spaces. The actual source has no `_TipLengthLimiter` class.

Existing HMOS `EditorTodosDraft.remaining` already implements the same aggregate budget formula through the injected native-backed count policy. Existing Add creates an empty local row, counts the future separator, and preserves the 100-row limit. These behaviors did not require a new pending-add input.

## Implemented boundary

`model/EditorTodos.ets:267–289` keeps failed rows separately from a single retry snapshot and exposes `canRetry` / `retryFormat`. `296–317` retains the original confirmed old basis for subsequent same-row input and does not let another row's successful check clear an unresolved row. `323–347` adopts only an owned, complete accepted/retained/truncated receipt; failures retain the whole captured candidate. Exact same-owner/full-value echoes retain the retry. Real owner, revision, incarnation, selection, composition or raw changes cannot rebase a fixed failed request silently. Stops and interrupted rebinds retain the failed qualification but do not revive an old retry snapshot.

`model/EditorTodos.ets:355` recognizes only a fully identical row selection and aggregate TextValue as a no-op. This preserves retry through repeated identical native selection callbacks. A genuinely changed selection still captures the new raw range, revokes the old retry, and cannot qualify a failed row.

`pages/EditorTodos.ets:131–132` routes the actual control through current ownership and model admission. The row builder at `256–260` renders the localized control and enables it only for the exact retry snapshot. It does not invoke Save, create/edit, draft retirement, intent replay or handoff. The existing parent raw capture/status callback remains the authority for publication; a confirmed pure check is not a business save qualification by itself.

Flutter's formatter runs synchronously inside the TextField. HMOS retains complete incoming SDK text before an asynchronous pure count/formatter request, defers formatting while composition is active, and needs an explicit failure recovery state. This checkpoint closes that asynchronous recovery gap. It does not claim identical cross-controller selection presentation: the model preserves cross-row raw selection/composition without inventing a restorable single-row range.

## Verification and historical stages

The final command is:

```powershell
& 'C:\Program Files\Huawei\DevEco Studio\tools\node\node.exe' hmos/reports/ui-source/v28/run-editor-todos-retry.cjs
```

The final run executed four actual-source suites: Todo model, Todo component methods, input-policy model, and Index Todo Save integration. **161/161 PASS**, zero failure/skip/cancellation, exit 0, **7,961.2857 ms**. The 22 executed-source/helper/runner inputs are byte-identical before and after in `editor-todos-retry-final-inputs-before.json` / `...-after.json`. The harness's 19 preloaded but unexecuted model sources are separately listed in `...-observed-only.json`; they are not claimed as executed or compiled sources. The source trace records actual SDK TypeScript transpilation and source reads. Its controlled worker receipts test the real ETS policy validator and exact request hashes, not native Unicode segmentation or Store execution.

Coverage includes exact original old/new/hash/limit retry; count Unknown before formatter admission; malformed receipt followed by another explicit same-wire attempt; duplicate presses; zero-budget filtered-old reverse UTF16 metadata; unresolved other-row failures; unchanged-text input with original accepted old; changed selection/revision/owner/incarnation/stop/composition rejection; stale retry replies; actual component callback admission and unexpected disabled focused SDK events. Existing row count, aggregate budget, IME, raw capture, late input and strict complete Save assertions remain.

Historical evidence is retained, without substituting it for the final source:

| Evidence | Result and meaning |
| --- | --- |
| `editor-todos-retry-stage1.log` | 124/126 PASS, two newly written test-fixture errors: wrong child method name and an omitted-selection affinity expectation that ignored native filter finalization. Production was not loosened. |
| `editor-todos-retry-stage2.log` | 126/126 PASS after correcting those fixtures; before the identical-selection no-op refinement. |
| `editor-todos-retry-runner-stage3.*` | Trace setup failed on the SDK TypeScript module's read-only export before any suite ran. This is runner diagnostic evidence, not a model result. |
| `editor-todos-retry-pre-noop.*` | Four-suite 159/159 PASS and stable inputs before the final identical-selection guard; retained as that earlier source stage. |
| `editor-todos-retry-final.*` | Final four-suite 161/161 PASS with the identical-selection positive tests and changed-selection negative tests. |

## Frozen scope and remaining acceptance

Final Todo model: 25,625 bytes, SHA256 `72D6E76423130E183DD770C1BEF9AAD1313DC297969C9882BDC0E03AE540A440`. Final Todo page: 25,410 bytes, SHA256 `00110309FF3DEE60905501D3E5AF45518AEAEB642CFC28AD3D8A2AA7CEC68870`. Index remains the actual v27 source `0C1620875C82C517BEA1D8C4A3029DC851452978A323667FED8AF91E483696B6` during this worker's final run. All allowlisted files and hashes are enumerated in `todo-worker-freeze.json`.

This worker did not modify Index, business/native models, Rust, or product documentation, and did not build, install, operate a device, commit or push. SDK packaging and branch publication are Root-owned later evidence. Root's ongoing Pura X View2 work initially used the frozen v27 package; these source tests cannot qualify this v28 control as installed or rendered. Full Flutter UI parity, cross-row visible selection, live IME/focus behavior, and the reported unknown frames remain subject to actual device evidence.
