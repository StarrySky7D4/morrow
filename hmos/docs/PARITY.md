# 功能对齐与 Rust 复用检查

基线：用户指定主任务的 `build/io-safety-refactor` 实际工作树，test.54 + 未提交增量，2026-09-23。本表是迁移状态，不是主任务整体完成声明；其上游报告尚未关闭的资格项，在 HMOS 同样不能填写完成。

| 功能块 | 现有来源 | 本次 HMOS 状态 / 验证边界 |
|---|---|---|
| Protobuf/LZ4、完整记录/未知字段、SQLite 事务、CAS、原操作查询 | `core` | 原样快照并编译 ARM64/x64；设备侧自检覆盖建卡、重启、幂等和 CAS；不是全核心平台回归 |
| 卡片 V2、类别/阶段、收藏、删除与限时撤销 | `plugins/workbench/cards_v2.rs` | 直接复用；UI 接入；共享 5 项专项测试在 Windows 通过，设备 UI 建卡保存通过 |
| TaskId 待办、同名独立、历史歧义 | `plugins/workbench/tasks_v2.rs` | dev.4 补重命名、上下移动、批量勾选与当前阶段原子提交、移除确认；主机适配器 5 项、OHOS runner 10 项检查通过，并验证 UI 重启读回。共享模块与 9/27 参照哈希一致。正式 V1→V2 迁移、任务拖动重排和任务文字复制菜单未接入；dev.11 卡片拖动与复制另列 |
| 搜索和查询 | `query_v2` / `workbench_host/query_plan_v2.rs` | dev.8 接入单次完整 WAL 快照、原分页筛选/排序归并及 128 项 / 64 KiB 真实帧预算；标题/正文/假设/结论/附件名语义、UTF-16 和稳定收藏排序复用原实现。ArkTS 去抖、串行/coalesce、旧回包丢弃和显式失败；开发库仍 256 张，无 UI 响应分页、大库资格和生产 guest/捕获证据。回收站为独立既有视图 |
| 阅读详情、卡片菜单、每页手动排序 | Flutter `openIdea` / `component_menus` / `card_order_preferences` / `hold_reorder` | dev.11 接原生 Markdown 阅读、显式进入草稿编辑、分类/阶段/TaskId勾选、收藏/复制/确认删除；卡片正文长按与菜单提供前后移动、转项目和组件设置。五页独立手动开关/顺序，筛选隐藏槽保持、保存失败回滚、过期视图与外部拖动拒绝。独立拖动柄采用鸿蒙原生手势；完整边缘自动滚动、鼠标/键盘菜单和详情任务编辑矩阵待补；不会写业务卡来存排序 |
| ArkUI 原生节点 | 新 `entry/src/main/cpp/bridge.cpp` | 实际 NativeNode Column/Text 外观预览，NodeContent 挂载和销毁；编辑控件/导航为 ArkTS，非全 NDK UI |
| ArkTS ↔ Rust | 新 N-API async work + Rust C ABI | 异步执行、串行准入、长度/UTF-8 校验、配对释放；不是 IPC 隔离或插件授权通道 |
| 正式工作台宿主 | `workbench_host` | 已检查 API 与平台边界；未整体复制/接入，非 Windows 开库仍拒绝；新适配器不能冒充其内容/任务证据链 |
| 密钥、身份固定、审计封存、库管理、备份/恢复 | `audit` + `workbench_host/storage.rs` | Windows DPAPI/租约后端不可照搬；HMOS HUKS 保护、库所有权、备份资格待实现。核心验签可复用不等于密钥管理已适配 |
| 宿主持久草稿、S1/S2、附件导入暂存与恢复 | `editor_draft*`, `editor_recovery*` | dev.9 复用原 schema/model 和文字 journal 的 CAS、固定完整来源、精确历史回执/弃稿；草稿与业务卡分离，16 活动槽/256 累计身份。仍是未封存开发 Store；附件/前驱/父子交接明确拒绝，正式业务原请求跨进程恢复与 captured S1/S2 未接。详见 dev.9 报告 |
| 附件原件、文件选择器、媒体预览/导出 | `core/attachment`, Flutter file_selector/media_kit | 核心已编译但无 UI/平台通路；需 SAF/URI 授权、流式校验、一致性提交和 AVPlayer/图像适配 |
| Wasm 插件解释器与包管理 | `plugin_runtime` + `sdk/rust` | SDK 随业务模块复用编译；运行期宿主权限、worker、动态包审批/UI 渲染未接入。未声称 Rust 原生直调等于 Wasm 隔离运行 |
| HTTP/服务/TLS/凭据 | `network_node`, `workbench_host/*control`, `io_tasks` | 审查依赖与平台边界；本次未连接外部服务，未编译/运行完整网络节点；后台任务、权限、HUKS/TLS 适配待实现 |
| 捕获/转换、富文本、表格/RTF | `plugins/workbench/capture.rs` | dev.10 使用系统授权 PasteButton 读取纯文字，正文复用 capture::plain 转 TSV 表格，按完整字段/选区插入；先检查全部行列避免原预算裁剪。完整 HTML/Office/RTF、文件及捕获证据仍未接入 |
| 歌词/媒体与格式解密 | Flutter lyrics/media + `third_party/um_decrypt` | 未移植；不能把共享 Rust 核心当作这些功能已经具备 |
| 语言、字体、主题、玻璃效果、稳定瀑布流 | Flutter `morrow_i18n`, fonts/layout/shaders | dev.3 已补齐专用分类卡片、外观/独立材质/色盘/系统字体设置、基础正文预览、日常清单和音乐空状态；复用九语 ARB、外观持久保存。模拟器验证详见 UI_DESIGN_DEV3.md；折射 shader、完整九语动态文案、字体/背景文件导入、媒体与宽屏设备验收仍待完成 |
| 七种风格、立体深度、组件材质跟随 | Flutter `appearance.dart`, `component_material_page.dart` | dev.5 基础面板圆角/边缘/阴影和完整材质引用；循环拒绝、取消/应用和重启验证通过。公共描边使用面板实测尺寸并限制绘制范围，修复跨卡片框线。控件浮起/按压动画及 shader 尚未复现；详见 dev.5 验证记录 |
| 工作区布局与位置 | Flutter `workspace_viewport.dart` / `stable_masonry_grid.dart` / `render_stable_masonry_grid.dart` | dev.7 改为原生 LazyVWaterFlowLayout + LazyForEach；页面/卡片身份稳定，修订内容和移动位置分别失效，保留 16 张交错记录验证。dev.8 最终包新增 440/744 vp 单/双列、四卡间距/排序/遍历与查询修订刷新；dev.6 保留 880/1488 vp 面板与五页位置恢复证据。完整主题、键盘/动画及内存/帧时资格仍未完成 |
| 编辑器文字与预览 | Flutter `main.dart::_bodyEditor` / `idea_markdown.dart` | dev.10 使用 Rust CommonMark 投影，接通原生强调、删除线、嵌套列表、引用、代码块、横向表格、安全链接和远程图片显式读取。HTML为文字；按块选择复制，跨块连续选择未接。保留 dev.9 的 590 vp 分栏与 journal；Harmony 字符计数/IME 会话与 Flutter 不完全相同，附件/完整富文本仍未接 |
| 平台分发 | DevEco API 26 | 双架构未签名 HAP 已构建；x64 模拟器安装/启动；ARM64 真机、签名、发布均未验收 |

