# dev.14 attachment media source audit

Observed 2026-10-07, 11:32–11:48 Asia/Shanghai. The HMOS checkout was on `codex/ArkTsUI`, HEAD `06e4d31085212de77a1f4441a3bcea26ed3dc675` when this audit began. This audit did not change Flutter/Rust reference code, HMOS source code, versions, commits, or remote refs. It did not install an app, drive the device UI, start/reset/reconfigure an emulator, or send a message to a source thread. It created this report, procedural fixtures and their provenance manifest. Root separately authorized a pinned fixture-only tool installation under ignored `hmos/.build/fixture-tools` after native Windows rendering failed; no global package installation or system PATH change was made.

## Current local references

| Worktree | Branch | Observed HEAD | Flutter version |
| --- | --- | --- | --- |
| `build/io-safety-refactor` | `codex/m03-stream-revocation-backpressure` | `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` | `0.1.9-test.57+61` |
| `build/win-cloud-20261005` | `codex/windows-sdk-convergence-20261005` | `e83cdf1d3506b001d07ffceaaa9f98b688bcc150` | `0.1.9-test.58+62` |
| `build/windows-sdk-reconstruction` | `codex/windows-sdk-qualification-20261003` | `20669f671152972470340a65eac3458dd2f61b4d` | `0.1.9-test.58+62` |

These three local HEADs match the dev.13 audit. Scoped `git status --porcelain` was empty for `lib/main.dart`, `lib/attachments/attachment_view.dart`, `lib/attachments/attachment.dart`, `lib/attachments/file_access_native.dart`, and `pubspec.yaml` in all three worktrees. Working-file Git blobs for the four Dart sources match across all three references:

| Source | Shared Git blob | Active-source raw SHA-256 |
| --- | --- | --- |
| `lib/main.dart` | `2e00f41251c3215f0b6d886712c86adfdf2bb6e6` | `6203B7465F320DC8463DFACB5387EA8B437EC3EC1458CCA7E215E44B8F65CB1E` |
| `lib/attachments/attachment_view.dart` | `76be11b131c15d8c4c101d15f6d9568400eca3c8` | `F3BAA59202D433DEB1683F77932B31A210FB4EE5D382AD0A8ECFEB2A4DEC0FE4` |
| `lib/attachments/attachment.dart` | `751b45933d25bfcd32b779885c104eea67c87ecc` | `8BBEB6A11AD7854B32EBD0C0E5C0EA2EDE9D5CDAC74E1D2DE9B7966BFA0FAE1F` |
| `lib/attachments/file_access_native.dart` | `3116024784acf5e9b1a93cd2a5306fc0a93acbbe` | `3AE2D37173F56E67AAA572D44AB2F4C197CA0EB503FDAEDC41410E07B8FD474A` |

This is a bounded local design audit. It is not a complete SDK audit, remote freshness check, or proof of unpublished source-thread edits.

## Exact Flutter attachment behavior

Locations below refer to `build/io-safety-refactor`. The other two audited references contain the same four source blobs.

| Requirement | Source and behavior |
| --- | --- |
| Attachment rows | `attachment_view.dart:62–115`: vertical padding 4; surface at alpha .16; rounded radius 12 and themed border; horizontal content padding 10. Leading icons distinguish image/GIF, video, audio, generic file, and recognized 3D/CAD suffixes. Name uses 12-point ink text, maximum two lines with ellipsis. Subtitle uses 10-point muted text and combines preview/default-open description, upper-case extension, and formatted size. |
| Tap routing | `attachment_view.dart:91–96`: image/GIF/audio/video opens the preview dialog. Generic file invokes the default external opener. `attachment.dart:52` defines previewable as every kind except generic file. |
| Export and removal | `attachment_view.dart:100–110`: every row includes an export/download action. Removal appears only when the editor supplies `onRemove`. `main.dart:5857–5865` removes the selected attachment and increments edit generation; the same row component is used in read detail at `main.dart:4772`. |
| Resolve before playback | `attachment_view.dart:141–164`: resolve the selected attachment via `TextureRepository`, release a late resolution if the component has unmounted, initialize MediaKit, create one `Player` and `VideoController`, subscribe to media errors, and open `Media(data.uri)` with `play: false`. Both audio and video take this same path. There is no autoplay or playlist construction in this dialog. |
| Dialog geometry and status | `attachment_view.dart:181–213`: dialog title is the attachment name, subtitle describes attachment preview, content size is 620 × 320. Unresolved content shows a spinner. Player-stream errors show media failure; resolution/open exceptions show read failure. Image decoding has a separate image-failure message. A close button dismisses the dialog. This source distinguishes failure classes but does not implement a retry button inside the preview. |
| Actual media widget | `attachment_view.dart:197–198`: both audio and video use `Video(controller: video!, controls: MaterialVideoControls)`. The source explicitly selects MaterialVideoControls, including on Windows; it does not select AdaptiveVideoControls or MaterialDesktopVideoControls. Audio therefore uses the same visual control area even without a video picture. |
| Lifetime | `attachment_view.dart:168–177`: cancel the player error subscription; if a player exists, wait for its disposal completion before releasing the resolved file. With no player, release the resolved data directly. A matching HMOS implementation must keep the verified file alive while the decoder holds it, and must not let a late callback operate on the next attachment. |
| File import limits | `main.dart:5439–5473`: import sequentially from a list; stop at 20 selected attachments; validate each import before storage; if title is blank after an import, insert its display name truncated to 60 UTF-16 code units. `attachment.dart:28,116–121`: ordinary source import allows up to 200 MiB per file. Existing HMOS 64 MiB preparation/pin limits remain a separate parity gap; media preview does not close it. |
| Kind classification | `texture_source.dart:28–37`: video suffixes are mp4, webm, mov, mkv, m4v; audio classification uses `music/audio_formats.dart` standardAudioExtensions (including wav, mp3, flac, m4a, aac, ogg, opus, and others). The list describes classification, not a guarantee every platform can decode every format. |

