# 功能对齐与 Rust 复用检查

基线：用户指定主任务的 `build/io-safety-refactor` 实际工作树，test.54 + 未提交增量，2026-09-23。本表是迁移状态，不是主任务整体完成声明；其上游报告尚未关闭的资格项，在 HMOS 同样不能填写完成。

2026-10-07 当前源码 `0.1.0-hmos-dev.16` / `1000016`：修复实际 dev.15 中心单击无法显示媒体控件的问题，隐藏控件阻止整个子树命中，手势/控件节点按原预览令牌重建。实际 ETS 模型 **303/303** 与 API 26 HAP 构建通过；最终未签名包 SHA-256 `A971227AC2D39730C2228972B513DBAAB4049E1C9E9AAB49397517BFB0208C7C`，24,857,891 字节。新包安装和版本读回通过，恢复同一公开媒体草稿的 WAV/MP4 两个 pin；WAV 原生准备两次为 0:00/0:12、无自动播放，中心单击后控件实际可见的 PNG 已复核，播放从0:01推进至0:03、暂停后两次0:06通过。首次全屏实际进入横屏，驱动裁剪断言停止使阶段为 **FAILED_OR_UNKNOWN**，只读横屏状态核对通过；首次Back驱动在动作前停止，修正驱动后fresh Back恢复竖屏/正常系统栏及同0:06预览、关闭回同编辑器两份原附件ID/名称通过，PNG已复核。本次按用户要求收束为媒体点击修复分支交付；seek/全屏手势、视频/系统预览和多选/空标题新设备闭环未跑，不据旧包结果填写通过。见 [dev.16 验证记录](../reports/ui-source/v16/validation.md)，完整目标仍 **OPEN**。

dev.15 历史源码交付 `0.1.0-hmos-dev.15` / `1000015`：文件选择器按剩余资产/完整暂存记录额度多选、单批最多 20 个，全部 URI 按顺序使用独立操作身份导入；原有 spool/request journal/draft pin 状态机继续负责每项确认和 Unknown。确认成功后仅填仍为空白的标题，与 pin 同次保存；清理等待后再次核对原编辑器，加入附件绑定原 card/draft，编辑器更替清空旧进度。媒体增加控件显隐/3 秒隐藏/交互保持/300 ms 过渡、全屏水平拖动和左右双击 ±10 秒及累计反馈，旧令牌和计时器不能操作新媒体。实际 ETS 模型 **303/303** 与 API 26 HAP 构建通过；最终未签名包 SHA-256 `7347C8A8E2136D33A888BDD606766D788A6729633D7449A4BBA16F9CF8624A4B`，24,856,916 字节。交付时 **dev.15 设备验收 NOT_RUN**。该轮在旧 dev.14 / `43E402…` 包上完成 WAV 准备/无自动播放/播放暂停/进度/全屏返回/关闭基线，视频仅导入成功、播放未跑，PDF未跑；这些不证明 dev.15 多选或新控件实际运行。后续实际 dev.15 的单击缺陷另存 [device-current](../reports/ui-source/v15/device-current)，不追改原 [dev.15 验证记录](../reports/ui-source/v15/validation.md)。

2026-10-07 dev.14 源码接入附件音视频 AVPlayer/XComponent、无自动播放、播放/暂停、进度与时长、后台暂停及原生全屏窗口恢复。普通文件接入 PreviewKit 系统预览，准确绑定原业务修订或草稿 pin，窗口关闭后仍保留文件，由用户显式结束查看再清理；未知系统派发结果不重放。全屏恢复和播放器释放失败分别保留原状态供显式重试。当前版本 `0.1.0-hmos-dev.14` / `1000014`；实际 ArkTS 模型 **226 项**和 API 26 HAP 构建通过，最终包已在既有 x64 模拟器安装并恢复同一公开文字草稿。音频选择在 Download 浏览后返回应用、未找到精确测试文件，未选中附件，驱动断言终止；复查同一草稿原文/零附件仍保留，业务未发布。不据此推断确定的选择器缺陷；原生解码、seek/全屏、音频输出及 PreviewKit 可读内容仍 **NOT_RUN**。Rust/原生实现本轮未变，旧 72 项 Rust 结果不计为本轮新跑。完整控制手势及任意文件默认应用打开仍未取得等价资格，见 [dev.14 验证记录](../reports/ui-source/v14/validation.md)、[媒体平台审计](../reports/ui-source/v14/platform-media-audit.md)及 [文件打开审计](../reports/ui-source/v14/platform-open-audit.md)。

