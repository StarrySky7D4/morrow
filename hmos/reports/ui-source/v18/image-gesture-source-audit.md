# v18 image gesture source audit

Date: 2026-10-07. Qualification: **source/model and API-build PASS_SCOPED**. Actual image gestures on the final v18 HAP: **NOT_RUN**. This report does not replace dev17 evidence or establish full Flutter/image-format parity.

## Actual reference and frozen sources

SHA-256 values below were freshly read from the files for this report.

| Source | SHA-256 |
| --- | --- |
| `lib/attachments/attachment_view.dart` | `F3BAA59202D433DEB1683F77932B31A210FB4EE5D382AD0A8ECFEB2A4DEC0FE4` |
| `C:/flutter/packages/flutter/lib/src/widgets/interactive_viewer.dart` | `943E1665721F7731957F58E774A0577B5D5274A35FCD2A47017E3C19ED27FB4F` |
| `hmos/entry/src/main/ets/pages/AttachmentImagePreview.ets` | `484076C31DCF78DDDB69B0EA27F6BB46C8FD753FC9763F0F07759CE9438442A8` |
| `hmos/entry/src/main/ets/model/AttachmentImageGestures.ets` | `73AF8DD840BAB941C7A5ACDBC45639920E0B423912D441472C1756FC77647FC9` |
| `hmos/tool/attachment-image-gestures-model.test.cjs` | `15CCA2D9CF119F63867A8428F91A5B4205E3750C1ED3ACF8BA8D7BC0749CEA14` |

The app reference is `attachment_view.dart:185–209`: a tight 620×320 `SizedBox`, default `InteractiveViewer`, and `Image.memory(..., fit: BoxFit.contain)`. It does not register a double-tap handler or custom min/max scale, pan axis, boundary margin, or transformation controller.

Installed Flutter source, rather than a generic gesture example, establishes the behavior:

- Constructor lines 73–89: zero boundary margin, constrained child, free pan, pan/scale enabled, nominal minimum .8 and maximum 2.5, hard-edge clipping.
- `_boundaryRect` at line 517 uses the child's **layout size**, including contain letterboxing; `_viewport` at line 542 uses the parent's size.
- `_matrixScale` at line 643 raises the total scale to at least `max(viewport.width / boundary.width, viewport.height / boundary.height)` before nominal clamping. The app's tight child/viewport therefore gives an **effective 1–2.5 range**. The previous .8 helper must not be presented as this preview's visible lower limit.
- `_onScaleUpdate` at line 728 keeps the moving local focal point on its scene point and rebases that point when a boundary stops translation. The centered HMOS transform uses the equivalent per-axis bound `±viewportExtent × (scale − 1) / 2`.

HMOS retains its existing responsive dimensions from the parent; this task did not change `Index.ets`. Matching the reference boundary semantics does not establish identical dialog dimensions on every device.

## What changed in this round

Basic two-finger pinch, one-finger pan, double-tap reset, and zoom/reset buttons already existed before v18. This round repairs their focal-point calculation, recognizer cooperation, cancellation, and callback ownership; it does not claim to introduce all image gestures from zero.

`AttachmentImageGestures.ets` now controls the actual component's transform. A two-finger pinch updates both scale and moving focal point. A separate two-finger pan supports translation without requiring a pinch-distance change. One-finger pan remains available. Pinch takes over both pan tickets, avoiding duplicate translation from parallel recognizers. Boundary clipping rebases pan/focal references, so reversing direction moves immediately. End/cancel retires the matching ticket; reset and button zoom revoke active tickets.

Every scope carries the admitted preview token and geometry generation. Every gesture has a distinct ticket. The component additionally captures exact token, URI, decode epoch and owner object; its canvas and controls share a token/epoch/generation/monotonic-owner `ForEach` key. Even a watch with unchanged dimensions cannot replace the owner while leaving event closures bound to the preceding owner. Actual SDK `BaseEvent.timestamp` fences reject queued older end/cancel events before they can borrow a newer same-owner ticket.

