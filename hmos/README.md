# Morrow HMOS

2026-10-09 **v25 当前分支检查点**：主页面保存已接入持久原请求的 prepare/issue/save/inspect 和只读发现/恢复入口；已确认业务后，准确 S1 关闭原草稿，完整较新 S2/S3 通过 typed business source0 子草稿接续，再条件退役父草稿。Unknown 保留固定原 wire，首次 prepare 明确未写入时只释放该提案，完整输入保留。重启后已计划步骤只读恢复 native 固定 literal，需显式核对，不生成另一套操作。

本轮完整实际 ETS/tool **857/857 PASS**，Rust **177 library +3 binary PASS**（13 条默认条件 ignored，文档检查单独顺序通过），新 **48 个实际 Store 崩溃边界 PASS**；fresh 双 ABI 和 API26 完整产品构建 **SUCCESS /15.363s**，未签名包 **30,173,833B /9590D87D…**。详细结果见 [v25 验证](reports/ui-source/v25/validation.md)。**新 native / 新页面流程设备验收 NOT_RUN**；父草稿已缺失或子草稿已推进后的页面重启接续、关闭后卡片重开并继续全文 owned todos、元数据/TaskId 编辑后的完整对齐仍 OPEN。未将局部检查计作完整 Flutter/Windows 功能等价。版本保持 **dev19/1000019**，只推送 `codex/ArkTsUI`，不并入主线。以下记录保留各轮历史范围。

2026-10-07 **v24 历史分支检查点**：普通字段恢复选区先核焦点与 lease owner；待办初建控件用一次性初始化能力确认原始显示回声，避免其被误报为未完整捕获。冻结 Index `AFF77EB7…` / Todo `83B6C6A5…`。API26/x64 的独立 UI 候选 **29,107,139 字节 / B62DBD23…** 恢复公开 D 草稿，准确读回 `first 汉字 🧪 é.` 并显示 **13/1000**；随后“保留草稿”成功关闭编辑器、草稿数仍为3，业务提交数为0，两个阶段限定 **PASS**。该候选复用旧 v22 native，不包含新 intent 后端；见 [实际阶段证据](reports/ui-source/v24/device-fixed/progress-clipboard.json)与 [安装身份](reports/ui-source/v24/device-fixed/installation.json)。此前 96C0、3116 和诊断候选的失败或未知结果保留，不以本次限定通过覆盖。

本轮另冻结独立业务 intent journal 与 `EditorBusinessSession` 基础：prepare 保留完整原 Submission/publication 和 pins，issue CAS 持久生成实际 save/inspect literal，分 part 原样读回后才显式保存；Unknown 只核对或重试固定原 wire，prepared 可条件取消、issued 不可取消。首请求签发后晚 S2/owner 失效可显式只读恢复后结算原 S1，不消费 S2。原生库 **165 PASS /11 default ignored**、附件二进制 **3 PASS**，新24个真实 Store crash 向量及完整 DTO 导出已分别执行；ETS Session **30/30 PASS**，含真实 Store issued/closed 全五 part DTO，独立 API26 public-API 编译 **PASS /9.480s**。见 [native审计](reports/ui-source/v24/editor-intent-native-audit.md)、[Session审计](reports/ui-source/v24/editor-business-session-audit.md)和 [隔离SDK](reports/ui-source/v24/session-sdk/sdk-audit.md)。阶段/元数据回执不证明业务提交，development proof 不是 protected handoff 资格。

**本轮完整实际 ETS 模型807/807 PASS、0 skipped（11837.3834ms）；271项native来源构建前后一致，新ARM64/x64 release采用分别为56,517,238字节/E416EA7F…及54,925,386字节/A99CF367…。完整API26产品构建SUCCESS /13.893s，34任务fresh、313项完整复制、364项仓库输入构建前后字节一致，4项包内.so核对PASS；最终未签名HAP为29,381,667字节/F6EEEAE2D2175A7EA54DCEF582EA1BD5D15E324D48E65583B752775B4EC8925C，未安装。** 首轮隔离副本漏hvigor配置、13ms/0任务失败的日志和清单保留；仅补完整验证副本后在新retry1构建成功，生产源码未改。见 [v24统一验证](reports/ui-source/v24/validation.md)。 新 Session 尚未接 Index 保存/恢复入口；业务 source0 子草稿交接、`saved_exact` 关闭、详情变更后重新编辑、新 native 设备运行及完整输入/保存/重启闭环仍 **OPEN**。SDK 或旧 native UI 候选不能代替这些资格。版本保持 **dev19/1000019**，只推送 `codex/ArkTsUI`、不并入主线；完整 Flutter/Windows 对齐仍 **OPEN**。以下 v23/v22 及更早段落保留各轮历史事实。

2026-10-07 **v23 历史分支检查点**：待办组件补上完整状态快照门禁和恢复选区回调分类，755 项模型检查与 API26 构建通过。最终未签名包 **29,102,519 字节 /935E1B2E…**，337 项仓库输入及四项包内 native 核对通过；复用未变的 v22 Rust 双 ABI，最终包尚未安装。首个 `3116FBB8…` 候选设备恢复仍 **FAILED_OR_UNKNOWN**，关闭未执行。新业务接续文件仅为候选设计，未接入生产；完整对齐仍 **OPEN**。见 [v23 验证记录](reports/ui-source/v23/validation.md)。以下 v22 与更早段落保留历史交付时的事实。

