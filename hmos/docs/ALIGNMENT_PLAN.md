# 持续追平 Windows 工作台

2026-10-09 **v30 当前音乐界面检查点**：实际 Index 接入 MusicWorkbench、文件选择与完整原件导入、播放列表/持久选曲、播放/暂停/seek、排序/移除、离线歌词导入/全文弹层及歌词页脚。界面布局依据 Flutter 音乐面板和 little_tips 源码；外层沿用现有组件材质，默认页脚透明。恢复只读取、不创建播放器或自动播放。后台暂停保留原播放器/FD，返回不自动续播；附件媒体须先确认音乐实际暂停。Unknown 不阻止原播放器停音，原导入核对不生成新 ID；未保存全文绑定原曲目，已知部分缓存清理只重试同原对象。

最终实际 ETS/tool **1118/1118 PASS，0 fail/skip/cancel**（47 suite 文件、151 项输入前后一致，29391.5177ms）；完整 API26 **SUCCESS /17.975s**，34/34 tasks 执行，323 复制/366 仓库输入一致；**八音乐模块均在实际产品 entry graph 检查并 emit**，四项包内原生库核对 PASS。Rust/C++ 实现未变，283 项 native 来源和双 ABI 静态库精确复用 v29；本轮不新增 Rust 测试/构建资格。最终 HAP **31,413,203B**，SHA256 `FB81FEF187D11B09E6DEF56B4B20280E68EED4135BD61C260638ED015408BFD1`。

版本 **dev20/1000020**，包 unsigned/uninstalled，当前 HDC target 为空、模拟器进程未运行，设备验收 **NOT_RUN**。实际文件提供者/音频 codec/声音/seek/拖拽/全样式渲染、pre-begin 导入持久恢复、重启后部分清理残留、在线歌词/解密/封面/原件 GC/protected 与完整 Flutter/Windows parity 均 **OPEN**。交付仅 `codex/ArkTsUI`，不并入主线。详见 [v30 验证](../reports/ui-source/v30/validation.md)。

以下 v29 及更早内容保留为历史；各轮数字和“当前”只指其当轮范围。本轮资格以上述 v30 摘要、最新表和验证记录为准。

2026-10-09 **v29 历史音乐基础检查点**：实现独立持久曲库、标准本地原件的完整 length/hash FD 导入与导出、歌词读取/定位、持久选择/排序/退役，以及共享 Rust 的纯播放决策。新增实际 MusicLibrary、MusicFiles 和 MusicPlayback/PlatformMusicPlayer 基础，固定原请求与 Unknown 保留；Pending、retained 原件和 Ready 发布属于独立事务。**Index 音乐 UI 尚未接入**，不能据此称音乐产品闭环。

最终实际 ETS/tool **1021/1021 PASS，0 fail/skip/cancel**（43 suite 文件、143 项输入前后一致，32,783.3028ms）；Native default **200 library +3 binary PASS**、20 explicit ignored，音乐子集 8 PASS、13 个真实进程中断边界和实际 Store DTO 导出分别通过。新双 ABI release PASS 并采用，283 项 native 来源；ARM64 **58,110,214B /DD86DF95…**、x64 **56,515,718B /E9C66A49…**。最终完整 API26 **SUCCESS /30.098s**，34/34 tasks 执行，319 复制/392 仓库输入前后一致，四项包内原生库核对 PASS；最终 HAP **30,842,790B**，SHA256 `4C4292881291C769B3039E01F895BF75C6C526070DB063AB78224698743015E1`。

版本保持 **dev19/1000019**。独立 Music SDK probe **SUCCESS /23.545s**，四音乐模块实际严格检查并 emit；final 产品 entry graph 未引用这四模块，probe 不作为音乐 UI 或设备运行资格。本轮包 unsigned/uninstalled，设备验收 **NOT_RUN**；实际音频 codec/声音、在线歌词、解密、封面、原件 GC、protected 宿主与完整 Flutter/Windows parity 均 **OPEN**。交付分支仅 `codex/ArkTsUI`，不并入主线。详见 [v29 验证](../reports/ui-source/v29/validation.md)。