Loading, read failure, viewport changes, hidden/resumed canvas and disappearance revoke prior tickets. Only successful status-1 decoding with finite positive dimensions enables interaction. Stale decode/error callbacks cannot affect a different token, URI or epoch. Decode failure stops interaction while retaining the original verified preview/export handle. This component neither reads native attachment bytes nor releases/deletes parent-owned leases. `VerifiedImages.ets` and attachment/native ownership code were not modified.

Double-tap reset and the zoom/reset buttons remain **existing HMOS supplemental actions**. They are not claimed to be registered double-tap or button actions in Flutter's reference.

## Verification

Fresh command:

```powershell
& 'C:/Program Files/Huawei/DevEco Studio/tools/node/node.exe' --test 'hmos/tool/attachment-image-gestures-model.test.cjs'
```

Result: **30 tests, 30 PASS, 0 FAIL** in [attachment-image-gestures-model-tests.log](attachment-image-gestures-model-tests.log). The test harness transpiles the actual new ETS model with the installed SDK TypeScript, and executes the actual component's fields/methods after removing only ArkUI decorators, struct syntax and declarative builders. Gesture/owner/decode/lifecycle logic is not copied into a parallel implementation. Coverage includes moving/off-center focal points, pure two-finger pan, clipping and reversal, parallel recognizer takeover, cancellation, current/source identity, reused tokens, geometry changes, background/resume, old callback owners and timestamp fences. The real `ForEach` key expression is checked alongside owner-key replacement.

The earlier bounded run with unchanged `VerifiedImages` and `AttachmentFiles` tests passed **135/135**. The root's freshly inspected [arkts-final-model-tests.log](arkts-final-model-tests.log) records the complete current model run: **464/464 PASS**.

Installed API26 declarations confirm pinch centers are component-local vp (`gesture.d.ts:898`), exact finger counts are supported since API15 (`BaseHandlerOptions`, line 1250), and gesture-event cancellation callbacks since API18 (`PanGestureInterface` / `PinchGestureInterface`). `common.d.ts:7988` exposes `BaseEvent.timestamp`; `enums.d.ts:3292` defines `HitTestMode.Block` for a canvas that accepts hits while blocking descendants/ancestors. Fresh declaration hashes:

| Installed SDK file under `C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/component/` | SHA-256 |
| --- | --- |
| `gesture.d.ts` | `CF7E797B192D58566B6C62E1241A56D7E0884EA594A44C3152229EC1F251CC9B` |
| `common.d.ts` | `2662A40432B2FA6EFCFE387C3BC4BDC79AC9A993DE1E415042B37A78D6315614` |
| `enums.d.ts` | `617B64850440A44DFC6A776D5E3B6005038C87D207A88536511BE3E0A648D1E9` |

The root performed the actual SDK builds. Freshly inspected [hap-first-build.log](hap-first-build.log) records `CompileArkTS` and HAP success in **18.203 s**; that interim package retained the dev17 native library and is not final/device evidence. [hap-release-build.log](hap-release-build.log) records final build success in **6.782 s**. Root-reported final HAP identity: **27,866,016 bytes**, SHA-256 `910F3069B7979E5F8C6CF9E7DC770DF6E342725D493798DDE38F317524DB7DE2`. This report's author did not operate the device, build or Git.

## Explicit remaining limits

- Flutter `_onScaleEnd` inertia/fling and scale-velocity animations are **not implemented** here. Direct gesture transforms stop at end/cancel; this is not complete `InteractiveViewer` physics parity.
- Actual final-v18 two-finger input, finger-count transitions, background-window visibility, reset, and visually clipped panning on the device are **NOT_RUN** in this image audit. SDK compilation and executable model tests do not establish those runtime outcomes.
- GIF animation, malformed/corrupt image decoding, large-image memory behavior, and image-format/device acceptance are **NOT_RUN**. The simulated status/error callback tests establish ownership boundaries only.

The two frozen production files and test harness above are ready for branch delivery with these scoped limits; no mainline merge or device qualification is implied.
