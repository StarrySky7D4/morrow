# dev24 Todo initial display callback review

2026-10-07. This change addresses the observed initial Todo TextArea callback during restoration. Its qualification is source and host actual-method/model checks. The fixed source has not been SDK-built, installed or device-qualified by this worker. Device restore/keep success and full IME equivalence are not established by these checks.

## Actual diagnosis and reference

[Detailed device diagnostic](device-diagnostic-detail/capture-diagnostic.log), PID19675 at 19:34:34.447, records a Todo input immediately followed by an uncaptured event:

```text
owned=true enabled=false focused=false same_display=true same_control=true
text_length=15 preview=true preview_length=0 preview_offset=-1
raw_composing_start=-1 raw_composing_end=-1 pending=false incarnation=1 serial=1
uncaptured name=todos kind=todo_input focused=false restore=true owned=true event_length=76
```

The same log records legal, unfocused initial title and description selections. The [earlier diagnostic](device-diagnostic/capture-diagnostic.log), PID16951, also identifies `todos / todo_input`, rather than an ordinary field selection, as the missing capture. The ordinary-field selection guard is therefore an independent change; it does not explain this Todo callback. Metadata matching the initial display supports a narrow initialization acknowledgement, but it is not an SDK-issued origin proof.

Freshly read `build/io-safety-refactor/lib/tip_list_editor.dart:330–349` and the matching win-cloud file. The row constructs `TextEditingController(text: widget.item.text)` before its listeners; its selection notification runs in a microtask and requires focus. `TextField.onChanged` delegates to `widget.onChanged` at line422. `card_tips_editor.dart:27–30` similarly calls `read()` before adding the aggregate controller listener. Flutter initialization is not itself a new user text edit. This does not justify ignoring all unfocused or equal-text callbacks on ArkUI.

The two working-tree references remain byte-equivalent after LF normalization for the files read:

| File | SHA256 of LF source, both references |
| --- | --- |
| `lib/tip_list_editor.dart` | `FEFDCE765A39CD0AC78ADF786A91F9267211ED7A844D298D5051874FE75104EA` |
| `lib/card_tips_editor.dart` | `5DFFDC4DFD6F45C17E7145721815E6070976E1E687CBEEDBA572DBF17C8C4C0D` |

The installed API26 SDK `text_area.d.ts:540,552` exposes text/preview and selection ranges. These callbacks contain no issued origin token tying an event to the initial controller value. No callback-queue drain guarantee is used.

## Implemented admission boundary

Only `entry/src/main/ets/pages/EditorTodos.ets` and its actual component-method harness changed. The existing parent full-snapshot Watch guard, row model, journal, Index lease fences, formatter, business save and native protocol are unchanged.

`EditorTodoRowInput.aboutToAppear` arms one initial-display capability only on the new instance's first appearance. It freezes the actual controller reference, an independent owner/row/incarnation ticket, every raw TextValue field, display/control text and initial pending state. Row refresh, any admitted focus or blur, destruction, and the first admitted input revoke it. Disabled focus also revokes it before the normal enabled-focus guard. Availability changes never create another capability, and reappearing the same instance cannot rearm it.

`changeFor` first requires the existing row identity. An obsolete ticket cannot consume the current capability. The first admitted input consumes it before examining content or enabled state. It acknowledges the initial display only when all of the following hold:

- The same controller and frozen ticket still own the row; no focus has ever been received, and it is unfocused.
- The initial and current row are not pending. The original raw has no composition, and every current raw TextValue field still equals the frozen value.
- Raw text, frozen display/control, current display/control and incoming text match exactly.
- Preview is absent, or its value is exactly empty and its offset is exactly `-1`.

Acknowledgement returns without normal capture or an uncaptured notification. It does not modify text, selection, composition, epoch, parent raw, model status, raw-capture completeness, business readiness or Unknown. It cannot retroactively repair an existing missing capture. Any other first event consumes the capability and follows the existing path: disabled input retains the complete event JSON; enabled input is forwarded to the actual model. Second and subsequent events do the same even when text is equal.

In particular, nonempty preview, empty preview at any other offset, malformed/null preview, changed raw metadata, changed controller/display, initially pending rows, and original composition do not qualify. Focused input and same-text input queued after blur remain observable. A prior incomplete preview whose displayed candidate differs from authoritative raw cannot qualify, and its missing-capture status remains blocked.

## Actual checks and frozen inputs

[Final tests](todo-initial-input-final-tests.log): **109/109 PASS, 0 fail, 0 skip, 1370.5662ms**. This reruns the prior95 checks plus14 new checks, executing the production row-model and component methods. The harness removes decorators/ArkUI builders and supplies a recording controller; it does not emulate the native renderer or prove SDK event scheduling.

The new checks cover the observed empty-preview/-1 signature and absent preview once; prior input, focus/disabled-focus/blur, refresh and disappearance; obsolete tickets; every TextValue field; both initial/current pending states; composition; controller/display mutation; no rearm on availability/lifecycle changes; and actual parent model raw/status/epoch preservation with existing Unknown or capture-incomplete state. The existing focused-disabled selection/input and late formatter/prop-Watch regressions remain passing.

[First test run](todo-initial-input-first-tests.log) is retained: 107/109 PASS with two incorrect new test expectations. One assigned the default affinity instead of a changed value. The other expected an incomplete-preview row's unmatched displayed candidate to qualify; the final test correctly requires that event to remain observable. Neither failure required a production relaxation. The existing v23 logs are unchanged.

| Frozen input | Bytes | SHA256 |
| --- | ---: | --- |
| `entry/src/main/ets/pages/EditorTodos.ets` | 24901 | `83B6C6A5C5811EAD9FA14EBB23F9B5DEA7F9A858FD0B5461181091E929473836` |
| `tool/editor-todos-component-methods.test.cjs` | 54242 | `D6D5A929D369E391BAC5F57DD5F93B98BF6F69AFF0CBA1BA28AF6A1BCB865AE7` |
| Unchanged `entry/src/main/ets/model/EditorTodos.ets` | 21509 | `E7A0748F5066939AD42FA4A9B6BDE3D9D9761E15DF2E42403356C1277A48D710` |
| `todo-initial-input-final-tests.log` | 11808 | `0568383CFCB007F5319FEE5147AA99F328F11486B3444DE35850C27FA443105A` |

The production and test source have LF endings and no trailing whitespace. No Index, Rust/native, version, SDK/HAP, device or Git action was performed by this worker.

## Remaining runtime boundary

This is a one-use initialization admission rule tied to a real component instance and its already-set complete display. ArkUI `onChange` still has no origin token. A genuine first, unfocused, same-display, nonpreview event with identical metadata cannot be independently distinguished from initialization by this callback API. The rule therefore does not prove universal lossless delivery, callback-queue exhaustion or full IME equivalence. Future real input, previews, late callbacks and retained Unknown must continue to use the existing capture/lease fences.

The prior installed v23 restore failure remains a failure; its keep stage did not run. The two diagnostic runs supply the callback metadata above, not successful keep/close qualification. The parent must build and test an exact package containing these frozen bytes before attributing any restored-draft/keep/close device result to this fix. The full Flutter/HMOS target remains OPEN.