以下 v28 及更早历史叙述原样保留；其中数字和“当前”只指各自当轮范围。文末音乐/分发当前表按 v29 更新，本轮资格以以上摘要及验证记录为准。

2026-10-09 **v28 当前分支检查点**：多行待办的纯行格式检查或剩余额度计数失败后，可显式“重新检查”。已派发的格式请求保留原 old/new、剩余额度及固定 wire；计数失败则在同一 owner、revision 和完整 raw 边界内显式重算；失败行独立保留，另一行成功不会清除其错误。同一完整选区回声保留重试资格，实际选区变化仍撤销原重试；迟到结果不覆盖较新原始输入。

最终实际 ETS/tool **922/922 PASS，0 fail/skip/cancel**（36 suite 文件、126 项输入前后一致，30,246.7672ms）；独立四 suite **161/161 PASS**、22 项运行输入一致，属于上述全量范围，不重复累计。最终完整 API26 **SUCCESS /17.582s**，34/34 tasks 执行，315 复制/381 仓库输入前后一致，四项包内原生库核对 PASS。最终 HAP **30,281,859B**，SHA256 `FF9C81704042FF053C2BCE6214E5AD56EDAD4A3906B2D66B06D2E9E273A1A2B5`。Rust/C++ 源码未变，双 ABI `.a` 复用冻结 v27，279 项 native 来源重新核对；本轮未新增 Rust 构建或测试。

版本保持 **dev19/1000019**。v28 包 unsigned/uninstalled，设备验收 **NOT_RUN**；本轮对既有 API26/x64 上已安装 v27 的标题/正文准确可见输入仅为 **PASS_SCOPED**，业务保存与重启恢复 **NOT_RUN**，见 [限定设备记录](../reports/ui-source/v28/device/validation.md)。音乐仅完成只读源码复用审计，无音乐产品源。完整 Flutter/Windows 对齐仍 **OPEN**，仅交付 `codex/ArkTsUI`，不并入主线。详见 [v28 验证](../reports/ui-source/v28/validation.md)。

以下 v27 及更早内容完整保留为历史；其中验证数字和“当前”表述仅指各自当轮范围。本轮资格以以上 v28 摘要及其验证记录为准。

2026-10-09 **v27 历史分支集成检查点**：基于 `4724f01f`，普通卡片新增从真实当前完整 format2 和迁移 Origin 验证得出的 `content_kind`。实际 Index 重新编辑先读最新全文 source，以 `current_v2` 保存正文并保留 TaskId、完成状态、顺序、退役身份、分类/阶段、收藏及未知字段；合法 `v2` / `legacy` 来源均使用该正文模式，旧 own LF 不授予新基线。当前完整子草稿重启恢复已接原严格 Session、固定计划、实际 S2 父历史及当前 S3 writer，读取不提交业务，退役/关闭仍显式核对，原 wire 和 Unknown 独立保留。

完整实际 ETS/tool **903/903 PASS**（36 suite 文件、125 项实际输入前后一致），SDK 窄修后 Recovery 子集 **143/143 PASS**，Native **191 library +3 binary PASS**（17 条默认条件 ignored）。新双 ABI release **PASS 并采用**，279 项 native 来源，ARM64 **57,055,756B /5329F277…**、x64 **55,469,756B /EB390E7F…**。fresh retry1 完整 API26 **SUCCESS /28.199s**，34/34 tasks 执行，315 复制/378 仓库输入前后一致，四项包内原生库核对 PASS；最终 HAP **30,267,399B /059504B9…**。版本仍 **dev19/1000019**，包 unsigned/uninstalled，新设备验收 **NOT_RUN**，完整 Flutter/Windows 对齐 **OPEN**；仅交付 `codex/ArkTsUI`，不并入主线。首轮 SDK 失败与模型旧文案失败/drift 保留为历史，不混作最终资格。详见 [v27 验证](../reports/ui-source/v27/validation.md)。

以下完整保留各轮交付时的历史记录；旧段落的“当前”仅指其当轮范围。