Rust + ArkUI 的鸿蒙迁移工程，独立保存在本目录。当前源码版本为 **0.1.0-hmos-dev.19 开发预览**；尚未与 Flutter 功能等价，不能替代正式资料库。各版本的构建、模型与实际设备证据分别记录。

2026-10-07 v22 历史同步为 **恢复输入门禁修复与严格业务保存基础检查点**。Index 的恢复标志与视图挂载归属改为响应式状态，owner 未挂载时不构建输入控件，解除恢复后的待办 Add 禁用。`editor_save` / `editor_commit_inspect`、单次事务的 `continued_todos` 和独立 ETS 协调器已实现并完成有界验证，**尚未接入 Index，也没有业务成功后的 source0 子草稿交接**。版本仍 dev19；完整模型 **742/742 PASS**、fresh双ABI及最终API26产品构建 **SUCCESS /9.653s**，HAP **29,101,587字节 /41688FDA…**，334输入与四项包内native核对通过；新最终包未安装，见 [本轮验证](reports/ui-source/v22/validation.md)。

现 Index 的 raw fork 继续保留原 source 和业务 CAS 冲突；新基础可核准确原 wire、publication 与历史结果，却不自动重基或恢复产品在途业务请求。完整 Rust **158 PASS /9 default ignored**、新专项14 PASS、实际 Store14故障边界、实际 Dart50全量 normalization 对照、ETS23项与隔离 API26 编译通过，分别见 [native审计](reports/ui-source/v22/editor-business-native-audit.md)和 [协调器审计](reports/ui-source/v22/editor-business-model-audit.md)。SDK 生命周期撤销仅是应用接纳边界，不证明系统输入队列已排空。完整 Flutter/Windows 目标仍 **OPEN**；[v21界面检查点](reports/ui-source/v21/validation.md)及更早报告保留各轮当时事实。

API26/x64 设备已安装独立 UI 候选 **28,799,011字节 /96C0AB68…**，它只含 f9 界面加上述两项状态修复，复用旧 b486 native，**不含本轮新业务后端**。公开 D 恢复与Add空行限定 PASS；首次文字输入仍为 **FAILED_OR_UNKNOWN**（准确捕获字段undefined，Back退出）。之后独立恢复阶段 **PASS**，实际读回`first 汉字 🧪 é.`，原失败没有覆盖；再点保留仍 **FAILED_OR_UNKNOWN**，因“当前事件尚未完整捕获…草稿清理已停止”保持编辑器打开、草稿保留，完整关闭/业务保存闭环未通过。新最终产品包设备资格不得套用这个候选。旧150003崩溃及 [v21记录](reports/ui-source/v21/validation.md)保留原事实。

此前发布的 **dev20 源码检查点** 新增 Flutter 输入格式处理、多行待办模型/组件和新卡待办原子保存，完整模型617项、Rust123项及实际Flutter/Dart对照通过。**当时 Index 尚未接入新UI，版本号仍dev19。** 该检查点包未安装；已发布dev19包的安装、旧草稿和TSV附件恢复另有设备记录。见 [原检查点范围](reports/ui-source/v20/checkpoint-status.md)与 [原验证记录](reports/ui-source/v20/validation.md)；以下dev19及更早段落保留其交付时事实。

2026-09-27 跟进：按 Flutter `versioned_task_panel.dart` 接入待办重命名、上下移动、批量完成与移除确认。共享 Rust TaskId 模块与当日参照一致；通过主机测试、双架构构建和 x64 模拟器验证。见 [dev.4 验证记录](reports/ui-source/v4/validation.md)。

已提供实际 HAP、Rust OHOS 双架构构建、ArkTS 界面、ArkUI NDK 原生外观预览、N-API 异步桥与共享核心事务。已在 Pura X View / HarmonyOS API 26 x86_64 模拟器安装、启动、建卡保存；另有设备侧 Rust 自检。

2026-09-27 dev.5：接入七种面板风格、独立深度及完整材质跟随关系；修复公共边缘层百分比尺寸造成的错位/贯穿框线。最终安装包的设备截图与此前诊断图分开保存，见 [dev.5 验证记录](reports/ui-source/v5/validation.md)。持续追平目标见 [对齐计划](docs/ALIGNMENT_PLAN.md)。

2026-09-27 dev.6：修复短内容被滚动容器居中造成的大块留白，按 Flutter 固定 14 vp 列间距排列卡片；增加侧栏收起/展开、五页滚动位置保留和设置返回恢复，并支持遵循系统旋转锁定的自适应方向。880 vp 竖屏与 1488 vp 横屏的实际设备证据见 [dev.6 布局验证](reports/ui-source/v6/validation.md)。

## UI 源码对齐

