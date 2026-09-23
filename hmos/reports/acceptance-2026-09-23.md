# HMOS 开发预览验证记录

本次完成独立工程与第一批实际功能，**没有完成与 Flutter 功能等价的 HMOS 版本**。版本 `0.1.0-hmos-dev.1`，包名 `dev.morrow.hmos`。正式生产资料库与完整模块缺口见 [PARITY](../docs/PARITY.md)。

## 已执行

| 检查 | 当前结果 |
|---|---|
| 参照定位 | 根目录 HEAD `457e023` 为旧基线；使用指定任务的 `build/io-safety-refactor`，HEAD `ddd9cc8` + 未提交源码 |
| 源码固定 | `shared/reference.json` 228 个文件的 SHA-256；末次差异核对无变化，未编辑参照工作树 |
| Rust 适配器 | 4 项 Windows 原生测试通过：持久化/CAS/同名 TaskId/历史重试，非法输入，确定拒绝与 Unknown 分离，8 秒撤销期限 |
| 原业务规则 | 直接快照内 `cards_v2` 5 项 + `tasks_v2` 7 项专项测试通过；保留未知字段、歧义、来源与稳定 TaskId 的原有规则 |
| OHOS 编译 | ARM64 与 x86_64 release 静态库构建成功，包含 bundled SQLite、现有 core 与第一方业务插件/SDK |
| HAP | ArkTS、NDK C++、Rust 静态链接成功；API 26，双 ABI；未配置发布签名 |
| 模拟器数据自检 | 在 Pura X View、API 26、x86_64 的 `/data/local/tmp` 新建隔离夹具，实际执行 OHOS ELF；六项检查 PASS |
| 实际应用 | HAP 安装成功，EntryAbility 启动；打开试验库、建卡、保存、强制停止本测试应用后重新启动读回通过 |
| 实际应用待办与收藏 | 新增 TaskId 待办，勾选/取消/再次勾选，收藏；最终修订 6，UI 与保存结果一致 |
| NDK 渲染 | 实际 NativeNode 文本显示卡片标题、类别/阶段和精确修订；不是仅 ArkTS 静态占位图 |

原规则测试与设备自检有语义覆盖重叠，不累计成“22 项全产品通过”。设备自检输出中的 `platform: linux` 来自 Rust OHOS target 的 `std::env::consts::OS`；执行文件由 `x86_64-unknown-linux-ohos` 构建并通过 hdc 在鸿蒙模拟器执行，不是桌面 Linux 测试。

## 实测发现与修复

- Windows SDK 路径含空格导致 SQLite C 编译参数拆分：启用 `CC_SHELL_ESCAPED_FLAGS`，采用正确 sysroot 引用后，两个目标均成功。
- 全局 hvigor daemon 注册锁阻塞：本工程用 `--no-daemon`，没有删除用户全局缓存、停止其他任务或修改系统策略。
- CLI 报镜像缺失，但本地镜像存在；直接使用官方 Emulator 明确 imageRoot 启动时，沙箱无法写实例虚拟磁盘。经工具审批使用原部署路径启动，无 reset/删除。CLI 初始失败不作为环境未部署结论。
- ArkUI ContentSlot 不支持直接 width：改用布局容器包裹，ArkTS 实际编译通过。
- 选择卡片的程序赋值回调误标脏：仅在值真正改变时标记编辑；复测可直接新增待办。
- ArkTS ForEach 缓存旧任务投影：节点键包含 TaskId 与可见值版本，复测同一任务可从完成→未完成→完成，修订递增 4→5→6。领域身份仍为原 TaskId。
- 恢复按钮对齐原规则：共享插件仅支持删除后 8 秒内撤销；UI 明确显示期限，超时拒绝不误装成永久恢复。

## 证据

- [构建源码与产物 SHA-256](build-manifest.json)
- [HAP 构建日志](hap-build.log)
- [适配器测试](rust-adapter-tests.log)、[共享规则测试](shared-rule-tests.log)
- [OHOS 设备运行结果](ohos-runtime-check.log)
- [应用重启读回](reopened-ui.txt)
- [待办取消后的真实界面](task-unchecked-ui.txt)、[重新完成后的真实界面](task-ui.txt)
- [最终工作台截图](final-workbench.png)、[最终待办截图](final-task.png)

测试生成的卡片与待办保留在独立模拟器应用内；隔离自检夹具位于模拟器 `/data/local/tmp/morrow-hmos-*`，没有使用用户原资料。模拟器以无窗口模式启动后保留运行，未擅自关闭。

## 未验证 / 未实现

未执行 ARM64 真机、发布签名、性能/长时稳定、前后台/权限撤回、生产 HUKS 密钥与审计、多实例所有权、正式资料迁移、附件/插件/网络/媒体/语言等全功能验收。当前试验库没有内容插件 Wasm 执行证据、生产审计封存和跨进程草稿/Unknown 恢复。单个 UI 或构建通过不代表这些门槛通过。