2026-10-09 **v26 当前分支基础检查点**：增加按真实当前全文 source/CAS 编辑 V2 正文的 `current_v2`，保留实际 TaskId、完成状态、顺序、分类/阶段、收藏和未知字段；增加固定父草稿/退役历史只读读取与模型当前子 writer 恢复。原固定请求和 Unknown 不变。真实 Store DTO 检查补修可选空游标省略时的回执解析，以及退役历史确认后仍停留在 Unknown 的状态。

完整实际 ETS/tool **869/869 PASS**，Rust **187 library +3 binary PASS**（16 条默认条件 ignored；新两份 DTO 导出与7个实际崩溃边界单独通过）；双 ABI release 和最终完整 API26 **SUCCESS /27.403s**，未签名包 **30,198,963B /90C3171D…**。版本仍 **dev19/1000019**；只推送 `codex/ArkTsUI`，不并入主线。**本次新增 current-child 重启恢复/current_v2 重开尚未接入 Index，设备验收 NOT_RUN，完整 Flutter/Windows 对齐 OPEN**。详见 [v26 验证](../reports/ui-source/v26/validation.md)。以下保留各轮历史范围。

2026-10-09 **v25 历史分支检查点**：主页面保存已接入持久原请求的 prepare/issue/save/inspect 和只读发现/恢复入口；已确认业务后，准确 S1 关闭原草稿，完整较新 S2/S3 通过 typed business source0 子草稿接续，再条件退役父草稿。Unknown 保留固定原 wire，首次 prepare 明确未写入时只释放该提案，完整输入保留。重启后已计划步骤只读恢复 native 固定 literal，需显式核对，不生成另一套操作。

本轮完整实际 ETS/tool **857/857 PASS**，Rust **177 library +3 binary PASS**（13 条默认条件 ignored，文档检查单独顺序通过），新 **48 个实际 Store 崩溃边界 PASS**；fresh 双 ABI 和 API26 完整产品构建 **SUCCESS /15.363s**，未签名包 **30,173,833B /9590D87D…**。详细结果见 [v25 验证](../reports/ui-source/v25/validation.md)。**新 native / 新页面流程设备验收 NOT_RUN**；父草稿已缺失或子草稿已推进后的页面重启接续、关闭后卡片重开并继续全文 owned todos、元数据/TaskId 编辑后的完整对齐仍 OPEN。未将局部检查计作完整 Flutter/Windows 功能等价。版本保持 **dev19/1000019**，只推送 `codex/ArkTsUI`，不并入主线。以下记录保留各轮历史范围。

2026-10-07 **v24 历史分支检查点**：普通字段恢复选区先核焦点与 lease owner；待办初建控件用一次性初始化能力确认原始显示回声，避免其被误报为未完整捕获。冻结 Index `AFF77EB7…` / Todo `83B6C6A5…`。API26/x64 的独立 UI 候选 **29,107,139 字节 / B62DBD23…** 恢复公开 D 草稿，准确读回 `first 汉字 🧪 é.` 并显示 **13/1000**；随后“保留草稿”成功关闭编辑器、草稿数仍为3，业务提交数为0，两个阶段限定 **PASS**。该候选复用旧 v22 native，不包含新 intent 后端；见 [实际阶段证据](../reports/ui-source/v24/device-fixed/progress-clipboard.json)与 [安装身份](../reports/ui-source/v24/device-fixed/installation.json)。此前 96C0、3116 和诊断候选的失败或未知结果保留，不以本次限定通过覆盖。

本轮另冻结独立业务 intent journal 与 `EditorBusinessSession` 基础：prepare 保留完整原 Submission/publication 和 pins，issue CAS 持久生成实际 save/inspect literal，分 part 原样读回后才显式保存；Unknown 只核对或重试固定原 wire，prepared 可条件取消、issued 不可取消。首请求签发后晚 S2/owner 失效可显式只读恢复后结算原 S1，不消费 S2。原生库 **165 PASS /11 default ignored**、附件二进制 **3 PASS**，新24个真实 Store crash 向量及完整 DTO 导出已分别执行；ETS Session **30/30 PASS**，含真实 Store issued/closed 全五 part DTO，独立 API26 public-API 编译 **PASS /9.480s**。见 [native审计](../reports/ui-source/v24/editor-intent-native-audit.md)、[Session审计](../reports/ui-source/v24/editor-business-session-audit.md)和 [隔离SDK](../reports/ui-source/v24/session-sdk/sdk-audit.md)。阶段/元数据回执不证明业务提交，development proof 不是 protected handoff 资格。