## 后续顺序

2026-09-27 跟进范围见 [dev.4 验证记录](../reports/ui-source/v4/validation.md)。原始共享快照保持固定；上游最新文件创建/删除、服务 SDK 尚未整体移植；dev.5 另行移植基础面板风格与材质跟随（[验证记录](../reports/ui-source/v5/validation.md)）。上游 Windows Core/runtime 的测试数字不是 HMOS 验收证据。

1. 从正式宿主提取平台存储会话接口；实现 HUKS 受保护密钥、稳定日志身份和单库所有者，再通过与 Windows 相同的 Store/封存/恢复契约验证。禁止用当前试验库直接替换正式库。
2. 复用原私有二进制协议和宿主业务入口，覆盖版本化内容、草稿 S1/S2、附件暂存与回复未知状态；逐步替换开发适配器，保持上游证据链。
3. 接入文件/媒体/剪贴板、插件包和动态 UI；按对象授权与分段协议保留边界。
4. 对齐设置、语言/字体、工作区布局、网络/TLS/后台生命周期。按功能记录 ArkTS、Rust、NDK 和真实设备证据，不能只看编译成功。
5. 签名后的 ARM64 真机测试、进程终止/重启/前后台/权限撤销/空间耗尽/并发开库/损坏拒绝矩阵通过后，再讨论与 Flutter 的功能等价验收。

本次 NativeNode/N-API 路线依据 SDK 实际头文件，并核对 [华为 ArkUI NativeModule](https://developer.huawei.com/consumer/cn/doc/doccenter-references/api/capi-arkui-nativemodule) 与 [Node-API](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides-V5/napi-introduction-V5)。Rust 平台支持参见 [官方平台支持表](https://doc.rust-lang.org/rustc/platform-support.html)。实际资格以本目录报告为准。

