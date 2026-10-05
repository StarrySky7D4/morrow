# dev.12 验证记录

日期：2026-10-05。分支：`codex/ArkTsUI`，报告读取时基底 HEAD 为 `3843e997259d5f9ad40c161a694977846eb274b7`，本轮变更位于工作树。应用版本为 `0.1.0-hmos-dev.12`，versionCode `1000012`，bundle `dev.morrow.hmos`。

当前结论：附件开发闭环已接通，主机、双架构构建及5557模拟器的限定原生/UI流程通过。功能参照仍为Windows test.57/test.58；完整Flutter等价、真机和生产发行资格继续开放。

本文件由 platform audit agent 只读整理现有最终日志、源码和产物；该 agent 没有执行设备操作、构建、安装、提交或推送。设备执行证据由 root 产生。详细模型和平台边界见 [附件主机报告](attachment-host-review.md) 与 [平台审计](platform-audit.md)。

## 最终验证证据

| 项目 | 结果与原始记录 | 实际覆盖及边界 |
| --- | --- | --- |
| Rust 主机测试 | **72 passed / 0 failed**，[final-rust-host.log](final-rust-host.log) | 开发隔离 Store、草稿/附件 pin、原请求历史重试、流读写、查询/Markdown/粘贴等；Windows 主机测试 |
| 实际 ETS 模型 | **119 passed / 0 failed / 0 skipped**，[final-arkts-models.log](final-arkts-models.log) | 加载实际 ETS 源码执行模型行为；包括附件 helper 的 **43 项**，43 是 119 的子集，不能重复累计；kit/native provider 是合成替身，不能证明实际选择器授权或 IME 会话 |
| OHOS 附件 runner | **PASS_SCOPED**，[native-attachment-final.log](native-attachment-final.log) | 5557 x86_64 上的独立原生程序；98,321 字节跨 32KiB、空文件、导入/发布/移除/重启/原 spool 消失后导出、失败边界及 256 业务卡容量；隔离 fixture `/data/local/tmp/hmos-dev12-attachment-final-20261005-A` |
| 原生 FD 检查 | **FD 10 → 10**，同一 [native-attachment-final.log](native-attachment-final.log) | prepare 接管 FD、无效 prepare、同 inode 拒绝且关闭两 FD、64 个无效 import schema FD、无效 export schema FD 关闭；只覆盖 runner 明确执行的 Rust consuming ABI 场景，不是所有 NAPI 排队失败路径的运行证明 |
| OHOS 基础 self-check | **28 个唯一检查，PASS**，[native-selfcheck-final.log](native-selfcheck-final.log) | 原生日志为 `platform=linux`、`arch=x86_64`、`profile=development-unsealed`；包含业务/CAS/历史重试/草稿、Markdown 和粘贴；不等同于 ArkUI 点击和渲染验收 |
| arm64 Rust + runners | **release 编译完成**，[final-rust-arm64.log](final-rust-arm64.log) | `aarch64-unknown-linux-ohos` 库和两个 runner 的构建；本轮没有 arm64 运行证据 |
| x86_64 Rust + runners | **release 编译完成**，[final-rust-x64.log](final-rust-x64.log) | `x86_64-unknown-linux-ohos` 库和两个 runner 的构建；原生运行范围如上 |
| 最终 HAP | **BUILD SUCCESSFUL**，[hap-icons-final-build.log](hap-icons-final-build.log) | 最后按 Flutter 类型图标修正后打包；ArkTS 编译、双架构本地库打包完成。构建脚本实际为 `buildMode=debug`，未配置签名而 skip sign |

附件 runner 的 `functional`、`failures`、`capacity` 和 `descriptors` 回报均通过。它明确返回 `production_capture_huks_device_picker_ui=NOT_QUALIFIED`、`profile=development-unsealed`，故生产 HUKS/audit/capture S1/S2 和系统选文件器 UI 没有被这个 runner 授予资格。

[附件主机报告](attachment-host-review.md) 的早期 Windows 执行仍保留 `NOT_RUN non-Unix host`，这是那次主机运行的边界。本轮新增的原生 FD 结果在 `native-attachment-final.log`，两份记录应按平台与时间分开读取。独立 host runner 与集成测试共享场景，也不作为两组独立产品验收重复计数。

## 严格编译修正与最终产物