**本轮完整实际 ETS 模型807/807 PASS、0 skipped（11837.3834ms）；271项native来源构建前后一致，新ARM64/x64 release采用分别为56,517,238字节/E416EA7F…及54,925,386字节/A99CF367…。完整API26产品构建SUCCESS /13.893s，34任务fresh、313项完整复制、364项仓库输入构建前后字节一致，4项包内.so核对PASS；最终未签名HAP为29,381,667字节/F6EEEAE2D2175A7EA54DCEF582EA1BD5D15E324D48E65583B752775B4EC8925C，未安装。** 首轮隔离副本漏hvigor配置、13ms/0任务失败的日志和清单保留；仅补完整验证副本后在新retry1构建成功，生产源码未改。见 [v24统一验证](../reports/ui-source/v24/validation.md)。 新 Session 尚未接 Index 保存/恢复入口；业务 source0 子草稿交接、`saved_exact` 关闭、详情变更后重新编辑、新 native 设备运行及完整输入/保存/重启闭环仍 **OPEN**。SDK 或旧 native UI 候选不能代替这些资格。版本保持 **dev19/1000019**，只推送 `codex/ArkTsUI`、不并入主线；完整 Flutter/Windows 对齐仍 **OPEN**。以下 v23/v22 及更早段落保留各轮历史事实。

2026-10-07 **v23 历史检查点**：完整状态快照门禁避免混合 props 打断 Todo formatter，恢复选区回调按焦点及实际 controller 标记分类。755 模型、最终 API26 构建与 337 输入核对通过，最终 `935E1B2E…` 包设备 NOT_RUN；首个 `3116FBB8…` 候选恢复仍 FAILED_OR_UNKNOWN，保留关闭未执行。新原 wire 持久恢复/source0 交接仅为候选设计，未进入生产。完整追平目标保持 OPEN，见 [v23 验证](../reports/ui-source/v23/validation.md)；下文 v22 与更早记录保留历史事实。

目标：持续跟进 `01a085bd-7a94-7f93-8a1f-1ecf417f5ee3`，直至 HMOS 追平 Windows 侧 UI 与大部分实现。独立 `hmos/` 工程、Rust 复用、ArkUI/NDK 路线不变；只推送 `codex/ArkTsUI`，不合并主线。

2026-10-07 v22 历史增量为 [恢复门禁修复与严格业务基础检查点](../reports/ui-source/v22/validation.md)：恢复标志和 lease 挂载归属改为响应式状态，解除恢复后 Add 禁用；新 `editor_save` / `editor_commit_inspect`、单事务 `continued_todos` 和独立 ETS 协调器已实现。准确原请求/publication/历史 result、Unknown 同 wire、恒定 root/latest baseline 与全文未知字段保留已有专项/Store/ETS证明。**新基础未接 Index，业务 source0 子草稿交接、产品在途原 wire 跨进程恢复和完整目标仍 OPEN**。原 rawfork 不自动解除 source 冲突；[v21报告](../reports/ui-source/v21/validation.md)及更早证据不追改。

设备已安装独立 UI 候选 **96C0AB68… /28,799,011字节**（f9加两项状态修复、旧b486 native，新业务源未包含）。公开D恢复与Add空行限定PASS；首次文字输入仍FAILED_OR_UNKNOWN，之后独立恢复PASS准确读回`first 汉字 🧪 é.`。随后保留关闭仍FAILED_OR_UNKNOWN，因未完整捕获事件而停止清理，现编辑器打开且草稿保留，不声称关闭/业务保存闭环完成。最终fresh新native产品742模型/HAP9.653s/29,101,587字节/41688FDA…通过但未安装，候选设备结果不能替代；旧失败证据保留。

