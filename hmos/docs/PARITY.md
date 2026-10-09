# 功能对齐与 Rust 复用检查

2026-10-09 **v33 当前检查点**：待确认任务在详情和现有卡编辑器均提供“确认已完成 / 确认未完成”，不再由普通勾选框推断方向。决定绑定原卡快照、实际 TaskId、CAS 与页面归属，未知结果保留原请求，迟到或不完整回执不能清除它。外观设置已接入独立 `AppearancePreferences`：读取失败禁写，保留未知 JSON 值的原 literal；保存必须通过原字节核对、put、flush 和准确读回。同 namespace 的跨页面在途调用保持屏障，未知结果须显式核对，再单独保存预览。

发布前完整实际 ETS/tool **1291/1291 PASS，0 fail/skip/cancel**，53 测试文件、171 输入前后一致；完整 API26 **SUCCESS /36.467s**，34 tasks全执行，325复制/368仓库来源精确核对。九语检查改用真实 Flutter 源固定fixture，默认不依赖本地build；12项无build隔离检查PASS，整套53文件的干净clone资格未验。字体导入本轮只有实际源/SDK设计审计，尚未实施。版本 **dev23/1000023**，HAP **31,564,598B /39F2E8A1…**，未签名、未安装。产品源/SDK/HAP保持不变，283 native来源和双ABI静态库精确复用v29，本轮无新Rust构建或测试资格。完整结果见 [v33 验证](../reports/ui-source/v33/validation.md)。

本轮设备观察实际安装的是已交付 v32 的 **dev22/1000022**、HAP **06342B5C…**，首页仍显示14张卡、4份草稿。新拟态首页对照确认额外内侧 raised 轮廓消失，搜索保留真实凹陷；七风格首页/设置观察单独记录。该旧包不授予 v33 新任务或外观持久化设备资格，也不证明所有页面未知框线已修复。真实音乐选择/授权/声音/seek/后台/重启、任务原请求重启持久化、全页面像素、第二外侧光阴影、签名/ARM64/protected 与完整 Flutter/Windows 对齐仍 **OPEN**。交付仅 `codex/ArkTsUI`，不并入主线。

以下 v32 及更早段落保留为历史；旧段落中的“当前”、安装状态和资格仅指其当轮范围。最新资格以上述 v33、当前表及验证记录为准。

2026-10-09 **v32 当前检查点**：按 Flutter `Glass` 的普通 depth=0 设计，移除普通面板及候选材质预览叠加的 raised 轮廓；搜索保留设计要求的凹陷，与音乐共用 `RecessedGlassRelief`。搜索使用自己的材质/depth/radius 与 -1 倍数，音乐内层使用全局色盘/depth/radius 与 -0.8 倍数，外层组件材质覆盖仍独立。主阴影按 SDK 物理 px 转换且 `fill=false`，候选预览按自身材质模式绘制。这是源码设计差异的修正候选，**不宣称所有界面未知框线已修复或完整 UI 追平**；第二外侧光阴影及父容器裁剪方案仍 **OPEN**。

完整实际 ETS/tool **1168/1168 PASS，0 fail/skip/cancel**，49 测试文件、156 模型输入前后一致；API26 **SUCCESS /40.621s**，34 tasks 全执行，324 复制/367 仓库输入精确，九项实际产品模块均检查并 emit（含 `RecessedGlassRelief`），四项包内原生库与本次 stripped outputs 核对 PASS。283 native 来源和双 ABI 静态库精确复用 v29，本轮无新 Rust 构建或测试资格。版本 **dev22/1000022**，HAP **31,442,946B /06342B5C…**，unsigned/uninstalled。

本轮实际安装并启动的是上一轮 **dev21/1000021**；DocumentPicker 两次进入 Download 后关闭，未观察到选中文件，尚无已确认的音乐导入或播放证据。该设备观察不授予 dev22 新绘制资格，也不能据此确定提供者缺陷。dev22 全界面/全样式像素、真实声音/seek/后台/重启验收均 **NOT_RUN**；在线歌词、解密、封面、原件 GC/protected、签名/ARM64 与完整 Flutter/Windows 对齐保持 **OPEN**。仅推送 `codex/ArkTsUI`，不并入主线，见 [v32 验证](../reports/ui-source/v32/validation.md)。

以下 v31 及更早段落保留为历史；旧段落中的“当前”、设备状态和资格仅指其当轮范围。最新资格以上述 v32、当前表及验证记录为准。

2026-10-09 **v31 当前检查点**：补齐音乐内层七种 Flutter 风格，当前曲目及选中行保持透明，flat 不增加框线；Index 传入实际色盘 surface。完整原导入请求须写入、fsync、关闭全部确认后才派发 Native begin，恢复保留原 IDs/CAS 并显式核对。同一页面的文件选择器往返使用一次性 ticket，返回前台后重读实际曲库；歌词目标变化时保留原目标下的完整原文。

