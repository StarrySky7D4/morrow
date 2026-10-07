# dev.14 system attachment preview API and ownership audit

Observed 2026-10-07 03:44 UTC. This audit and the model checks did not operate a device, install a provider, change system settings, build/sign a HAP, commit, or push. The root task separately owns UI/helper integration and runtime qualification.

## API authority

The installed SDK is API26, platform26.0.0, package26.0.0.105 Release. The vendor file-preview module is `filePreview` from **`@kit.PreviewKit`**; there is no installed `@kit.FilePreviewKit` declaration.

| Installed declaration | SHA-256 |
| --- | --- |
| `C:/Program Files/Huawei/DevEco Studio/sdk/default/hms/ets/api/@hms.filemanagement.filepreview.d.ts` | `F005F11712D5C7BC2CC795AB8A1B90594FD02AA53CDC3B08F4BE311DD681D5C3` |
| `C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/api/@ohos.file.fileuri.d.ts` | `AB53A9664AC62ACE44096A551CB7F4210CD4083206302D91F709515DD3F48937` |

Confirmed SDK signatures:

- `filePreview.openPreview(context: Context, file: PreviewInfo, info?: DisplayInfo): Promise<void>`. The context currently supports only `UIAbilityContext`. It launches a system preview window; repeated calls within one second can have no effect. A successful callback means the **window was displayed**, not that a file reader finished.
- `filePreview.canPreview(context, uri): Promise<boolean>` checks file existence and supported type. It does **not** establish delegated access; the caller must supply a URI that supports delegation.
- `filePreview.hasDisplayed(context): Promise<boolean>` observes the singleton preview window. The SDK states false before creation and after closure. The result does not identify arbitrary other-app readers.
- `filePreview.closePreview(context): Promise<void>` closes an existing window; calling before window creation can fail. It is followed by an actual `hasDisplayed` query instead of treating promise resolution alone as closure.
- `PreviewInfo` has optional `title`, required `uri` and `mimeType`. Empty MIME is allowed; the system then infers from the URI suffix.
- `fileUri.getUriFromPath(path)` constructs an application sandbox file URI. Neither handwritten file URI text nor metadata/provider paths are authority in the model.