2026-10-07 此前交付的 [主页面 UI 集成源码检查点](../reports/ui-source/v20/integration/validation.md)：新卡多行待办与输入协调已接入 Index，粘贴/自动标题完整选区交接及迟到输入守卫已补。交付时版本号仍 dev19，新 UI 未设备验收；旧草稿清理成功后迟到输入的新身份持久接续尚未实现，完整目标 **OPEN**。此前 [dev20 源码检查点](../reports/ui-source/v20/checkpoint-status.md) 尚未接入 Index 的记录及其617项模型、123项Rust和实际Flutter/Dart对照保留为历史限定证据，以下记录同样保留各轮交付时事实。

2026-10-07 dev.19：字段按真实 Flutter `characters 1.4.1 / Unicode 16.0.0` 的 grapheme 规则计数，标题/传入待办字段/假设/结论/正文上限分别为 **60/1000/5000/10000/20000**。当前UI的待办字段仅为待添加单条输入，未复现Flutter全部已有行与待添加行的聚合1000和行间选区模型。粘贴先异步检查选区替换后的完整未来文字，再导入原件；确认 pin 后用实际资产引用重检，失败保留已确认附件。移除编辑控件的 UTF-16 `maxLength`，显示异步计数，超限完整输入保留；raw journal/IME 沿用既有结构与字节预算，业务保存和粘贴拒绝活跃 composing 或超限。待办重命名成功回执仅在原 owner、epoch 和完整输入一致且无 composition 时关闭编辑器，迟到候选继续保留。真实 Flutter 的 **1,198 组完整对照 PASS**，完整实际 ETS 模型 **535/535 PASS**，Rust **106 PASS**，三项默认条件 ignored 已单独 fresh 比较通过。最终 **0.1.0-hmos-dev.19 /1000019** 的 API26 未签名 debug HAP **SUCCESS /11.302s**，**28,164,478 字节**，SHA-256 `F7A913980CF3213536820727A677EEC04EA169FE2B65CE16E305F352D14BE4EC`；见 [dev.19 验证记录](../reports/ui-source/v19/validation.md)。**dev.19/dev.18 均未安装，当前设备仍 dev.17，新包设备验收 NOT_RUN**。完整 Windows/Flutter 对齐目标仍 **OPEN**。

dev.19 已改正旧 dev.18 字段按 UTF-16 计数的误限，并将待添加单条待办输入从500提高到1000；Flutter多行待办聚合和行间选区模型仍未齐。富转换输出也按 Unicode16 grapheme 检查。完整 worker/转换输出/序列化及共享业务字节预算仍独立生效，不保证 20,000 grapheme 一定可保存。直接输入保留超限，与实际 Flutter 默认 formatter 的自动截短行为存在明确差异；系统 IME、选区 affinity 和全篇连续选择仍未齐。独立 API26 双指测试工具已构建 main/test HAP，10 项工具模型通过，但未安装、未注入实际设备手势；不能计作图片手势验收，见 [字段审计](../reports/ui-source/v19/editor-field-policy-source-audit.md)与 [双指测试工具审计](../reports/ui-source/v19/image-multipointer-tester-audit.md)。

2026-10-07 dev.18 历史交付：剪贴板来源按真实 Flutter UTF-16 单元限制分档，修复中文/emoji 被旧 2 MiB 字节上限误拒，以及系统 Unicode RTF string 无编码标记导致的 ANSI 误读；图片基础手势修复焦点、边界、识别器接管和旧事件身份。最终 **0.1.0-hmos-dev.18 / 1000018**，完整实际 ETS 模型 **464/464 PASS**，Rust 主机 **98 PASS**，两项条件实际 Flutter 对照单独通过，ARM64/x64 原生重新构建。API 26 未签名 debug HAP **SUCCESS / 6.782 s**，**27,866,016 字节**，SHA-256 `910F3069B7979E5F8C6CF9E7DC770DF6E342725D493798DDE38F317524DB7DE2`。本轮为源码、模型与构建分支交付；**dev.18 未安装，设备验收 NOT_RUN**，设备仍为 dev.17。见 [dev.18 验证记录](../reports/ui-source/v18/validation.md)。完整 Windows/Flutter 对齐目标仍 **OPEN**。