[hap-final-build.log](hap-final-build.log) 保留了一次真实失败：`AttachmentFiles.ets:136:7` 的 `throw error` 违反 `arkts-limited-throw`，日志为 `BUILD FAILED`。root 将其改为 `throw new Error('附件缓存读取失败，已保留。')`，保留非 ENOENT 读取失败的闭合行为，然后重新运行实际 ETS 模型及 HAP 构建。[final-arkts-models.log](final-arkts-models.log) 中的附件 43 项与总体 119 项仍通过，[hap-release-build.log](hap-release-build.log) 是修正后的最终成功记录。旧失败日志不应删除或标成成功。

截图复核发现附件行误用 `format_quote`，最后按照 Flutter `AttachmentTile` 和本机 Flutter `icons.dart` 改为图片/GIF、视频、音频、普通文件及三维文件类型图标，并显示扩展名与大小。此最后变更只涉及附件行展示，Rust、C++ 与 URI helper 字节保持不变；[hap-icons-final-build.log](hap-icons-final-build.log) 为最终成功构建。

最终 HAP：`hmos/entry/build/default/outputs/default/entry-default-unsigned.hap`，24,404,611 字节，SHA-256：

```text
07ae2562e04cbe189f04bfe1b60f1cc7458cdecbfe3ecf02f3bc0b7c2d83a467
```

只读 ZIP 清单确认包内含：

- `libs/arm64-v8a/libmorrow.so`：8,777,960 字节，以及该架构的 `libc++_shared.so`。
- `libs/x86_64/libmorrow.so`：9,219,592 字节，以及该架构的 `libc++_shared.so`。

最终源与静态库 SHA-256：

| 文件 | SHA-256 |
| --- | --- |
| `entry/src/main/ets/model/AttachmentFiles.ets` | `497edacad0398f66404845542103fc59ac3985026ab155f79345d58d59819115` |
| `tool/attachment-files-model.test.cjs` | `921478fe66a6895231ffccc8c7ac09918a827bbd945add783d3d525bf7624a0c` |
| `entry/src/main/cpp/bridge.cpp` | `9a52517d98bf3aa4fec2b95f9b53e48ad6126fa97b64feea9cf6066f0de94ba7` |
| `entry/src/main/cpp/rust/arm64-v8a/libmorrow_hmos.a` | `e324e0a173d5aeffab3382ff8d7e49d780a6c40118194d6623eddce2205a29f9` |
| `entry/src/main/cpp/rust/x86_64/libmorrow_hmos.a` | `8baf319f8dc8b790e5f0b66a46c19d976fde7fa9c0a77d95765cc92a167fe55b` |

## 设备状态与 UI 证据分界

root 已将最终 HAP 安装到 `127.0.0.1:5557`。[bundle-icons-final.json](bundle-icons-final.json) 记录 `versionName=0.1.0-hmos-dev.12`、`versionCode=1000012`、`cpuAbi=x86_64`、`nativeLibraryPath=libs/x86_64`，compile SDK 为 `26.0.0.105`。设备为 Pura X View / HarmonyOS 7.0.0.106，1320×2232 px / density3 / 440 vp；未改密度/方向/系统参数、未重置、不操作已知故障5555设备。bundle 元数据支持安装版本与平台信息；本报告没有对设备端 HAP 文件重新做独立哈希核验。

`device/candidate-*`、`device/draft-image-preview-candidate.*`、选择器浏览和 `device/progress.json` 中的 candidate attachment 记录来自最终严格编译修正之前，属于旧候选 UI 证据，不能自动用于最终 HAP 完整验收。它们保留作路径与问题复现材料。

设备流程按包分开记录，实际步骤见 [device/release-checks.json](device/release-checks.json)。早期候选通过实际编辑器创建唯一测试卡 `HMOS-attachment-20261005-A`，身份 `204235e9-2eb4-4b94-91d3-5a84ff881bf9`，并首次导入图片。后续没有重seed、重发已确认的创建/修改或改写应用数据库。

`031923A0…` 是严格编译修正后的功能候选，24,402,299字节。该包执行：