2026-10-07 dev.19：字段按真实 Flutter `characters 1.4.1 / Unicode 16.0.0` 的 grapheme 规则计数，标题/传入待办字段/假设/结论/正文上限分别为 **60/1000/5000/10000/20000**。当前UI的待办字段仅为待添加单条输入，未复现Flutter全部已有行与待添加行的聚合1000和行间选区模型。粘贴先异步检查选区替换后的完整未来文字，再导入原件；确认 pin 后用实际资产引用重检，失败保留已确认附件。移除编辑控件的 UTF-16 `maxLength`，显示异步计数，超限完整输入保留；raw journal/IME 沿用既有结构与字节预算，业务保存和粘贴拒绝活跃 composing 或超限。待办重命名成功回执仅在原 owner、epoch 和完整输入一致且无 composition 时关闭编辑器，迟到候选继续保留。真实 Flutter 的 **1,198 组完整对照 PASS**，完整实际 ETS 模型 **535/535 PASS**，Rust **106 PASS**，三项默认条件 ignored 已单独 fresh 比较通过。最终 **0.1.0-hmos-dev.19 /1000019** 的 API26 未签名 debug HAP **SUCCESS /11.302s**，**28,164,478 字节**，SHA-256 `F7A913980CF3213536820727A677EEC04EA169FE2B65CE16E305F352D14BE4EC`；见 [dev.19 验证记录](reports/ui-source/v19/validation.md)。**dev.19/dev.18 均未安装，当前设备仍 dev.17，新包设备验收 NOT_RUN**。完整 Windows/Flutter 对齐目标仍 **OPEN**。

dev.19 已改正旧 dev.18 字段按 UTF-16 计数的误限，并将待添加单条待办输入从500提高到1000；Flutter多行待办聚合和行间选区模型仍未齐。富转换输出也按 Unicode16 grapheme 检查。完整 worker/转换输出/序列化及共享业务字节预算仍独立生效，不保证 20,000 grapheme 一定可保存。直接输入保留超限，与实际 Flutter 默认 formatter 的自动截短行为存在明确差异；系统 IME、选区 affinity 和全篇连续选择仍未齐。独立 API26 双指测试工具已构建 main/test HAP，10 项工具模型通过，但未安装、未注入实际设备手势；不能计作图片手势验收，见 [字段审计](reports/ui-source/v19/editor-field-policy-source-audit.md)与 [双指测试工具审计](reports/ui-source/v19/image-multipointer-tester-audit.md)。

2026-10-07 dev.18 历史交付：剪贴板来源按真实 Flutter UTF-16 单元限制分档，修复中文/emoji 被旧 2 MiB 字节上限误拒，以及系统 Unicode RTF string 无编码标记导致的 ANSI 误读；图片基础手势修复焦点、边界、识别器接管和旧事件身份。最终 **0.1.0-hmos-dev.18 / 1000018**，完整实际 ETS 模型 **464/464 PASS**，Rust 主机 **98 PASS**，两项条件实际 Flutter 对照单独通过，ARM64/x64 原生重新构建。API 26 未签名 debug HAP **SUCCESS / 6.782 s**，**27,866,016 字节**，SHA-256 `910F3069B7979E5F8C6CF9E7DC770DF6E342725D493798DDE38F317524DB7DE2`。本轮为源码、模型与构建分支交付；**dev.18 未安装，设备验收 NOT_RUN**，设备仍为 dev.17。见 [dev.18 验证记录](reports/ui-source/v18/validation.md)。完整 Windows/Flutter 对齐目标仍 **OPEN**。

dev.18 来源上限为 plain/HTML/XML **2 Mi UTF-16 单元**、RTF **8 Mi**，分别使用 **6,291,459 / 25,165,827 bytes** 包络；RTF 8 Mi 只对齐 Flutter 可达的 **无插件本地分支**，正式 RustStudioPlugin 仍统一 2 Mi 单元且插件请求 64 KiB，无异常 fallback。系统 Unicode RTF string 保存为确定性 **UTF-8+BOM serialization**，不声称原始 document bytes；ArrayBuffer 原件逐字节保留。孤立 surrogate 在编码前拒绝，合法 emoji/有意 U+FFFD 在读取层保留；native RTF 输出仍保守拒绝 U+FFFD，严格损坏编码/NUL/不支持 RTF 与 Flutter 宽松解码有差异。64 MiB/20 槽/sidecar 与草稿 pin 预算未放宽；该轮字段 grapheme 误限和待添加单条500限制已由dev.19改进；Flutter多行待办总量模型仍未齐，Flutter 200 MiB 附件容量仍未对齐。

dev.18 图片基础手势模型 **30/30 PASS**：移动双指焦点同步缩放/平移，单指与纯双指 pan、pinch 接管、边界立即反向及 end/cancel/旧令牌/时间戳围栏。按实际 Flutter tight InteractiveViewer 的布局边界限制有效手势缩放 1–2.5；既有按钮和双击复位是 HMOS 补充。最终包图片手势 **NOT_RUN**，惯性/fling/scale-velocity 动画尚未实现，GIF/损坏格式未验，见 [图片手势审计](reports/ui-source/v18/image-gesture-source-audit.md)。

dev.17 的独立后续设备观察限定确认：自有控件系统复制的普通文字粘贴、TSV 转 Markdown 与 UI 已确认原件 pin；系统导出选择器已打开，但目的地和导出字节核验未完成。跨进程恢复、业务保存及 HTML/RTF/Office 等实际提供者仍未验收，详见 [后续记录](reports/ui-source/v17/follow-up-validation.md)。这不计为 dev.18 设备通过。

