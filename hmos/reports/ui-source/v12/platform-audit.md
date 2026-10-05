# dev.12 attachment platform and recovery audit

Date: 2026-10-05 (Asia/Shanghai). Scope: local SDK declarations, native bridge implementation, and actual ArkTS helper executed with synthetic filesystem/picker/native providers. This report does **not** assert device picker, provider save, image decoding, Rust runtime, IME, or full application acceptance. The platform audit agent did not operate a device, build a HAP, commit, or push during this recovery pass. Root's separately produced device/build results are independent evidence.

## Implemented contract

- Native methods: `importFile(request: string, fd: number): Promise<string>`, `exportFile(request: string, fd: number): Promise<string>`, `prepareFile(sourceFd: number, destinationFd: number, maxBytes: number): Promise<string>`.
- Native bridge duplicates caller FDs synchronously using `F_DUPFD_CLOEXEC` before queuing. Its RAII work object owns duplicates until execution; execution transfers ownership exactly once to the Rust consuming ABI. Queue/cancellation/setup failures close bridge-owned descriptors. Caller descriptors remain the caller's responsibility. Rust ABI implementations belong to root.
- ABI: `morrow_hmos_import(const char*, int owned_fd)`, `morrow_hmos_export(const char*, int owned_fd)`, `morrow_hmos_prepare(int source_owned_fd, int destination_owned_fd, uint64_t max_bytes)`. Export/prepare return JSON `{ok,error,byte_length,sha256}` with decimal string length and lowercase SHA-256. Import returns the Engine reply with import/effect projections.
- `AttachmentFiles.pick()` reads only the exact URI returned by `DocumentViewPicker`, opens it `READ_ONLY`, and streams into an app-private uniquely created spool. No provider URI, filesystem path, or raw FD is serialized into an Engine request or durable attachment metadata.
- `importPrepared(value, serialized, send)` journals the exact first request before Workbench admission; the caller-provided `send` must use the shared Workbench queue. Unknown results retain the proposal and data. Reusing a different proposal is refused. Helper request checking covers import schema, metadata agreement, canonical generation, and known kinds; Rust remains the final schema/hash/authority check.
- Import private cache budget is 64 MiB including data and conservative per-entry sidecar reservation, at most 20 entries. Corrupt retained sidecars exceeding the reservation count by their actual size. Permission/stat failures cannot be treated as absence or bypass quota. Export/preview verified temporary content has a separate per-file 200 MiB limit.
- `previewFile(serialized, name, send)` verifies the native export reply and private length, syncs the private copy, and returns `VerifiedPreview {token,path,name,byte_length,sha256}`. The path is a local UI image resource only. Only one preview is active per helper; `releasePreview` deletes only a registered token's own directory.
- `exportToPicker` verifies into private temporary storage first, then uses the save picker URI with `WRITE_ONLY | CREATE | TRUNC`. It copies, syncs the provider FD, and requires destination `stat` size agreement. Unsupported provider stat/sync/copy is explicitly unconfirmed, without automatic retry. Truncation avoids a longer existing file's stale tail. Picker cancellation returns `false`.

## Recovery and interrupted preparation

The earlier recovery loop silently skipped malformed preparations while retaining their quota. The current loop distinguishes an unadmitted preparation from a retained import request.

The import root must be an actual directory without a symlink, and recovered child directories must match the immediate-child namespace `import-[A-Za-z0-9]{6}`. Before cleanup, all existing children must be among `data`, `metadata.json`, and `request.json`, and be regular non-symlink files. Only SDK `lstat` error `13900002` proves a request is absent; an access failure, dangling symlink, malformed request, or unsupported read is not proof of absence. App helper instances share one in-process admission queue.

If preparation is incomplete, request absence is positively established, and the directory/children pass those checks, recovery may delete the partial preparation. Cleanup rechecks request absence before each unlink and before directory removal, then requires an empty directory before calling SDK `rmdir`, which is documented as recursive. The helper never uses a picker-derived path in this removal routine. This is an app-private single-process ownership boundary, not protection against another process with the same sandbox ownership modifying paths concurrently.

Any existing request, including empty, partial, invalid JSON, wrong schema, or metadata mismatch, is retained. Missing/corrupt metadata with an existing request is also retained. Unreadable/unsafe/unrecognized entries receive diagnostics; they do not become `PreparedAttachment` import handles. Complete metadata/data and a structurally valid exact request can be recovered; payload SHA/authority is checked again by Rust when explicitly admitted. Recovery does not replay requests automatically. Corruption invalidates an existing in-memory handle after a scan.

UI contract:

```ts
const prepared = await files.recover();
const issues = files.recoveryDiagnostics();
const needsReview = issues.some((issue: AttachmentRecoveryDiagnostic): boolean => issue.requires_review);
```

