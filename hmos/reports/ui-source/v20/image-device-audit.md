# Image device qualification preparation

2026-10-07. **Independent tester API26 build PASS; 22 host/tool-boundary tests PASS. Tester installation, provider startup, image decoding and actual multipointer device effects: NOT_RUN by this reviewer.** Root exclusively owns device operations. No HDC command, installation, production source edit, Git action, fixture import, application-data reset or global-setting change was performed here. Root separately reported installation/readback of frozen dev19; this report does not substitute for that device evidence.

## Frozen tools and real SDK boundaries

The installed `@ohos.UiTest.d.ts` declares `Driver.injectMultiPointerAction(PointerMatrix, speed?): Promise<boolean>` at line3331, not `performMultiPointerAction`. `PointerMatrix.create(2,9)` plus every `setPoint(finger,step,{x,y,displayId})` supplies true simultaneous two-finger traces. Speed600 is px/s, within SDK range200–40000; coordinates are physical display pixels. Calls are sequential.

This round keeps the standalone bundle `dev.morrow.hmos.gesturetester`, `entry_test`, `/ets/testrunner/ImageGestureRunner`, and raises only the tester version to200001 (`0.1.0-image-tester.20`). It adds no product entry/test hook. Necessary improvements require enabled `Component.isEnabled()` on the actual reset control, no read/decode loading/failure markers, exact image description, and a final focused/active product-window check before input. Readonly observe can describe not-ready decode loading without injecting. Pure two-finger pan now holds exact integer-pixel separation, so rounding does not accidentally create a scale input. Explicit `pan-reverse-one`/`pan-reverse-two` stages permit a separate reverse-motion boundary check; they do not automatically run after a pan.

`image-device-driver.cjs` is a **host-only printer/inspector**. Importing or executing it performs no device call. `install-plan` validates actual file SHA256 and bounded ZIP `module.json` bundle/module/debug/version identity, rejects a product bundle disguised under a tester filename, and prints one-time root commands. `probe-plan` uses actual transpiled `ImagePointerPlan.ets`, exact caller-supplied fixture/version/bounds and the existing command printer. `inspect-report` reads only collected host artifacts, reports hashes/PNG-header dimensions, and preserves UNKNOWN/no-replay. It never turns injection ACK, percentage or component rectangle into a pixel PASS.

Fresh source/API hashes:

| Source | SHA256 |
| --- | --- |
| `ImageGestureRunner.ets` | `64E8702F0CD0FF095AEE6CBCA1D75D201D95CCB5BE3B1017B89DF2377C4E9FD4` |
| `ImagePointerPlan.ets` | `C764FF351ED2C86E7E45EE1EF80C86DB861E201A4EF947003E6EAD24DECB07D7` |
| `image-device-driver.cjs` | `9D34369DDFDDDC7A28E4189E3576859436BCD85842607253A17153ACA4962B11` |
| `image-device-driver.test.cjs` | `4BD0F2B9B73A204F29512781FB981BC5B86429A2A3B5CB9FCD3BD87D1921FCF3` |
| Untouched product `pages/AttachmentImagePreview.ets` | `484076C31DCF78DDDB69B0EA27F6BB46C8FD753FC9763F0F07759CE9438442A8` |
| API26 `ets/api/@ohos.UiTest.d.ts` | `7CFFE0842708BB6DCDFDF7ED5FA87FAD8ADB9F9AA172D47E5B1964908F322EE6` |
| API26 `ets/component/image.d.ts` | `8ABC19167E94C6A10564D04D2D28582E21FD292F804E264FF6E94B47509F118A` |

SDK root: `C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/`. `Component.isEnabled` is declared at UiTest line2231; `screenCap`3080 saves PNG to the calling application's sandbox; `dumpLayout`3116 is API26. Installed DevEco `@ohos/coverage/.../ohosTest/executeOhosTest.js` uses `bm install -p <stagingDirectory>` (line84) and the leading-slash `-s unittest /ets/testrunner/...` AA convention (169–172). Its automatic uninstall wrapper is deliberately not used.

## Exact independent binary and root installation recipe

Final generated project: `hmos/tool/image-gesture-tester/work/build-1791358849475` (ignored). Original v19 project/HAPs and the first dev20 build are preserved; use these **final** artifact hashes for this frozen source:

| Artifact relative to final project | Bytes | SHA256 |
| --- | ---: | --- |
| `entry/build/default/outputs/default/entry-default-unsigned.hap` | 11,233 | `F580A08FFD221CC7C7ADA7E1AE3F913757D45A1E0A6148320CE770401F4FF6D4` |
| `entry/build/default/outputs/ohosTest/entry-ohosTest-unsigned.hap` | 55,044 | `E6E5677144CD0A6BD0450B6E13A119E105B5949D0AD221384B3CA9B400A46A22` |

Actual fresh build logs: [main6.287s SUCCESS](image-tester-main-build.log), [ohosTest6.779s SUCCESS](image-tester-ohosTest-build.log), with real `OhosTestCompileArkTS`. The SDK's throwing-API warnings and no-signing-config warning are retained. Unsigned device-install/debug-signature acceptance is not proven by a build.

From repository root, this command **prints only** the exact verified installation commands:

```powershell
& 'C:/Program Files/Huawei/DevEco Studio/tools/node/node.exe' hmos/tool/image-device-driver.cjs install-plan hmos/reports/ui-source/v20/image-tester-install-config.json
```

The checked config selects the root-provided device `127.0.0.1:5555` and a new staging directory `/data/local/tmp/morrow-image-tester-dev20-image-owned-20261007-a`. Its printed stages are `hdc -t <device> shell mkdir <freshDirectory>`, `file send` each exact HAP to that directory, `shell bm install -p <directory>` once, then `shell bm dump -n dev.morrow.hmos.gesturetester` to confirm both modules/version200001. Root executes each only after the previous acknowledgement. Existing-directory failure, signature rejection or an unknown outcome stops this plan; no uninstall/reinstall or signature/global-setting bypass is prescribed. The report/config does not execute those commands.

## Safe observation and probe setup

Root first reconciles its existing picker and draft, retains the owned content, and independently binds installed product archive/hash to a fresh product `bm dump`. The tester must not do that preparation. Root opens its own confirmed image from an editor with exact exposed `draft-title`; a detail-only preview without draft title intentionally stops this tester. Before a probe, root supplies the exact title, preview image name and fresh `attachment-image-gesture-canvas` native pixel bounds. **No current image coordinates, fixture name, token, viewport or product version were guessed or generated in this work.** All coordinates in local tests are mock-only.

Create a new config with `operation=observe`, fresh simple `runLabel`, fresh `expectVersionCode`, exact `expectDraftTitle`/`expectImageName` and `left/top/right/bottom`. Optional `expectHapSha`/`expectToken` are recorded expectations only. Print the AA arguments with:

```powershell
& 'C:/Program Files/Huawei/DevEco Studio/tools/node/node.exe' hmos/tool/image-device-driver.cjs probe-plan <root-observed-config.json>
```

The remote command is `aa test -b dev.morrow.hmos.gesturetester -m entry_test -s unittest /ets/testrunner/ImageGestureRunner -s operation observe -s runLabel <fresh> -s expectVersionCode <fresh> -s expectDraftTitle <exact> -s expectImageName <exact> -s left <observedPx> -s top <observedPx> -s right <observedPx> -s bottom <observedPx> -s timeout 120000`. Use the printer's quoted string/argument array, not manually concatenated fixture names. Root invokes it through its existing device channel.

Run readonly observe first. Every observation rechecks installed product version, active/focused bundle `dev.morrow.hmos`, unique modal/canvas/image/reset IDs, exact modal name and image description, exact original draft title, expected bounds within1px, ready-control status and markers. A second fresh observation follows the first capture. Another final foreground check precedes input. The product's current Index supplies responsive vp width `min(660,viewportWidth-24)-36` and height `max(160,viewportHeight-270)`; those source formulas are not substituted for actual device pixel bounds.

## Per-stage visual acceptance

Each gesture config gets a new label and runs only that named operation. Root independently prepares the starting state; the tester never resets/pinches to prepare another operation.