完整实际 ETS/tool **1158/1158 PASS，0 fail/skip/cancel**，48 测试文件、153 输入前后一致；API26 **SUCCESS /33.904s**，34 tasks 全执行，323 复制/366 仓库输入一致，八音乐模块实际检查并 emit，四项包内原生库核对 PASS。283 native 来源和双 ABI 静态库精确复用 v29，本轮没有新 Rust 构建或测试资格。版本 **dev21/1000021**，HAP **31,438,704B /92258D5D…**，unsigned/uninstalled。

新版本设备验收 **NOT_RUN**；现有模拟器已安装上一轮 dev20，旧草稿全文读回和 dev20 启动仅为限定证据。真实 picker/grant、声音/seek/后台/重启、全样式像素与所有界面框线仍待验证；在线歌词、解密、封面、原件 GC/protected 和完整 Flutter/Windows 对齐 **OPEN**。仅推送 `codex/ArkTsUI`，不并入主线，见 [v31 验证](../reports/ui-source/v31/validation.md)。

以下 v30 及更早段落均为历史记录；旧段落中的“当前”、设备状态和资格仅指其当轮范围。最新资格以上述 v31 及当前表为准。

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

2026-10-07 **v23 历史状态**：待办恢复回调分类和完整 props 快照门禁已实现，755 模型与最终 API26 构建通过。最终 `935E1B2E…` 包未安装；首个 `3116FBB8…` 候选恢复仍 FAILED_OR_UNKNOWN，keep 未执行。Rust 复用未变的 v22 archives，业务接续仅新增候选设计，完整目标 OPEN。见 [v23 验证](../reports/ui-source/v23/validation.md)。下文 v22 及更早段落均为各轮历史范围。

2026-10-07 v22 历史增量：恢复标志 `draftRestoreInput` 与 lease `mountedOwner` 改为响应式状态，owner 挂载前不构建输入控件，解除恢复后的 Add 禁用。另实现严格 `editor_save` / `editor_commit_inspect`、单次事务 `continued_todos` 和独立 ETS 协调器；原 wire/publication/历史 Card 完整绑定、Unknown 同请求、恒定 root 与 latest baseline 已有有界验证。**新业务基础尚未接 Index，source0 子草稿交接/准确保存后全流程仍未实现，完整目标 OPEN**。本轮见 [v22验证](../reports/ui-source/v22/validation.md)，[v21界面验证](../reports/ui-source/v21/validation.md)与更早报告保留当轮事实。

v22 当轮设备为独立 UI 候选 **96C0AB68… /28,799,011字节**，只含 f9 加两项状态修复并复用旧 b486 native；不含新业务后端。API26/x64 公开 D 恢复及Add空行限定 PASS；首次准确输入 **FAILED_OR_UNKNOWN**（capture字段undefined、Back退出），后来独立恢复 **PASS**并读回`first 汉字 🧪 é.`。随后保留关闭仍 **FAILED_OR_UNKNOWN**，未完整捕获事件导致清理停止，编辑器仍打开、草稿保留；不填写完整关闭/业务保存闭环PASS。最终新产品 **742模型/fresh双ABI/HAP9.653s/29,101,587字节/41688FDA…**通过，尚未安装，不能由此UI-only候选代替。历史段落和报告保留各轮当时事实。

基线：用户指定主任务的 `build/io-safety-refactor` 实际工作树，test.54 + 未提交增量，2026-09-23。本表是迁移状态，不是主任务整体完成声明；其上游报告尚未关闭的资格项，在 HMOS 同样不能填写完成。

2026-10-07 dev.19：字段按真实 Flutter `characters 1.4.1 / Unicode 16.0.0` 的 grapheme 规则计数，标题/传入待办字段/假设/结论/正文上限分别为 **60/1000/5000/10000/20000**。当前UI的待办字段仅为待添加单条输入，未复现Flutter全部已有行与待添加行的聚合1000和行间选区模型。粘贴先异步检查选区替换后的完整未来文字，再导入原件；确认 pin 后用实际资产引用重检，失败保留已确认附件。移除编辑控件的 UTF-16 `maxLength`，显示异步计数，超限完整输入保留；raw journal/IME 沿用既有结构与字节预算，业务保存和粘贴拒绝活跃 composing 或超限。待办重命名成功回执仅在原 owner、epoch 和完整输入一致且无 composition 时关闭编辑器，迟到候选继续保留。真实 Flutter 的 **1,198 组完整对照 PASS**，完整实际 ETS 模型 **535/535 PASS**，Rust **106 PASS**，三项默认条件 ignored 已单独 fresh 比较通过。最终 **0.1.0-hmos-dev.19 /1000019** 的 API26 未签名 debug HAP **SUCCESS /11.302s**，**28,164,478 字节**，SHA-256 `F7A913980CF3213536820727A677EEC04EA169FE2B65CE16E305F352D14BE4EC`；见 [dev.19 验证记录](../reports/ui-source/v19/validation.md)。**dev.19/dev.18 均未安装，当前设备仍 dev.17，新包设备验收 NOT_RUN**。完整 Windows/Flutter 对齐目标仍 **OPEN**。

