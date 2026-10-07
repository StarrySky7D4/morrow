# 持续追平 Windows 工作台

目标：持续跟进 `01a085bd-7a94-7f93-8a1f-1ecf417f5ee3`，直至 HMOS 追平 Windows 侧 UI 与大部分实现。独立 `hmos/` 工程、Rust 复用、ArkUI/NDK 路线不变；只推送 `codex/ArkTsUI`，不合并主线。

2026-10-07 dev.17：接入授权 PasteButton 的系统剪贴板快照，按实际提供的 plain/HTML/RTF/Spreadsheet XML、原始二进制图片/文件和 PixelMap 准备原件，复用 durable import/pin；HTML 内嵌图片仅在确认 pin 后替换为持久资产引用。一次性来源绑定剪贴板 changeCount 和编辑目标，URI 不进入文件选择器授权表、不持久保存或跨读取复用；先完整检查正文、槽位和预算，再逐项确认，未知导入保留原请求。原件、转换、图片提取和冻结选区插入已有源码与模型覆盖；**真实系统富内容粘贴、Office 提供者及保存/重启/导出闭环 NOT_RUN**。最终版本 **0.1.0-hmos-dev.17 / 1000017**，完整实际 ETS 模型 **402/402 PASS**，API 26 未签名 debug HAP 构建 **PASS / 9.467 s**；包 **27,815,034 字节**，SHA-256 `D9DECC46BB0E953BB56A4CDC5DBBB71D0B863E6380C273CC677B73C36689C03F`。301 项构建输入 disk 核对 PASS，staged 核对以最终日志为准，见 [dev.17 验证记录](../reports/ui-source/v17/validation.md)。本轮按用户要求只交付分支源码和构建，不新增设备流程。

dev.17 容量仍不等价：原生转换上限为 2 MiB 原件字节，Flutter 来源上限按 2 Mi UTF-16 单元计；较大富格式原件可在附件预算内保留，但转换失败须明确提示。系统快照最多 20 条记录，展开图片/原件仍共用剩余 20 个 spool 槽和含 sidecar 的 64 MiB 预算，不能据此宣称 Flutter 200 MiB 附件容量等价。字段仍按 UTF-16 计数而非 Flutter grapheme，待办 500 与 Flutter 1000 的差距仍在；IME、全篇连续选择、生产捕获证据链及 HUKS 保护未补齐。

这是目标的验收清单，不把单轮构建或小功能交付当成最终完成。dev.16 历史交付 **dev.16 / 1000016** 修复实际 dev.15 中心单击无法显示媒体控件的问题：隐藏控件阻止整个子树命中，手势和控件节点按预览令牌重建。实际 ETS 模型 **303/303**、API 26 HAP 构建通过，最终未签名 HAP 为 **24,857,891 字节 / A97122…**。已在既有 x64 模拟器安装、读回版本并恢复同一 `HMOS-media-20261007-A` 草稿的 WAV/MP4 两个 pin；原生 WAV 两次为 **0:00 / 0:12、无自动播放**，中心单击后实际显示控件，播放0:01推进至0:03及暂停后两次0:06通过。首次全屏进入横屏但驱动裁剪断言停止，阶段 **FAILED_OR_UNKNOWN**，只读横屏状态核对通过；首次Back驱动在动作前停止，修正驱动后fresh Back恢复1320×2232竖屏/正常系统栏及同0:06预览、关闭回同编辑器两份原附件ID/名称通过，PNG已复核。按用户现在推送要求，本轮收束为媒体点击修复交付，不开展新的多选设备流程；seek/全屏手势、视频/系统文件预览没有本轮完成结论。见 [dev.16 验证记录](../reports/ui-source/v16/validation.md)。本轮四个双架构 so 与 dev.15 完整解压字节一致、native 源码无差异，298项构建输入从工作文件和暂存索引均核对PASS且匹配最终A971包，见[工作文件核对](../reports/ui-source/v16/build-manifest-disk.log)与[暂存核对](../reports/ui-source/v16/build-manifest-staged.log)；没有新 Rust 测试、ARM64 构建、真机或签名资格。完整对齐目标仍 **OPEN**。

dev.16 后续媒体观察单独记录：同一已安装 `A97122…` 包的 MP4 0:00/0:12 准备且无自动播放、播放时间推进、暂停稳定、横屏全屏、左右双击 ±10 秒/水平拖动、普通进度拖动和 Back 返回竖屏已有真实限定 PASS。初次 Back 驱动错误仍保留；Home 尝试后仍在应用内，后台生命周期为 **NOT_QUALIFIED**。这些结果不证明 dev.17 设备运行，也不证明真实音频输出、完整格式矩阵或系统文件预览，见 [后续限定验证](../reports/ui-source/v16/follow-up-validation.md)。

