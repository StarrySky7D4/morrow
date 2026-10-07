# v24 Session — isolated API26 SDK compilation

**PASS, compile only.** [Build log](sdk-build.log): API26 `assembleHap`, no daemon, SUCCESS **9.480 s**, `CompileArkTS` **3.232 s**, 34 tasks executed. [Result](sdk-build-result.json) records exit/verification code 0. This is independent qualification of the frozen Session module's public API; no product entry, native adoption or device installation was performed.

The fresh project is `hmos/.build/checkpoint-sdk-smoke/dev24-business-session-retry1`, bundle `dev.morrow.hmos.editorbusinesssessionsdk`, smoke version `0.1.0-editor-business-session-sdk-smoke.24`. The isolated main page imports Session/Fields/DraftRecord/FieldPolicy and actually type checks prepare/discover/restore construction, prepare/issue/loadTransport/read/save/inspect/cancel/continueTodos, every retry, all getters, `mayConsume` and the required `parentReady` hook. Controlled providers throw rather than issue native Store requests. The reference method is not invoked by the smoke UI, and the unsigned HAP was never installed or run. Merely importing an unused module was not the test.

## Source copy and wrapper scope

[Prepare script](prepare-session-sdk-smoke.cjs) reuses unchanged `hmos/tool/checkpoint-sdk-smoke/prepare.cjs`, SHA `4CEC216A906852A286CAA8F04DE9262A135F1DA57A3A1FBF1B884B8500E57EFB`. It byte-copies a new project with 312 input files, then rewrites only the fresh base-generated app metadata and entry page. Module/page profile/ability remain generated base wrappers. Original production files, original CPP type declarations and older smoke projects are untouched. `entry/oh_modules/libmorrow.so` declarations are complete independent byte copies from their original types, not a new shared junction to be modified in place.

The [source-copy manifest](source-copy-manifest.json), **200,104 bytes / SHA `0D34A9EDD50A36A1EFFA9582288D346674506FD4373289119B68FD4B3D1004FA`**, records every copied input, source/real path, bytes/hash, generated wrapper and before/after rewrite identities. [Before-build verification](verify-before-build.json) and [after-build verification](verify-after-build.json) both PASS: 312 copied inputs, 5 generated wrappers, zero live-source differences. These two results are byte-identical, SHA `FA68BE4A95DE628C02DDFFE5DD72291F0B39D717110299288AB83899D1786A86`.

Six frozen ETS inputs and the complete CPP type dependency were checked before/after:

| Input | Bytes | SHA-256 |
|---|---:|---|
| `EditorBusinessSession.ets` | 32,782 | `792E95BC45D5D589A08810B4A3BED24FA0C6229612FB38FDA18ABD12A9821FBD` |
| `EditorBusiness.ets` | 26,767 | `6469D1F2AF4E2DA6B9066B52917E658BC13ECECB4EF2A4E6B92FBB4AD0D996CF` |
| `EditorDraft.ets` | 29,143 | `A3AE1E99D198FCBA3CDEF741A6F138612AD16979BE42C7A8809003F00D04F965` |
| `EditorFieldPolicy.ets` | 6,929 | `10C3162E77A947335ACF0C3DF2E464C80195D0AC2BBC2922BBB09C0A8A626842` |
| `Index.ets` (snapshot, not the smoke entry) | 285,929 | `AFF77EB7F1C90E53DDF5D64DD89FB3A037DAB98AC0B08AE0382090A2055EFEAF` |
| `EditorTodos.ets` (snapshot) | 24,901 | `83B6C6A5C5811EAD9FA14EBB23F9B5DEA7F9A858FD0B5461181091E929473836` |
| Original CPP `types/libmorrow/index.d.ts` | 1,524 | `6791086C2D958F95E54733A3FC0A87D5908ACBA9BDDCAB279AA69807363526F3` |

All generated wrapper bytes are saved below `generated/`, with their exact manifest identities:

| Wrapper | Bytes | SHA-256 |
|---|---:|---|
| `AppScope/app.json5` | 284 | `671AF4E06A1C83D117ED6BB20F91183FE33C9064979890226114B3B7A5667518` |
| `entry/src/main/module.json5` | 1,197 | `118A0B40CD48E7F9495B5E496E038182A70BD7193DBE3D35C7B1B1A052975D20` |
| `main_pages.json` | 50 | `BEAEC735946F5729B91041117F79AEFA42A069039A56CF65885F0669313D2004` |
| `CheckpointSmokeAbility.ets` | 254 | `E0A4A05120EBF3784623D7CCF3E26041B67BA84BCCDE3A7E7D7084A419229306` |
| [Session public API page](generated/entry/src/main/ets/pages/CheckpointSdkSmoke.ets) | 5,149 | `EE1B9337574A1B5D3403CBBA1C14B39B71DF7233597EACF405E1E9CA2913B451` |

## Preserved first wrapper failure and reproduction

The first project `dev24-business-session` stopped after `clean` because an **outer shell's relative log destination** was resolved after the build wrapper changed directory. Its [57-byte log](stage1-sdk-build.log), [copy manifest](stage1-source-copy-manifest.json) and [before-build verification](stage1-verify-before-build.json) are preserved. It produced no ArkTS error or qualification. The retry used a fresh project and an absolute outer log destination. No production source or API reference was changed to obtain success.

To reproduce from the repository root, choose another fresh path strictly beneath the smoke directory and absolute log paths:

```powershell
& 'C:/Program Files/Huawei/DevEco Studio/tools/node/node.exe' hmos/reports/ui-source/v24/session-sdk/prepare-session-sdk-smoke.cjs '<absolute fresh path under hmos/.build/checkpoint-sdk-smoke>'
& hmos/reports/ui-source/v24/session-sdk/build-session-sdk-smoke.ps1 -Project '<same absolute path>'
```

The [build script](build-session-sdk-smoke.ps1) and [verifier](verify-session-sdk-smoke.cjs) preserve earlier build evidence and require the exact session manifest. The verifier checks actual public API references, complete snapshot identity and frozen dependencies. SDK warnings for crypto API device capability/exception handling and toolchain/path limits remain in the successful log; compilation is not device crypto qualification.

## Artifact and limits

The unsigned artifact remains ignored inside the fresh project only: **26,169,517 bytes / SHA `2F56774CEBB9BAA65636B924977AC09F09819E55C8DF1DA34A7C8B4899228DF1`**. Runtime, device and installation: **NOT_RUN**.

CPP linkage used byte-copied **old v22** adopted archives: ARM64 56,104,946 bytes / `F3815F306E96618F8389B17E59679CDD1E1E477BC1324BE34943B579DBF2DAA4`, x64 54,511,556 bytes / `5ADCB7BAD62417868DD6A2D03699B9D69C3267DE1072B8534E0926FE30DE6B41`. This does not qualify the new intent backend or registered native transport at runtime. It does not qualify Index integration, source-0 business handoff, protected provenance or the full input/save/keep/close product goal. The preceding 30-model audit remains frozen historical evidence; this separate report supplies the later SDK-only result.

Author: `/root/dev19_delivery_docs`, 2026-10-07 UTC. No Git/device/product HAP/root build-manifest mutation. Session production bytes remained `792E…` throughout.