2026-10-07 dev.17 历史交付：接入授权 PasteButton 的系统剪贴板快照，按实际提供的 plain/HTML/RTF/Spreadsheet XML、原始二进制图片/文件和 PixelMap 准备原件，复用 durable import/pin；HTML 内嵌图片仅在确认 pin 后替换为持久资产引用。一次性来源绑定剪贴板 changeCount 和编辑目标，URI 不进入文件选择器授权表、不持久保存或跨读取复用；先完整检查正文、槽位和预算，再逐项确认，未知导入保留原请求。原件、转换、图片提取和冻结选区插入已有源码与模型覆盖；**真实系统富内容粘贴、Office 提供者及保存/重启/导出闭环 NOT_RUN**。最终版本 **0.1.0-hmos-dev.17 / 1000017**，完整实际 ETS 模型 **402/402 PASS**，API 26 未签名 debug HAP 构建 **PASS / 9.467 s**；包 **27,815,034 字节**，SHA-256 `D9DECC46BB0E953BB56A4CDC5DBBB71D0B863E6380C273CC677B73C36689C03F`。301 项构建输入 disk 与 staged 核对均 PASS，见 [dev.17 验证记录](reports/ui-source/v17/validation.md)。完整 Windows/Flutter 对齐仍 **OPEN**。

dev.17 交付时的容量差距（来源字节限制已由 dev.18 改进）：原生转换上限为 2 MiB 原件字节，Flutter 来源上限按 2 Mi UTF-16 单元计；较大富格式原件可在附件预算内保留，但转换失败须明确提示。系统快照最多 20 条记录，展开图片/原件仍共用剩余 20 个 spool 槽和含 sidecar 的 64 MiB 预算，不能据此宣称 Flutter 200 MiB 附件容量等价。该轮字段UTF-16计数和待添加单条500限制已由dev.19改进，Flutter多行待办聚合/行间选区仍未齐；直接输入控件、IME、全篇连续选择、生产捕获证据链及 HUKS 保护仍未补齐。

2026-10-07 dev.16 历史交付：实际运行 dev.15 后复现媒体中心单击仍不能显示控件，关闭入口可以响应；负面截图另存于 [dev.15 后续设备观察](reports/ui-source/v15/device-current)，原公开验证报告保留交付时的事实。dev.16 修复隐藏控件子树继续参与命中，并按预览令牌重建手势和控件节点。最终实际 ETS 模型 **303/303**、API 26 HAP 构建通过，未签名包为 **24,857,891 字节**，SHA-256 `A971227AC2D39730C2228972B513DBAAB4049E1C9E9AAB49397517BFB0208C7C`。已在既有 x64 模拟器安装并读回版本 `1000016`，恢复同一 `HMOS-media-20261007-A` 草稿的 WAV/MP4 两个 pin；原生 WAV 两次观察均为 **0:00 / 0:12、无自动播放**，中心单击后控件实际可见的截图已复核。播放时间从 0:01 推进至 0:03，暂停后两次均为 0:06，限定播放/暂停检查通过；真实音频输出未证明。首次全屏实际进入横屏，但驱动裁剪断言停止，阶段为 **FAILED_OR_UNKNOWN**，随后只读核对横屏状态通过；首个Back驱动在动作前断言停止，修正驱动后fresh Back恢复1320×2232竖屏、正常系统栏和同一0:06暂停预览，关闭后回编辑器保留原两份附件ID/名称，截图已实际复核。本次按用户要求收束为媒体点击修复分支更新，多选/空标题新设备流程、seek/全屏手势、视频与系统文件预览仍未验收，详见 [dev.16 验证记录](reports/ui-source/v16/validation.md)。完整对齐目标仍开放。

dev.16 后续媒体观察单独记录：同一已安装 `A97122…` 包的 MP4 0:00/0:12 准备且无自动播放、播放时间推进、暂停稳定、横屏全屏、左右双击 ±10 秒/水平拖动、普通进度拖动和 Back 返回竖屏已有真实限定 PASS。初次 Back 驱动错误仍保留；Home 尝试后仍在应用内，后台生命周期为 **NOT_QUALIFIED**。这些结果不证明 dev.17 设备运行，也不证明真实音频输出、完整格式矩阵或系统文件预览，见 [后续限定验证](reports/ui-source/v16/follow-up-validation.md)。

dev.16 最终构建输入 **298 项**已分别从工作文件和暂存索引核对通过，并匹配 `A97122…` 包；见 [工作文件核对](reports/ui-source/v16/build-manifest-disk.log)与 [暂存索引核对](reports/ui-source/v16/build-manifest-staged.log)。字节核对不代表推送、签名或未跑的设备资格。

2026-10-07 dev.15：按 Flutter `importFiles` 接入系统选择器多选，单批最多 20 个并受当前草稿已选附件和全部暂存导入记录的剩余额度限制；按返回顺序逐项准备、独立操作身份导入和确认 pin，不只处理首项。确认导入后，仅当当前标题仍为空白时，以文件名前 60 个 UTF-16 单元补齐标题，和新增 pin 同次保存；新标题不被迟到结果覆盖。取消、失败、未知结果、编辑器或世代变化停止后续项，已确认项和原请求保留。清理 spool 后再次核对原编辑器，加入附件绑定卡片/草稿身份；切换编辑器清空旧批次进度。

