# HMOS dev.16 validation — 2026-10-07

Scope: deliver the dev.16 media-tap fix following the user's request to push the current update now. No new multi-selection device flow is started in this delivery. The continuing Windows UI and implementation parity goal remains **OPEN**. This record distinguishes actual dev.16 observations from the published dev.15 source delivery and old dev.14 baseline; incomplete results are not PASS. Delivery remains limited to `codex/ArkTsUI`, with no main merge or main push.

## Runtime issue and fix

Subsequent actual dev.15 testing reproduced a center tap that left media controls invisible, while the close action responded. Negative captures remain under [dev.15/device-current](../v15/device-current), including `audio-center-tap-dev15.png` and `audio-controls-shown-settled-dev15.png`. The original [dev.15 validation](../v15/validation.md) retains its publication-time source/model/build facts and device NOT_RUN boundary.

Dev.16 applies `HitTestMode.BLOCK_DESCENDANTS` to the hidden controls subtree. Gesture and control nodes are rebuilt by the admitted preview token using `ForEach`, so hidden child controls do not swallow the visibility tap and old nodes do not borrow a replacement lease. Existing original-token callbacks, no-autoplay, background pause, release-before-file-cleanup and fullscreen restoration contracts remain.

The inherited dev.15 import path still accepts the complete ordered multi-file selection, bounded by 20 and the remaining selected-asset/complete staging-record slots. It confirms each independent original import/pin before advancing, fills a still-empty title only after confirmed success, and preserves issued requests and confirmed items when cancelled, failed, Unknown or stopped. Spool cleanup is followed by another original-owner check; asset insertion binds the captured card/draft, and editor replacement resets progress. Source/model qualification does not substitute for the pending actual multi-file/title/save/restart stages.

## Final package and local checks

| Check | Current result |
|---|---|
| Version / bundle | `0.1.0-hmos-dev.16` / `1000016`, `dev.morrow.hmos` |
| Final HAP | **24,857,891 bytes**, SHA-256 **`A971227AC2D39730C2228972B513DBAAB4049E1C9E9AAB49397517BFB0208C7C`** |
| API 26 build | [hap-release-build.log](hap-release-build.log): **SUCCESS**, 10.267 seconds; unsigned debug package, signing skipped |
| Actual-source model checks | [arkts-final-model-tests.log](arkts-final-model-tests.log): **303/303 PASS**, executing actual ETS source through the installed SDK TypeScript harness; provider/player/window operations are synthetic |
| Exact local archive | `.build/artifacts/dev16/entry-default-unsigned.hap`; complete hash and size checked; ignored HAP is not a committed source artifact |
| Native-entry byte comparison / native source diff | [native-package-comparison.json](native-package-comparison.json): **PASS**, all four ARM64/x64 `libmorrow.so`/`libc++_shared.so` entries have identical complete uncompressed bytes to dev.15; native source diff is empty. Verified reuse, not a fresh native build |
| Final input manifest / disk and staged checks | **298 inputs PASS** from disk and the staged index, both matched to the final `A97122…` HAP; [build-manifest-disk.log](build-manifest-disk.log) and [build-manifest-staged.log](build-manifest-staged.log) |
| Fresh Rust host tests / fresh ARM64 native build | **NOT_RUN**; earlier Rust and native build results are historical reuse evidence |

## Actual dev.16 device stages

Root alone operates the existing API 26 / x64 emulator `127.0.0.1:5555`. [device-final-install.log](device-final-install.log) records successful installation of the final archive; [bundle-final.json](bundle-final.json) reads back `1000016` / dev.16. The fixture is the existing public `HMOS-media-20261007-A` draft with its WAV and MP4 pins; procedural file provenance remains in [dev.14/fixture-provenance.json](../v14/fixture-provenance.json). No new business card is asserted by restoration or preview.

