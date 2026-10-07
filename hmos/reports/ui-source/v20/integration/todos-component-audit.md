# dev20 todo component integration preparation

This report covers the row model/component changes made after the published dev20 source checkpoint: complete raw capture status, independent formatter readiness and the source-based adaptive row layout. The earlier [checkpoint row audit](../todos-source-audit.md), [isolated SDK smoke](../sdk-smoke-audit.md) and their logs remain historical evidence for their original source bytes. This worker did not change Index, Rust, product version, Git or the device, and did not build a new HAP. The overall Flutter/HMOS target remains OPEN.

## Fresh source and SDK read

[todos-component-inputs.json](todos-component-inputs.json) records the actual reference HEADs and raw/LF hashes, the local SDK declaration hashes and the ETS/test bytes used for this round.

| Flutter checkout | Actual HEAD |
| --- | --- |
| `build/io-safety-refactor` | `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` |
| `build/win-cloud-20261005` | `e83cdf1d3506b001d07ffceaaa9f98b688bcc150` |

Both references still have the same normalized `card_tips_editor.dart`, `tip_list_editor.dart`, `tip_preferences.dart` and `hold_reorder.dart` sources. Their normalized SHA256 values are respectively `5dffdc4dfd6f45c17e7145721815e6070976e1e687cbeedba572dbf17c8c4c0d`, `fefdce765a39cd0ac78adf786a91f9267211ed7a844d298d5051874fe75104ea`, `82563338a1f706d64653dde84b4b4c07bb0966872e9e73c81d6117033b5f30ee` and `2bd70dcd16ee8bea076a7bf10b9fe1e99129d899899610d79827d7ae2ee97007`. The io checkout has CRLF, so these normalized identities differ from its raw disk SHA256.

The actual Flutter `_TipRow` (`tip_list_editor.dart:361–431`) uses min1/max3 lines, font14 with height1.65, 8 vertical padding, a focused accent underline and no ordinary field border. Its container uses padding16/6/12/14, radius18, a palette line border and palette surface alpha0.30 in dark or0.58 in light. Row labels are11/w600/accent. Up/down/close icons are17; ordinary Material IconButtons resolve to `ColorScheme.onSurfaceVariant`, not palette accent. The installed Flutter Material implementation uses minimum40×40/padding8; theme visual density and Material tap-target rules can enlarge effective targets. `HoldReorder` uses a36×40 handle with icon19, muted color and disabled alpha0.30.

The local API26 SDK declarations contain `TextArea.minLines` and `maxLines(lines, MaxLinesOptions)` since API20. `MaxLinesMode.SCROLL` only takes effect with `TextOverflow.None` or `Clip` (`text_area.d.ts:866–898`, `text_common.d.ts:2155–2201`). They also expose `lineHeight`, bottom-only `borderWidth`, `Shape.viewPort` and `Path.commands`. No `TextArea.onContentSizeChange` declaration was found in this installed component SDK. The implementation uses the documented min/max-line layout directly and does not infer rendered line counts from characters.

## Explicit raw and business status API

`model/EditorTodos.ets` exports `EditorTodosStatus`; `pages/EditorTodos.ets` now calls `onStatus(status: EditorTodosStatus)` instead of the ambiguous four-argument complete/pending/error callback. `model.view()` exposes the same status fields.

| Field | Meaning |
| --- | --- |
| `owner`, `revision`, `value` | Exact editor identity, input epoch and an independent copy of the complete aggregate TextValue, including UTF16 selection/affinity/direction/composition. |
| `raw_capture_complete` | This value accounts for the current SDK input, or is the intact bound raw source. A formatter delay/failure does not make it false. An unlocatable preview or rejected parent capture does. |
| `business_ready` | This exact value has no pending/failed row formatting, missing capture or composition. It is a row-state prerequisite only; it is not full save qualification. |
| `format_pending` | An admitted pure count/formatter intent is outstanding, including the initial remaining-count await. |
| `capture_error`, `format_error` | Separate explanations for missing capture and unconfirmed formatting. |