Primary web documentation was consulted. [OpenHarmony File URI reference](https://raw.githubusercontent.com/openharmony/docs/master/en/application-dev/reference/apis-core-file-kit/js-apis-file-fileuri.md) explains sandbox URI form and the encoding performed by `getUriFromPath`; its URI-to-path conversion explicitly does not verify file accessibility. [Huawei PreviewKit API reference](https://developer.huawei.com/consumer/cn/doc/harmonyos-references/preview-arkts) opened but provided no substantive body to the extractor. The [Huawei file-preview guide](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/preview-filepreview) and [Preview FAQ](https://developer.huawei.com/consumer/en/doc/harmonyos-guides-V14/preview-faq-V14) timed out in this audit. Therefore exact API, supported MIME and delegated-access statements above are grounded in the installed vendor declaration; no third-party blog is used as API authority.

The SDK exposes no arbitrary downstream-reader completion API. If a system/provider preview hands a file to another app through opening/sharing controls, that reader's completion is unobservable through this module. The implementation conservatively keeps bytes after its preview window closes; it does not infer universal reader completion from foreground events or `hasDisplayed(false)`. Actual provider controls and transferred access remain runtime qualification items.

## Concrete implementation and integration contract

`AttachmentOpen.ets` exports `AttachmentOpenState`, `attachmentOpenBlocked(name)`, `attachmentPreviewMime(name)`, and `AttachmentOpen`.

```ts
new AttachmentOpen(context, active: (lease: VerifiedPreview) => boolean,
  release: (token: string) => Promise<void>, changed: (state: AttachmentOpenState) => void)
open(lease: VerifiedPreview): Promise<boolean>
owns(lease: VerifiedPreview): boolean
snapshot(): AttachmentOpenState
reconcile(): Promise<void>
close(): Promise<void>
finishReadersClosed(): Promise<void>
```

The state exposes `phase`, `name`, `message`, `retained`, `busy`, `can_finish`; it never exposes the private URI or path. `open(false)` leaves the candidate with its caller. `open(true)` means the controller adopted it, including an unsupported/pre-dispatch error it safely cleaned, or any unknown dispatch result it retained. Root must check `owns(candidate)` before candidate-finally cleanup.

Root supplies a genuine private `VerifiedPreview` of purpose `open`, read via the exact selected Core/business revision or confirmed draft pin generation, with native whole-file length/hash/stat/fsync checks. The helper must preserve a bounded safe basename/suffix because `canPreview` receives the URI only. Root independently binds the read result to the still-current selected owner before calling `open`.

The model requires the caller's opaque-handle identity check, bounded whole length, lowercase whole SHA-256, a generated known directory token under the exact cache root, a single file basename, and exact equality to the SDK-generated file URI. It captures immutable token/path/URI/name/length/hash/purpose before awaiting. Changed/forged handles cannot dispatch another URI. Cleanup uses the captured original token, even if a UI caller later changes its object.

Per-cache-root static sessions retain only a known owned lease and the original active/release callbacks across UI adapter recreation. Opening is singleflight; a pre-existing unrelated system singleton is refused rather than replaced or closed. Unsupported types and errors before any system dispatch can release immediately. Every exception after dispatch is an unknown outcome: retain the lease, block a new open, query/close explicitly, and never replay automatically.

`reconcile` and `close` preserve bytes. The product's explicit **“已结束查看，清理临时文件”** action invokes `finishReadersClosed`: the human assertion is that other applications are done with the file; the model additionally verifies the observable system preview is absent. A still-visible window or an unknown query retains the lease. Failed removal preserves the same immutable handle for explicit retry. This is a product action, not a Codex approval prompt. The model deliberately has no destructor that deletes files on background, navigation or UI teardown.

This registry is process-local. It does not scan or delete orphan files after process death, does not establish a total persistent on-disk quota, and cannot prove external readers have stopped. Physical-device ownership/permission/lifetime behavior requires separate runtime evidence.

## Type selection and parity scope

The pure mapper follows the installed vendor table: text/source/XML/XHTML/HTML, JPEG/PNG/GIF/WebP/BMP/SVG, M4A/AAC/MP3/OGG/WAV, MP4/MKV/TS, PDF, DOC/DOCX/XLS/XLSX/PPT/PPTX/CSV/OFD. JPEG is a standard suffix alias for the SDK's JPG MIME; actual URI support remains decided by `canPreview`. Unrecognised suffixes receive an empty MIME, never a fabricated wildcard guarantee. The mapper is only a viewer hint; bytes and permission come from the verified handle.

Freshly read Flutter source: `lib/attachments/file_access_native.dart` SHA-256 `3AE2D37173F56E67AAA572D44AB2F4C197CA0EB503FDAEDC41410E07B8FD474A`; `lib/attachments/attachment_view.dart` SHA-256 `F3BAA59202D433DEB1683F77932B31A210FB4EE5D382AD0A8ECFEB2A4DEC0FE4`. Like that default-open guard, direct opening rejects `exe`, `com`, `msi`, `bat`, `cmd`, `ps1`, `vbs`, `js`, `lnk`, `url`, `scr`, and `reg` (case-insensitive final suffix). The root UI may offer the existing explicit save/export action for unsupported or guarded files; export is not reported as opening parity.

Flutter's generic file tile calls its desktop default opener; HMOS now takes the vendor system-preview route, which may offer other applications. This is concrete functionality toward that behavior, **not proof of an equivalent direct default opener for arbitrary file types**. No `startAbility` fallback, provider installation, broad URI/file permission, or system-setting workaround is introduced. Office and format/decoder/provider support depend on the installed platform; MIME/type-check success alone is not readable content proof.

## Verification performed here

`node --test hmos/tool/attachment-open-model.test.cjs` passed **19/19** on 2026-10-07, executing the actual ETS through installed SDK TypeScript with synthetic PreviewKit/CoreFileKit providers. Checks cover verified URI/MIME dispatch without premature removal, forged/provider/purpose/path/length/hash rejection, twelve executable families, MIME values, unsupported/pre-dispatch cleanup, refusal of an unrelated existing singleton, window-close/foreground retention, explicit finish and visible-window refusal, unknown dispatch/no replay, unobserved launch retention, close/query failures, closure-result verification, cleanup retry across adapter recreation, immutable identity across asynchronous mutation, queued finish during dispatch, and immutable UI snapshots.

These are model decisions, not strict ArkTS compilation or actual provider/runtime evidence. Root must separately record the integrated HAP build and device content preview, own-window close/reconcile, retained cleanup action, no-business-mutation result, and supported/unsupported format outcomes. ARM64 physical-device/signing/production storage qualification remains outside these checks.
