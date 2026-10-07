# dev.13 native image and attachment platform audit

Observed 2026-10-07 03:04 UTC / 11:04 Asia/Shanghai. This API audit inspected the installed SDK, current Flutter behavior, dev.12 HMOS helper/build/test entrypoints, and primary web documentation. This stage did not operate a device, run model tests, build a HAP, alter production code, commit, or push. A later separately authorized emulator recovery is recorded in `environment-audit.md`. Findings describe available APIs and required acceptance; they do not establish dev.13 runtime completion.

## Installed declarations

OpenHarmony SDK root: `C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony`. `ets/oh-uni-package.json`: API 26, platform 26.0.0, package 26.0.0.105, Release. HMS overlay root: `C:/Program Files/Huawei/DevEco Studio/sdk/default/hms`.

| Installed SDK input | SHA-256 |
| --- | --- |
| `openharmony/ets/component/image.d.ts` | `8ABC19167E94C6A10564D04D2D28582E21FD292F804E264FF6E94B47509F118A` |
| `openharmony/ets/component/gesture.d.ts` | `CF7E797B192D58566B6C62E1241A56D7E0884EA594A44C3152229EC1F251CC9B` |
| `openharmony/ets/component/video.d.ts` | `2054C0B24DB7346343FDD136AD6B25A89613707CFD429AB33D0752C3B21BCAB1` |
| `openharmony/ets/api/@ohos.file.fileuri.d.ts` | `AB53A9664AC62ACE44096A551CB7F4210CD4083206302D91F709515DD3F48937` |
| `openharmony/ets/api/@ohos.multimedia.media.d.ts` | `8A7DCA59E33B0797070CA8C8E2FB3FEF51980B0887A7DEF350171CDB43175789` |
| `openharmony/ets/api/@ohos.file.picker.d.ts` | `ABB8A0BDAA693E3C1C9FB098FA6D7A7BFC08C0030C26A4AE4EE540712D376D47` |
| `hms/ets/api/@hms.filemanagement.filepreview.d.ts` | `F005F11712D5C7BC2CC795AB8A1B90594FD02AA53CDC3B08F4BE311DD681D5C3` |

Confirmed signatures and behavior in these local files:

- `image.d.ts:944`: `objectFit(value: ImageFit): ImageAttribute`.
- `image.d.ts:1265`: `onComplete(callback: (event?: {...}) => void)` returns source width/height and loading/layout properties; source/render dimensions are in pixels. A component using those for viewport bounds needs px-to-vp conversion where applicable.
- `image.d.ts:1559`, `:1829`, `:1843`: `onError(callback: ImageErrorCallback)`, callback `(error: ImageError) => void`, image error object. Verified export success is not decode success; these are separate states.
- `gesture.d.ts:1635`, `:1661`, `:1673`, `:1685`, `:1697`: PanGesture options and start/update/end/cancel callbacks.
- `gesture.d.ts:1806`, `:1832`, `:1843`, `:1854`, `:1866`: PinchGesture options and start/update/end/cancel callbacks.
- `gesture.d.ts:1060` onwards: GestureEvent offsets are relative to finger-down position in vp, and `scale` describes the current pinch gesture. Keep a gesture-start baseline when composing successive updates; cancellation/end must leave a bounded usable state.
- `gesture.d.ts:179`, `:2010`: parallel gesture grouping exists; gesture competition with parent scrolling still requires actual-device verification.
- `@ohos.file.fileuri.d.ts:162`: `getUriFromPath(path: string): string` is available for sandbox image paths.
- `@ohos.multimedia.media.d.ts:86`, `:2174`, `:2208`, `:2232`, `:2256`, `:2328`, `:2899`, `:3244`: AVPlayer creation, prepare/play/pause/release, FD source and state-change callbacks exist. Keep verified file/FD ownership alive until player release completes. This is a next-iteration route, not current playback acceptance.
- `video.d.ts:420`, `:742`, `:756`, `:911`: VideoController, explicit autoplay and controls flags, and errors exist. Native video surface/media state support must be tested on each supported architecture.
- HMS `@hms.filemanagement.filepreview.d.ts:63`, `:132`, `:305`: `openPreview(context, PreviewInfo, info?)`, `canPreview(context, uri)`, and `PreviewInfo.uri` exist; `title` is optional. UIAbilityContext is required. The SDK warns that a provided URI must support delegated access; `canPreview` checks only file existence/type and does not establish delegated access. Do not treat a private sandbox path or a successful type check as proof that another preview process can read it. The preview service's closure/lifetime and any delegated grant need runtime qualification before deleting temporary bytes.

## Primary web documentation checked

The installed API26 SDK remains the build authority. Public current-master documentation can include later APIs; compare signatures before using them.

