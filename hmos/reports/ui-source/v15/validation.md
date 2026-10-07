# HMOS dev.15 validation — 2026-10-07

Scope: continue the isolated Rust/ArkUI migration toward Windows UI and implementation parity, and deliver the authorized update only to `codex/ArkTsUI`. The full goal remains **OPEN**. This delivery qualifies source/model/build scope. **The dev.15 package has not been installed or exercised on a device.** No main merge or main push is authorized.

## Implemented behavior

- DocumentViewPicker supports a bounded multi-file result, at most 20 and no more than the remaining selected-asset and complete staging-record slots. Consumed/retired records still count until the existing explicit reconciliation prunes them. The full returned list is validated before admission, including oversized, duplicate, malformed and sparse lists; no first-only slicing is used.
- Each exact selected URI is an ephemeral one-shot grant. The original provider order is preserved, preparation/import/pin confirmation runs sequentially, and every item has its own frozen operation identity and exact decimal draft generation. Existing private spool/request journals, durable Rust import records and EditorDraftCoordinator remain authoritative; there is no parallel persistent batch state machine.
- An unchanged existing card's cancelled picker creates no owner journal. Already scheduled raw edits are settled before selection. After a valid non-empty selection, owner confirmation cannot adopt a changed editor/title/edit epoch. Stop, cancellation, failure, Unknown and owner/generation changes prevent subsequent items; already confirmed items remain and admitted requests are neither automatically discarded nor replayed.
- Only an exact confirmed import may propose a filename for the current still-whitespace-empty title. Its first 60 UTF-16 units are inserted together with the new pin in one values update and confirmed draft save. A trailing half-surrogate is removed for validity. A new non-empty user title wins. Selection and composition are reset for the inserted title without replacing current input from a stale receipt.
- After spool cleanup awaits, the original editor owner is checked again. Asset addition binds the captured draft and validates record card/draft IDs before mutating values; editor replacement resets batch progress and rejects late old-owner UI updates.
- Media controls now have visibility state, 3-second inactivity hiding, interaction/slider holds and a 300 ms transition. Fullscreen horizontal drag seeks with source rounding/bounds, and left/right double-tap applies ±10-second steps with accumulated feedback, a 400 ms submit delay and 200 ms feedback fade. The center region remains a visibility tap. Timers, gestures and play/seek/fullscreen controls retain the original lease token.
- Video surface destruction is bound to the actual originating surface/token, pauses that media session and does not redirect an old callback to a new surface. Existing no-autoplay, background pause, native-player/FD release-before-file-cleanup and original-window restoration contracts remain.

Current Flutter `attachment_view` does not pass the volume/brightness callbacks required to enable vertical control gestures in its installed dependency. They are disabled for this reference and must not be described as already-enabled default requirements. The fresh correction is documented in [media-controls-source-audit.md](media-controls-source-audit.md); dev.14's historical report is not rewritten.

## Final package and build

- Version `0.1.0-hmos-dev.15` / `1000015`, bundle `dev.morrow.hmos`; unsigned debug HAP, **24,856,916 bytes**.
- Final SHA-256: `7347C8A8E2136D33A888BDD606766D788A6729633D7449A4BBA16F9CF8624A4B`.
- [hap-release-build.log](hap-release-build.log): integrated API 26 ArkTS/HAP packaging **SUCCESS**, 11.268 seconds. Signing is skipped because no signing profile is configured. Earlier candidate/failure logs are retained separately and are not the final package identity.
- An identical ignored local archive exists at `.build/artifacts/dev15/entry-default-unsigned.hap`; its size and complete SHA-256 were freshly checked. The HAP itself is not committed as a source artifact.
- [native-package-comparison.json](native-package-comparison.json): the four ARM64/x64 `libmorrow.so`/`libc++_shared.so` entries have exactly the same uncompressed bytes as dev.14; native source differences are empty. This is verified reuse, not a new native build, current upstream full Rust parity or ARM64 physical-device evidence.
- [manifest-disk-check.log](manifest-disk-check.log) and [manifest-staged-check.log](manifest-staged-check.log): **298 production build inputs PASS**, against the final `7347C8…` HAP, both from disk and the staged index. These byte checks do not establish remote publication or device qualification.

## Actual-source model checks

- [arkts-final-model-tests.log](arkts-final-model-tests.log): **303/303 PASS**, executing actual ETS models and adapter source through the installed SDK TypeScript harness. Picker/provider/player/window operations in these checks are synthetic; passing them does not prove runtime grants, native formats, IME or rendering.
- AttachmentImportSelection: **28/28**, covering the complete three-URI sequence, independent original identities and exact large generations, all-list capacity validation including sparse arrays, cancellation before confirmation, generation-zero owner establishment, shared live handles, stop during an issued operation, late confirmed preservation, failed/retained/Unknown stopping, foreign receipts and exact title/epoch progression.
- AttachmentFiles: **97/97**, including the new multi-file selection/one-shot registry, sequential preparations, exact URI use, changed shared spool budgets/counts, whole-list refusal before native reads, failed URI consumption without automatic retry, preserved already-prepared spools and existing preview/cleanup ownership cases. The final full run also covers current playback, fullscreen, gesture/control and previous appearance/query/card/draft/paste/order/Markdown/image-lifetime models.
- Fresh Rust host tests and fresh ARM64 native build: **NOT_RUN**. Historical dev.13 72/72 Rust results are not counted as this round's execution.
- [shared-reference.log](shared-reference.log): **228 frozen shared SHA-256 values match**. [reference-drift.json](../../reference-drift.json) separately records **122 changed/added upstream paths not synchronized**. Upstream `matches=false` means reference drift, not failure of the frozen snapshot's hashes.