- [device-restore-release.log](device-restore-release.log)：进程重启后恢复此前已确认的 image pin，实际预览显示完整图片，再关闭；`release-restored-pin-preview.png` 为该包截图。
- [device-document-release.log](device-document-release.log)：系统文件选择器打开精确中文文档URI，38字节文档被确认加入同一草稿，保留两个附件；未把URI、路径或FD放入Rust请求。
- [device-publish-release.log](device-publish-release.log)：保存两份附件到原卡，同一身份、原Markdown中文/emoji正文读回，没有创建副本。
- [device-export-image-release.log](device-export-image-release.log)、[device-export-document-release.log](device-export-document-release.log)：实际保存选择器生成自己的新测试文件；[export-byte-comparison.json](device/export-byte-comparison.json)确认图片1,924,867字节和文档38字节均逐字节一致。SHA分别为 `D0C0BDD622E6349D8E7357035376C818F2DBD7DA5EE28E1752D4D8405E52AD0F`、`BAEF51C8AC22F4485193FF57A071CB32510B2C3F56599B4D240596E7E9DF44D4`。
- [device-cancel-release.log](device-cancel-release.log)：业务重启后两份附件仍在；重新进入未修改编辑器、取消选择、保留关闭，实际草稿列表无此卡journal。

最终 `07AE2562…` 包只追加类型图标与扩展名显示，复用完全相同的Rust/C++/URI helper，随后重新安装并执行：

- [device-icons-final-read.log](device-icons-final-read.log)：读回两份业务附件，实际PNG/TXT图标和扩展名显示正确，图片预览仍显示完整内容；`icons-final-two-assets.png`、`icons-final-image-preview.png`已人工查看。
- [device-remove-icons-final.log](device-remove-icons-final.log)：显式编辑只移除图片引用，保留文档；保存、进程重启、原身份回读后只有文档，完整中文/emoji正文仍在。`icons-final-one-reference-reopened.png`为本包截图。
- [device-export-document-icons-final.log](device-export-document-icons-final.log)：对移除后保留的文档再次使用实际保存URI导出到新的测试文件；[icons-final-export-byte-comparison.json](device/icons-final-export-byte-comparison.json)确认38字节仍与原件完全一致，SHA与上表文档一致。

最终本地HAP另保存在 `hmos/.build/artifacts/dev12/entry-default-unsigned.hap`，SHA与构建输出一致，避免下一轮构建覆盖本轮包；二进制位于忽略的构建目录，分支提交源码、来源声明及验收记录。

上述操作仅针对本任务命名的公开测试文件，未操作用户原文件。普通文档提供导出入口，音视频播放和系统默认应用打开仍未接；图片解码失败/缩放、实际拒权/空间耗尽/进程在途终止、所有provider的sync/stat行为、长内容/宽屏/全部主题未完成设备矩阵。移除当前引用不会清掉核心事件历史，不能承诺立即释放空间。附件原import请求可在私有sidecar中保留核对；abandon原JSON与正式业务在途请求仍只保存在当前进程，未声明跨进程正式Unknown恢复。

本轮在该开发库实际保留 original raw schema/model、durable import Pending/Ready/Retired/pruned语义、0/2/3来源pin与别名、完整源CAS和原操作历史比较。无效/未知导入不自动重放；中断准备只有确认未准入的安全残片可清理，存在或不可读请求保留并显示通用核对提示。主机模型证明不替代上述设备范围。

上游观察见 [upstream-audit.md](upstream-audit.md)。共享快照228文件保持固定，最新122项漂移为50个已有变化与72个新增；没有直接覆盖正在使用的快照。构建输入与最终HAP由 `reports/build-manifest.json` 的290项哈希固定，并核对磁盘及Git暂存字节。仅提交/推送 `codex/ArkTsUI`，不合并或推送主线。

## 平台和发行边界

本机 SDK：`C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony`，API 26、platform 26.0.0、Release SDK 26.0.0.105；项目 target/compatible SDK 为 26.0.0。该安装和 5557 的兼容结果没有覆盖其他 API 级别、机型或 arm64 实机。

`hmos/build-profile.json5` 的 `signingConfigs=[]`，最终构建日志明确跳过签名；产物为开发 unsigned debug HAP，没有生产签名或应用市场发行资格。当前设备运行范围是 x86_64；arm64 编译和包内库存在不等于 arm64 真机通过。

完整 Flutter 功能对齐、音视频控制、图片缩放/失败表现、文件默认打开、内嵌附件图片、HTML/Office/剪贴板图片文件、跨段连续全篇选择及宽屏矩阵继续按 [PARITY](../../../docs/PARITY.md) 和 [ALIGNMENT_PLAN](../../../docs/ALIGNMENT_PLAN.md) 追踪。普通控件保存 composing 字段也不代表能够重建系统 IME 会话。生产保护、captured S1/S2 与跨进程 Unknown 的资格边界保持原状态。