媒体预览补齐控件显隐、交互后 3 秒隐藏、拖动时保留控件和 300 ms 过渡；全屏增加水平拖动定位、左右双击 ±10 秒及连续点击累计反馈，迟到手势/计时器/旧 surface 不借用新预览身份。最终实际 ETS 模型 **303/303**、API 26 HAP 构建通过，未签名包为 **24,856,916 字节**，SHA-256 `7347C8A8E2136D33A888BDD606766D788A6729633D7449A4BBA16F9CF8624A4B`。本轮交付范围为源码、模型和构建；**dev.15 实际设备验收 NOT_RUN**。本轮另在已安装的 dev.14 / `43E402…` 包上完成 WAV 12 秒准备、无自动播放、播放/暂停、进度、全屏/返回和关闭基线；视频仅导入成功，播放与 PDF 尚未跑，不能算 dev.15 新控件或多选验收。详见 [dev.15 验证记录](reports/ui-source/v15/validation.md)与 [多选源码审计](reports/ui-source/v15/import-source-audit.md)。当前 Flutter 参照未传纵向音量/亮度回调，相关手势默认禁用，不作为已默认启用的附件目标；真实音频输出、媒体格式矩阵和完整控制设计的设备资格仍开放。

2026-10-07 dev.14：按 Flutter 附件预览源码接入音频/视频播放、暂停、进度拖动和当前/总时长，打开后不自动播放。视频通过原生 XComponent 显示，完整核验的私有文件以只读 FD 和准确长度交给 AVPlayer；关闭须先等播放器释放和 FD 关闭，再清理文件，失败保留原会话供显式重试。前后台变化暂停播放，不自动恢复。全屏接入 WindowKit 的方向、布局与系统栏控制，退出时恢复原窗口快照；恢复失败保留退出入口。该轮未复现全屏水平/双击定位和完整控制过渡，随后由 dev.15 补接源码。dev.15 重新核对确认纵向音量/亮度需回调且本参照未传，不应将其描述为当前参照已默认启用的目标；dev.14 原验证报告保留当轮记录。

普通文件的“打开”接入 PreviewKit 系统预览，沿用 Flutter 可执行/脚本类型的导出限制。预览文件保留安全名称与扩展名，仍按当前业务修订或已确认草稿 pin 完整核验；系统窗口关闭或返回应用不等于其他阅读者已经结束，不自动删除。用户通过“已结束查看，清理临时文件”入口结束本次查看，状态未知时保留原文件且不重放打开请求。这是系统预览路径，不是任意格式默认应用打开的等价证明。dev.14 实际 ArkTS 模型 **226 项**及 API 26 HAP 构建通过；Rust/原生实现本轮未变，旧 72 项 Rust 结果不计为本轮新跑。最终未签名包已安装到既有 x64 模拟器，并恢复本轮公开文字草稿；音频选择在浏览 Download 后返回应用，未观察到精确测试文件且未选中任何文件，驱动断言终止。复查同一草稿仍保留原文、零附件且未发布业务卡；这不能证明确定的选择器缺陷。原生解码、进度/全屏、音频输出与 PreviewKit 可读内容全部 **NOT_RUN**。包身份和原始观察见 [dev.14 验证记录](reports/ui-source/v14/validation.md)，旧 dev.13 截图不计为新功能验收。

2026-10-07 dev.13：按 Flutter `idea_markdown.dart` 的 URI 规则解析 `attachment:` 图片/GIF，以名称或资产 ID 匹配当前选中附件；段落与表格的同资产引用共用一次完整字节核验。图片使用 SDK 文件 URI，来源、pin 或世代改变时释放旧预览，迟到回复不会恢复已移除图片。独立预览加入读取/解码状态、0.8–2.5 倍缩放、受限平移和复位；编辑器附件行可直接导出已确认 pin，无需先保存业务卡。清理失败保留原令牌与预算，并提供显式重试入口。

dev.13 最终检查：Rust 主机 **72 项**、实际 ArkTS 模型 **163 项**通过，API 26 HAP 构建成功。最终包在既有 x64 Pura X View2 实际显示中文名称引用的内嵌图片，点击预览、按钮放大至 125% 和复位至 100% 通过；编辑器原始 pin 通过系统选择器导出，1,924,867 字节与原件逐字节一致。同一测试卡保存后，实际表格单元格图片、缺失引用/远程读取提示和重启无新增草稿通过；查询观察中断后按现有卡身份核对，没有重复保存。移除该选中图片后，保存前旧图立即变为缺失提示；显式保存并重启保持同一卡身份、原始 Markdown 不变、零附件且无剩余 raw journal。完整设备范围见 [dev.13 验证记录](reports/ui-source/v13/validation.md)。本轮复用 dev.12 的双架构原生库字节，未新增 ARM64 构建或真机证据；实际损坏图片解码、双指/平移手势及 GIF 动画尚未验收。当前仍为未签名 debug HAP。

dev.12 历史检查：Rust 72 项、实际 ArkTS 模型 119 项通过；双架构构建与 x64 原生附件/FD 检查通过。实际选择器导入中文文档、图片草稿恢复/预览、同卡保存、两份导出逐字节一致、取消无新增 journal、只移除一个引用后重启读回通过。最终类型图标和扩展名来自 Flutter 源码；这是 dev.12 包的限定证据，详见该轮验证记录。