dev.15 历史源码交付：补齐单批最多 20 个且受剩余额度约束的多选，顺序处理全部 URI、独立导入身份和确认后空标题填充；迟到结果不覆盖新标题，失败/Unknown/停止或编辑器变化保留已确认项及原请求。清理等待后复查原 owner，pin 与标题同次保存，切换编辑器清空旧进度；媒体补控件显隐/计时/过渡、全屏水平 seek 与双击 ±10 秒。303 项模型/HAP 构建通过，包为 24,856,916 字节 / `7347C8…`，交付时设备 NOT_RUN，见 [原验证报告](../reports/ui-source/v15/validation.md)。后续在实际 dev.15 包上复现单击后控件仍不可见、关闭可响应，负面截图另存 [device-current](../reports/ui-source/v15/device-current)，不追改原报告。旧 dev.14 / `43E402…` 包上的 WAV 准备/播放暂停/seek/全屏返回/关闭基线不能替代 dev.16 尚未确认的阶段。dev.15 原生四库字节与 dev.14 相同、228 冻结哈希匹配、122 上游路径未同步，均为该轮范围的证据。

dev.14 历史交付：接入无自动播放、播放/暂停、进度/时长、后台暂停、原生全屏及恢复，以及普通文件系统预览与显式结束查看清理。该轮 226 项模型/HAP 构建通过，最终包安装与公开文字草稿恢复已验，但其选择观察停在未找到精确 WAV、零附件，未取得媒体或系统预览运行结果；当轮事实保留于 [dev.14 验证记录](../reports/ui-source/v14/validation.md)。后来新增的旧包 WAV 基线证据另记 dev.15，不追改历史报告，也不用于证明新包未验功能。普通文件系统预览仍不等于任意格式默认应用打开。

dev.13 历史交付：在 dev.12 URI 附件导入、原 durable staging、草稿 pin 和按修订导出基础上，接入 Markdown 内嵌图片、图片读取/解码状态、缩放/受限平移/复位及编辑器附件行导出。该轮 Rust 主机 72 项、实际 ArkTS 模型 163 项与 API 26 HAP 构建通过；最终包 x64 模拟器已验中文名称引用的图片、按钮 125% 放大/100% 复位和未发布 pin 的导出字节一致。测试卡保存后的表格图片、缺失/远程提示与重启无新增草稿通过；查询观察中断后核对现有卡，未重复保存。移除图片后保存前即更新为缺失提示；显式保存/重启保持同一卡身份、原始 Markdown 不变、零附件且无剩余 raw journal。原生双架构库复用 dev.12 字节，真实手势/损坏解码/GIF 未验收，完整范围见 [dev.13 验证记录](../reports/ui-source/v13/validation.md)。

保留 dev.11 阅读详情/卡片菜单/五页持久顺序、dev.10 Markdown/授权粘贴、dev.9 raw journal 和 dev.4–8 的待办、材质、边框修复、响应布局、懒加载瀑布流与 Rust 查询。dev.12 的 Windows 主机范围见 [附件主机验证](../reports/ui-source/v12/attachment-host-review.md)，该版 x64 原生附件/FD 与实际 UI 闭环另记 [dev.12 验证记录](../reports/ui-source/v12/validation.md)，旧包证据不能充作本轮回归。真机、生产保护和完整功能对齐仍开放。

2026-10-05 dev.12 只读版本跟进：原参照工作树 `build/io-safety-refactor` 为 `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` / `0.1.9-test.57+61`；较新 Windows 汇合工作树 `build/win-cloud-20261005` 为 `772466177fe589cee53bc633e69f411c34610104` / `0.1.9-test.58+62`，另有 `build/windows-sdk-reconstruction` 的 `20669f671152972470340a65eac3458dd2f61b4d` / 同应用版本。三者所查主界面、附件、富文本、草稿和暂存源码 Git blobs 相同，相关工作文件 clean；文件换行差异不代表源码功能变化。功能源线程当前仍在构建 Windows 测试组和示例插件宿主流程。具体排序语义和平台边界见 [dev.11 上游审计](../reports/ui-source/v11/upstream-audit.md)及 [原生交互审计](../reports/ui-source/v11/platform-audit.md)；源任务编译进展不等于 HMOS 或普通工作台已具备相同资格。

