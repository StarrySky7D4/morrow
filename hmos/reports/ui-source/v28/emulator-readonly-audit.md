# v28 existing emulator readonly startup audit

Observed 2026-10-09T00:58:07.4524320Z (08:58:07 Asia/Shanghai). Root narrowed this audit after the target appeared: verify the live process, socket, exact existing instance, and actual device API; do not expand startup diagnosis.

## Current evidence

- `Emulator.exe` PID **23912**, parent15172, remains live. Its actual command line selects `-hvd "Pura X View2"`, existing deployed root `C:\Users\Administrator\AppData\Local\Huawei\Emulator\deployed`, existing image root `C:\Users\Administrator\AppData\Local\Huawei\Sdk`, `-bootmode snapshot -noWindow`. Process creation is **2026-10-09T00:56:28.208821Z**. This matches Root's [launch record](device/launch.json); no replacement process was created by this audit.
- Windows TCP state shows **PID23912 owns 127.0.0.1:5555 Listen** and an **Established** connection to 127.0.0.1:62665. HDC server PID17972 owns the other endpoint. The target is therefore connected to the same live launched emulator process.
- Readonly selected fields from the deployed `Pura X View2\config.ini` confirm name **Pura X View2**, UUID **01fc19c8-444a-42f1-9f70-79321a2502b3**, exact instance path, `system-image/HarmonyOS-7.0.0/phone_all_x86/`, x86_64, and configured API26.0.0. Configuration alone is not used as proof of current device availability. The pre-launch instance object's `isRunning:false` is an old observation, not current process state.
- Actual HDC `list targets` now returns **127.0.0.1:5555**, exit0. Commands explicitly targeting this endpoint returned:

| Read | Actual result |
| --- | --- |
| `param get const.ohos.apiversion` | `26` |
| `param get const.ohos.fullname` | `OpenHarmony-7.0.0.105` |
| `param get const.product.model` | `emulator` |
| `uname -m` | `x86_64` |

The configured software label7.0.0.107 is not substituted for the actual runtime fullname7.0.0.105. Both the actual API result and the existing instance configuration indicate API26. The observed runtime process/socket/target are sufficient to proceed with Root's next bounded device reads; lack of a separately named QEMU child does not establish startup failure when this same process serves HDC and replies to shell reads.

## Outcome and safe next step

Startup has progressed from Root's earlier Empty HDC state to an available API26/x86_64 target on the exact existing launched process/instance. No restart is indicated by the current evidence. Root can now read the installed bundle/version and installation identity on **127.0.0.1:5555**, retain that baseline, then perform the separately authorized new-package device validation.

This audit performed only process/socket reads, selected ordinary emulator configuration fields, existing local launch evidence, and the five listed HDC readonly commands. It did not read credentials, inspect password files, restart/kill a process, change emulator configuration, remove cache/data, download images, install/start an application, or run a build/test. New HAP installation, rendering, input/IME, business save, restart recovery, and full product acceptance remain outside this startup audit; their results must be recorded separately by Root.
