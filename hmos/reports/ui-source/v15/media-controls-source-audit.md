# dev.15 media controls source audit

Audited 2026-10-07 against the current local Flutter reference and installed dependency. This report describes source alignment and host model checks. The new dev.15 media controls have **device acceptance NOT_RUN**; the dev.14 audio baseline under `device-baseline/` does not qualify the changed dev.15 gesture surface, controls, input hit testing or rendered layout. This audit did not operate a device or change production source, tests, versions, commits or remote refs.

## Reference identity

Fresh local reference HEADs:

| Checkout | HEAD |
| --- | --- |
| `build/io-safety-refactor` | `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` |
| `build/win-cloud-20261005` | `e83cdf1d3506b001d07ffceaaa9f98b688bcc150` |
| `build/windows-sdk-reconstruction` | `20669f671152972470340a65eac3458dd2f61b4d` |

The primary inspected checkout is `build/io-safety-refactor`. Its `pubspec.lock:551-558` pins `media_kit_video` **2.0.1**, package digest `afaa509e7b7e0bf247557a3a740cde903a52c34ace9810f94500e127bd7b043d`. `.dart_tool/package_config.json:431-432` resolves that exact package to the installed local Pub cache. Scoped Git status was empty for `lib/attachments/attachment_view.dart`, `lib/main.dart`, `pubspec.lock` and the package configuration.

Inspected files and freshly recomputed raw SHA-256:

- `C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/io-safety-refactor/lib/attachments/attachment_view.dart`: `F3BAA59202D433DEB1683F77932B31A210FB4EE5D382AD0A8ECFEB2A4DEC0FE4`.
- `C:/Users/Administrator/AppData/Local/Pub/Cache/hosted/pub.dev/media_kit_video-2.0.1/lib/media_kit_video_controls/src/controls/material.dart`: `D5A85321C74FDA473FE990F63928F769CF92D51E7132053038EDADB354DE6917`.

These local observations do not establish remote freshness or every behavior in the three reference applications.

## Actual enabled Flutter design

`attachment_view.dart:184-198` gives the preview a **620 × 320** content area and selects `Video(controller: video!, controls: MaterialVideoControls)` for both audio and video. It does not select the desktop adaptive control variant. Playback is opened paused in the surrounding attachment implementation.

The inspected Material implementation supplies:

- **Initially hidden controls**, single-tap visibility toggling, **3-second** interaction timeout and **300 ms** transition. The fullscreen defaults are at `material.dart:38-57`, constructor defaults at `:294-316`, and tap/timeout behavior at `:682-703`. Seek-bar dragging suspends the hide timer and restarts it after release.
- Central play/pause controls and the seek bar, current/total position and fullscreen action. Default central icon size is **48** normally and **56** in fullscreen; playlist skip controls are hidden for this application's single-item playback.
- Fullscreen horizontal seeking, including while controls are visible. `material.dart:719-761` computes whole seconds as `-round((initialX - currentX) * duration.inSeconds / 1000)`. Movement outside `[0, duration.inSeconds]` leaves the last accepted delta unchanged; release seeks from the then-current position and clamps to the valid duration.
- Fullscreen double-tap seeking in the left and right equal thirds; the center third has no seek action. Defaults are **−10 / +10 seconds** (`:306-307`). The side indicator accumulates subsequent ten-second steps and submits after **400 ms** (`:2024-2200`), followed by **200 ms** disappearance (`:1379-1454`).

**Vertical volume/brightness dragging is disabled in this application's actual reference.** Although fullscreen theme flags are true, `material.dart:933-978` also requires non-null `onVolumeChanged` / `onBrightnessChanged`. Those callbacks are not supplied by the inspected attachment widget or main application. The HMOS implementation therefore does not introduce global audio/brightness changes or a new brightness restoration lifecycle.

## HMOS implementation and lifetime

`AttachmentMediaPreview.ets` now uses the full supplied content area for the picture, with central play/pause and bottom seek/time/fullscreen controls layered over it. `AttachmentMediaGestures.ets` implements the enabled horizontal and double-tap behaviors plus timer-driven control visibility. Invisible controls also disable their hit testing. The 16-unit gesture inset leaves outer edges available for system interaction; actual touch routing and visual parity still need device evidence.

The horizontal calculation preserves Dart's signed rounding: halfway values round **away from zero**, whereas JavaScript `Math.round(-0.5)` produces negative zero. The implementation rounds the magnitude with the original sign and normalizes zero. Independent golden cases use a 100-second duration: −5 units means −1 second, +5 means +1 second, ±15 means ±2 seconds, and ±4.99 means zero.

Each accepted gesture, double-tap submission and visibility timeout carries its original lease token and timer generation. Lease replacement, fullscreen exit, failure, background disappearance or explicit cancellation prevents delayed submissions from controlling another attachment. Index passes captured tokens through `onToggle(token)`, `onSeek(position, token)` and `onFullscreen(token)`; the playback model validates nonempty supplied tokens against its current source.

Surface ownership is separately guarded. Each token/epoch gets an independent `XComponentController`. `onLoad` captures that controller, and API 26 `onSurfaceDestroyed(actualId)` forwards the actual destroyed surface ID, rather than reading a mutable newer ID. The component rejects mismatched token/epoch/ID callbacks. `AttachmentPlayback.clearSurface(token, id)` accepts only the currently admitted exact ID, disables controls for a lost video surface, cancels an unexecuted preparation, and pauses playback. A stale destruction cannot clear a replacement surface or another lease.

The relevant installed SDK definitions are `C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/component/xcomponent.d.ts:251-289` and the existing AVPlayer interface. Native player release and descriptor closure remain ordered before the parent deletes the verified preview lease; rejected release retains ownership for explicit retry.

## Verification scope

Fresh host execution of the actual ETS implementation passed **45 / 45** tests, zero failures, skipped or cancelled:

```text
node --test hmos/tool/attachment-playback-model.test.cjs hmos/tool/attachment-media-gestures-model.test.cjs
```

The **24 new** gesture/control tests cover horizontal formulas and signed-round golden cases, out-of-range/zero/unknown-duration handling, token/fullscreen/failure cancellation, equal-third double taps, ten-second accumulation, exact 400/200 ms timing, timer generation rejection, three-second visibility, drag timer suspension, token-bound playback commands and lost/stale Surface handling. The **21 existing** playback/SDK-adapter tests cover paused preparation, callback ordering, background pause, replacement admission, immutable source/view snapshots, release retry and readonly exact-length descriptor ownership.

These tests transpile the actual ETS models and run SDK adapter cases with injected mocks. They prove the stated host logic and ordering, not actual HarmonyOS decoder behavior, ArkUI touch recognition, input latency, audible output, fullscreen rendering/restoration or physical ARM64 acceptance. The parent delivery report records the final HAP build and the broader 303-test result separately; this source audit does not substitute those counts for dev.15 device acceptance.