2026-10-07 dev.13 再次只读检查：Windows 汇合 HEAD 为 `e83cdf1d3506b001d07ffceaaa9f98b688bcc150`，另外两个参考 HEAD 与上述历史观察相同。三个工作树所查 17 个 UI/附件/Markdown/草稿/暂存源码 Git blobs 仍一致，相关工作文件没有未提交修改；不扩大为整个 SDK 或工作树资格。功能线程当时仍在修复 Windows pipe/ConPTY 错误传播，最终结果未取得。观察时间、源码位置与边界见 [dev.13 上游审计](../reports/ui-source/v13/upstream-audit.md)和 [平台审计](../reports/ui-source/v13/platform-audit.md)。

dev.14 三个参照 HEAD 与 dev.13 相同；四个主界面/附件 Dart blobs 一致且相应工作路径 clean，并核对实际安装的 `media_kit_video 2.0.1` 控制实现。本应用显式使用 MaterialVideoControls，单项媒体的上一项/下一项被隐藏。dev.15 再次检查实际依赖后明确：全屏水平 seek 与双击 ±10 秒为当前参照的启用设计；纵向音量/亮度依赖 `onVolumeChanged` / `onBrightnessChanged`，`attachment_view` 没有传入，因而当前参照默认禁用，不能称为已默认启用的手势目标。dev.14 的源线程快照记录 12 项 Windows 输出/等待修复测试通过；dev.15 最新快照仍在处理进程输出结算与会话接口，未提供新完整 SDK/VM 资格，不计入 HMOS 验收。见 [dev.15 上游审计](../reports/ui-source/v15/upstream-audit.md)与 [媒体控制源码审计](../reports/ui-source/v15/media-controls-source-audit.md)，dev.14 原报告保留当轮观察。

附件 UI 可直接对照 Windows 工作树 `lib/main.dart:5439` 的导入/空标题补齐、`:5498` 的正文/预览、`:5596` 的富剪贴板插入和 `:5857` 的附件行；`lib/attachments/attachment_view.dart:41` 提供类型图标、扩展名、大小、点击预览/默认打开、导出和移除，`:141` 提供音视频播放与图片缩放；`lib/content/idea_markdown.dart:61` 按名称/位置/资产 ID 解析 `attachment:` 图片。`lib/attachments/clipboard_import.dart`、`office_clipboard.dart`、`file_access_native.dart` 是后续平台设计来源。`main.dart` 未直接实例化 `EditorDraftBinding` / `EditorDraftSession` / `EditorDraftWorkspace`，其原专项测试资格不能继承到 HMOS raw journal。

dev.15 的 [后续对齐审计](../reports/ui-source/v15/next-parity-audit.md) 保留当时 **NEXT_PROPOSAL_ONLY** 的事实；dev.17 已实现本地授权快照、原件和正文转换、内嵌资源提取、确认 pin 后引用映射及冻结选区插入。真实系统提供者、完整 Office 和保存/重启/预览/导出/移除设备闭环仍 **NOT_RUN**；具体格式须按实际提供者分别验收，见 [dev.17 验证记录](../reports/ui-source/v17/validation.md)。

dev.8 已接入原 `query_plan_v2` 的分页过滤、稳定排序和归并，复用已冻结且与上述参照逐字节相同的 `query_v2` / codec / TaskId / 卡片业务块，替换正常视图的 ArkTS 内存筛选。查询调度新增来源单独记录；不全量覆盖 `shared/reference.json`。该轮 Rust 12 项、纯 ArkTS 模型 10 项、OHOS x64 原生 13 项检查通过；该版最终包 440 vp 的查询 UI、重启读取与单列四卡遍历通过。证据及未闭合资格见 [dev.8 验证记录](../reports/ui-source/v8/validation.md)。

