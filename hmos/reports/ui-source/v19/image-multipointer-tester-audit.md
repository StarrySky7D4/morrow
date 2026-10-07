# Independent image multi-pointer tester

2026-10-07. **API26 unsigned build PASS; 10 executable tool-boundary tests PASS. Device execution, installation, cross-window UITest-provider availability, actual picture movement and final image acceptance: NOT_RUN.** No production Preview/model/Index/Rust/C++ source, application data, global settings or official product build outputs were modified by this work. No device or Git operation was performed.

## Real installed API

`C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/kits/@kit.TestKit.d.ts` exports `Driver`, `PointerMatrix`, `ON`, `TestRunner` and `abilityDelegatorRegistry`. The installed API26 driver does **not** declare `performMultiPointerAction`. The correct method is:

```typescript
const matrix = PointerMatrix.create(2, 9);
matrix.setPoint(fingerIndex, stepIndex, { x: observedPxX, y: observedPxY, displayId: 0 });
const acknowledged = await driver.injectMultiPointerAction(matrix, 600);
```

Actual `@ohos.UiTest.d.ts:3313–3331` declares `injectMultiPointerAction(PointerMatrix, speed?): Promise<boolean>`, `@test`, since API9, with speed in **px/s**, valid range 200–40000, and no concurrent driver calls. `PointerMatrix` at lines 4252–4295 allows 1–10 fingers and 1–1000 steps; all finger/step entries are populated by this tester. Coordinates are device-display **px**, unlike the production ArkUI gesture callbacks' vp. The two-finger probes submit two independent, simultaneous traces; a CLI single-pointer swipe is not used as pinch evidence.

