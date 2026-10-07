# dev20 explicit API26 SDK compile smoke

The dormant EditorTodos component/model, EditorInputPolicy and EditorInputHash have now been compiled from an explicit independent entry point by the real API26 SDK. **Final smoke BUILD SUCCESSFUL, 10.754s; installation and device execution NOT_RUN.** Product Index has not imported these modules, and product AppScope remains `0.1.0-hmos-dev.19 / 1000019`. This is source/SDK qualification of a distinct smoke bundle, not evidence that the new row editor or formatter is active in the Morrow product UI.

## Exact isolated project and boundary

The [prepare helper](../../../tool/checkpoint-sdk-smoke/prepare.cjs) creates a fresh project only inside ignored `hmos/.build/checkpoint-sdk-smoke`, preserving every earlier attempt. It copies the actual final AppScope, entry, SDK/Hvigor configuration, installed dependency files, native bridge/types and both already-built Rust archives. It records every original byte length/SHA256 and immediately verifies the copied bytes. No production Index, AppScope, native or resource file is rewritten by the helper.

Five files are explicit generated smoke harness overrides/additions: AppScope app identity, module entry ability, main_pages profile, `CheckpointSmokeAbility.ets` and `CheckpointSdkSmoke.ets`. The isolated entry explicitly imports and instantiates the real EditorTodos component and `EditorInputPolicy(native.editorInput, editorInputHash)`, with the real EditorFieldPolicy count callback and complete-value row formatter adapter. It also references direct `editorInputHash` and formatter calls from a button. That button was **never run**; compilation alone does not establish runtime hashing/NAPI/IME behavior.

The smoke app is `dev.morrow.hmos.checkpointsdk / 0.1.0-sdk-smoke.20 / 2000020`, with an unexported primary ability and no installation. Its generated identity is not a product version bump.

## Preserved first failure and minimum source repair

First project: `hmos/.build/checkpoint-sdk-smoke/dev20-2026-10-07T08-06-46-808Z`.

[First build log](sdk-smoke-first-build.log): **CompileArkTS FAILED**, overall 9.740s. Three specific production component declarations collided with inherited CustomComponent methods: parent `enabled:boolean`, parent `onFocus(owner,row)` and child `enabled:boolean`. The compiler reported their incompatible base-property types. The failed build produced no unsigned HAP; [first result](sdk-smoke-first-result.json) records exit −1 and an empty artifact array. The original [307-input source copy manifest](sdk-smoke-first-source-copy.json) and [copy verification PASS](sdk-smoke-first-copy-verification.json) are retained, rather than replaced by the fixed source.

The only production change for this smoke repair is naming: parent/child `editingEnabled`, parent callback `onRowFocused`. Their existing logic and method calls to ArkUI `.enabled(...) / .onFocus(...)` remain. Component-method tests and the row integration recipe were updated. A fresh search found no product caller importing or instantiating the new component/policy/hash, so there was no product Index call site to alter. The owned row checks remain [55/55 PASS](editor-todos-model-tests.log): 36 model and 19 actual component-method checks.

## Final compile and hash verification

Fresh final project: `hmos/.build/checkpoint-sdk-smoke/a2`. This shorter path also avoided the first attempt's CMake long-object-path warning.

[Final build log](sdk-smoke-final-build.log): API26 `assembleHap`, debug, **`--no-daemon`**, 34 tasks executed / 0 up-to-date. `CompileArkTS` took 4.144s; complete build was **SUCCESSFUL in 10.754s**. Installed compiler identity and distinct bundle/version are captured directly from the built HAP [module metadata](sdk-smoke-final-hap-module.json): `compileSdkVersion=26.0.0.105`, target/min API26, `debug:true`. Signing was skipped because no signingConfigs profile was configured.

The [final copy manifest](sdk-smoke-final-source-copy.json) contains **307 copied input files plus 5 generated harness files**. After compilation, the [read-only verifier](../../../tool/checkpoint-sdk-smoke/verify.cjs) checked every original source hash against that manifest, all untouched copied files and all five generated harness hashes: [final verification PASS](sdk-smoke-final-copy-verification.json). The verification specifically compares current product source bytes, not just copied source file names. AppScope and product Index hashes are identical in the first and final manifests.

| Explicitly compiled final source | Bytes | SHA256 |
| --- | ---: | --- |
| `entry/src/main/ets/pages/EditorTodos.ets` | 16,854 | `3d01061aa8a062860d742f3e308420cf623505cf0cae01ade3b143f3ec80d5cf` |
| `entry/src/main/ets/model/EditorTodos.ets` | 18,871 | `1ceacea4f25a00fa815904b4a13f1219fd320d11a0b1bb662a1daa3945fa066f` |
| `entry/src/main/ets/model/EditorInputPolicy.ets` | 7,887 | `64c86330d3dbc8fa85f88d6f0692d9ed15c88814c711f4f2b665827b3628ae2b` |
| `entry/src/main/ets/model/EditorInputHash.ets` | 463 | `c894e8f44aa1a447096a8ac12b9a9c437eeaa64dd4875aa6116e7635893be628` |
| `entry/src/main/cpp/types/libmorrow/index.d.ts` | 1,524 | `6791086c2d958f95e54733a3fc0a87d5908acba9bddcab279aa69807363526f3` |
| copied ARM64 `libmorrow_hmos.a` | 55,415,340 | `8295affe01a0c8c2f24b6080dc29caa304f03687245be240acb0722e0e664ab5` |
| copied x64 `libmorrow_hmos.a` | 53,821,348 | `24635bf2b6a4a57ccc512227d87a25ee76b3e9d7bf49d7fc0ef96e866eb15a33` |

The [final result](sdk-smoke-final-result.json) records the independently generated unsigned smoke artifact:

`hmos/.build/checkpoint-sdk-smoke/a2/entry/build/default/outputs/default/entry-default-unsigned.hap`

**26,464,308 bytes; SHA256 `B28AF8BF91FDE3A990205830F4605901318C6FAB4DFF6682EDB7BB0D329EB480`.** This exact artifact is preserved in the ignored project and is not a product release/distribution artifact.

## Reproduction and remaining limits

Run `tool/checkpoint-sdk-smoke/build.ps1` to prepare a new copy and compile it, or supply `-Project` for an unused prepared copy. The script rejects an existing sdk-build.log and never deletes or moves prior projects. It uses the installed DevEco Node/Hvigor, SDK/JBR and a no-daemon build; it performs no installation/device/Git action. Individual process-output waits during this run were at most 10 seconds.

The final SDK warnings remain recorded: API syscap portability for TextAreaController and crypto Md, exception-handling advice for the generated ability and crypto calls, and the native compiler's unused gcc-toolchain option. They did not prevent the SDK build. No runtime syscap, crypto, rendering, input, focus, drag, IME, journal or business save/readback behavior was exercised. The separate [row source audit](todos-source-audit.md) retains those limitations. The main application's new component/formatter integration and the full Flutter/HMOS target remain OPEN.