| 范围 | 当前状态 | 达标证据 / 后续工作 |
|---|---|---|
| 首页、导航、分类卡片、收藏、编辑器、响应布局 | dev.8 新包已验 440 vp 单列、744 vp 两列的查询、四卡间距/遍历/重排；dev.7 保留 16 张交错高度设备回归，dev.6 保留 880/1488 vp 面板与位置恢复。完整响应矩阵仍未追平 | 真实 Flutter 源码与 HMOS 在手机/平板宽度、浅/深主题逐页对照；列表、弹窗、键盘、返回流程实际可用；大库内存/帧时与隐藏节点资源资格不能用可访问性节点数替代 |
| 七种风格、玻璃模式、深度、色盘、组件材质跟随 | dev.5 已接基础面板/深度/材质跟随；错位边缘已修复 | 渲染参照、七风格交叉玻璃模式、主题切换、控件与过渡、跟随/循环/取消/保存/重启测试；不能只显示风格名称 |
| 字体、语言、背景、窗口行为 | 系统字体/部分九语/内置纹理已接；文件导入与完整文案缺失 | 字体和背景选择器、持久 URI/授权、动态文案、可访问性与宽屏实测 |
| 卡片、TaskId、分类/阶段、回收站 | dev.11 接阅读详情、显式编辑、复制、长按/菜单及持久前后移动和独立拖动柄；dev.4 完整 TaskId 编辑保留 | 补任务迁移、详情完整任务操作、拖动边缘自动滚动和鼠标/键盘交互矩阵；保持未知字段、CAS、原操作回执、删除时间规则 |
| 查询、排序、大库加载 | dev.8 已接原 Rust 计划、单次完整快照、完整属性搜索与稳定排序；去抖/待发合并/过期回包丢弃，真实失败与空结果分开 | 仍限定 256 张；需 UI 响应分页、大库内存/帧时和生产 guest/捕获资格。旧“最近添加”为反向 ID 顺序，不能宣称为创建时间排序 |
| 正式 Rust 宿主和存储会话 | 未接；当前为独立未封存开发库 | 提取平台会话接口，HUKS、稳定身份、单库所有者、审计/备份/恢复契约；禁止绕过原宿主非 Windows 拒绝规则 |
| 持久草稿、S1/S2、未知结果核对 | dev.12 在原 schema/model、固定来源/raw journal 上接选中资产、pin、计费、consumed import 清理和持久 import 核对；SAME 已提交业务操作可跨草稿推进/弃稿精确核对 | 草稿确认与业务提交分开；仅来源 0/2/3，仍拒绝 1/4、前驱捕获/父子交接。继续接业务提案跨进程原请求恢复、captured S1/S2 续写重基；主机通过不提供生产保护资格 |
| 附件、剪贴板、Markdown/富文本、导入导出 | dev.17 授权系统快照、原件、HTML/RTF/Spreadsheet XML/TSV 转换、PNG/原图及 HTML 内嵌图提取已接；复用 durable import/pin，完整正文/槽位 preflight 后逐项确认，仅确认资产映射正文，冻结选区和 owner 后插入。原件与图片共用64 MiB/20槽和sidecar预算，Unknown原请求保留。保留dev.15多选/空标题与dev.16控件命中修复 | 新ClipboardInput 36项、ClipboardFiles 46项和既有AttachmentFiles 97项模型通过；Rust91项、五个实际Flutter完整输出对照和双架构静态库通过，完整实际ETS模型402/402、API26 HAP 9.467s通过。真实系统富内容及Office提供者/保存重启设备闭环NOT_RUN。dev.16 MP4后续prepared/play/pause/landscape/seek/Back限定PASS单列，Home仍在App使background NOT_QUALIFIED。多选/标题、PDF、真实声音、完整格式和图片手势/损坏/GIF仍待验；纵向音量/亮度当前Flutter未启用 |
| 音乐、歌词、解密 | 空状态 | 复用 Rust 解密模块，播放器、播放列表、歌词、文件权限、后台/中断恢复实测 |
| 插件包、动态 UI、HTTP/服务/文件任务 | 未接运行期宿主 | 跟进 Windows 实现与合约，接 Wasm 执行/权限/资源预算/服务与任务控制；区分 Windows 平台实现和共享业务块 |
| 分发与资格 | dev.17 / 1000017 API26未签名debug HAP 9.467s、完整实际ETS模型402/402 PASS；27,815,034字节 / D9DECC46…，最终输入核对见dev.17报告；本轮不新增安装/设备流程。Rust双架构重新构建，不是复用dev.16四库。dev.16已发布A97122…包的限定WAV/MP4观察另记 | 完整系统富剪贴板、进程中断、前后台、权限、损坏、空间耗尽和并发，以及签名/ARM64真机与生产保护分别验收；源代码、模型和构建通过不关闭Windows/Flutter完整对齐目标 |

每一轮更新源码观察与实际功能证据，保护 `shared/reference.json` 的冻结来源。上游在途代码不得未经审查直接覆盖。新版本截图、测试日志和构建输入哈希分别记录，旧图不能被当成新包验收。

远端推送失败时保留本地提交；必须读取远端分支确认是否收到，再决定是否重试。其他任务对 `main` 的更新不属于本任务操作，不重置、不覆盖。