| Stage | Current evidence / status |
|---|---|
| Final install / version readback | **PASS**, final dev.16 package and bundle version |
| Restore existing exact media fixture | **PASS**, [device-restore.log](device-restore.log); same fixture, WAV/MP4 **two pins** |
| Native WAV preparation / no autoplay | **PASS**, [device-audio-preview.log](device-audio-preview.log); `preview-audio-prepared` and `preview-audio-still-paused` capture **0:00 / 0:12**, paused |
| Center tap shows controls | **PASS for the observed tap**, root visually reviewed [audio-shown-root-probe.png](device-current/audio-shown-root-probe.png); node presence alone is not visibility proof |
| Audio playback advancement / pause stability | **PASS**, [device-audio-playback.log](device-audio-playback.log): playing position **0:01 → 0:03 / 0:12**; paused and later stable captures both **0:06 / 0:12**, paused glyph; audible output is **NOT_PROVEN_BY_UI** |
| First fullscreen attempt | **FAILED_OR_UNKNOWN**, [device-fullscreen-audio.log](device-fullscreen-audio.log): actual landscape entry observed, then the helper's cropped-editor fixture assertion found zero visible pins and stopped; this is not a full fullscreen PASS or proof that stored pins disappeared |
| Read-only fullscreen reconciliation | **PASS for current-state reconciliation**, [device-reconcile-fullscreen-audio.log](device-reconcile-fullscreen-audio.log): same dev.16 package, stable **2232 × 1320** landscape captures and paused position **0:06 / 0:12**; root reviewed the stable PNG. The unknown fullscreen request was not replayed |
| First Back driver stage | **FAILED before action**, [device-back-audio.log](device-back-audio.log): helper incorrectly chose the reconciliation capture as the prior preview baseline; its assertion stopped before sending Back. This does not prove a Back failure |
| Fresh Back / window restoration | **PASS**, [device-back-audio-final.log](device-back-audio-final.log): corrected baseline filtering, fresh stage restores **1320 × 2232** portrait while the preview remains open at paused **0:06 / 0:12**. Root visually reviewed [back-audio-final-restored.png](device-current/back-audio-final-restored.png), including normal system bars |
| Close / return to same editor | **PASS**, [device-close-audio.log](device-close-audio.log): modal closes and fresh editor readback retains the exact two original asset IDs and names. Root visually reviewed [close-audio-closed.png](device-current/close-audio-closed.png). UI does not prove physical lease deletion or every failure path |
| Seek / fullscreen horizontal or double-tap / background | **NOT_RUN** for dev.16 qualification |
| Video playback / decoded frames | **NOT_RUN** for dev.16 qualification |
| Multi-file selection / blank title / retain / restore / one save / restart readback | **NOT_RUN**, deferred from this immediate media-fix delivery; no new `HMOS-multi-20261007-A` seed/import/save flow is started and no unknown outcome is replayed |
| PDF readable system preview / explicit end-viewing lifecycle | **NOT_RUN** |

Old dev.14 WAV playback/seek/fullscreen/close results and dev.15 negative observations cannot qualify unfinished dev.16 stages. Player position labels do not prove audible output. Format/error/corrupt-content matrices, grants/revocation, storage failure, process-death cache accounting, full image gestures/GIF, wide/theme/keyboard/IME behavior, signing and ARM64 physical-device qualification remain open.

The unrun multi-selection device helper has two identity-proof limits before further hardening: it does not verify the installed bundle identity at each stage, and `raw_card_id` binding through `bindRawIdentity` is optional. Its current PASS could therefore not, by itself, prove execution on the exact final package or that raw and published card IDs stayed identical across save/restart. Future multi-file qualification must close or independently document both checks; no three-file PASS is claimed in this delivery.

## Source follow and full-goal boundary

[Media controls source audit](../v15/media-controls-source-audit.md) confirms enabled reference horizontal/double-tap behavior. Vertical volume/brightness gestures require callbacks absent from current Flutter `attachment_view`; they are disabled for this reference. [Import source audit](../v15/import-source-audit.md) records sequential multi-selection and confirmed empty-title behavior.

[Next parity audit](../v15/next-parity-audit.md) proposes the complete local lifecycle for structured rich clipboard content. Its status is **NEXT_PROPOSAL_ONLY**, not implementation or device qualification. Existing plain-text/TSV paste does not supply HTML/RTF/Office/image/file clipboard parity.

The unsealed 256-card development Store, production HUKS/identity/ownership/audit/backup, captured S1/S2 and formal business cross-process Unknown recovery, complete rich clipboard and continuous full-document selection, full languages/fonts/backgrounds, plugin/HTTP/TLS/service runtime and music/lyrics workspace remain open. Import/pin 64 MiB, 20-record/asset limits, eight inline images/64 MiB and separate one-media/one-system-preview 200 MiB bounds are unchanged. No test count, package version or narrow device stage closes the full parity goal.

## Branch delivery

This source delivery targets only `codex/ArkTsUI`. Exact push and remote readback evidence will be retained separately in ignored `.build/delivery/dev16/branch-delivery.json`, completed by root after publication; the commit does not embed its own final hash or assert a push PASS in advance. No main operation, release tag or signed distribution is part of this delivery.