The parent must match owner, epoch **and the complete value** before consuming status. It must not treat formatter readiness as permission to discard a raw draft. Raw autosave/flush/retain may preserve an exact current `raw_capture_complete:true` snapshot even when `business_ready:false`, `format_pending:true`, or composition is active. They still obey the existing raw-journal validation/transport budgets and original operation identity. Incomplete capture blocks confirmation because this component cannot account for the current widget input; the previous raw remains preserved.

Paste and business save additionally require `business_ready:true`, no active composition and the parent's fresh full-field/backend preflight. The status does not replace Unicode16 field counts, total1000 graphemes including LF, native request/reply SHA and byte validation, renderer/backend string budgets or business operation reconciliation. A readonly counter failure or nonmutating failed Add does not declare the already captured source incomplete; the parent must still reject an unconfirmed field count during business preflight.

Complete raw capture occurs before asynchronous formatting and pending status is established before that capture is published. A confirmed formatter result is then captured as a new full TextValue. Unknown/malformed replies preserve the whole candidate and leave business readiness false. An exact same-value parent echo retains pending status; a different epoch cancels that formatting intent without silently qualifying the candidate. Disable/stop preserves complete raw and blocks a canceled pending format. Disabling an already confirmed value does not invalidate its completed formatter receipt.

Existing `onCapture(value, owner) -> newEpoch|undefined`, `isCurrent(owner,epoch,value)`, `count(text,owned)` and `format(oldValue,newValue,remaining,owned)` hooks retain their contracts. Format must use the parent's validated `EditorInputPolicy` `todo_row` worker. ETS does not duplicate native filtering/truncation; zero-remaining retained receipt validation still uses `editorTodoFilteredOld`, and positive remaining still requires exact-old retained values. Local row IDs never become business TaskIds or enter the raw journal.

## Row layout and parent hooks

The TextArea now uses `.minLines(1).maxLines(3,{overflowMode:MaxLinesMode.SCROLL}).textOverflow(TextOverflow.None)`, lineHeight23.1fp (14×1.65), 8 vertical padding and a focused bottom accent border. Fixed76vp was removed. No platform maxLength or string slicing was added. The SDK lays out one through three visible lines and scrolls longer text while the model retains the full string.

Row labels and title now use weight600. Ordinary up/down/close controls use vector glyphs17 in40×40 wrappers; the handle uses a six-dot glyph19 in36×40 and the supplied muted color. New `controlInk` is an explicit theme prop for ordinary controls, falling back to muted when absent. It lets the parent provide the actual Flutter-derived onSurfaceVariant color. Parent-supplied `surface`, `line`, `ink`, `muted`, `accent`, `radius` and `family` remain explicit; surface alpha must be resolved by the parent. The vectors, Material interaction overlays, disabled ordinary-control alpha, font rasterization and effective tap target are not pixel-qualified equivalents.

Stable controller, owner/incarnation, guarded Add focus/reveal and drag tickets remain intact. `onRowFocused(owner,row)` and `onRevealRow(owner,row,elementId)` expose only the admitted owned row. `onDragPosition(owner,windowY,active)` remains the parent's outer editor Scroll hook; ending/dropping/disabling/stale revision cancels the private drag. This worker did not implement or qualify the parent's edge scrolling. The actual Flutter handle delay380ms, 16ms edge-scroll loop and drag insertion marker are not established by ArkUI model tests.

Valid SDK preview text is retained in full with its exact offset/range before commit, without applying a destructive formatter during active preview. That deliberate raw-journal behavior still differs from Windows enforced formatting during composition. Raw-journal-valid mixed sentinels and cross-row composition are preserved as complete source; the model does not invent a row restoration range or let another row callback clear them. Such an unmappable candidate cannot be continued through a guessed row controller; parent recovery/user completion is still required. The SDK controller interface does not establish arbitrary composition restoration or live affinity/direction equivalence.