dev.18 来源上限为 plain/HTML/XML **2 Mi UTF-16 单元**、RTF **8 Mi**，分别使用 **6,291,459 / 25,165,827 bytes** 包络；RTF 8 Mi 只对齐 Flutter 可达的 **无插件本地分支**，正式 RustStudioPlugin 仍统一 2 Mi 单元且插件请求 64 KiB，无异常 fallback。系统 Unicode RTF string 保存为确定性 **UTF-8+BOM serialization**，不声称原始 document bytes；ArrayBuffer 原件逐字节保留。孤立 surrogate 在编码前拒绝，合法 emoji/有意 U+FFFD 在读取层保留；native RTF 输出仍保守拒绝 U+FFFD，严格损坏编码/NUL/不支持 RTF 与 Flutter 宽松解码有差异。64 MiB/20 槽/sidecar 与草稿 pin 预算未放宽，该轮字段 grapheme 误限和待添加单条500限制已由dev.19改进；Flutter多行待办总量模型仍未齐；Flutter 200 MiB 附件容量仍未对齐。

2026-10-07 dev.17 历史交付：接入授权 PasteButton 的系统剪贴板快照，按实际提供的 plain/HTML/RTF/Spreadsheet XML、原始二进制图片/文件和 PixelMap 准备原件，复用 durable import/pin；HTML 内嵌图片仅在确认 pin 后替换为持久资产引用。一次性来源绑定剪贴板 changeCount 和编辑目标，URI 不进入文件选择器授权表、不持久保存或跨读取复用；先完整检查正文、槽位和预算，再逐项确认，未知导入保留原请求。原件、转换、图片提取和冻结选区插入已有源码与模型覆盖；**真实系统富内容粘贴、Office 提供者及保存/重启/导出闭环 NOT_RUN**。最终版本 **0.1.0-hmos-dev.17 / 1000017**，完整实际 ETS 模型 **402/402 PASS**，API 26 未签名 debug HAP 构建 **PASS / 9.467 s**；包 **27,815,034 字节**，SHA-256 `D9DECC46BB0E953BB56A4CDC5DBBB71D0B863E6380C273CC677B73C36689C03F`。301 项构建输入 disk 核对 PASS，staged 核对以最终日志为准，见 [dev.17 验证记录](../reports/ui-source/v17/validation.md)。本轮按用户要求只交付分支源码和构建，不新增设备流程。