### Controls come from the installed dependency

The active reference's `.dart_tool/package_config.json` resolves `media_kit` 1.2.6 and `media_kit_video` 2.0.1. The inspected installed source is `C:/Users/Administrator/AppData/Local/Pub/Cache/hosted/pub.dev/media_kit_video-2.0.1/lib/media_kit_video_controls/src/controls/material.dart`, SHA-256 `D5A85321C74FDA473FE990F63928F769CF92D51E7132053038EDADB354DE6917`.

- Normal controls (`material.dart:294–356`) have a seek bar, a 48-point central play/pause button, current/total position, and a fullscreen button. Previous/next controls are playlist navigation; the implementation at lines 1834–1881 hides them for the single-item media opened by this dialog. They are not fixed ±10-second jump buttons.
- Normal-mode seek, volume, brightness and double-tap seek gestures are disabled by default. Controls start hidden, respond to interaction, use a 3-second hover timeout and 300 ms transition; the player is separately opened paused.
- Fullscreen defaults (`material.dart:38–104`) enable horizontal seek and vertical volume/brightness gestures, plus left/right double-tap seek. Forward/backward double-tap durations default to 10 seconds (`:306–307`). The fullscreen central play/pause icon is 56 points. There is no default normal-mode volume slider or playback-rate menu in this application's selected control configuration.
- `MaterialPositionIndicator` listens independently to position and duration, then displays current / total (`material.dart:1958–2020`). The seek bar invokes player seek (`:1594,1611`), and the play button invokes playOrPause (`:1797–1798`).

Source control defaults are an implementation target. They do not prove Flutter runtime behavior on every OS or HMOS device parity. If HMOS adapts controls to touch size or exposes extra seek/volume buttons, root should record that adaptation and still verify the source's core playback, seek, position, fullscreen, error, and lifetime behavior.

### Generic opening and fallback

`file_access_native.dart:33–61` checks the lower-case attachment suffix before external opening. The exact export-only list is `exe`, `com`, `msi`, `bat`, `cmd`, `ps1`, `vbs`, `js`, `lnk`, `url`, `scr`, `reg`. These formats invoke the save-location/export flow (`:49–51`). An ordinary file checks existence and launches its file URI with externalApplication; Android uses its own MethodChannel. Failure reports that a corresponding app may need installation or the attachment can be saved first. Row-level errors are surfaced by a snack bar (`attachment_view.dart:18–37`). No success should be inferred merely from an external-handler intent returning.

## Source-thread compact snapshot

One immediate read-only `wait_threads` call used the dev.13 cursor. It returned thread `01a085bd-7a94-7f93-8a1f-1ecf417f5ee3` as active, turn `01a11447-5538-7513-b2bf-9cbc56a7d463` still in progress, revision 3, cursor `652321f3-8157-4672-ab76-c1c1442eb0fe:3`. Current commentary reports 12 Windows output/wait repair tests passed, covering cancellation after reading began, read failure before channel closure, independent stderr failure, and wait failure without a fabricated exit code. It explicitly leaves complete SDK and VM sandbox qualification unfinished. This snapshot is scoped source progress, not a completed source goal or a new attachment UI commit. No source-thread message was sent.