- [OpenHarmony Image reference](https://github.com/openharmony/docs/blob/master/en/application-dev/reference/apis-arkui/arkui-ts/ts-basic-components-image.md): `onError` is a loading-failure callback. Use `ImageFit.Contain` for full-image display. Sandbox paths should be converted with `fileUri.getUriFromPath`. The docs warn that changing a valid source to an invalid source can retain previously decoded content. Thus each verified-image identity must clear/reconstruct its render state and reject stale callbacks. These are documentation-derived design requirements, not observed dev.13 behavior.
- [OpenHarmony PinchGesture reference](https://github.com/openharmony/docs/blob/master/en/application-dev/reference/apis-arkui/arkui-ts/ts-basic-gestures-pinchgesture.md): the example composes current gesture scale with a previous baseline and distinguishes start/update/end/cancel. Its image example converts content px dimensions to vp and tracks the focal point. Local SDK signatures were also checked above.
- [OpenHarmony PanGesture reference](https://github.com/openharmony/docs/blob/master/en/application-dev/reference/apis-arkui/arkui-ts/ts-basic-gestures-pangesture.md): the example adds gesture-relative offsets to a stored pan baseline and stores the final position at gesture end.
- [Flutter InteractiveViewer constructor](https://api.flutter.dev/flutter/widgets/InteractiveViewer/InteractiveViewer.html), [minimum scale](https://api.flutter.dev/flutter/widgets/InteractiveViewer/minScale.html), and [maximum scale](https://api.flutter.dev/flutter/widgets/InteractiveViewer/maxScale.html): default parameters are minScale 0.8 and maxScale 2.5, with boundaryMargin zero. The minimum-scale documentation notes that the boundary can prevent actual scaling below 1 in common layouts. Parameters alone therefore do not prove full pan/zoom behavior parity. Local `C:/flutter/packages/flutter/lib/src/widgets/interactive_viewer.dart:73`, `:77`, `:78` matches those defaults; raw SHA-256 `943E1665721F7731957F58E774A0577B5D5274A35FCD2A47017E3C19ED27FB4F`.
- [Huawei Picker reference](https://developer.huawei.com/consumer/en/doc/harmonyos-references/js-apis-file-picker): DocumentViewPicker selection/save runs in UIAbility and returns URI strings. DocumentSelectOptions supports `maxSelectNumber`. Multi-select is available as a follow-up but does not bypass per-draft asset-count/byte budgets.
- [Huawei PreviewKit file-preview guide](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/preview-filepreview) and [Preview FAQ](https://developer.huawei.com/consumer/en/doc/harmonyos-guides-V14/preview-faq-V14): primary pages were found/opened, but the guide body was unavailable to the browser text extractor; the FAQ lists URI/openPreview failures without substantive extracted answers. Precise delegated-access/lifetime statements above come from the installed HMS declaration, not from a guessed web answer. No secondary blog was used as API authority.

## Required implementation checks

Dev.12 `AttachmentFiles.previewFile` exposes only one active preview per helper; an inline-image implementation must allow multiple independently owned files or use a separate bounded lease/cache owner. Starting inline image B must not delete A while A is still rendered. Handle repeated references without overwriting another renderer's ownership. Close/disable/navigation/source-revision changes should invalidate reads before their completion and release only that request's known files. No stale read should change a new card/editor/preview or resurrect an old image.

Keep payload validation in Rust/native export: expected selected attachment identity, source revision or confirmed draft generation, declared length, full SHA-256, and private file size/sync success. URI parsing/matching may choose a known asset; it must not open a URI path directly. Raw editor attachment changes require explicit confirmation or a separately proven selected-owner import path before pin export. A failed verification must not render a partial file; a valid payload with invalid image bytes must show decode failure.

Image interaction should initially preserve contain-fit, support bounded zoom/pan, reset and close, give read/loading/decode feedback, and disable file dragging to avoid conflict with card reorder. Inline images should follow Flutter's bounded max-height300 display; remote images must retain explicit user consent. Test source changes with identical display names and a valid image followed by invalid image to catch old-pixel retention.

## Verification entrypoints and scope

From the repository root, the existing script/test entrypoints are:

```powershell
node --test hmos/tool/markdown-model.test.cjs
node --test hmos/tool/attachment-files-model.test.cjs
./hmos/scripts/build-rust.ps1 -Test
./hmos/scripts/build-rust.ps1 -Abi arm64-v8a
./hmos/scripts/build-rust.ps1 -Abi x86_64 -Runner
./hmos/scripts/build-hap.ps1
```

The first two execute actual ETS model/helper sources through installed SDK TypeScript with controlled native/fs/picker providers. They establish model behavior only; strict ArkTS compilation requires HAP build. New inline coordinator/geometry tests should use their actual production ETS source rather than copied logic. Root should record exact executed tests and outputs after implementation, without inheriting dev.12 counts as dev.13 evidence.

`build-rust.ps1` uses offline locked Cargo and project-local `.build/rust`; `-Runner` builds both native self-check and attachment-check executables. `build-hap.ps1` uses the installed DevEco runtime and `--no-daemon`, avoiding a daemon ownership assumption. `tool/update-ui-report.cjs` requires the version's `validation.md` before updating hashes; its manifest-input list needs any new production page/model added explicitly by its owner. The audit did not execute a build or update that manifest.

Device acceptance must separately capture: saved and confirmed-draft inline image display; Unicode and encoded slash/name/asset-ID matches; absent/non-image references; image decode errors; old card/editor result rejection; multiple simultaneous inline references; preview zoom/pan/reset/close; resources released after repeated navigation; unchanged metadata/body/card identity on read-only preview; editor export byte comparison; restart and removal behavior. A live image screenshot is stronger evidence than UI-tree Image presence. A simulator HAP/runtime result is not ARM64 physical-device, signing, media, generic external-open, protected production store, or full Flutter parity evidence.
