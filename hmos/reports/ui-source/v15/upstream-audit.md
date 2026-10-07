# dev.15 upstream source audit

Initial observation 2026-10-07 12:01–12:06 Asia/Shanghai; delivery-state supplement after 12:22. The HMOS checkout was `codex/ArkTsUI`, HEAD `748ea8e7c01beed72a303cb5c5e7ed425f9d7331` before the uncommitted dev.15 changes. The initial source audit is read-only apart from its two reports. This delivery supplement also only edits these reports; it does not operate the device, reset or start an emulator, update a reference checkout, commit, push, or message another Codex thread. Separately authorized model-test additions are reviewed in the delivery supplement below.

## Fresh local references

| Worktree | Branch | HEAD | Flutter version |
| --- | --- | --- | --- |
| `build/io-safety-refactor` | `codex/m03-stream-revocation-backpressure` | `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` | `0.1.9-test.57+61` |
| `build/win-cloud-20261005` | `codex/windows-sdk-convergence-20261005` | `e83cdf1d3506b001d07ffceaaa9f98b688bcc150` | `0.1.9-test.58+62` |
| `build/windows-sdk-reconstruction` | `codex/windows-sdk-qualification-20261003` | `20669f671152972470340a65eac3458dd2f61b4d` | `0.1.9-test.58+62` |

These HEADs are unchanged from the dev.14 audit. Fresh scoped status was empty for `lib/main.dart`, `lib/attachments/attachment_view.dart`, `lib/attachments/attachment.dart`, `lib/attachments/file_access_native.dart` and `pubspec.yaml`. This does not assert that the entire reference worktrees are clean.

Fresh working-file Git blobs are identical across all three references:

| Source | Git blob |
| --- | --- |
| `lib/main.dart` | `2e00f41251c3215f0b6d886712c86adfdf2bb6e6` |
| `lib/attachments/attachment_view.dart` | `76be11b131c15d8c4c101d15f6d9568400eca3c8` |
| `lib/attachments/attachment.dart` | `751b45933d25bfcd32b779885c104eea67c87ecc` |
| `lib/attachments/file_access_native.dart` | `3116024784acf5e9b1a93cd2a5306fc0a93acbbe` |

## Concrete source target retained

The following paths/lines use `build/io-safety-refactor` and were freshly inspected:

- `main.dart:5798` invokes `importFiles(await openFiles())`: the import affordance allows the file selector's list result; `:5439–5473` imports sequentially, limits the selected attachment list to 20, validates before storing, increments edit generation, and fills a blank title from the imported name, truncated to 60 UTF-16 code units. The dev.14 baseline still used single selection and did not fill blank titles; dev.15's implementation changes are described separately below. The 64 MiB preparation limit remains a gap against `attachment.dart:28,119`, which defines the ordinary Flutter 200 MiB import ceiling.
- `attachment_view.dart:91` routes previewable kinds to a dialog and generic files to external opening. `:155–159` creates the player/controller and opens with `play: false`; `:168–177` releases the resolved file after player disposal. `:185–198` uses 620 × 320 media content and explicitly selects `MaterialVideoControls` for both video and audio. The complete installed dependency control audit remains in [dev.14/upstream-audit.md](../v14/upstream-audit.md).
- At the initial observation, the published dev.14 device record stopped before choosing the exact WAV. Root subsequently gathered the successful dev.14 baseline evidence described below. These new baseline observations do not qualify the changed dev.15 controls or its batch selection.

## Source-thread compact observation

One fresh `wait_threads` call with `timeoutMs: 0` returned thread `01a085bd-7a94-7f93-8a1f-1ecf417f5ee3` as active, turn `01a1147f-71bf-7b82-b5ad-a35161a025f6` in progress, revision **4**, cursor `652321f3-8157-4672-ab76-c1c1442eb0fe:4`.

Its latest commentary reports an unverified C24 path: terminating a process also cancels output-reading tasks, which may classify a normal termination as incomplete output evidence and prevent resource settlement. That thread will exercise a real synthetic subprocess before choosing a repair. Another agent is adding session connection interfaces using the existing content repository and worker thread. This is current source-thread work in progress, not a completed repair or HMOS acceptance. The compact snapshot did not provide a new passing-test count, UI commit or full SDK qualification. No follow-up message was sent.

This audit does not fetch remote branches or assert freshness beyond the three observed local references and one compact thread snapshot. Full Windows/HMOS feature parity remains open.

## Dev.15 delivery-state supplement

The reviewed dev.15 source separates `AttachmentFiles.selectUris(maximum)` from `prepareUri(uri)`. Its coordinator bounds the one picker request by both remaining selected-asset slots and remaining per-draft import-record slots, up to 20. Every returned URI is checked before any first-item preparation, including sparse arrays, duplicates, non-string/empty/control-containing and oversized results. Each helper grants only its own returned URIs once; preparation consumes that grant and recalculates the shared physical spool count/byte budget. Discarding unread grants does not remove an already prepared/imported spool. Each item's exact owner, edit epoch, generation, operation and receipt are checked before moving to the next; stop/Unknown retains existing per-item journals without replay. A confirmed import may fill a blank title before the existing draft pin update. Picker filtering/default URI settings remain unchanged. Import stays within the existing 64 MiB preparation budget.

Root's `arkts-final-model-tests.log` was freshly read: **303/303 PASS**, actual ETS source with synthetic dependencies. `hap-release-build.log` records final integrated ArkTS build/package success. A fresh hash of `entry/build/default/outputs/default/entry-default-unsigned.hap` was **7347C8A8E2136D33A888BDD606766D788A6729633D7449A4BBA16F9CF8624A4B**, **24,856,916 bytes**, version dev.15 / 1000015. This is an unsigned debug HAP and does not establish signing, remote publication or native device acceptance. The reviewed source adds control visibility/timers, fullscreen horizontal and double-tap seek, exact token-bound callbacks and XComponent surface destruction handling. Actual new controls/gestures and batch-provider behavior on the dev.15 package remain **NOT_RUN**.

The root-owned [device-baseline](device-baseline/) evidence uses the already installed **dev.14** package and the same recoverable `HMOS-media-20261007-A` draft. The actual own WAV was selected/imported; `audio-preview-dev14.log` and its screenshot observe paused/no autoplay, `0:00 / 0:12`; `audio-playback-dev14.log` records play/pause, stable paused time and seek from `0:04` to `0:07`; `audio-fullscreen-dev14.log`/screenshots record fullscreen and Back restoration; `audio-close-dev14.log` records close without a cleanup-retry notice. These are scoped native playback/UI observations, not independently heard audio or complete decoder/window-policy coverage. The own MP4 is visible as an imported draft asset in `video-selected-1.json` but was not played; the PDF was not selected. PreviewKit readable content, MP4 playback and final dev.15 device behavior remain unqualified. Successful later selection does not determine the cause of the earlier empty return.
