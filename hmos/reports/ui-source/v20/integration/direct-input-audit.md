# Direct input integration coordinator

2026-10-07. This worker added only `entry/src/main/ets/model/EditorDirectInput.ets`, `tool/editor-direct-input-model.test.cjs` and this new integration evidence. The published dev20 source checkpoint and its historical reports remain unchanged. Root owns Index/control integration, SDK/HAP checks, device qualification and Git delivery. This model does not save a draft, submit a command, edit Rust, segment characters or truncate text locally.

The coordinator receives a complete `TextValue` **after Root has retained it**. Raw retention remains possible while formatting is pending, failed or deferred. Confirmation of paste/business save additionally requires the corresponding direct-input slots to be ready; that readiness does not replace the native field policy, source/IME checks, backend budgets or original business request identity.

## Root interface and ownership

`new EditorDirectInput(hooks)` accepts four injected hooks:

| Hook | Required contract |
| --- | --- |
| `isCurrent(owner,key,revision,value)` | Synchronously check the stable editor/draft/lifecycle identity, **this field's** revision and the entire original text, UTF16 selection/affinity/direction/composing value. Root supplies these identities; the coordinator does not derive authority from matching text alone. |
| `format(field,oldValue,newValue,owned)` | Call the existing `EditorInputPolicy.format`, which uses the actual native `editorInput` worker, verifies request SHA/complete echoes/Unicode16 metrics and applies independent 512KiB request/reply bounds. Synthetic host decisions are not a replacement formatter. |
| `apply(proposal,owned)` | Check `owned()` immediately before synchronously adopting the **complete** proposal into the original editor raw state and updating the control. Return its new field revision, strictly greater than the proposal revision. The coordinator then independently checks `isCurrent` with the returned revision and a frozen copy of the complete result. A refusal, missing/old/wrong receipt or exception fails confirmation; the coordinator never overwrites Root's raw in recovery. |
| `changed()` | Refresh state/notice. It grants no save authority and must not mutate the current raw/ownership as a side effect. |

`bind(key,field,owner,revision,value,preview=false)` initializes a restored/attached field without truncating its existing raw. Field limits remain fixed by native policy. Keys are independent from native fields: `taskRename` can use native field `todos` and its own owner/revision, separately from the ordinary single-task input. An explicit rebind is a handoff or separately checked external edit; it is not evidence that a pending worker accepted raw. `unbind(key)` revokes a departed slot; `stop()` revokes all queued and running jobs across lifecycle changes.

Root calls `capture(key,owner,revision,value,preview=false)` after advancing that field's revision and capturing complete raw. Stale owners/revisions and reused revision numbers with changed complete values are refused. `view(key)` returns copies of raw/state; `canConfirm(key)` checks one field and `canConfirm()` checks every bound slot. Root must unbind finished rename slots or bind the complete current field set after attachment. `retry(key)` reissues a failed complete intent only when explicitly requested and still owned; failures do not automatically replay.

Root's formatter `apply` must update `editorValues`/raw draft synchronously, advance its per-field revision and return that revision. It must suppress an onChange echo from being treated as another user edit during that synchronous adoption. Any delayed controller selection restoration needs a separate exact owner/revision/value check. If a genuine new raw event occurs reentrantly during apply, its newer slot version wins; the previous result cannot rewrite it.

## Asynchronous event behavior

The model queues a microtask, coalescing onChange/onSelection updates in the same event turn. A later selection callback revokes the prior worker's `owned()` and reissues a pure read against the latest **complete** new value. While a text edit is pending or failed, selection-only updates retain that edit's original old value: sending old=new at this boundary would turn the request into a selection-only update and bypass the original enforced truncation. Once a result is settled, an independent selection-only event correctly uses the preceding raw as old, following the actual Flutter gate.

A subsequent text/composing/preview change establishes a new edit intent with the immediately preceding complete raw as old. Jobs bind slot object identity, owner, revision, complete current raw, local version and lifecycle epoch. Another field's input does not revoke this field. Stop, unbind/rebind with identical text or identity, changed root metadata, a newer input, or a late error cannot adopt an older result into a newer slot. The coordinator clears the old intent and updates its expected raw/revision directly after successful synchronous adoption; it does not resubmit the formatter's own result.

The states are queued/pending/applying, ready, deferred, failed and stopped. Only current ready slots with no active preview/composing can confirm. Pending/failed status does not cancel or inhibit Root's raw journal flush. `grapheme_count` describes the complete current value only after an identical accepted result or a successful adoption; it is `-1` when only a different, unapplied proposal was counted. Root's independent full-raw counter remains separate, so a truncated proposal cannot label an unmodified preview as shorter text.

## IME and actual Flutter boundary

This turn freshly read installed `C:/flutter/packages/flutter/lib/src/widgets/editable_text.dart:4604` and `services/text_formatter.dart:540`. Actual EditableText runs the chain on changed text or a noncollapsed-to-collapsed composing commit. Windows enforced returns the entire old value when old count is exactly the limit and old selection is collapsed; otherwise it takes a grapheme prefix and adjusts UTF16 selection/composing. The existing fixed Unicode16 native implementation and prior 297 real Flutter complete identities cover that algorithm. This worker did not change or rebuild it.

