# 模拟器环境复核（2026-10-05）

- CLI：`1.3.0-stable`；Emulator：`26.0.0.400`；DevEco Studio：`26.0.0.821`。
- 已部署镜像：`C:\Users\Administrator\AppData\Local\Huawei\Sdk\system-image\HarmonyOS-7.0.0\phone_all_x86`，`sdk-pkg.json` 显示 HarmonyOS `7.0.0.106`、API `26`，主要镜像文件存在。
- 已有实例：`Pura X View`；此次使用原有实例、数据和快照，未下载镜像、创建实例或修改环境配置。

## CLI 误判与启动结果

`devecocli emulator start "Pura X View"` 在启动前报告 `The system image file HarmonyOS 7.0.0(26.0.0) cannot be found`。CLI 的 `assertSystemImageAvailable` 调用原生 `-imageList -downloaded true`；本次该查询退出码为 `0`，但输出为空，导致 CLI 提前拒绝启动。原生 `-list -details` 则正确给出用户 SDK 的 `imageRoot` 和已有实例路径。

首次原生启动使用 `-start`，并将 `-instancePath` 指向实例目录。进程 PID `25268` 数秒后退出，HDC 仍为空，未进入可连接的设备状态。没有足够证据将该失败归因于某一个参数。

随后使用与 CLI 后备策略一致的 `-hvd` / `-path`，其中 `-path` 指向实例的父目录 `deployed`：

```text
Emulator.exe -hvd "Pura X View"
  -path "C:\Users\Administrator\AppData\Local\Huawei\Emulator\deployed"
  -imageRoot "C:\Users\Administrator\AppData\Local\Huawei\Sdk"
  -bootmode snapshot -noWindow
```

由 `Start-Process -WindowStyle Hidden` 启动，进程 PID `24952` 持续运行，随后 HDC 已恢复目标 `127.0.0.1:5555`。此次使用 `snapshot`，没有使用清除数据的 `reset`。

## 原实例的安装失败诊断

更新 `Pura X View` 内的应用时，`hdc install` 和覆盖安装均返回 `9568289`（`grant request permissions failed`），设备的 `bm dump` 仍确认已安装 dev7 / `1000007`。

只读解压 `/data/log/hilog/hilog.260.20261005-145735.gz` 后，`2026-10-05 14:59:39` 的日志显示完整失败链：

1. BMS 为 `dev.morrow.hmos` 更新 TokenID `537093490`，请求权限数与 ACL 数均为 `0`。
2. AccessToken 服务读取旧权限状态时发生 SQLite `Error(11)`；日志中的部分内容经过系统隐私遮罩，但明确记录数据库损坏。
3. `RestoreAndQueryIfCorrupt` 检测到数据库损坏并自动尝试恢复；备份完整性检查仍失败，返回 `27394104`，随后报告 `Db restore failed` 和无法取得旧权限状态。
4. `UpdateHapToken` 返回 `27394104`，BMS 内部映射为 `8519711`，安装工具最终返回 `9568289`。

同一时段 Launcher 系统库也报告 `14800011` / 数据库损坏，HiView 事件文件创建失败并记录 `errno=74`。`/data` 使用率为 `19%`。这些证据将本次安装失败定位到原设备的系统权限数据库；底层损坏起因尚未确定，不能直接归因于快照。

诊断仅使用日志与状态查询。未重启原设备、卸载应用、清除数据或修改设备数据库；日志里的数据库恢复动作由系统自动执行。

## 现有第二实例的 dev8 安装

父任务使用已有 `Pura X View2` 目录，通过相同的原生后备参数首次启动该实例：`-hvd "Pura X View2"`、`-path` 指向 `deployed` 父目录、`-imageRoot` 指向用户 SDK，使用 `snapshot` / `noWindow`，并设置 `Start-Process -WindowStyle Hidden`。启动进程 PID 为 `24716`，没有创建新实例或使用 `reset`。

观察到实例端口 `5557` 后，通过 HDC `tconn` 连接。新 HAP 安装成功，`bm dump` 已确认 dev8 / `1000008`。运行设备报告的软件版本为 `7.0.0.106`；该实例 `config.ini` 的 `7.0.0.107` 只是配置值，不作为实际运行版本证据。

该实例密度为 `480`，屏宽 `1320px` 对应 `440vp`。设备 UI 验证正在进行，尚未完成验收。首次文本输入触发输入法隐私引导；测试选择“取消”，再同意基本模式（无需个人数据），并在默认 26 键选择后继续测试。

## 验证边界

`emulator-start.stdout.log` 记录 Hypervisor 加速器可用；`emulator-start.stderr.log` 包含 `hash_table != NULL` 断言和 VTubeStudioCam / DirectShow 缓冲警告。警告出现后 HDC 仍恢复连接，不能据此直接判定设备不可用。

本记录确认环境路径、原生后备启动、原实例安装失败原因和第二实例的 dev8 安装结果。新实例的 UI 渲染及功能验收仍在本轮后续设备验证中，不能由安装成功推断完成。