The installed declarations also provide `ON.inWindow(bundleName)` (line 1970), `Driver.findWindow`, `UiWindow.getBundleName/isFocused/isActive`, `Component.getOriginalText` (since20), `Driver.screenCap` (line3080: PNG in the current application's sandbox), and `Driver.dumpLayout` (line3116, since26). `TestRunner` declares `onPrepare/onRun` and optional API26 `onStop`. The installed DevEco coverage runner uses `aa test ... -s unittest /ets/testrunner/OpenHarmonyTestRunner` in `executeOhosTest.js:170`; the command printer retains this real **leading-slash runner path** convention.

Fresh SHA-256:

| Source | SHA-256 |
| --- | --- |
| API26 `ets/api/@ohos.UiTest.d.ts` | `7CFFE0842708BB6DCDFDF7ED5FA87FAD8ADB9F9AA172D47E5B1964908F322EE6` |
| API26 `ets/kits/@kit.TestKit.d.ts` | `CF3197BD59993F131FD113F83CD312A231348B82334814D352FBEAACC4CDB48F` |
| API26 `ets/api/@ohos.application.testRunner.d.ts` | `D1237E6BF71C84CD3E2630394E12673B7C1A21AB94B26A3C885B4B778E391659` |
| Installed `tools/hvigor/hvigor-ohos-plugin/node_modules/@ohos/coverage/lib/src/commandLine/ohosTest/executeOhosTest.js` | `86EA162A7671D9607EB01F6A24FE0C36F5CA97B15F37E607474D225F8FCEBBCF` |
| `tool/image-gesture-tester/ImageGestureRunner.ets` | `98A3C8BD8074076E6B51BA20C2E69E6F9D616209A8BAF520E080F250468509A0` |
| `tool/image-gesture-tester/ImagePointerPlan.ets` | `892D0742CCB1BC3DC461E7DC0B337B783DE63A0C82B9B5DD599ECC395B5B1372` |

The SDK/API paths above are relative to `C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/`; the installed tool path is relative to the Studio root.

## Scope and independent artifacts

The independent bundle is **`dev.morrow.hmos.gesturetester`**, test module **`entry_test`**, runner **`/ets/testrunner/ImageGestureRunner`**. `prepare.cjs` creates an isolated project only under `hmos/tool/image-gesture-tester/work/`; existing projects are preserved. It adds no target, test ability or test hook to `dev.morrow.hmos`. Both tester abilities have no startup implementation; the custom runner does not call `startAbility`, focus a window, open a file picker, create/import a fixture, reset an emulator, clear an application or change system settings.

Publish these seven source/helper files:

- `hmos/tool/image-gesture-tester/.gitignore`
- `hmos/tool/image-gesture-tester/ImageGestureRunner.ets`
- `hmos/tool/image-gesture-tester/ImagePointerPlan.ets`
- `hmos/tool/image-gesture-tester/prepare.cjs`
- `hmos/tool/image-gesture-tester/build.ps1`
- `hmos/tool/image-gesture-tester/command.cjs`
- `hmos/tool/image-gesture-tester.test.cjs`

Publish this audit and the three sibling evidence logs. The generated work tree/HAPs are ignored build artifacts, not product/source-delivery files.

Final isolated project: `hmos/tool/image-gesture-tester/work/build-1791357367787`. Actual unsigned artifacts:

| Relative to the final project | Bytes | SHA-256 |
| --- | ---: | --- |
| `entry/build/default/outputs/default/entry-default-unsigned.hap` | 11,233 | `0872126135F6D11C282A592C24AD835A527B37DC8BC4E8CBE814885A60793B61` |
| `entry/build/default/outputs/ohosTest/entry-ohosTest-unsigned.hap` | 49,223 | `D0C794B53EF029E5D6CE0BD0AFAA53CA8ED1DDC501BE4A1D3DA005A05A6ACD64` |

[Main build log](image-gesture-tester-main-build.log): `CompileArkTS`/HAP **SUCCESS in 6.581 s**. [Test build log](image-gesture-tester-ohosTest-build.log): actual `OhosTestCompileArkTS`/HAP **SUCCESS in 9.073 s**. The expected no-signing-config warning confirms these are unsigned; no keys or certificate files were read. SDK warnings that called APIs may throw are retained in the log. The runner catches probe failures and never replays input, but these warnings are not described as a warning-free build or a release-signing qualification.

## Root-operator execution recipe

1. The root operator first reconciles any pending save picker, retains its own draft, installs the intended frozen product version if appropriate, and opens an owned image preview. No tester invocation performs these steps. Capture fresh ownership and native tree evidence; obtain the exact draft title, original image name, installed product version code, and `attachment-image-gesture-canvas` pixel bounds. Do not reuse the mock coordinates in the local test file as a device observation.
2. Build only the independent tester with `hmos/tool/image-gesture-tester/build.ps1`. Installing the independent main/test HAPs is a later explicit root device step; this audit did not install them. Runtime debug-signature acceptance is NOT_RUN.
3. Create a host JSON config containing `operation`, a fresh `runLabel`, `expectVersionCode`, `expectDraftTitle`, `expectImageName`, and observed `left/top/right/bottom`. Optional `expectToken` and `expectHapSha` are recorded as external expectations, not falsely read-back identities. `command.cjs <config.json>` prints `aaArguments` and a quoted `remoteShellCommand`; it never calls HDC. Root passes that printed command to its existing HDC device channel. The generated command starts `aa test -b dev.morrow.hmos.gesturetester -m entry_test -s unittest /ets/testrunner/ImageGestureRunner ...`.
4. Run **`observe` first** to qualify runner startup, provider availability, product foreground, exact fixture and readable zoom text before attempting a gesture. Each subsequent invocation executes only the named operation: `pinch-out`, `pinch-in`, `pinch-moving-focal`, `pan-two`, `boundary-pan-two`, `pan-one`, or the existing HMOS `double-tap-reset`. Outward pinch requires an explicitly prepared 100% image; inward pinch, pan and double-tap reset require an already zoomed image. There is no automatic setup/reset or whole-scenario replay.
5. Root collects the runner's actual printed `reportDirectory` and sandbox artifacts using the device's verified sandbox mapping. The tester uses its own `getAppContext().filesDir/image-gestures/<runLabel>`; no host-visible physical path or Android/HMOS user-directory mapping is guessed. It stores a fresh PNG/tree and typed observation before input, a durable `input-intent.json`, then a settled after PNG/tree/observation and `result.json`. An existing run label fails before input; a repeated runner instance also stops without overwriting its prior evidence. On uncertain input outcome, inspect current state and the original intent instead of rerunning that label.

Every observation reads `bm dump -n dev.morrow.hmos` through the real TestKit delegator and requires the requested version code. It requires the product window to be focused/active, a unique current preview/image/canvas/zoom component, an exact image name inside the preview, the exact visible draft title, and observed bounds matching the supplied rectangle within 1 px. Missing or unreadable identity, foreground or bounds stops input. A second fresh observation follows the pre-input capture. Driver calls remain sequential. `onStop` blocks any next input; it cannot retrospectively cancel an already submitted native gesture.

## What the evidence does and does not prove

[Fresh tool tests](image-gesture-tester-model-tests.log): **10/10 PASS**. They transpile the actual ETS trajectory generator and TestRunner with the installed SDK TypeScript; synthetic Driver/filesystem/delegator providers verify complete 2×9 matrices, pure-pan separation, identity/version/foreground/bounds gates, readonly observation, intent-before-input, no retry after unknown injection, preserved prior evidence, and owned build paths. These are executable tool-boundary checks, not a simulated device acceptance claim.

An `injectMultiPointerAction` return of true only acknowledges injection. Even when the observed percentage increases/decreases or resets as requested, the runner reports **`ACKNOWLEDGED_REQUIRES_VISUAL_REVIEW`** and **`SCREENSHOTS_CAPTURED_NOT_AUTOMATICALLY_QUALIFIED`**. Root must inspect the actual image pixels, focal movement, clipping and boundary reversal. Unchanged percentage during pure pan establishes no rendered displacement by itself.

Preview token/pin identity and installed archive bytes are not exposed by this UI provider: `tokenStatus=NOT_EXPOSED_BY_UI`, `hapShaStatus=HOST_DECLARED_NOT_READ_BACK`. Root-owned fixture/pin and exact-package install evidence must independently bind any later device claim. Internal late-token/cancel behavior remains covered by the production model tests, not by this tester's visual probes.

Actual multi-pointer behavior, cross-window provider access, screenshot retrieval/sandbox mapping, decoded-image effects, GIF/malformed-image acceptance, inertia and hardware qualification are **NOT_RUN** here. Any future probe on frozen dev18 must be labeled dev18 device evidence; it must not be counted as final dev19 device validation merely because this tester's source audit is stored under v19.