2026-10-07 dev.13 补齐按当前附件解析的 Markdown 图片、读取/解码状态、图片缩放/平移/复位及编辑器附件行导出。Rust 主机 72 项、实际 ArkTS 模型 163 项和 API 26 HAP 构建通过；既有 x64 Pura X View2 实际显示中文名称引用的图片，按钮 125%/100% 复位与编辑器 pin 导出逐字节一致通过。测试卡保存后的表格图片、缺失/远程提示和重启无新增草稿通过；中断查询按已保存卡身份核对，没有重复保存。移除图片后保存前即显示缺失提示，显式保存/重启保持同一卡身份、原始 Markdown 不变、零附件且无剩余 raw journal。损坏图片实际解码、双指/平移手势和 GIF 动画仍待验收；双架构原生库复用 dev.12 字节，不是本轮新 ARM64 构建证据。完整范围见 [dev.13 验证记录](../reports/ui-source/v13/validation.md)。持续追平目标仍开放。

2026-10-05 dev.12 已实现文件选择器附件、原 durable import/schema、私有 journal/pin、按修订导出及图片/GIF 预览。最终Rust主机72项、实际ArkTS模型119项通过；另外完成x64原生附件/FD与实际选择器、恢复/预览、保存/导出、取消/单引用移除验证。各项按主机、原生runner及实际UI分开记录，不扩大为生产或真机资格。见 [dev.12 验证记录](../reports/ui-source/v12/validation.md)与 [附件主机验证](../reports/ui-source/v12/attachment-host-review.md)。

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
| 宿主持久草稿、S1/S2、附件导入暂存与恢复 | 原 `editor_draft.proto` / `editor_draft*` / `editor_recovery*` | dev.9 文字 journal 在 dev.12 扩展选中附件、完整元数据、pin 计费及 consumed import 清理；固定来源/CAS、原请求与历史回执仍保留。原 durable import 的 Pending/Ready/Retired/Pruned 与 spool 原请求有显式恢复/核对入口。16 活动槽/256 累计身份、含 pin 的 64 MiB 活动预算；允许来源 0/2/3，拒绝 1/4、前驱捕获/父子交接。仍为未封存开发 Store，captured S1/S2 与正式业务原请求跨进程 Unknown 恢复未接 |
| 附件原件、文件选择器、媒体预览/导出 | `core/attachment`、原 `editor_draft_staging.rs`，Flutter `attachments/*` | 保留 dev.12 exact URI/native FD/SHA256、durable import/pin/一致性保存/按修订导出，dev.13 图片与内嵌图/行导出。dev.15 多选按顺序逐项确认，最多 20 个且不超已有资产和完整暂存记录的剩余位置；空标题仅在成功导入后补确认文件名，取消/失败/Unknown/owner或世代变化停止后续且保留原请求。媒体保留 dev.14 无自动播放/后台暂停/释放顺序/原窗口恢复，dev.15 补控件显隐、全屏水平 seek 与左右双击 ±10 秒；dev.16 修复隐藏子树命中和令牌更替后的手势/控件节点身份。普通文件仍走 PreviewKit 与显式结束查看清理，未知派发不重放。准备/pin 64 MiB 未放宽；一个媒体对话框和一个系统预览各 200 MiB，内嵌图 8 个/64 MiB。dev.16 303 项模型/HAP通过，安装/同草稿两pin恢复、WAV 0:00/0:12无自动播放与单击显示已有实际证据；播放时间推进和暂停后0:06稳定通过；首次全屏阶段FAILED_OR_UNKNOWN，横屏当前状态只读核对、fresh Back恢复竖屏和正常系统栏、关闭回原两附件编辑器通过，初次Back驱动为preaction FAILED。多选/空标题、全屏手势、视频播放/PDF/真实音频输出未取得本轮资格。旧 dev.14 WAV 基线单列。完整格式、任意默认打开、图片双指/损坏/GIF、拒权/空间耗尽仍待验收或补齐。纵向音量/亮度需当前 Flutter 参照未传的回调，不是已默认启用的目标 |
| Wasm 插件解释器与包管理 | `plugin_runtime` + `sdk/rust` | SDK 随业务模块复用编译；运行期宿主权限、worker、动态包审批/UI 渲染未接入。未声称 Rust 原生直调等于 Wasm 隔离运行 |
| HTTP/服务/TLS/凭据 | `network_node`, `workbench_host/*control`, `io_tasks` | 审查依赖与平台边界；本次未连接外部服务，未编译/运行完整网络节点；后台任务、权限、HUKS/TLS 适配待实现 |
| 捕获/转换、富文本、表格/RTF | `plugins/workbench/capture.rs` | dev.10 使用系统授权 PasteButton 读取纯文字，正文复用 capture::plain 转 TSV 表格，按完整字段/选区插入；先检查全部行列避免原预算裁剪。完整 HTML/Office/RTF、剪贴板图片/文件及捕获证据仍未接入；dev.12 文件选择器导入独立于此路径 |
| 歌词/媒体与格式解密 | Flutter lyrics/media + `third_party/um_decrypt` | 未移植；不能把共享 Rust 核心当作这些功能已经具备 |
| 语言、字体、主题、玻璃效果、稳定瀑布流 | Flutter `morrow_i18n`, fonts/layout/shaders | dev.3 已补齐专用分类卡片、外观/独立材质/色盘/系统字体设置、基础正文预览、日常清单和音乐空状态；复用九语 ARB、外观持久保存。模拟器验证详见 UI_DESIGN_DEV3.md；折射 shader、完整九语动态文案、字体/背景文件导入、媒体与宽屏设备验收仍待完成 |
| 七种风格、立体深度、组件材质跟随 | Flutter `appearance.dart`, `component_material_page.dart` | dev.5 基础面板圆角/边缘/阴影和完整材质引用；循环拒绝、取消/应用和重启验证通过。公共描边使用面板实测尺寸并限制绘制范围，修复跨卡片框线。控件浮起/按压动画及 shader 尚未复现；详见 dev.5 验证记录 |
| 工作区布局与位置 | Flutter `workspace_viewport.dart` / `stable_masonry_grid.dart` / `render_stable_masonry_grid.dart` | dev.7 改为原生 LazyVWaterFlowLayout + LazyForEach；页面/卡片身份稳定，修订内容和移动位置分别失效，保留 16 张交错记录验证。dev.8 最终包新增 440/744 vp 单/双列、四卡间距/排序/遍历与查询修订刷新；dev.6 保留 880/1488 vp 面板与五页位置恢复证据。完整主题、键盘/动画及内存/帧时资格仍未完成 |
| 编辑器文字与预览 | Flutter `main.dart::_bodyEditor` / `idea_markdown.dart` | dev.10 使用 Rust CommonMark 投影，接通原生强调、删除线、嵌套列表、引用、代码块、横向表格、安全链接和远程图片显式读取。dev.13 按 Flutter URI 规则将 `attachment:` 解析为当前选中图片/GIF 的名称或资产 ID（非数字下标）；纯模型支持显式已知位置别名，当前 HMOS DTO 不提供桌面原始路径。段落/表格同资产共用准确来源/修订或草稿确认世代的核验读取，缺失/失败有提示，移除、替换及迟到回复受图片生命周期模型约束。实际保存后的表格图片和重启只读验证通过。活动内嵌图限 8 个/64 MiB；已登记预览的清理失败仍计费，未登记读取失败目录只尽力清理、不在重试账本内，账本不扫描进程死亡后的全部物理缓存。HTML 为文字；按块选择复制，跨块连续选择未接。保留 dev.9 的 590 vp 分栏与 journal；dev.15 已接确认导入后的空标题补齐及 owner保护，dev.16 多选/标题设备闭环仍待跑。Harmony 字符计数/IME 会话和完整富文本未对齐 |
| 平台分发 | DevEco API 26 | dev.16 / 1000016 未签名 HAP 构建通过，24,857,891 字节 / A97122…；303实际ETS模型通过，既有x64模拟器安装/版本读回、同一媒体草稿恢复和WAV准备/单击显示/播放暂停/Back恢复/关闭已有实际限定证据。四个双架构so与dev.15完整解压字节一致、native源码无差异；298项构建输入工作文件/暂存索引均PASS，匹配最终A971包，见[工作文件核对](../reports/ui-source/v16/build-manifest-disk.log)与[暂存核对](../reports/ui-source/v16/build-manifest-staged.log)；无新Rust测试/ARM64构建、真机或签名资格。旧dev.14/dev.15包观察分别保存，不替代dev.16未验阶段 |