| Stage | Starting observation | Required real result beyond injection ACK |
| --- | --- | --- |
| `observe` | exact owned image/dialog/version; stable canvas | View original PNG features, enabled reset, absent loading/failure marker, 100% at initial open. Component existence alone is insufficient. |
| `pinch-out` | explicitly observed100% | Genuine2×9 injection; percentage increases within100–250 and visible landmarks grow around the focal center, bounded by canvas clipping. Do not demand exactly250: recognizer threshold affects the first admitted scale event. |
| `pan-one`, `pan-two` | already visibly zoomed | Percentage remains unchanged; recognizable image pixels translate in requested direction within the same canvas. A changed component rectangle or unchanged percentage alone is not movement evidence. |
| `boundary-pan-two`, then separately `pan-reverse-one`/`pan-reverse-two` | current zoom and position freshly observed | Landmark motion reaches a boundary without moving the layout box beyond the permitted `(scale-1)*width/2`, `(scale-1)*height/2` translation; opposite input moves away from that edge without a sticky overshoot. One outward trace from center may not reach the edge at high scale; root must inspect the result before choosing another new stage. Existing ImageFit.Contain letterboxing is not an overscroll defect. |
| `pinch-in` | already zoomed | Actual two-finger injection; percentage decreases and rendered features shrink, never below effective100%. |
| `pinch-moving-focal` | explicitly observed100% | Features grow and follow the moving two-finger center while clipping/bounds remain correct. Requires identifiable nonsymmetric pixels; percentage alone does not establish focal correctness. |
| `double-tap-reset` | already zoomed | Percentage100% and pixels match the original fitted/centered state. This is the existing HMOS supplement; Flutter attachment_view does not register this double tap. |
| Close/reopen or Home/reopen (root's separate UI stages) | original pin inventory already bound | Exact owned pin/bytes remain; new preview starts centered100%, without stale completion from a prior preview or autoplay/implicit input. Internal token/late-callback race qualification remains model evidence unless separately observed. |

Root reviews before/after PNGs and writes a scoped per-stage visual verdict with concrete landmark displacement/size, source fixture/pin identity and exact installed-product evidence. Unambiguous featureless images cannot establish focal/pan correctness. Moving/animated content must not be confused with translation. GIF, malformed files, inertia, hardware and full Flutter parity remain outside this bounded still-image qualification.

## Artifacts, Unknown and decoder triage

Runner prints the actual `reportDirectory`: tester `getAppContext().filesDir/image-gestures/<runLabel>`. Root obtains the real verified sandbox mapping before retrieving `result.json`, `before/after.png`, `before/after.tree.json`, `before/after.observation.json` and `input-intent.json`. Do not guess `/data/app/...`, userID or host mapping. After collection, `node hmos/tool/image-device-driver.cjs inspect-report <collectedDirectory>` records hashes and observable percentages; PNG checks cover signature/IHDR only, not image decoding or visual acceptance.

Existing label creation fails before input. Repeated runner instance stops. `INPUT_INTENT` is fsynced before one injection; a provider false/exception or failure after injection remains `UNKNOWN_INPUT_EFFECT`, preserving its intent. Do not rerun the same operation to resolve uncertainty: first obtain a new readonly observation/screenshot and reconcile the previous intent. `onStop` blocks subsequent input, but cannot cancel a gesture already submitted. Even successful input reports `ACKNOWLEDGED_REQUIRES_VISUAL_REVIEW`, never an automatic visual PASS.

Preview token/pin identity is `NOT_EXPOSED_BY_UI`; archive hash is `HOST_DECLARED_NOT_READ_BACK`. Exact root fixture/pin/package evidence must supply these bindings. The actual product delegates still-image decoding to ArkUI `Image(session.uri)` after an independently verified read/export; `AttachmentFiles.previewFile` retains exact verified bytes at an owned `data` path and MIME/name chooses the viewer. API26 image declaration explicitly states `loadingStatus=0` means data loaded, `1` means decoded; current `completeFor` correctly waits for1 plus positive dimensions and current token/URI/epoch. No decoder defect was reproduced from this source review. If pixels remain absent, inspect readiness marker/control state, original verified bytes/export and current `onError`/failure evidence before gestures; do not equate valid source-read/100% label with successful decode. Preserve the original pin/export and avoid unrelated codec or source changes without actual failure evidence.

[Fresh22 tests](image-device-tool-tests.log): **14 actual runner/trajectory checks +8 host driver checks**, synthetic SDK/filesystem/service only. Tests do not require ignored built HAPs: host install checks use explicitly synthetic ZIP manifests, while final real artifacts were separately hash/manifest checked above. Production source and original v19 reports remain unchanged. All actual image device stages in this reviewer report remain **NOT_RUN** until root supplies and reviews the fresh evidence.