2026-10-05 dev.12：接入系统文件选择器的实际 URI 内容流，复用原 durable import 状态机和草稿附件 pin，支持新卡与已有卡导入、保留、重启恢复、移除当前引用和按修订核验后导出。图片/GIF 可从已核验的私有临时文件预览；原 URI 或导入 spool 消失后，已确认 pin 仍可读取。附件身份、别名、元数据和完整字节校验参与草稿与业务保存，已提交的原业务操作可在草稿推进或弃稿后精确核对。Windows 主机草稿 24 项、实际 ArkTS 草稿模型 25 项、实际 Engine 附件集成 3 项通过，独立主机 runner 为 PASS_SCOPED；这不是 OHOS FD、系统选择器或该轮设备验收。完整证据与设备结果由 [dev.12 验证记录](reports/ui-source/v12/validation.md)分别记录，主机范围见 [附件主机验证](reports/ui-source/v12/attachment-host-review.md)。该版尚缺的 Markdown 内嵌附件图片和编辑器附件行导出已由 dev.13 接入；音视频播放、通用文件外部打开和多文件选择仍待补齐。

2026-10-05 dev.11：按 Flutter 的阅读流程，点击卡片先打开 Markdown 详情，再显式进入现有草稿编辑器。详情提供分类、阶段、独立 TaskId 勾选、收藏、复制和删除确认；卡片正文长按与菜单提供查看、编辑、复制、转项目、组件设置和前后移动。五个页面各自持久保存自定义顺序，筛选移动保持隐藏卡片的位置；独立拖动柄通过当前来源、页面/查询条件、视图修订和一次性令牌检查。完整拖动边缘自动滚动、鼠标/键盘上下文手势和详情内完整任务编辑仍需补齐。见 [dev.11 验证记录](reports/ui-source/v11/validation.md)。

2026-10-05 dev.10：正文预览改用 Rust CommonMark 解析投影，原生显示行内强调、删除线、嵌套列表、引用、完整代码块和横向表格，链接由用户点击打开，远程图片点击后读取。原始 HTML 只显示为文字。粘贴入口接入鸿蒙授权 PasteButton，按字段和 UTF-16 选区插入完整文字，正文 TSV 复用原 Rust capture 转为 Markdown 表格；超限与异步目标变化拒绝插入，原文字保留。当轮尚未接完整富文本、Office 和剪贴板图片/文件；dev.17 后来接入本地富内容路径，设备提供者资格单独记录。文件选择器附件另由 dev.12 接通。见 [dev.10 验证记录](reports/ui-source/v10/validation.md)。

2026-10-05 dev.9：按原 `editor_draft` schema/model 接通文字草稿私有事务日志，保留完整来源、世代 CAS 和原操作历史回执。新卡草稿不创建业务卡；关闭时确认最新文字再保留，提供草稿列表、恢复和确认弃稿入口。仅查看未编辑的已有卡片不占用草稿槽。正文宽度达到 Flutter 的 590 vp 条件时编辑/预览并排，实验字段改为多行。真实验证和限制见 [dev.9 验证记录](reports/ui-source/v9/validation.md)。原始候选串可保留为文字，但普通控件不能重建原 IME 会话。

2026-10-05 dev.8：接入原 Rust 查询计划和 `query_v2`，覆盖标题、正文、假设、结论与附件元数据名称搜索，分批筛选、UTF-16 标题排序与稳定收藏排序；ArkTS 查询去抖并丢弃过期结果，切换条件立即清除旧结果，同页保存保留最近确认的成员并刷新修订。仍限定 256 张开发库；默认顺序是源协议的反向 ID 顺序，不是创建时间。实际验证与设备环境边界见 [dev.8 验证记录](reports/ui-source/v8/validation.md)。该轮仅验证附件查询元数据，附件导入的实现和证据另见 dev.12。

2026-09-27 dev.7：卡片区改为单滚动视口中的原生懒加载瀑布流，按页面/卡片身份维护视图，按修订刷新内容；排序、过滤及卡片高度变化同步使位置缓存失效，避免错位或重叠。16 张长短交错记录通过设备上的收藏、正文增减、排序、空结果及完整遍历检查。见 [dev.7 验证记录](reports/ui-source/v7/validation.md)。这不解除现有 Rust 查询的 256 张限制，也不是内存峰值或真机验收。

已按活跃 Flutter 的 main.dart / appearance.dart 补齐紫灰玻璃工作台、专用分类卡片、外观与材质子页、色盘、字体、日常清单、音乐空状态和独立编辑弹窗，并实际渲染源码参照图与鸿蒙截图比对。详见 [源码对应和验证](docs/UI_DESIGN_DEV3.md)，可查看 [并排截图](reports/ui-source/compare.html)。这不是完整 UI 等价声明。

## 使用与构建

使用 DevEco Studio 打开本目录。默认包名 `dev.morrow.hmos`，不覆盖 Flutter 应用。启动后打开独立的本地试验工作区。卡片保存于应用沙箱的 `hmos-development.sqlite`，不读取现有桌面资料。

```powershell
# 在本目录执行；首次 ohpm 依赖安装可用 devecocli build 或 Studio 同步。
./scripts/build-rust.ps1 -Test
./scripts/build-rust.ps1 -Abi arm64-v8a
./scripts/build-rust.ps1 -Abi x86_64 -Runner
./scripts/build-hap.ps1
./scripts/check-reference.ps1
```

环境：Rust 标准库 `aarch64-unknown-linux-ohos`、`x86_64-unknown-linux-ohos`；API 26 SDK；Cap'n Proto 1.4.0。脚本提供本机已验证路径，`build-rust.ps1 -Sdk` 与 `build-hap.ps1 -Studio` 可覆盖 SDK/Studio 位置。Cap'n Proto 可执行文件须在 PATH，脚本也加入本机已部署目录。