Root captures the SDK PreviewText reconstruction and composing metadata first. While `preview=true` or any composing metadata remains, the coordinator can obtain a validated pure formatting proposal, records its action and defers adoption. It does not replace, clear or reconstruct an arbitrary active IME candidate. A commit event creates a fresh intent whose old value includes the preceding actual composing range; the worker therefore still receives the actual Flutter commit gate, rather than a fabricated noncomposing old value.

If a committed input receives a native retained result containing the old active composition, ArkUI cannot be assumed to restore that candidate. The coordinator leaves the current raw untouched, reports explicit failure and blocks confirmation until the input is resolved. It never silently strips old composition and claims that the original Windows result was adopted. Pure formatter/controller afterprocessing remains separate, as already established by the actual Flutter widget fixtures. Preserving preview until commit differs from Windows enforced formatting during active composition. Current ArkUI runtime/IME equivalence remains **OPEN / NOT_RUN by this worker**.

Lone UTF16 surrogates and wire-budget failures are rejected by the existing strict receipt policy, after Root has captured original raw. The coordinator does not replace them with U+FFFD or truncate a source into success. Caller-side preservation is distinct from protobuf's ability to durably restore malformed UTF16. A bound/selection-only ready field is not proof that its full raw satisfies a business grapheme limit; independent `EditorFieldPolicy` remains required.

## Fresh validation and frozen inputs

`direct-input-model-tests.log`: **23 PASS, 0 FAIL, 0 skipped**, 444.0814ms. Tests load the actual new ETS coordinator and existing `EditorInputPolicy` using the installed SDK TypeScript transpiler. Controlled synthetic native replies and a host Segmenter are explicitly used for ordering/receipt validation; they do not independently prove Unicode16 segmentation, Windows formatter behavior, ArkUI DSL compilation, control rendering or device IME operation.

The cases cover complete raw pending retention; same-turn coalescing and later selection reissue; next-text and per-field independence; taskRename/todos separation; owner/ABA/unbind/stop/full-metadata revocation; preview and commit old-composing identity; destructive preview proposal non-adoption/count separation; complete positive retained adoption; unsupported old-composition restoration; explicit retry/no automatic replay; late failure; controller refusal/wrong or throwing receipts; reentrant new raw; copied views/inputs; strict Unicode and complete request bytes; malformed receipts/invalid offsets; settled selection-only gate; and stale/reused revisions.

| Input | SHA256 |
| --- | --- |
| `entry/src/main/ets/model/EditorDirectInput.ets` | `3CC296C8EC7B0BDBCBC14250C9657318C6DDFEF08C3C188A6212E422AF17A96E` |
| `tool/editor-direct-input-model.test.cjs` | `B920A02BF949702649EA42732BF5337D22A06E6A38D822C07BD799B460F342BB` |
| Existing `EditorInputPolicy.ets` | `64C86330D3DBC8FA85F88D6F0692D9ED15C88814C711F4F2B665827B3628AE2B` |
| Existing `rust/src/editor_input.rs` | `A564A1291A15C2F60E41C9F1487332D3F8CBBB6E7357096086FB1D943047E376` |
| Existing test harness | `FE8DB46B347C64FB28DF463AD335438576AEEED528B85BFCD14C70A631C96B13` |
| Fresh Flutter `editable_text.dart` | `029E723F5BCF8CC154EC5F2BA8A542DF84362BA3D983DBA20A6B4931247B0B0E` |
| Fresh Flutter `text_formatter.dart` | `B9EBC849139798B6405D28351BB74461C2C6BFB80A351A72DEEAE21E58D60DDB` |

Root integration/build/device evidence must be recorded separately against its final sources and package. This worker did not edit Index, earlier reports, Rust, shared schemas or versions, operate Git or a device, or claim a new HAP/device qualification. The complete Flutter/Windows goal remains open.

## Final parent integration correction

Root subsequently fixed the multi-slot stop/rebind path: a stopped slot is never treated as an unchanged ready binding merely because rebinding the first field has made the coordinator active again. The actual model adds a five-field same-owner/same-revision/full-value regression. [Final model log](direct-input-final-tests.log) is **24/24 PASS**; the earlier23-check log and hashes above remain evidence for the earlier worker bytes.

Final coordinator SHA256 is `A56119B3DFC3EA52E83082F0654C6BD9164383A646AF503B6D72CE2F0D7E6030`; final test SHA256 is `FF1C0D41AC3756D4BB7BF1FD20E2C85F79D97F3FB637434202DB0AD5F6F32D19`. Actual Index paste and both automatic-title paths now adopt complete externally validated values before binding. See [Index audit](index-business-audit.md) and [final product build/scope](validation.md); no device or full parity qualification is added by this correction.