dev.19 已改正旧 dev.18 字段按 UTF-16 计数的误限，并将待添加单条待办输入从500提高到1000；Flutter多行待办聚合和行间选区模型仍未齐。富转换输出也按 Unicode16 grapheme 检查。完整 worker/转换输出/序列化及共享业务字节预算仍独立生效，不保证 20,000 grapheme 一定可保存。直接输入保留超限，与实际 Flutter 默认 formatter 的自动截短行为存在明确差异；系统 IME、选区 affinity 和全篇连续选择仍未齐。独立 API26 双指测试工具已构建 main/test HAP，10 项工具模型通过，但未安装、未注入实际设备手势；不能计作图片手势验收，见 [字段审计](../reports/ui-source/v19/editor-field-policy-source-audit.md)与 [双指测试工具审计](../reports/ui-source/v19/image-multipointer-tester-audit.md)。

2026-10-07 dev.18 历史交付：剪贴板来源按真实 Flutter UTF-16 单元限制分档，修复中文/emoji 被旧 2 MiB 字节上限误拒，以及系统 Unicode RTF string 无编码标记导致的 ANSI 误读；图片基础手势修复焦点、边界、识别器接管和旧事件身份。最终 **0.1.0-hmos-dev.18 / 1000018**，完整实际 ETS 模型 **464/464 PASS**，Rust 主机 **98 PASS**，两项条件实际 Flutter 对照单独通过，ARM64/x64 原生重新构建。API 26 未签名 debug HAP **SUCCESS / 6.782 s**，**27,866,016 字节**，SHA-256 `910F3069B7979E5F8C6CF9E7DC770DF6E342725D493798DDE38F317524DB7DE2`。本轮为源码、模型与构建分支交付；**dev.18 未安装，设备验收 NOT_RUN**，设备仍为 dev.17。见 [dev.18 验证记录](../reports/ui-source/v18/validation.md)。完整 Windows/Flutter 对齐目标仍 **OPEN**。

dev.18 来源上限为 plain/HTML/XML **2 Mi UTF-16 单元**、RTF **8 Mi**，分别使用 **6,291,459 / 25,165,827 bytes** 包络；RTF 8 Mi 只对齐 Flutter 可达的 **无插件本地分支**，正式 RustStudioPlugin 仍统一 2 Mi 单元且插件请求 64 KiB，无异常 fallback。系统 Unicode RTF string 保存为确定性 **UTF-8+BOM serialization**，不声称原始 document bytes；ArrayBuffer 原件逐字节保留。孤立 surrogate 在编码前拒绝，合法 emoji/有意 U+FFFD 在读取层保留；native RTF 输出仍保守拒绝 U+FFFD，严格损坏编码/NUL/不支持 RTF 与 Flutter 宽松解码有差异。64 MiB/20 槽/sidecar 与草稿 pin 预算未放宽；该轮字段 grapheme 误限和待添加单条500限制已由dev.19改进；Flutter多行待办总量模型仍未齐，Flutter 200 MiB 附件容量仍未对齐。

2026-10-07 dev.17 历史交付：接入授权 PasteButton 的系统剪贴板快照，按实际提供的 plain/HTML/RTF/Spreadsheet XML、原始二进制图片/文件和 PixelMap 准备原件，复用 durable import/pin；HTML 内嵌图片仅在确认 pin 后替换为持久资产引用。一次性来源绑定剪贴板 changeCount 和编辑目标，URI 不进入文件选择器授权表、不持久保存或跨读取复用；先完整检查正文、槽位和预算，再逐项确认，未知导入保留原请求。原件、转换、图片提取和冻结选区插入已有源码与模型覆盖；**真实系统富内容粘贴、Office 提供者及保存/重启/导出闭环 NOT_RUN**。最终版本 **0.1.0-hmos-dev.17 / 1000017**，完整实际 ETS 模型 **402/402 PASS**，API 26 未签名 debug HAP 构建 **PASS / 9.467 s**；包 **27,815,034 字节**，SHA-256 `D9DECC46BB0E953BB56A4CDC5DBBB71D0B863E6380C273CC677B73C36689C03F`。301 项构建输入 disk 核对 PASS，staged 核对以最终日志为准，见 [dev.17 验证记录](../reports/ui-source/v17/validation.md)。完整 Windows/Flutter 对齐仍 **OPEN**。

