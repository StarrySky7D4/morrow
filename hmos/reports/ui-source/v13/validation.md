# HMOS dev.13 validation — 2026-10-07

Scope: continue the original Windows UI/implementation parity goal in the isolated `hmos/` project. The goal remains **OPEN**. This report distinguishes actual-source model tests, the built package, actual emulator UI, and remaining platform/product qualifications.

## Implemented behavior

- Markdown `attachment:` images resolve image/GIF names or asset IDs in attachment order, following the Flutter `idea_markdown.dart` URI rules. The pure resolver also supports an explicitly supplied known location alias; current HMOS asset DTOs provide names and IDs, not original desktop filesystem locations. Numbers are literal identifiers, never asset positions. Names decode exactly once. Paragraph and table images share a verified read per exact selected asset/request.
- The UI reads current business assets with complete frozen source/revision or selected draft pins with exact confirmed generation. It never turns an arbitrary Markdown path into filesystem access. Source/pin replacement, removal, disposal and late completion clear/release the old image. Images use the SDK file URI API and reconstruct their ArkUI identity for each verified temporary path. Remote images retain an explicit user read button and validated HTTP(S) URI.
- Image preview distinguishes native verification, decode loading and decode failure. Buttons/pinch use Flutter's configured 0.8–2.5 range, constrained pan and double-tap/reset. Matching scale parameters do not prove full Flutter gesture equivalence.
- Editor attachment rows now export through the existing exact source/pin read and whole-file verified save picker, without publishing a business card.
- A failed preview cleanup retains its immutable registered directory/token and quota. An explicit retry can clean failed entries even when their old UI/helper disappeared; healthy live images are excluded. There is no automatic retry or inferred cleanup success.

## Final package and build evidence

- Version `0.1.0-hmos-dev.13` / `1000013`, bundle `dev.morrow.hmos`; unsigned debug HAP, 24,549,446 bytes.
- SHA-256 `F782293778D5049DFD8F4D59C77E63559DCF8D80A8C94F32C29B50DC3EC39143`.
- An identical local copy is archived at `.build/artifacts/dev13/entry-default-unsigned.hap`. Build outputs and this archive are ignored by Git; the repository records its hash and evidence, not a signed release binary.
- [hap-final-build.log](hap-final-build.log): actual API26 ArkTS compilation and HAP packaging succeeded; signing was skipped because the project has no signing profile. [hap-build.log](hap-build.log) is the earlier integration candidate, SHA `8DE3BE2564F07DF2080B55FB773BF895AF4DB4175A4E3311CC2E67C8A787FFFA`, not the final package.
- [device-final-install.log](device-final-install.log) and [bundle-final.json](bundle-final.json): final package installed on the existing x64 Pura X View2 and reported dev.13. No uninstall/reset/recreation occurred.
- [native-package-comparison.json](native-package-comparison.json): ARM64 and x64 `libmorrow.so` bytes are identical to dev.12. This round reuses those native libraries; it does not claim a fresh ARM64 build or physical-device runtime qualification.

## Actual-source model checks

- [rust-host-tests.log](rust-host-tests.log): **72/72 PASS** on the Windows host. Shared Rust sources and native ABI were not changed in this round.
- [arkts-final-model-tests.log](arkts-final-model-tests.log): **163/163 PASS** across the actual ETS appearance/query/card/draft/paste/order/attachment/Markdown/image-lifetime models. Attachment provider operations are synthetic stubs, not system-provider qualification.
- AttachmentFiles **64** checks include independent inline/dialog admission, same-cache ledger scope, exact length and verified identity, cleanup failure/quota preservation/explicit retry, foreign-cache isolation and scale/pan bounds. Markdown **23** checks include **26 independent Dart 3.12 URI golden cases**, first eligible match, table references and explicit safe remote reads. VerifiedImages **8** checks cover deduplication, exact request replacement, stale success/failure, decode failure/retry, snapshots and disposal.
- [shared-reference.log](shared-reference.log): the frozen 228 shared files pass their byte hashes. Its `matches=false` describes upstream drift, not a modified HMOS snapshot. Current drift is recorded separately in `reports/reference-drift.json`.

## Final-package device evidence

Actual instance identity is UUID `01fc19c8-444a-42f1-9f70-79321a2502b3`, Pura X View2, HarmonyOS7.0.0.106/API26/x64, 1320×2232 pixels. The allocated HDC address this round is **127.0.0.1:5555**; addresses are not stable instance identities. [environment-audit.md](environment-audit.md) explains the native fallback for the CLI's missing-image false rejection. No system parameters or instance configuration were changed.