## Checks and remaining qualification

[editor-todos-final-tests.log](editor-todos-final-tests.log): **70/70 PASS** —44 actual ETS row-model checks and26 actual component-field/method checks. The harness removes ArkUI builders/decorators and struct syntax; the layout checks inspect the actual DSL and documented interfaces, without rendering pixels. Count/format workers are injected synthetic receipts, so these checks do not replace the native Unicode16/actual Flutter formatter qualification in [editor-input-native-audit.md](../editor-input-native-audit.md).

New regressions cover independent full-value status copies, no transient candidate business readiness, count/formatter delay and failure, exact echo versus epoch cancellation, Unknown versus incomplete capture, full preview composition, invalid preview, failed nonmutating Add, canceled versus confirmed disable, raw sentinel/cross-row preservation and refusal of another row's stale composition-clearing callback. Existing owner/epoch/edit-away-and-back, selection, retained receipt, all row actions, local ID, private drag, cursor/controller and lifecycle checks remain passing.

| Boundary | Qualification for these new bytes |
| --- | --- |
| Source/read contract and actual ETS host methods | PASS within the70-check scope above. |
| Product Index hookup and API26 HAP build | Owned by parent; not run by this worker. Earlier isolated smoke is for earlier checkpoint bytes. |
| Rendered adaptive1..3 row heights, themes, glyphs and focus underline | NOT_RUN on device. SDK declarations and source assertions only. |
| Live row IME, affinity, cursor/reorder stability and outer edge scroll | NOT_RUN on device. |
| Raw restart/retain and business save/readback | Parent integration/device qualification required with exact draft/card IDs and original operation identity. |

The new integration evidence is kept separately from the prior published checkpoint. It does not turn a dormant-source checkpoint, an earlier SDK smoke or a model pass into product/device acceptance.

## Disabled late SDK callback notification

A subsequent narrow safety change adds optional `onUncaptured(owner:string,row:string,event:string)` with a default no-op. A live matching owner/row incarnation that receives input or selection while disabled now reports the entire original event JSON to its parent. Input JSON is `{kind:'input',text,preview?}` with the original committed string and optional SDK preview object; selection JSON is `{kind:'selection',base,extent}` with the original UTF16 positions. The component does not guess an aggregate, alter local/raw text, call a formatter or update a draft from this notification.

The nested row input previously also discarded disabled callbacks before they could reach the parent methods. Its disabled branch now uses the same identity-only liveness check and an internal ticket callback; the parent rechecks owner and incarnation before forwarding the public notification. Old owner/incarnation, missing rows and destroyed components remain ignored. Normal enabled capture/preflight rules and the UI remain unchanged. Identity-only notification deliberately does not require a current business epoch: the first uncaptured event may already invalidate that epoch, and the next current-row event must still be reported.

The parent connects this callback to `retirementCapture('todos',event)` or its equivalent: retain the diagnostic event, advance the input epoch, mark capture incomplete and revoke pending retirement/close. The parent's incomplete-capture marker must not be cleared by a later status/counter callback describing the older aggregate; this notification is not a successful aggregate capture. This narrow fix does not reactivate a discarded draft, create a successor scope, recover released pins or qualify device IME.

[editor-todos-uncaptured-tests.log](editor-todos-uncaptured-tests.log): **74/74 PASS** — the preceding70 checks rerun plus4 actual component-method regressions for disabled parent/child forwarding, complete preview/selection JSON, no raw/formatter mutation, current identity after preflight invalidation and old-owner/incarnation/destruction refusal. [todos-uncaptured-inputs.json](todos-uncaptured-inputs.json) binds the exact new page/test bytes and unchanged model/tests. The earlier70-check log/input record remains intact for its source snapshot. Product SDK/HAP and device qualification for this additional change are still owned by the parent, not established by these host tests.