dev.17 交付时的容量差距（来源字节限制已由 dev.18 改进）：原生转换上限为 2 MiB 原件字节，Flutter 来源上限按 2 Mi UTF-16 单元计；较大富格式原件可在附件预算内保留，但转换失败须明确提示。系统快照最多 20 条记录，展开图片/原件仍共用剩余 20 个 spool 槽和含 sidecar 的 64 MiB 预算，不能据此宣称 Flutter 200 MiB 附件容量等价。该轮字段UTF-16计数和待添加单条500限制已由dev.19改进，Flutter多行待办聚合/行间选区仍未齐；直接输入控件、IME、全篇连续选择、生产捕获证据链及 HUKS 保护仍未补齐。

2026-10-07 dev.16 历史交付源码 `0.1.0-hmos-dev.16` / `1000016`：修复实际 dev.15 中心单击无法显示媒体控件的问题，隐藏控件阻止整个子树命中，手势/控件节点按原预览令牌重建。实际 ETS 模型 **303/303** 与 API 26 HAP 构建通过；最终未签名包 SHA-256 `A971227AC2D39730C2228972B513DBAAB4049E1C9E9AAB49397517BFB0208C7C`，24,857,891 字节。新包安装和版本读回通过，恢复同一公开媒体草稿的 WAV/MP4 两个 pin；WAV 原生准备两次为 0:00/0:12、无自动播放，中心单击后控件实际可见的 PNG 已复核，播放从0:01推进至0:03、暂停后两次0:06通过。首次全屏实际进入横屏，驱动裁剪断言停止使阶段为 **FAILED_OR_UNKNOWN**，只读横屏状态核对通过；首次Back驱动在动作前停止，修正驱动后fresh Back恢复竖屏/正常系统栏及同0:06预览、关闭回同编辑器两份原附件ID/名称通过，PNG已复核。本次按用户要求收束为媒体点击修复分支交付；seek/全屏手势、视频/系统预览和多选/空标题新设备闭环未跑，不据旧包结果填写通过。见 [dev.16 验证记录](../reports/ui-source/v16/validation.md)，完整目标仍 **OPEN**。

dev.16 后续媒体观察单独记录：同一已安装 `A97122…` 包的 MP4 0:00/0:12 准备且无自动播放、播放时间推进、暂停稳定、横屏全屏、左右双击 ±10 秒/水平拖动、普通进度拖动和 Back 返回竖屏已有真实限定 PASS。初次 Back 驱动错误仍保留；Home 尝试后仍在应用内，后台生命周期为 **NOT_QUALIFIED**。这些结果不证明 dev.17 设备运行，也不证明真实音频输出、完整格式矩阵或系统文件预览，见 [后续限定验证](../reports/ui-source/v16/follow-up-validation.md)。

dev.15 历史源码交付 `0.1.0-hmos-dev.15` / `1000015`：文件选择器按剩余资产/完整暂存记录额度多选、单批最多 20 个，全部 URI 按顺序使用独立操作身份导入；原有 spool/request journal/draft pin 状态机继续负责每项确认和 Unknown。确认成功后仅填仍为空白的标题，与 pin 同次保存；清理等待后再次核对原编辑器，加入附件绑定原 card/draft，编辑器更替清空旧进度。媒体增加控件显隐/3 秒隐藏/交互保持/300 ms 过渡、全屏水平拖动和左右双击 ±10 秒及累计反馈，旧令牌和计时器不能操作新媒体。实际 ETS 模型 **303/303** 与 API 26 HAP 构建通过；最终未签名包 SHA-256 `7347C8A8E2136D33A888BDD606766D788A6729633D7449A4BBA16F9CF8624A4B`，24,856,916 字节。交付时 **dev.15 设备验收 NOT_RUN**。该轮在旧 dev.14 / `43E402…` 包上完成 WAV 准备/无自动播放/播放暂停/进度/全屏返回/关闭基线，视频仅导入成功、播放未跑，PDF未跑；这些不证明 dev.15 多选或新控件实际运行。后续实际 dev.15 的单击缺陷另存 [device-current](../reports/ui-source/v15/device-current)，不追改原 [dev.15 验证记录](../reports/ui-source/v15/validation.md)。

2026-10-07 dev.14 源码接入附件音视频 AVPlayer/XComponent、无自动播放、播放/暂停、进度与时长、后台暂停及原生全屏窗口恢复。普通文件接入 PreviewKit 系统预览，准确绑定原业务修订或草稿 pin，窗口关闭后仍保留文件，由用户显式结束查看再清理；未知系统派发结果不重放。全屏恢复和播放器释放失败分别保留原状态供显式重试。该轮版本 `0.1.0-hmos-dev.14` / `1000014`；实际 ArkTS 模型 **226 项**和 API 26 HAP 构建通过，最终包已在既有 x64 模拟器安装并恢复同一公开文字草稿。音频选择在 Download 浏览后返回应用、未找到精确测试文件，未选中附件，驱动断言终止；复查同一草稿原文/零附件仍保留，业务未发布。不据此推断确定的选择器缺陷；原生解码、seek/全屏、音频输出及 PreviewKit 可读内容仍 **NOT_RUN**。Rust/原生实现本轮未变，旧 72 项 Rust 结果不计为本轮新跑。完整控制手势及任意文件默认应用打开仍未取得等价资格，见 [dev.14 验证记录](../reports/ui-source/v14/validation.md)、[媒体平台审计](../reports/ui-source/v14/platform-media-audit.md)及 [文件打开审计](../reports/ui-source/v14/platform-open-audit.md)。