## Live environment observation

Read-only process, listener, native instance, HDC and runtime queries establish the current identity:

- `Emulator.exe` PID 16972 is live, with command line naming existing `Pura X View2` and `-bootmode snapshot -noWindow`. Listener 127.0.0.1:5555 is owned by the same PID. HDC lists that address.
- Native `Emulator.exe -list -details` identifies `Pura X View2`, UUID `01fc19c8-444a-42f1-9f70-79321a2502b3`, as running. Original Pura X View and Huawei_TripleFold are not running. This matches the dev.13 recovered instance; the port is not treated as identity by itself.
- Runtime queries return API 26, x86_64, software `emulator 7.0.0.106(SP1DEVC00E999R4P11)`. Native config details still describe 7.0.0.107, so the runtime version is authoritative for this observation.

This audit does not establish current app rendering/playback, decoder health, real ARM64 behavior, or signing. No logfile or lock file was used as evidence of a live process.

## Procedural qualification fixtures

`fixture-provenance.json` records hashes, exact generation recipes, the pinned tool/binary identity, failed native-generation observations, host decode checks, and the qualification boundary. Files under `fixtures/` are newly generated test content with no network media, user recordings, camera, or microphone input:

| Fixture | Actual content | Size | SHA-256 |
| --- | --- | --- | --- |
| `HMOS-dev14-tone.wav` | 12-second signed PCM16 mono 48 kHz; one 250 ms 1 kHz integer-triangle pulse each second | 1,152,044 | `4CE919D013F1C31F116EC9487A8DC6CFDB80CF71377816AFA8EE5AA27619A60C` |
| `HMOS-dev14-bars.mp4` | 12-second 320 × 180 / 10 fps; three sequential shifted color-bar phases with bottom marker; H.264 Constrained Baseline yuv420p plus AAC LC mono 48 kHz | 51,509 | `DC9D8BA93F826029FB7C4D60BA7AEAD4AC302319FA29228533A2022E2EBC067A` |
| `HMOS-dev14-generic.pdf` | One PDF 1.4 page, English test label and RGB vector rectangles | 740 | `451AB55AED30FE494A0A32E5916D03D8D3FCF853B1CD96FEF17F7DD97ADD2C79` |

Three BMP files are the original procedural MP4 color-bar sources; they are retained with hashes for reproduction. The final MP4 was encoded once. Full host decode returned 120 video frames / 20,736,000 RGB24 bytes, and 1,152,000 PCM16 bytes for 12 seconds of audio. Nine sampled frames span all three phases; bar-center colors differ by at most 4 per RGB channel against the source, within the declared tolerance 15, and each phase's moving marker remains visible. The initial verifier incorrectly required exactly three decoded frame hashes despite lossy coding; there are seven. The current verifier uses content samples plus full frame count and does not replay encoding.

Standalone FFmpeg was not found in the inspected installed locations. PowerShell WinRT projection could not mutate MediaComposition collections. A compiled typed bridge created a zero-byte output but the rendering task failed before writing media; its process ended with exit1 and no task remained live. Only that exact, generated, still-empty output was removed after checking its path and size. This does not establish a general Windows renderer defect. Microsoft's [MediaComposition documentation](https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/media-compositions-and-editing) and [RenderToFileAsync API](https://learn.microsoft.com/en-us/uwp/api/windows.media.editing.mediacomposition.rendertofileasync) were used to verify the attempted native route.

Root then authorized project-local `imageio-ffmpeg==0.6.0` installation. The [official PyPI release](https://pypi.org/project/imageio-ffmpeg/0.6.0/) supplies the Windows wheel. Its published wheel SHA-256 is `02fa47c83703c37df6bfe4896aab339013f62bf02c5ebf2dce6da56af04ffc0a`. The installed bundled FFmpeg reports version 7.1, with binary SHA-256 `2CE797A0F88D7F067180338FB227F7B1928EA727BD9A4D7A1D022F7C52AF71A3`. The tool stays in ignored `hmos/.build/fixture-tools`; it is not an application dependency or packaged HAP input.

Host fixture generation and full decode are evidence that these public test inputs are valid on the host. They do not prove HMOS playback, audible output, seek/fullscreen interaction, PreviewKit PDF rendering, decoder error recovery, exact draft/business binding, or cleanup. Root records final HMOS build and actual device outcomes separately. Physical ARM64/signing, rich clipboard capture, import-size parity, full media-format coverage, production HUKS/identity/lease/audit/backup, and the complete Flutter UI/IME/theme/layout matrix remain broader requirements.
