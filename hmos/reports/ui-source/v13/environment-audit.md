# dev.13 existing emulator recovery

2026-10-07 03:06–03:07 UTC / 11:06–11:07 Asia/Shanghai. Scope: diagnose the CLI's missing-image rejection, then start only the previously used `Pura X View2` instance after root's explicit authorization. No emulator was deleted, reset, recreated, downloaded, or reconfigured. No app was installed/uninstalled, no application UI was driven, and no system settings were changed by this audit. Root takes over app package installation and UI acceptance.

## Image and instance diagnosis

Before launch, HDC reported `[Empty]` and no Emulator/qemu process existed. Native `Emulator.exe -list -details` listed three existing instances, all not running. `Pura X View2` had UUID `01fc19c8-444a-42f1-9f70-79321a2502b3`, instance path `C:/Users/Administrator/AppData/Local/Huawei/Emulator/deployed/Pura X View2`, `imageRoot` `C:/Users/Administrator/AppData/Local/Huawei/Sdk`, and image subpath `system-image/HarmonyOS-7.0.0/phone_all_x86/`.

The actual image directory exists at that root. `sdk-pkg.json` declares API26 / HarmonyOS 7.0.0.106(SP1), Release. Its SHA-256 is `BCE1BA34ADACDFD5A87A821773D5E78A209BFFAB6106CC4BFF586AFB98C35E79`.

| Actual image input | Existing bytes |
| --- | --- |
| `bzImage` | 10,244,992 |
| `ramdisk.img` | 2,752,488 |
| `system.img` | 3,670,016,000 |
| `sys_prod.img` | 838,860,800 |
| `vendor.img` | 209,715,200 |
| `userdata.img` | 104,857,600 |

Existing instance files include `userdata.img.qcow2` (1,178,009,600 bytes), `ram.img` (4,294,967,296 bytes), and system/vendor cache overlays. These existing files were preserved. The directory sizes are existence observations, not image integrity verification.

Native `Emulator.exe -imageList -downloaded true` returned exit0 with empty stdout, both before and after launch. The CLI's downloaded-image preflight consequently rejects the configured image even though native instance details and physical files establish its location. This is the same failure shape documented in the prior dev.8 environment report, now independently re-observed. It does not justify a new download or instance reset.

The instance `config.ini` says software7.0.0.107, while the actual installed base image is7.0.0.106. The config SHA-256 before and after launch is unchanged: `98B6CD6F9395909AF05A5B97C3CF359A14250EF3D16635F1240D25076948CCD6`. Runtime version is checked below rather than inferred from that config value.

## Authorized native launch

Start time: 2026-10-07T03:06:34.9809078Z. Windows PID **16972**. Launched with `Start-Process -WindowStyle Hidden` and output redirected to `emulator-start.stdout.log` / `emulator-start.stderr.log`. Arguments:

```text
Emulator.exe -hvd "Pura X View2"
  -path "C:\Users\Administrator\AppData\Local\Huawei\Emulator\deployed"
  -imageRoot "C:\Users\Administrator\AppData\Local\Huawei\Sdk"
  -bootmode snapshot -noWindow
```

The `-path` is the deployed parent directory. It is not the instance child directory. The pre-launch process check refused to launch if any emulator/qemu process already existed. Launch used the existing snapshot and never passed reset/wipe flags.

Stdout confirms the Windows Hypervisor Platform accelerator is operational. Stderr contains camera/DirectShow `VTubeStudioCam` buffer warnings; these did not prevent subsequent HDC connection. The process remained live during the 03:07:37Z observation. Its stdout/stderr files may continue to grow while the emulator runs. Publication therefore uses `emulator-start.stdout.snapshot.log` and `emulator-start.stderr.snapshot.log`; the still-open streams remain local and ignored. `emulator-log-snapshots.json` records the snapshot time, sizes and hashes.

## Connected runtime and handoff

The new process owns listener **127.0.0.1:5555**, and HDC reports that target. This port differs from the previous run's5557; port numbers alone are not emulator identity. Native `-list -details` after launch confirms `Pura X View2` / exact UUID above as the only running instance; original `Pura X View` and `Huawei_TripleFold` remain not running. `hdc tconn 127.0.0.1:5555` returned already connected, followed by successful shell queries.

Fresh read-only device queries:

- Software: `emulator 7.0.0.106(SP1DEVC00E999R4P11)`.
- API: `26`.
- ABI: `x86_64`.
- Existing `dev.morrow.hmos` bundle: versionName `0.1.0-hmos-dev.12`, versionCode `1000012`.

Raw pre-install bundle metadata is preserved as `bundle-before-install.json`; native instance metadata is `emulator-instances-after-start.json`; process/target/image-query/config-hash observation is `emulator-recovery-observation.json`. Starting an existing snapshot necessarily resumes its emulator disk activity, but no configuration or user data reset was requested or performed.

The device is available for root's dev.13 installation and UI acceptance. This recovery establishes only live emulator/HDC/bundle identity, not dev.13 rendering, image gestures, media decoding, asset lifecycle, or ARM64 physical-device qualification.