HAP 输出：`entry/build/default/outputs/default/entry-default-unsigned.hap`。当前模拟器接受该调试安装；**不是已签名发布包，也不是 ARM64 真机验收**。构建脚本关闭本工程 hvigor daemon，避开当前全局 daemon 锁问题，不删除全局缓存或停止其他构建。

## 已接通范围

- 卡片标题、正文、假设、结论，新建和修改；类别、阶段、收藏、搜索和删除视图。
- 字段与待添加单条待办输入按 Unicode16 grapheme 异步计数，粘贴/业务保存检查完整未来值；原始输入、UTF-16 选区与 IME 草稿分别保留，超限不静默截短。完整字节预算和保存失败仍独立检查。
- V2 TaskId 待办新增、独立勾选、重命名、上下移动、确认移除；同名任务不会联动。批量勾选经确认后与当前阶段在同一事务提交，普通排序/重命名保持阶段和完成状态。
- 直接调用现有 `cards_v2` / `tasks_v2`，通过 `morrow-core::HostRuntime` 的对象授权、版本化 CAS 与原操作幂等事务保存。
- 开发库查询从单次 WAL 快照读取完整属性，复用 `query_plan_v2` 的 128 项 / 64 KiB 帧预算、分页过滤和排序归并；回收站保持独立撤销视图。此本地调用没有生产 guest 执行、查询捕获或审计权限证据。
- 删除后仅允许 **8 秒内撤销**，沿用共享核心的规则，不提供永久恢复承诺。
- `u64` 修订和时间戳跨 ArkTS 边界使用十进制字符串；完整源记录随修改请求绑定，不用界面投影覆盖正式内容。
- 数据库操作在 N-API worker 执行；ArkTS 串行提交。回复未确认保留同一序列化请求；正式提交后回读错误仍保留核对状态。编辑输入不会被迟到的保存回复覆盖。
- 文件选择器按当前剩余额度多选、单批最多 20 个，逐项顺序导入；取消未编辑已有卡片的选择不创建草稿 owner。确认成功后只填当前仍为空白的标题；原 import 请求持久保留并显式核对，停止后续项不抛弃已确认项。主草稿选择来源资产、持久导入或同草稿前代 pin。保存核验完整 source CAS 和 pin 字节，私有 journal/import 不计入普通卡片查询；移除引用保持其他字段与 TaskId，历史 blob 不承诺即时回收。
- 已保存附件按完整源记录及修订导出，编辑器草稿附件按准确世代导出；图片/GIF/音视频预览先核验字节。音视频预览提供播放/暂停、进度和全屏控制；普通文件走系统预览并保留显式结束查看的清理入口。Markdown 按当前附件名称/ID 解析内嵌图片，未知引用显示缺失提示，远程图片保留显式读取入口。文件流经有界原生接口，ArkTS 不接收完整附件字节数组。

## 尚未完成的关键边界

当前使用原核心的 **未封存试验 Store**，并非生产 `Workbench` 的安全降级。现有正式宿主 `storage.rs` 在非 Windows 平台明确拒绝打开；此检查保持原样。Harmony HUKS 密钥保护、库身份、单所有者租约、审计封存、恢复与备份还需适配并单独验收。

试验库限定 256 张卡片，保留核心默认事件预算（1024 条 / 64 MiB），没有绕过队列上限或静默清除历史。达到容量会停止写入。系统自动备份关闭，避免普通文件备份冒充 SQLite 一致性快照。

dev.9 的原始文字草稿在 dev.12 加入选中附件与 pin，最多 16 份活动草稿、256 个累计身份与包含 pin 字节的 64 MiB 活动预算，沿用事件队列上限。原 schema/model 字节保持固定，适配层允许来源 0、持久导入 2、同草稿前代 pin 3；来源 1/4、前驱捕获证据和父子交接仍明确拒绝。附件准备缓存也有 64 MiB 总预算，与 Flutter 普通附件的 200 MiB 上限不等价。普通控件重启后以普通文字恢复候选串，不能重建 IME 组合会话；方向/affinity 也不能从 Harmony 的起止选区事件反推。较新草稿遇到已变化的业务来源会保留并显示冲突，不隐式覆盖或重基。

dev.13 内嵌预览另限同一进程/缓存根下最多 8 个活动图片、64 MiB 活动文件；已登记预览的清理失败仍计入预算，用户显式重试仅处理这些已登记的失败目录。读取失败且尚未登记的临时目录仅作尽力清理，不在此重试账本内。此账本不扫描进程死亡后的全部物理缓存，也不是空间耗尽或拒权实测。

dev.14 媒体对话框与系统文件预览分别只允许同一进程/缓存根下一个活动文件，各自上限 200 MiB；新增系统预览入口不放宽附件导入/草稿的 64 MiB 预算。系统预览句柄同样只在进程内保留；系统窗口状态不能证明下游应用已经释放读取。API 26 没有系统栏显示开关的读回接口，因此全屏只接受已明确建立本应用系统栏基线的同一窗口；分屏、悬浮及 2-in-1 模式仍需单独验证。