`AttachmentRecoveryDiagnostic` contains `spool_id`, `code`, `message`, `request_state` (`present`, `absent`, or `unknown`), `requires_review`, and `preserved`. The returned list contains copies. Index should show only the user-facing message **“有附件缓存需要核对，已保留。”** for review entries, without paths or platform error codes. Successful partial-prepare cleanup has `code=PREPARE_INTERRUPTED_CLEANED`, `requires_review=false`, and `preserved=false`. Invalid journals intentionally remain quota-consuming until explicitly reconciled; no deletion entry point is exposed for unverified handles.

## Local SDK evidence

Installed SDK root: `C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony`.
`ets/oh-uni-package.json`: API **26**, platform **26.0.0**, SDK **26.0.0.105**, release type **Release**.

Declarations checked from this actual installation:

- `ets/api/@ohos.file.picker.d.ts`: `DocumentViewPicker` constructor (around lines 628–640), document selection options/modes, `select` returning URI strings (679), save names/options (574), and `save` (721).
- `ets/api/@ohos.file.fs.d.ts`: URI/FD `open` (2491), FD `copyFile` (761), `fsync` (1515), `lstat` with `13900002` ENOENT (1697–1711), `mkdtemp` replacing six `X` characters (1944), bounded `readText` (2881), recursive `rmdir` (3080–3105), FD `stat` (3184), `Stat.isFile`/`isSymbolicLink` (5032/5056). `rmdir`'s recursive semantics are why cleanup preflights every child and verifies emptiness first.
- `ets/kits/@kit.BasicServicesKit.d.ts` imports `BusinessError` from `@ohos.base` and re-exports it (24/60). The helper reads its numeric `code`; it does not interpret an arbitrary exception as ENOENT.
- `ets/api/@ohos.util.d.ts`: `TextEncoder.encodeInto` provides the small metadata/request `Uint8Array`; attachment payloads are streamed natively rather than loaded into an ArkTS array.

URI grants and picker lifetime follow the returned URI/FD API. No Android path resolution or persistent URI authorization is inferred from these declarations. Actual picker/provider behavior remains a device acceptance item.

## Replayable synthetic checks

Run from the repository root:

```powershell
node --test hmos/tool/attachment-files-model.test.cjs
```

The test loads **the actual** `entry/src/main/ets/model/AttachmentFiles.ets`, transpiles it with this installed SDK's TypeScript module, and executes it in an isolated VM with synthetic kit/native providers. It does not copy the helper implementation into a second model. `HMOS_TYPESCRIPT_PATH` can override the installed TypeScript module path on a different host. This catches TypeScript parse errors and model behavior; it is not an ArkTS strict compiler/HAP build or a real provider.

Final result: **43 passed, 0 failed, 0 skipped**. Saved output: `attachment-files-model-tests.log`.

Coverage includes the original 14 picker/import/export/preview cases plus missing/empty/partial metadata during preparation, a crash before data creation, live metadata write failure, clearing 20 unadmitted preparations, exact unknown-request preservation and explicit same-proposal reuse, empty/partial/malformed/schema-invalid requests, missing metadata with a retained request, all directory/data/metadata/request symlink positions, request permission failure, unexpected recursive child content, unrecognized namespaces, explicit valid-handle release, cached-handle invalidation, cleanup failure diagnostics, oversized retained sidecar quota, and copied diagnostics. Export checks include shorter replacement truncation, sync/stat confirmation, bad hash/length reply, provider copy/stat failure, and cleanup of private temporary copies. All file providers and native stream replies in these checks are synthetic.

Source snapshot SHA-256:

- `AttachmentFiles.ets` final strict-compiler correction: `497edacad0398f66404845542103fc59ac3985026ab155f79345d58d59819115`
- `attachment-files-model.test.cjs`: `921478fe66a6895231ffccc8c7ac09918a827bbd945add783d3d525bf7624a0c`

Native bridge previously passed independent `clang++ -fsyntax-only` for x86_64 and aarch64 OHOS within this dev.12 work. Native bridge syntax checks are not a linked ABI runtime test. The platform audit agent did not rebuild a HAP during the recovery pass. Root subsequently preserved a strict ArkTS compiler failure in `hap-final-build.log`: `throw error` at helper line 136 was rejected by `arkts-limited-throw`. Root replaced it with `throw new Error('附件缓存读取失败，已保留。')`, then rebuilt successfully (`hap-release-build.log`) and reran actual ETS checks (`final-arkts-models.log`: attachment subset 43/43, overall 119/119). The pre-correction helper snapshot was `df2418b603f12088c58551f8e4e006219aaf60b85017a66bee62b9478e25d0d0`; the final source hash above includes this compiler correction. Root's native/device results and the pending final UI acceptance are recorded separately in [validation.md](validation.md).