## Device scope: old dev.14 baseline only

**dev.15 device qualification is NOT_RUN.** There is no dev.15 install, multi-select/empty-title, controls-visibility, horizontal/double-tap, video-playback or PreviewKit result in this delivery. Old-package observations cannot qualify newly changed controls or import coordination.

Root used the existing Pura X View2 / HarmonyOS 7.0.0.106 / API 26 / x64 instance and the already installed **dev.14 / 1000014**, whose final HAP SHA-256 is `43E4021710E06FEAA268CC8BA762DFF43FAD4D9CAD7881D4EE72CC58BFF91502`. The following files under [device-baseline](device-baseline) belong to that old package, not dev.15:

- `restore-dev14.log`, the complete picker transition trees/screenshots, `picker-callback-after-audio.log` and `audio-selected-1.*`: the existing public **HMOS-media-20261007-A** raw fixture was restored, its own `HMOS-dev14-tone.wav` was actually visible/selected, the provider reported error code 0, and one attachment pin was confirmed. The preceding dev.14 stopped-driver observation is kept as history; it is not a deterministic provider-defect claim.
- [audio-preview-dev14.log](device-baseline/audio-preview-dev14.log) and `audio-prepared.*`: actual native WAV preparation reports **0:00 / 0:12**, paused without autoplay.
- [audio-playback-dev14.log](device-baseline/audio-playback-dev14.log) and `audio-paused-seek.*`: play/pause and the seek control were exercised; the observed position changed from **0:04 / 0:12** to **0:07 / 0:12**. This is UI/player position evidence, not audible-output proof.
- [audio-fullscreen-dev14.log](device-baseline/audio-fullscreen-dev14.log), `audio-fullscreen.*` and `audio-fullscreen-restored.*`: fullscreen and Back/return to the prior preview were observed on dev.14. These observations do not qualify dev.15 visibility/gesture logic or arbitrary system-bar/window modes.
- [audio-close-dev14.log](device-baseline/audio-close-dev14.log): the WAV preview closed successfully. This is a scoped healthy-runtime close observation, not rejection/storage-failure or all-reader lifetime qualification.
- `video-selected-1.*`: the task's own MP4 import succeeded into the existing fixture. **Video playback/decoded frames are NOT_RUN**. PDF/PreviewKit readable content and actual reader lifetime are **NOT_RUN**. No business-card publication is asserted from these draft import observations.

The WAV/MP4/PDF procedural content and original hashes remain documented in [dev.14/fixture-provenance.json](../v14/fixture-provenance.json). Host fixture decoding, copied-file hashes, synthetic model callbacks and UI position labels do not prove audible output or all platform formats.

## Source follow and remaining qualification

[upstream-audit.md](upstream-audit.md) freshly records unchanged local reference HEADs (`925fb8ca…`, `e83cdf1d…`, `20669f67…`) and matching clean scoped UI/attachment Dart blobs. Its compact source-thread snapshot remains active and does not supply new complete Windows SDK/VM qualification. [import-source-audit.md](import-source-audit.md) records exact sequential import/title design; [platform-picker-audit.md](platform-picker-audit.md) separates observed picker outcomes from unsupported defect inferences.

The final dev.15 package still needs actual multi-file grants/import/title/save/restart, stop/failure/Unknown recovery, visibility and fullscreen horizontal/double-tap interaction, video frames, background/close/restoration and generic readable-preview/lifetime validation. Actual audible output, format/error/corrupt-content matrices, permissions/storage failure, physical cache after process death, wide/theme/keyboard/IME performance, image pinch/pan/GIF and signing/ARM64 physical devices remain unqualified.

Current import/pin 64 MiB budgets, 20-record/asset bounds, eight inline images/64 MiB and separate one-dialog/one-system-preview 200 MiB bounds remain unchanged. Session preview ledgers are process-local, retain failed registered cleanup and do not scan/delete all orphan cache after process death. PreviewKit is a system-preview route; Flutter's arbitrary-format default opener is not proven equivalent.

The unsealed 256-card development Store, production HUKS/identity/ownership/audit/backup, captured S1/S2 and formal business cross-process Unknown recovery, rich HTML/Office/clipboard files/images, continuous full-document selection, complete languages/fonts/backgrounds, plugin/HTTP/TLS/service runtime and separate music/lyrics workspace remain open. No test count or package version closes the full parity goal.

## Publication

This source delivery targets only `codex/ArkTsUI`. The freshly observed remote baseline was `748ea8e7c01beed72a303cb5c5e7ed425f9d7331`; remote `main` was `4de7fe2dbc2504e8342598fabea37ae41fa3d583`. The commit does not embed its own final hash or claim a push result before publication. Root records post-push remote readback separately in ignored `.build/delivery/dev15/branch-delivery.json` and reports the verified result in the delivery message. No merge, main push, release tag or signed distribution is part of this delivery.