2026-10-07 dev.13 补齐按当前附件解析的 Markdown 图片、读取/解码状态、图片缩放/平移/复位及编辑器附件行导出。Rust 主机 72 项、实际 ArkTS 模型 163 项和 API 26 HAP 构建通过；既有 x64 Pura X View2 实际显示中文名称引用的图片，按钮 125%/100% 复位与编辑器 pin 导出逐字节一致通过。测试卡保存后的表格图片、缺失/远程提示和重启无新增草稿通过；中断查询按已保存卡身份核对，没有重复保存。移除图片后保存前即显示缺失提示，显式保存/重启保持同一卡身份、原始 Markdown 不变、零附件且无剩余 raw journal。损坏图片实际解码、双指/平移手势和 GIF 动画仍待验收；双架构原生库复用 dev.12 字节，不是本轮新 ARM64 构建证据。完整范围见 [dev.13 验证记录](../reports/ui-source/v13/validation.md)。持续追平目标仍开放。

2026-10-05 dev.12 已实现文件选择器附件、原 durable import/schema、私有 journal/pin、按修订导出及图片/GIF 预览。最终Rust主机72项、实际ArkTS模型119项通过；另外完成x64原生附件/FD与实际选择器、恢复/预览、保存/导出、取消/单引用移除验证。各项按主机、原生runner及实际UI分开记录，不扩大为生产或真机资格。见 [dev.12 验证记录](../reports/ui-source/v12/validation.md)与 [附件主机验证](../reports/ui-source/v12/attachment-host-review.md)。

