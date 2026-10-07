# dev.14 media and native fullscreen source audit

Audited 2026-10-07. This document records source/API evidence; it does not qualify actual decoding, native fullscreen, restoration or physical-device behavior.

## Flutter behavior inspected

The current reference `build/win-cloud-20261005/lib/attachments/attachment_view.dart` opens audio/video with `Player.open(..., play: false)` and uses `Video` with `MaterialVideoControls` inside a 620 × 320 preview. Its disposal waits for the player before releasing the resolved attachment bytes.

The reference lockfile pins `media_kit_video 2.0.1`; that exact installed package was inspected under the local Pub cache. `controls/methods/fullscreen.dart` pushes a root-navigator fullscreen route with the same player/controller and removes width/height restrictions. The fullscreen route intercepts Back to exit native fullscreen. In `src/video/video_texture.dart`, mobile default entry hides system overlays and requests landscape, while exit restores overlays and clears the orientation override. Playing-state changes acquire/release wake lock, and background pauses without automatic foreground replay. The Windows implementation in `windows/utils.cc` removes the frame, fills the monitor, and restores the earlier window placement. Primary upstream: [media-kit source](https://github.com/media-kit/media-kit).

## API 26 surface and descriptor ownership

The installed SDK `@ohos.multimedia.media.d.ts` exposes AVPlayer `fdSrc` with explicit offset/length; the caller owns descriptor closure. A video surface is first assigned in `initialized` before `prepare`, and may be updated in prepared/playing/paused/completed states. `VIDEO_SCALE_TYPE_SCALED_ASPECT` preserves aspect ratio with black padding. The implementation supplies only an already verified private-file descriptor, opened read-only with NOFOLLOW, and checks its exact byte length before passing it to AVPlayer. It waits for release acknowledgment before closing that descriptor. Rejected release or descriptor close retains the adapter for explicit retry. See the official [AVPlayer sample](https://gitcode.com/openharmony/codelabs/tree/master/Media/VideoPlayer) and [XComponent documentation](https://github.com/openharmony/docs/blob/master/zh-cn/application-dev/reference/apis-arkui/arkui-ts/ts-basic-components-xcomponent.md).

## Native fullscreen and reversible state

The installed API 26 `@ohos.window.d.ts` provides `getWindowProperties().isLayoutFullScreen`, `getImmersiveModeEnabledState()`, `getPreferredOrientation()`, `setWindowLayoutFullScreen`, `setImmersiveModeEnabledState`, `setWindowSystemBarEnable` and `setPreferredOrientation`. `getWindowSystemBarProperties()` returns color, icon and animation properties; it does not expose status/navigation enable flags. `isFullScreen` alone cannot reconstruct those flags. The feature therefore depends on EntryAbility establishing and recording the explicit app-owned status/navigation baseline before admitting fullscreen on that same window ID. It does not infer arbitrary prior bar policy from color properties. Official [WindowKit reference](https://gitcode.com/openharmony/docs/blob/OpenHarmony_feature_20250328/en/application-dev/reference/apis-arkui/js-apis-window.md).

`AttachmentFullscreen` captures the original layout, immersive request, orientation and app-owned bar policy before any mutation. Entry hides bars and requests landscape. Exit restores bars/orientation/layout/immersive on the original window. Every restoration is attempted even when one rejects. A failed restoration keeps the snapshot and an exit control; only a further explicit exit retries it. Back, close, background and media error must request exit through this controller. A window restoration failure is separate from player ownership: the root must still wait for player disposal before deleting the verified file.

WindowKit says system-bar configuration acknowledgment does not itself prove the bars are visibly hidden. Floating/split-screen and 2-in-1 modes may limit these APIs. Rendered UI plus actual window/system-bar evidence is required for those modes. Volume/brightness/seek gestures, wake lock, desktop shortcuts and the complete MaterialVideoControls design remain broader parity work.

## Local behavioral evidence

`attachment-playback-model.test.cjs`: 21 tests against actual ETS coordinator and adapter, including initialization and close races, stale callbacks, no autoplay, accepted-command serialization, background pause, stale surface disappearance isolation, failed release retention, exact descriptor range and release-before-close.

`attachment-fullscreen-model.test.cjs`: 15 tests against actual ETS controller and WindowKit adapter, including original-state preservation, concurrent admission/cancellation, ambiguous mutation acknowledgment, all-step restoration, retained failed restoration and explicit retry without recapturing the altered window.

ArkTS compilation and emulator/device evidence are maintained by the root task and must be added to the final validation report. These local checks use mocked platform responses and do not prove the native operations on a device.