The task creates only its unique public development fixture **HMOS-inline-20261007-A** and named public test files. [device-seed.log](device-seed.log) records raw Markdown creation on the early package, without business publication; [device-restore-final.log](device-restore-final.log) restores that exact acknowledged draft on the final package rather than reseeding it.

- [device-preview-final.log](device-preview-final.log): after actual single-file URI selection, one image pin was confirmed. The test initially assumed a separate Done control and stopped; inspecting the app showed the single-selection picker had already accepted the clicked file. No import was replayed. The continuation verified the one exact selected asset, displayed an actual inline image, opened it, changed to125%, reset to100% and closed. `device/editor-inline-image.png`, `image-zoom-125.png` and `image-zoom-reset.png` are actual final-package screenshots and were visually inspected.
- [device-editor-export-save.log](device-editor-export-save.log) and [device/editor-export-byte-comparison.json](device/editor-export-byte-comparison.json): the confirmed raw pin exported through the actual system save URI to `HMOS-dev13-editor-export-20261007-A.png`; returned bytes equal the original **1,924,867** bytes, SHA-256 `D0C0BDD622E6349D8E7357035376C818F2DBD7DA5EE28E1752D4D8405E52AD0F`. This occurred before business publication.

- [device-publish-final.log](device-publish-final.log) stopped after successful save because the title predicate also matched the search TextInput. [device-publish-reconcile.log](device-publish-reconcile.log) inspected the already saved outcome; [device-published-resume.log](device-published-resume.log) selected the observed Text card title and verified its rendered image. Save was not replayed. Exactly one business card exists with ID `7eb2c5ee-5c8f-4fe6-b334-5972c8be5598`.
- [device-readonly-final.log](device-readonly-final.log) traversed the published body, observed one shared attachment image identity, explicit missing and remote placeholders, and read back the same single card after a process restart without creating a raw journal. [device-table-edit.log](device-table-edit.log) independently asserted that the table cell contains the shared attachment image; `device/published-table-image.png` is its actual final-package screenshot.
- [device-removal-final.log](device-removal-final.log): entering the existing card editor preserved the exact raw body and one selected image. Removing that exact asset immediately cleared the displayed inline image and replaced its reference with the missing placeholder before business save (`device/removed-image-raw-preview.png`). One explicit save, then process restart, retained the same business ID with zero selected assets and the byte-for-byte same raw Markdown. Reopening the unchanged editor and closing it left no raw journal. `device/removed-image-saved.png`, `removed-image-reopened.png` and `removed-image-reopened-raw.png` record the corresponding final-package states; `device/progress-inline.json` records each accepted stage.

The 292 production build inputs, final HAP byte count/hash and dev.13 version passed both disk and staged-index manifest verification. The evidence includes interrupted observations rather than removing them or counting them as accepted checks.

## Interrupted provider observations and test-fixture changes

The initial public `HMOS-dev13-invalid.png` contained54 bytes of intentional non-image data (original retained locally). Several browser observations returned a picker cancellation `errorcode=-1` after opening Download. [provider-click.log](provider-click.log) records a damaged-image metadata read failure; [picker-callback.log](picker-callback.log) records cancellation. Those events are correlated, not proof of a deterministic platform defect or an app decode callback. A later browse displayed the directory successfully.

An attempted move of this task-owned file out of Download was refused with Permission denied. The task then replaced only that same task-owned test filename with the known valid PNG via HDC, leaving the original invalid54 bytes in the local fixtures. It did not touch user files. This allowed normal-image validation to continue, but **actual app decode-failure qualification remains NOT_RUN**. The raw body keeps an unimported invalid-name reference, which is a missing-asset test, not a successfully imported corrupt image. Early stopped driver assumptions/logs are preserved and must not be counted as successful device stages.

## Remaining qualifications

Inline preview is bounded to eight registered active images /64MiB per process/cache root, plus one dialog with the existing export limit. Failures are explicit, not silently omitted. The retry ledger covers registered VerifiedPreview entries; cleanup after a read failure before registration remains best-effort and is outside that ledger. This budget does not scan all physical cache leftovers after process death. Full cleanup under real device permission/storage failures, corrupt-image decoding, pinch/pan/gesture interruption, GIF animation, large/wide/theme/IME matrices and ARM64 physical devices remain unqualified unless separately recorded above.

Audio/video playback, generic external file opening, multi-select/empty-title import, HTML/Office/clipboard image capture, production HUKS/identity/owner/audit/backup, captured S1/S2 and formal cross-process business Unknown recovery remain open. Current unsealed development Store,256-card limit and draft/import budgets stay in place. Local build/model/emulator evidence does not close full Flutter parity or signed distribution.

Source/platform observations are in [upstream-audit.md](upstream-audit.md) and [platform-audit.md](platform-audit.md). Only `codex/ArkTsUI` is authorized for publication; no main merge or main push.