**v25 正式业务原请求已持久保留并接入主页面发现、只读恢复和显式原请求核对。** 未确认草稿保存/弃稿及附件清理的在途协调仍有进程内边界；恢复原业务请求不等于重启后已恢复当前子草稿 writer。父草稿缺失、子草稿已推进或关闭后重开 owned todos 的页面接续仍 OPEN。附件原 import 请求与缓存另有持久恢复和核对入口；成功提交的卡片可重启读回。新 native 设备闭环、多选/空标题、真实音频输出/完整媒体格式矩阵、任意文件默认应用打开、完整富剪贴板/Office 提供者、跨段连续全篇选择、插件运行/管理、网络服务、TLS、音乐工作区/歌词、字体文件导入、完整九语文案、完整宽屏矩阵和备份恢复仍待验收或补齐。外观与日常清单保存于 Preferences；草稿使用 Rust 核心事务日志，详见 [功能与复用清单](docs/PARITY.md)。

## 跟随主任务

功能参照：[检查项目并完成更名](codex://threads/01a085bd-7a94-7f93-8a1f-1ecf417f5ee3)。环境参照：[安装 Deveco CLI](codex://threads/01a0cc0a-76a5-7973-a6c8-eb7566100932)。

初始快照来自 `../build/io-safety-refactor`，HEAD `ddd9cc8eec224af51f4e58654c9b297332147d96`，版本 `0.1.9-test.54+58`，包含未提交增量。2026-10-05 核对该参照为 `925fb8ca` / test.57；较新 Windows 汇合工作树为 `77246617` / test.58。冻结快照未整体替换；当前有 50 个共享路径变化、72 个新增路径，文件 IO、通道、生产监督/恢复和 SDK 仍需独立审查。四个查询/卡片/TaskId 业务块哈希仍一致；查询调度和文字草稿模型分别固定来源。见 [上游审计](reports/ui-source/v9/upstream-audit.md)。

dev.12 再次只读核对三个本地参考 HEAD：`io-safety-refactor` 为 `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` / `0.1.9-test.57+61`，`win-cloud-20261005` 为 `772466177fe589cee53bc633e69f411c34610104` / `0.1.9-test.58+62`，`windows-sdk-reconstruction` 为 `20669f671152972470340a65eac3458dd2f61b4d` / 同应用版本。所查主界面、附件、富文本、草稿和暂存源码 Git blobs 相同，相关工作文件没有未提交修改；不同换行造成的文件 SHA256 差异不代表设计变化。功能线程仍在构建 Windows 测试组和验证示例插件，环境线程此前的 CLI 项目构建结论不提供本轮签名或运行资格。

2026-10-07 dev.13 只读跟进：`win-cloud-20261005` 推进至 `e83cdf1d3506b001d07ffceaaa9f98b688bcc150`，另外两个参考 HEAD 与上述 dev.12 观察相同。所查 17 个主界面、附件、Markdown、草稿和暂存源码 Git blobs 在三者中仍一致，相关工作文件没有未提交修改；本轮未发现这些设计来源漂移。功能线程当时仍在修复 Windows pipe/ConPTY 错误传播，尚未取得最终结果。范围与观察时间见 [dev.13 上游审计](reports/ui-source/v13/upstream-audit.md)，不据此扩大 HMOS 资格。

dev.14 再次只读检查上述三个参考 HEAD，没有变化；所查四个主界面/附件 Dart 源码在三者中仍为相同 Git blobs，相关路径没有未提交修改。额外核对实际安装的 `media_kit_video 2.0.1` 控制源码，明确本应用使用 `MaterialVideoControls`，单媒体不显示上一项/下一项。功能线程本轮快照报告 12 项 Windows 输出/等待修复测试通过，但仍在进行完整 SDK/VM 资格；这是源线程进度，不是 HMOS 验收。具体来源与观察时间见 [dev.14 上游审计](reports/ui-source/v14/upstream-audit.md)。

dev.15 只读核对三个本地参考 HEAD 和四个附件/UI Dart blobs，仍与 dev.14 相同；功能线程最新快照仍在进行 Windows 进程输出结算和会话接口工作，未提供新的完整资格结论。见 [dev.15 上游审计](reports/ui-source/v15/upstream-audit.md)。本轮多选和控制过渡复用已核对的 Flutter 源码设计，不把源线程进展计为 HMOS 验收。

dev.15 的结构化富剪贴板审计保留当时 **NEXT_PROPOSAL_ONLY** 的事实；dev.17 已按其方向接入本地授权快照、原件/转换/内嵌资源和确认 pin 后插入。完整提供者、保存/重启/预览/导出/移除设备闭环仍待验收，见 [原方案审计](reports/ui-source/v15/next-parity-audit.md)与 [dev.17 验证](reports/ui-source/v17/validation.md)。持续追平 Windows UI 与大部分实现的目标仍 **OPEN**。

`shared/reference.json` 固定 228 个共享文件的 SHA-256，包括 V2 新增源码。`check-reference.ps1` 比较真实工作树与快照并写入差异报告；不自动覆盖正在使用的源码。后续同步必须审查差异、更新功能清单、重跑 Rust/HAP/设备验证。没有创建定时任务或向原任务发送消息。

共享源码保留原 AGPL-3.0-only 许可与 `shared/LICENSE`；快照不是另起一套业务规则。ArkTS/C++/Rust 适配代码属于本目录；原 Flutter 与活跃工作树未修改。