## 后续顺序

2026-09-27 跟进范围见 [dev.4 验证记录](../reports/ui-source/v4/validation.md)。原始共享快照保持固定；上游最新文件创建/删除、服务 SDK 尚未整体移植；dev.5 另行移植基础面板风格与材质跟随（[验证记录](../reports/ui-source/v5/validation.md)）。上游 Windows Core/runtime 的测试数字不是 HMOS 验收证据。

1. 从正式宿主提取平台存储会话接口；实现 HUKS 受保护密钥、稳定日志身份和单库所有者，再通过与 Windows 相同的 Store/封存/恢复契约验证。禁止用当前试验库直接替换正式库。
2. 在已接 raw journal、durable import 和 pin 的基础上，继续复用原私有二进制协议和宿主业务入口，覆盖 captured S1/S2、父子交接及正式业务跨进程 Unknown；逐步替换开发适配器，保持上游证据链。
3. 完成 dev.16 最终包的实际多选/空标题、seek与全屏水平/双击手势、视频解码/真实音频输出及文件预览生命周期资格；当前安装/恢复/准备/单击显示/播放暂停和旧 dev.14 WAV 基线都不替代尚未确认的阶段。补完整格式、任意默认打开和完整 HTML/Office/剪贴板，补图片真实手势/损坏/GIF 资格，接插件包和动态 UI；按对象授权与分段协议保留边界。结构化富剪贴板闭环的 [后续审计](../reports/ui-source/v15/next-parity-audit.md) 为 **NEXT_PROPOSAL_ONLY**，未开始实现或设备验收。
4. 对齐设置、语言/字体、工作区布局、网络/TLS/后台生命周期。按功能记录 ArkTS、Rust、NDK 和真实设备证据，不能只看编译成功。
5. 签名后的 ARM64 真机测试、进程终止/重启/前后台/权限撤销/空间耗尽/并发开库/损坏拒绝矩阵通过后，再讨论与 Flutter 的功能等价验收。

本次 NativeNode/N-API 路线依据 SDK 实际头文件，并核对 [华为 ArkUI NativeModule](https://developer.huawei.com/consumer/cn/doc/doccenter-references/api/capi-arkui-nativemodule) 与 [Node-API](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides-V5/napi-introduction-V5)。Rust 平台支持参见 [官方平台支持表](https://doc.rust-lang.org/rustc/platform-support.html)。实际资格以本目录报告为准。

