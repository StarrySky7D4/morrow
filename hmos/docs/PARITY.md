# 功能对齐与 Rust 复用检查

基线：用户指定主任务的 `build/io-safety-refactor` 实际工作树，test.54 + 未提交增量，2026-09-23。本表是迁移状态，不是主任务整体完成声明；其上游报告尚未关闭的资格项，在 HMOS 同样不能填写完成。

| 功能块 | 现有来源 | 本次 HMOS 状态 / 验证边界 |
|---|---|---|
| Protobuf/LZ4、完整记录/未知字段、SQLite 事务、CAS、原操作查询 | `core` | 原样快照并编译 ARM64/x64；设备侧自检覆盖建卡、重启、幂等和 CAS；不是全核心平台回归 |
| 卡片 V2、类别/阶段、收藏、删除与限时撤销 | `plugins/workbench/cards_v2.rs` | 直接复用；UI 接入；共享 5 项专项测试在 Windows 通过，设备 UI 建卡保存通过 |
| TaskId 待办、同名独立、历史歧义 | `plugins/workbench/tasks_v2.rs` | 新增/勾选/移除接入，设备侧同名独立验证；共享 7 项专项测试通过；正式 V1→V2 迁移入口未接入 |
| 搜索和查询 | `query_v2` / `workbench_host/query_*` | 当前只有已加载 256 张卡片内的标题/正文筛选；上游查询计划、捕获证据、排序未接入 |
| ArkUI 原生节点 | 新 `entry/src/main/cpp/bridge.cpp` | 实际 NativeNode Column/Text 外观预览，NodeContent 挂载和销毁；编辑控件/导航为 ArkTS，非全 NDK UI |
| ArkTS ↔ Rust | 新 N-API async work + Rust C ABI | 异步执行、串行准入、长度/UTF-8 校验、配对释放；不是 IPC 隔离或插件授权通道 |
| 正式工作台宿主 | `workbench_host` | 已检查 API 与平台边界；未整体复制/接入，非 Windows 开库仍拒绝；新适配器不能冒充其内容/任务证据链 |
| 密钥、身份固定、审计封存、库管理、备份/恢复 | `audit` + `workbench_host/storage.rs` | Windows DPAPI/租约后端不可照搬；HMOS HUKS 保护、库所有权、备份资格待实现。核心验签可复用不等于密钥管理已适配 |
| 宿主持久草稿、S1/S2、附件导入暂存与恢复 | `editor_draft*`, `editor_recovery*` | 参照 9/23 最新报告记录，尚未移植。当前仅进程内编辑值与原请求保留 |
| 附件原件、文件选择器、媒体预览/导出 | `core/attachment`, Flutter file_selector/media_kit | 核心已编译但无 UI/平台通路；需 SAF/URI 授权、流式校验、一致性提交和 AVPlayer/图像适配 |
| Wasm 插件解释器与包管理 | `plugin_runtime` + `sdk/rust` | SDK 随业务模块复用编译；运行期宿主权限、worker、动态包审批/UI 渲染未接入。未声称 Rust 原生直调等于 Wasm 隔离运行 |
| HTTP/服务/TLS/凭据 | `network_node`, `workbench_host/*control`, `io_tasks` | 审查依赖与平台边界；本次未连接外部服务，未编译/运行完整网络节点；后台任务、权限、HUKS/TLS 适配待实现 |
| 捕获/转换、富文本、表格/RTF | `plugins/workbench/capture.rs` | 源码复用编译；剪贴板来源、捕获证据和 ArkUI 编辑通路未接入 |
| 歌词/媒体与格式解密 | Flutter lyrics/media + `third_party/um_decrypt` | 未移植；不能把共享 Rust 核心当作这些功能已经具备 |
| 语言、字体、主题、玻璃效果、稳定瀑布流 | Flutter `morrow_i18n`, fonts/layout/shaders | dev.3 已补齐专用分类卡片、外观/独立材质/色盘/系统字体设置、基础正文预览、日常清单和音乐空状态；复用九语 ARB、外观持久保存。模拟器验证详见 UI_DESIGN_DEV3.md；折射 shader、完整九语动态文案、字体/背景文件导入、媒体与宽屏设备验收仍待完成 |
| 平台分发 | DevEco API 26 | 双架构未签名 HAP 已构建；x64 模拟器安装/启动；ARM64 真机、签名、发布均未验收 |

## 后续顺序

1. 从正式宿主提取平台存储会话接口；实现 HUKS 受保护密钥、稳定日志身份和单库所有者，再通过与 Windows 相同的 Store/封存/恢复契约验证。禁止用当前试验库直接替换正式库。
2. 复用原私有二进制协议和宿主业务入口，覆盖版本化内容、草稿 S1/S2、附件暂存与回复未知状态；逐步替换开发适配器，保持上游证据链。
3. 接入文件/媒体/剪贴板、插件包和动态 UI；按对象授权与分段协议保留边界。
4. 对齐设置、语言/字体、工作区布局、网络/TLS/后台生命周期。按功能记录 ArkTS、Rust、NDK 和真实设备证据，不能只看编译成功。
5. 签名后的 ARM64 真机测试、进程终止/重启/前后台/权限撤销/空间耗尽/并发开库/损坏拒绝矩阵通过后，再讨论与 Flutter 的功能等价验收。

本次 NativeNode/N-API 路线依据 SDK 实际头文件，并核对 [华为 ArkUI NativeModule](https://developer.huawei.com/consumer/cn/doc/doccenter-references/api/capi-arkui-nativemodule) 与 [Node-API](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides-V5/napi-introduction-V5)。Rust 平台支持参见 [官方平台支持表](https://doc.rust-lang.org/rustc/platform-support.html)。实际资格以本目录报告为准。

