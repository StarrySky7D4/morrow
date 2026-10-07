# dev19 actual Index field integration review

Date: 2026-10-07. This review added only an independent test harness and this evidence. The main agent owns the production changes. No device, installation, Git, signing, product build or production source edit was performed by this reviewer.

## Fresh execution and identity

- `hmos/tool/index-editor-field-integration.test.cjs`: **20/20 PASS**, SHA256 `F894781D53E87B960A2AE06F6E49368F7157DC56795A93621CDF27F17AE55354`.
- Current `hmos/tool/index-clipboard-integration.test.cjs`: **13/13 PASS**. Combined fresh execution: **33/33 PASS**, zero failures/skips, in `index-editor-field-integration-tests.log`.
- Actual tested `hmos/entry/src/main/ets/pages/Index.ets`: SHA256 `9B897AC560A05A91D05007C4706E9B96576FC24329AE037B3E7C8CB8A5B6C85A`.
- Actual `EditorFieldPolicy.ets`: SHA256 `10C3162E77A947335ACF0C3DF2E464C80195D0AC2BBC2922BBB09C0A8A626842`.
- Installed API26 SDK TypeScript **4.9.5** transpiles fresh, verbatim extracted Index methods, actual submission-field initializers and the actual rename-cancel builder callback. The harness loads actual `EditorFieldPolicy`, `EditorDraft`, `EditorPaste` and `Workbench.Command`; it does not implement replacement Index business logic.

Run from the repository using the installed Studio Node:

```powershell
& 'C:/Program Files/Huawei/DevEco Studio/tools/node/node.exe' --test hmos/tool/index-editor-field-integration.test.cjs hmos/tool/index-clipboard-integration.test.cjs
```

## Reviewed behavior

All five fields retain exact original combining sequences, emoji/ZWJ sequences, CRLF and whitespace in the independent raw journal. UTF-16 selection direction and composing range remain editing coordinates; no trim or substring is introduced by the field-count check. Valid IME candidates are retained in the raw journal while business submission is blocked. Invalid preview offsets block retention rather than inventing a candidate position. Business checks preserve confirmed original attachment pins.

Count results cannot supersede a newer field edit or selection. Business validation freezes draft/owner, full raw values, input epoch, selected card/task and the exact command. Text, selection, category, owner, command, foreground, real close/dispose, or changes during the final raw flush prevent issuing the older business request. Over-limit and worker-failure cases preserve full input for independent raw retention. Ordinary card submission leaves unsubmitted todo input and its raw draft intact.

Unknown raw-journal results preserve the precise pending journal request and last confirmed pins. Unknown business results retain the precise serialized request and frozen raw snapshot. The existing actual clipboard integration verifies that confirmed original/image pins precede portable body text; late foreground loss retains one already-confirmed pin and starts neither the second import nor body insertion. Future-field counting rejects text/selection roundtrips and complete over-limit values before native import.

## Reproduced defect and verified correction

The first run reproduced a P2: after a `task_rename` request was issued, a late native input callback with unchanged base text but a new IME candidate changed the full rename editing value. The former success branch compared only trimmed base text and closed the rename editor, hiding the newer candidate.

The main agent corrected this with frozen `submittedRename`, `submittedRenameEpoch` and `submittedRenameEditor` values. Success consumes the rename only when exact card/task/editor identity, epoch, full `samePasteTarget` value and absent composition still match. Fresh tests confirm ordinary rename still closes, late candidates and same-text roundtrips remain open, actual cancel revokes a delayed check and permits subsequent reopening, foreground loss revokes consumption, Unknown retains the original request/snapshot through explicit reconciliation, and confirmed `not_committed` clears only frozen submission state without consuming user input.

No other production defect was reproduced in this bounded review. A preliminary synthetic `editorOpen=false` probe was replaced with the real `closeEditor`/`keepDraftAndClose` path; the real path disposes the owner and passes, so that probe is not a reported product defect.

## Qualification limits

Worker/service/receipt replies and controllable input callbacks are synthetic. The synthetic worker uses the existing test helper's `Intl.Segmenter`; these tests do **not** establish native Unicode16 segmentation correctness, ArkUI IME event ordering, signed HAP behavior or real-device acceptance. Native Flutter/Rust character evidence is recorded separately. Device/IME/paste qualification for this new dev19 integration is **NOT_RUN** by this reviewer. Independent multipointer tester source/build evidence remains in `image-multipointer-tester-audit.md`; no test-runner entry was added to the product.