| 功能块 | 现有来源 | 本次 HMOS 状态 / 验证边界 |
|---|---|---|
| Protobuf/LZ4、完整记录/未知字段、SQLite 事务、CAS、原操作查询 | `core` | 原样快照并编译 ARM64/x64；设备侧自检覆盖建卡、重启、幂等和 CAS；不是全核心平台回归 |
| 卡片 V2、类别/阶段、收藏、删除与限时撤销 | `plugins/workbench/cards_v2.rs` | 直接复用；UI 接入；共享 5 项专项测试在 Windows 通过，设备 UI 建卡保存通过 |
| TaskId 待办、同名独立、历史歧义 | `plugins/workbench/tasks_v2.rs`，Flutter `versioned_task_panel.dart` | v33 详情/编辑器对 completion=2 显示 help_outline 和两向明确决定，普通0/1沿用真实Checkbox bool回调；原TaskId/source/CAS/owner和完整原输入约束准入，Unknown保留固定wire，迟到owner和坏receipt/DTO不装入结果。18项新Task源执行及既有回归126项限定PASS，最终全量1291项另验；九语实际设计查找已对照。新Task设备、窄屏/glyph、真实业务和重启持久intent仍NOT_RUN/OPEN；正式迁移、任务拖动与文字复制未由本轮补齐，dev.4/dev.11历史资格独立保留 |
| 字段字数、完整选区粘贴与业务准入 | Flutter `main.dart` / `tip_list_editor.dart` / `characters 1.4.1`，`editor_field.rs` / `editor_input.rs` | Unicode16/Windows formatter原真实对照保持，Index新卡多行1000/100行及一次create真实TaskId保持。v24 AFF普通字段焦点门禁与83B一次性初建回声能力修复已冻；B62旧native UI候选恢复`first 汉字 🧪 é.`并显示13/1000、keep关闭各限定PASS，业务提交0。新的文字/IME、多行编辑、业务保存/重启全闭环未由该流程验收；SDK实时输入、全篇连续选择与既有V2全文编辑仍OPEN，见 [实际阶段](../reports/ui-source/v24/device-fixed/progress-clipboard.json) |
| 搜索和查询 | `query_v2` / `workbench_host/query_plan_v2.rs` | dev.8 接入单次完整 WAL 快照、原分页筛选/排序归并及 128 项 / 64 KiB 真实帧预算；标题/正文/假设/结论/附件名语义、UTF-16 和稳定收藏排序复用原实现。ArkTS 去抖、串行/coalesce、旧回包丢弃和显式失败；开发库仍 256 张，无 UI 响应分页、大库资格和生产 guest/捕获证据。回收站为独立既有视图 |
| 阅读详情、卡片菜单、每页手动排序 | Flutter `openIdea` / `component_menus` / `card_order_preferences` / `hold_reorder` | dev.11 接原生 Markdown 阅读、显式进入草稿编辑、分类/阶段/TaskId勾选、收藏/复制/确认删除；卡片正文长按与菜单提供前后移动、转项目和组件设置。五页独立手动开关/顺序，筛选隐藏槽保持、保存失败回滚、过期视图与外部拖动拒绝。独立拖动柄采用鸿蒙原生手势；完整边缘自动滚动、鼠标/键盘菜单和详情任务编辑矩阵待补；不会写业务卡来存排序 |
| ArkUI 原生节点 | 新 `entry/src/main/cpp/bridge.cpp` | 实际 NativeNode Column/Text 外观预览，NodeContent 挂载和销毁；编辑控件/导航为 ArkTS，非全 NDK UI |
| ArkTS ↔ Rust | 新 N-API async work + Rust C ABI | 异步执行、串行准入、长度/UTF-8 校验、配对释放；不是 IPC 隔离或插件授权通道 |
| 正式工作台宿主 | `workbench_host` | 已检查 API 与平台边界；未整体复制/接入，非 Windows 开库仍拒绝；新适配器不能冒充其内容/任务证据链 |
| 密钥、身份固定、审计封存、库管理、备份/恢复 | `audit` + `workbench_host/storage.rs` | Windows DPAPI/租约后端不可照搬；HMOS HUKS 保护、库所有权、备份资格待实现。核心验签可复用不等于密钥管理已适配 |
| 宿主持久草稿、S1/S2、附件导入暂存与恢复 | 原 editor_draft 模型，独立 development fork/intent/business handoff | v25 原请求保存/typed incoming15-outgoing16 source0 handoff 保留；v27 Index 已接原严格 Session 的固定计划、实际 S2 父历史和完整当前 S3 writer 重启恢复，父缺失/退役不构造 active parent，先确认当前子再显式核原退役/关闭。原 wire/Unknown、rawfork13/14、pins及16活动/256累计/64MiB预算不放宽。新 native 设备、protected S1/S2、未交付 SDK 事件保全和跨独立进程全局配额原子性仍 OPEN，见 [v27 验证](../reports/ui-source/v27/validation.md) |
| 严格业务保存、历史检查与当前 V2 正文/own 接续 | editor_business / editor_intent / editor_handoff，ETS Business/Session/Handoff/Recovery，复用真实 Core Store/transaction | v25 完整 publication/prepare/issue/save/inspect、准确 S1 close 与 S2/S3 child 保留；v27 已接固定原 Session 当前完整子 writer 恢复，以及关闭后或元数据/TaskId 变更后的最新完整 source current_v2 正文重新编辑，保真实任务/状态/顺序/退役身份/元数据 bytes，不把旧 own LF 授为新基线。Unknown 保固定 wire，known commit 不因坏 DTO/刷新失败清除，计划/退役/close 各自独立。全篇 LF 任务编辑语义、protected 资格与新设备闭环仍 OPEN，见 [v27 验证](../reports/ui-source/v27/validation.md) |
| 附件原件、文件选择器、媒体预览/导出 | `core/attachment`、原 `editor_draft_staging.rs`，Flutter `attachments/*` | 保留 dev.12 exact URI/native FD/SHA256、durable import/pin/一致性保存/按修订导出，dev.13 图片与内嵌图/行导出。dev.15 多选按顺序逐项确认，最多 20 个且不超已有资产和完整暂存记录的剩余位置；空标题仅在成功导入后补确认文件名，取消/失败/Unknown/owner或世代变化停止后续且保留原请求。媒体保留 dev.14 无自动播放/后台暂停/释放顺序/原窗口恢复，dev.15 补控件显隐、全屏水平 seek 与左右双击 ±10 秒；dev.16 修复隐藏子树命中和令牌更替后的手势/控件节点身份。普通文件仍走 PreviewKit 与显式结束查看清理，未知派发不重放。准备/pin 64 MiB 未放宽；一个媒体对话框和一个系统预览各 200 MiB，内嵌图 8 个/64 MiB。dev.16 303 项模型/HAP通过，安装/同草稿两pin恢复、WAV 0:00/0:12无自动播放与单击显示已有实际证据；播放时间推进和暂停后0:06稳定通过；首次全屏阶段FAILED_OR_UNKNOWN，横屏当前状态只读核对、fresh Back恢复竖屏和正常系统栏、关闭回原两附件编辑器通过，初次Back驱动为preaction FAILED。dev.16 交付时多选/空标题、全屏手势、视频播放/PDF/真实音频输出未取得资格；后续 MP4 的准备/播放暂停/横屏/seek/Back 限定 PASS 单列于 follow-up-validation，后台仍 NOT_QUALIFIED。dev.17 未新增设备验收。旧 dev.14 WAV 基线单列。完整格式、任意默认打开、图片双指/损坏/GIF、拒权/空间耗尽仍待验收或补齐。纵向音量/亮度需当前 Flutter 参照未传的回调，不是已默认启用的目标 |
| Wasm 插件解释器与包管理 | `plugin_runtime` + `sdk/rust` | SDK 随业务模块复用编译；运行期宿主权限、worker、动态包审批/UI 渲染未接入。未声称 Rust 原生直调等于 Wasm 隔离运行 |
| HTTP/服务/TLS/凭据 | `network_node`, `workbench_host/*control`, `io_tasks` | 审查依赖与平台边界；本次未连接外部服务，未编译/运行完整网络节点；后台任务、权限、HUKS/TLS 适配待实现 |
| 捕获/转换、富文本、表格/RTF | 原 capture 转换块，Flutter `clipboard_import.dart` / `office_clipboard.dart` / `rich_content.dart` | 保留dev.17授权快照/原件/内嵌图与dev.18来源分档、完整SHA和RTF string UTF8+BOM serialization。dev.19完整输出改为Unicode16 grapheme≤20000，回执绑定完整UTF-16/UTF-8长度和版本，无旧回执fallback；converter/output JSON各512KiB仍独立生效。正式Flutter插件RTF仍2Mi+64KiB且无异常fallback，严格坏编码/RTF U+FFFD、DOCX/XLSX/OLE包转换与捕获票据资格未等价；64MiB/20槽/sidecar不放宽。旧dev.18条件对照本轮在final Rust fresh重跑，真实系统富格式/Office提供者/原件图片持久闭环NOT_RUN，见 [dev.19字段审计](../reports/ui-source/v19/editor-field-policy-source-audit.md)与 [dev.18来源审计](../reports/ui-source/v18/clipboard-capacity-audit.md) |
| 歌词/媒体与格式解密 | Flutter music_panel/little_tips + Native 独立曲库 | 保留v31完整原wire持久确认、一次picker ticket及原目标歌词，v32搜索/音乐共用七风格凹陷。v33源/完整模型/产品构建保持；本轮实际安装旧dev22，仅首页/设置绘制观察，不是音乐导入/播放资格。旧dev21两次Download后picker关闭保留为历史，不能断定提供者缺陷；真实选择/grant/codec/声音/seek/后台/上下首/中断/重启、第二外侧光阴影/父裁剪、在线/解密/metadata/封面/GC/protected/full parity OPEN，见[v33验证](../reports/ui-source/v33/validation.md) |
| 语言、字体、主题、玻璃效果、稳定瀑布流 | Flutter `morrow_i18n`, fonts/layout/shaders，实际Windows Rust `ui_preferences.rs` | v33Task三条文案的27项期望取自18份真实ARB，默认固定fixture/显式live核对各12项PASS；无build隔离copy7输入/3Node/13读、外部0，限定证明12项可运行，整套53文件cleanclone未验。UiStrings字节未改，发布全量模型1291/171输入另验。字体只有31项实际源/SDK/官方本地文档设计审计：Rust FontPreference/global checked loader可复用，原件/选择/注册/恢复尚未实施；完整九语动态文案、字体/背景文件导入、真实glyph/宽屏设备仍OPEN |
| 外观偏好读取、完整保存与未知结果 | actual Index、AppearancePreferences、ArkData Preferences；Flutter storageReadFailed与原Rust未知字段合同 | v33实际Index接studio-appearance/appearance-v1。正向has确认缺失；坏/空/非string或读取失败禁写；完整候选与confirmed current分开，未知JSON值literal保留。原raw preflight→put→flush→exact readback，同namespace跨页live屏障；终止Unknown必须显式recover，不put/default/replay，再单独save。61模型/32真实Index及独立6交接向量限定PASS，最终全量含本项。不是跨进程CAS或崩溃journal，真实ArkData/设备重启/断电NOT_RUN |
| 七种风格、立体深度、组件材质跟随 | Flutter `appearance.dart`, `component_material_page.dart` | v32普通Glass额外raised轮廓移除、候选自身模式与主阴影物理px/fill=false在本轮旧dev22首页/设置观察；新拟态前后图片确认额外内侧轮廓消失且搜索凹陷保留。七风格PNG/实际UI树单独记录，不能关闭全部界面框线/主题/宽度像素资格。v33没有新paint改动；第二外侧光阴影/父裁剪、完整像素/动画/shader仍OPEN。dev.5历史资格独立保留 |
| 工作区布局与位置 | Flutter `workspace_viewport.dart` / `stable_masonry_grid.dart` / `render_stable_masonry_grid.dart` | dev.7 改为原生 LazyVWaterFlowLayout + LazyForEach；页面/卡片身份稳定，修订内容和移动位置分别失效，保留 16 张交错记录验证。dev.8 最终包新增 440/744 vp 单/双列、四卡间距/排序/遍历与查询修订刷新；dev.6 保留 880/1488 vp 面板与五页位置恢复证据。完整主题、键盘/动画及内存/帧时资格仍未完成 |
| 图片基础手势 | Flutter `attachment_view.dart` / 实际安装 `InteractiveViewer` | dev.18修复焦点、单指/纯双指pan、pinch接管、tight边界与cancel/旧事件身份，30/30实际ETS/组件模型PASS；有效手势缩放1–2.5，按钮/双击复位为既有HMOS补充。dev.19新增独立API26真实PointerMatrix/injectMultiPointerAction测试工具，main/test HAP编译及10工具模型PASS，未安装、未执行设备手势；不增加生产手势或图片渲染资格。惯性/fling/scale-velocity未实现，GIF/损坏格式未验。见 [生产手势审计](../reports/ui-source/v18/image-gesture-source-audit.md)与 [工具审计](../reports/ui-source/v19/image-multipointer-tester-audit.md) |
| 编辑器文字与预览 | Flutter `main.dart::_bodyEditor` / `idea_markdown.dart` | CommonMark/准确修订pin、590vp分栏与内嵌图8个/64MiB保持；v27 Index 已接当前完整子恢复及关闭后最新 source 的 current_v2 重开，真实 CardView 分类 v2/legacy 均以当前 format2 全文编辑，LF todos 为空，保 TaskId/完成/顺序/退役身份/分类阶段收藏/未知字段；old own LF 不授新基线，active原Session接续独立。B62旧native恢复/keep不替代新保存闭环，真实新输入/IME/连续选区/富文本与设备验收仍 OPEN |
| 平台分发 | DevEco API26 | v33/dev23/1000023，只交付codex/ArkTsUI、不并main；发布前1291/1291模型、53测试文件/171输入一致；产品/SDK/HAP未变，API26 SUCCESS36.467s/34执行任务/325复制/368仓库来源精确核对。HAP31,564,598B/39F2E8A1… unsigned/uninstalled，见[v33验证](../reports/ui-source/v33/validation.md)。283 native来源/双ABI静态库精确复用v29，无新Rust构建/测试；新Task/Preferences设备NOT_RUN。本轮实际安装旧dev22并观察首页/设置，签名/ARM64运行/HUKS/full parity OPEN |