dev.17 交付时的容量差距（来源字节限制已由 dev.18 改进）：原生转换上限为 2 MiB 原件字节，Flutter 来源上限按 2 Mi UTF-16 单元计；较大富格式原件可在附件预算内保留，但转换失败须明确提示。系统快照最多 20 条记录，展开图片/原件仍共用剩余 20 个 spool 槽和含 sidecar 的 64 MiB 预算，不能据此宣称 Flutter 200 MiB 附件容量等价。该轮字段UTF-16计数和待添加单条500限制已由dev.19改进，Flutter多行待办聚合/行间选区仍未齐；直接输入控件、IME、全篇连续选择、生产捕获证据链及 HUKS 保护仍未补齐。

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
| 字段字数、待办输入、完整未来粘贴 | Unicode16/formatter原对照保持，Index新卡多行1000/100行及create TaskId已接；v24 AFF普通字段恢复焦点门禁、83B一次性Todo初建能力已冻，B62旧native候选准确恢复公开D/13/1000与keep关闭限定PASS | 补新文字/IME/多行编辑和业务保存重启的设备闭环；SDK未交付事件/实时formatter/任意候选恢复/连续选择仍OPEN，恢复和保留关闭不代替保存验收 |
| 查询、排序、大库加载 | dev.8 已接原 Rust 计划、单次完整快照、完整属性搜索与稳定排序；去抖/待发合并/过期回包丢弃，真实失败与空结果分开 | 仍限定 256 张；需 UI 响应分页、大库内存/帧时和生产 guest/捕获资格。旧“最近添加”为反向 ID 顺序，不能宣称为创建时间排序 |
| 正式 Rust 宿主和存储会话 | 未接；当前为独立未封存开发库 | 提取平台会话接口，HUKS、稳定身份、单库所有者、审计/备份/恢复契约；禁止绕过原宿主非 Windows 拒绝规则 |
| 持久草稿、S1/S2、未知结果核对 | v25 原请求保存、S1 close、S2/S3 source0 business handoff 保留；v27 Index 已接实际父历史/当前完整子 writer 重启恢复，读取不提交业务或生成新操作，原 Session 已知结果及各项 Unknown/wire 保留 | 新 native 保存/关闭/重启设备闭环、protected 捕获、SDK 未交付事件及跨进程全局配额资格仍 OPEN；显式核对固定退役/close，不能复活 inactive parent |
| 严格业务、当前 V2 正文与 own 接续 | v27 ordinary CardView 已据真实 decode/Origin 分类，详情重新编辑读最新完整 source/revision 并接 current_v2，保 TaskId/元数据 bytes；active原Session仍按continued_todos接续。191库+3附件和实际两类来源/迁移/新修订Store检查通过 | 不投影 old own LF，不自动重基，不把独立 plan/child/retire/close 事务称为多对象原子；本轮事务实现未改、未重跑旧fault vectors；完整设备/任务编辑/产品资格 OPEN |
| 附件、剪贴板、Markdown/富文本、导入导出 | dev.19在dev.17/18快照/原件/富转换基础上将完整输出改为Unicode16 grapheme≤20000，回执绑定完整UTF-16/UTF-8长度和版本；完整未来选区粘贴异步preflight及pin后重检。原source限制、SHA、64MiB/20槽/sidecar保持。独立API26双指tester编译main/test HAP与10工具模型通过，未安装/执行设备手势 | 本轮Rust106和三项条件Flutter对照fresh通过；完整ETS/HAP/native身份见v19最终验证。系统富内容/Office持久闭环、dev19字段/IME与图片手势设备NOT_RUN。RTF插件仍2Mi+64KiB，无异常fallback；严格坏编码/RTF U+FFFD、直接输入formatter行为、更严格字节预算、200MiB容量、完整格式/惯性/GIF仍有差距 |
| 音乐、歌词、解密 | v30 实际 Index 音乐操作、播放列表、离线歌词全文/页脚；复用实际 Library/Files/Playback 与 Rust 曲库，八音乐模块进入实际产品图，后台/Unknown 停音、原请求恢复和已知部分清理有界覆盖 | 下一阶段验真实提供者/声音/seek/上下首/后台/中断/重启/FD与全部样式渲染；补 pre-begin/重启部分清理/在线/解密/metadata/封面/GC/protected/full parity，见[v30验证](../reports/ui-source/v30/validation.md) |
| 插件包、动态 UI、HTTP/服务/文件任务 | 未接运行期宿主 | 跟进 Windows 实现与合约，接 Wasm 执行/权限/资源预算/服务与任务控制；区分 Windows 平台实现和共享业务块 |
| 分发与资格 | dev20/1000020，仅codex/ArkTsUI。v30 final1118模型/151来源，API26 17.975s/34执行任务/323复制/366仓库输入及8模块emit/4包库核对PASS；31,413,203B/FB81FEF1… unsigned/uninstalled；283 native 来源与双ABI精确复用v29，本轮无新Rust测试/构建，见[v30验证](../reports/ui-source/v30/validation.md) | HDC target空且模拟器进程未运行，设备NOT_RUN；签名/ARM64运行/HUKS与完整对齐OPEN；a1 SDK失败及模型final来源drift保留，资格只授 final2 + a2产品包 |

每一轮更新源码观察与实际功能证据，保护 `shared/reference.json` 的冻结来源。上游在途代码不得未经审查直接覆盖。新版本截图、测试日志和构建输入哈希分别记录，旧图不能被当成新包验收。

远端推送失败时保留本地提交；必须读取远端分支确认是否收到，再决定是否重试。其他任务对 `main` 的更新不属于本任务操作，不重置、不覆盖。