## 后续顺序

2026-09-27 跟进范围见 [dev.4 验证记录](../reports/ui-source/v4/validation.md)。原始共享快照保持固定；上游最新文件创建/删除、服务 SDK 尚未整体移植；dev.5 另行移植基础面板风格与材质跟随（[验证记录](../reports/ui-source/v5/validation.md)）。上游 Windows Core/runtime 的测试数字不是 HMOS 验收证据。

1. 从正式宿主提取平台存储会话接口；实现 HUKS 受保护密钥、稳定日志身份和单库所有者，再通过与 Windows 相同的 Store/封存/恢复契约验证。禁止用当前试验库直接替换正式库。
2. v27 开发源码已接严格原业务 source0 子草稿/祖先约束、条件父退役、固定在途 wire、原 Session 当前完整子恢复及 Index 最新 source 的 current_v2 重新编辑。后续需验新包的实际输入、保存/接续/关闭/进程重启/Unknown 设备闭环和正式 protected 权限；development marker 不代替 captured S1/S2/protected 授权，不自动重基，不将 kind1 rawfork 冲突当最终接续或将独立事务称为多对象原子。
3. 补齐Flutter多行todos映射/总量并验收dev.19新包字段计数/IME/完整粘贴、系统富剪贴板提供者、真实来源容量/严格编码、原件/正文/图片pin的保存、重启、预览、导出与移除闭环，以及图片基础手势。独立双指工具须先验provider/身份/新鲜坐标再注入，并实际复核PNG；工具构建/模型不代替设备。dev.16 MP4后续限定证据、dev.17运行与dev.18源码交付分别保留。多选/空标题、后台、真实音频输出、系统预览、完整格式、图片损坏/GIF/惯性、任意默认打开及完整Office仍待验收或补齐。继续接插件和动态UI，保持对象授权和捕获证据边界；完整资格仍OPEN。
4. 对齐设置、语言/字体、工作区布局、网络/TLS/后台生命周期。按功能记录 ArkTS、Rust、NDK 和真实设备证据，不能只看编译成功。
5. 签名后的 ARM64 真机测试、进程终止/重启/前后台/权限撤销/空间耗尽/并发开库/损坏拒绝矩阵通过后，再讨论与 Flutter 的功能等价验收。

本次 NativeNode/N-API 路线依据 SDK 实际头文件，并核对 [华为 ArkUI NativeModule](https://developer.huawei.com/consumer/cn/doc/doccenter-references/api/capi-arkui-nativemodule) 与 [Node-API](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides-V5/napi-introduction-V5)。Rust 平台支持参见 [官方平台支持表](https://doc.rust-lang.org/rustc/platform-support.html)。实际资格以本目录报告为准。
