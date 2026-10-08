# 后续编码看板

<!-- C28-CURRENT-BEGIN -->
## 当前开发检查点（2026-10-09，C28 离线工具与恢复准备）

本次保存六份通用工具原字节源码、验证摘要与看板，应用版本和运行逻辑不变。捕获 10 项及同字节加载 1 项合成检查分别退出 0；禁用恢复候选完成有限独审，四份 PowerShell 5.1 源码解析无错误，七个命令参数组合核对通过，未执行脚本正文或恢复。

registry 缓存 12 项合成检查通过，toy Cargo 离线锁定 metadata 退出 0；Git PACK 31 项通过。公开副本在新目录分别复验相同 12/31 项且源码无漂移，重复不累计。Git HTTP 后继 36 项 mock 通过，最终独审待完成，未发布该后继源码。真实依赖获取、安装、TLS/Git 接入和 Git 缓存采用未验收，不能解除底层 12 项测试阻塞。

H011 仍仅隔离构建通过、程序未执行；公开完整构建 NOT_RUN，三个 Wasm include 路径尚待接线。历史 Native Start 仍 Unknown/BackendError，原 owner 的正常退出、EOF、ACK、cleanup/join 与资源债务 pending，不自动重放。Agent 会话层、安全执行层、SDK26/G04 继续 OPEN，SDK 未冻结，release_eligible=false。

下一步完成固定依赖、Git 缓存与恢复运行资格，再做 Windows 生命周期复验；达到两层验收后暂停准备预览，扩展执行层不是暂停前提。见 [当前结果与范围](../reports/reconstruction-2026-10-09/windows-agent-sdk-c28-offline-tools.md)和 [通用工具](../companions/morrow-codex/qualification/offline-preparation/README.md)。下方保留各历史时点。
<!-- C28-CURRENT-END -->

<!-- C16-CURRENT-BEGIN -->
2026-10-06 C16 本地实验性候选：新增可信 Rust Agent start/submit/poll/cancel/recover/acknowledge
入口，移动完整原 owner，沿用同 Core/Store/runtime/连接；每 import 维护原 owner。
借用执行与 scheduler 回收有界；真实 join 后仅同步唯一 Runtime 所有权可回收，别名保留 16 有界债务。
普通无沙箱、非 ProtectedSession 的封存 Rust Wasm＋实际 Windows worker 耦合 3 项通过；各组收据见报告，
不作为生产 GUI、真实受保护库/DPAPI 或生产沙箱资格。旧 SDK327/57、合同、Linux 与 C15 工件守卫通过。
接口仍 experimental，SDK26/G04 OPEN；未提交、推送、Release 或 CI，不代表正式 SDK 冻结。
见 [原生 owner 接口](PLUGIN_AGENT_NATIVE_OWNER.md) 与 [C16 报告](../reports/reconstruction-2026-10-06/codex-sdk-c16.md)；下方 C15 及更早内容保留其历史范围。
<!-- C16-CURRENT-END -->

<!-- C15-CURRENT-BEGIN -->
2026-10-06 C15本地候选：工作台完整会话插件管理已接独立admin协议/owner与设置界面，
借原Manager/持久catalog，保留原busy/lost/恢复门禁和顶层native管道，不改旧Core/IO。
admin10、owner18、旧host18/14/13/1回归分别通过；Dart18（含实际Rust五对向量互验
和原管道Busy路由）通过。新库strict Clippy、Dart fatal-infos/限定格式和Windows宿主check
通过；不是新OS执行/真实桌面/ProtectedSession资格。327/57/旧合同/Linux/封存工件保持。
实际native port借原runtime/worker、生产GUI、认证/sandbox、交互控制及全SDK26/G04仍OPEN。
没有commit/push/Release/CI。详见 [C15报告](../reports/reconstruction-2026-10-06/codex-sdk-c15.md)；下方C14及更早内容保留其历史范围。
<!-- C15-CURRENT-END -->

<!-- C14-CURRENT-BEGIN -->
2026-10-06 C14本地候选：完整Agent wrapper持久catalog/approval接口已实现，原Manager撤销seam
和同原connection/freshadmission接线保持。新catalog18、Manager新3与其原6、原回归11/1/6、
旧managed/route/schema14/13/1分别通过；实际Windows新1通过（helper1过滤，sandbox=None）。
大包装不受原快照512KiB限制，static审批不恢复livegrant；保存故障/Unknown即时撤旧Core，
旧效果不重放。327/57/旧合同/封存工件保持；Workbench新协议/GUI、ProtectedSession/native
owner、认证transport、sandbox、交互控制/其他平台及SDK26/G04仍OPEN。未commit/push/Release。
详见 [C14报告](../reports/reconstruction-2026-10-06/codex-sdk-c14.md)；下方C13及更早文字为各日期历史，不替代本条实际范围。
<!-- C14-CURRENT-END -->


<!-- C13-CURRENT-BEGIN -->
当前 C13（2026-10-06，本地未提交）：会话/进程新入口已接原 Catalog/Registry/Manager，
共用原实例限额、Control 与连接；基础包与完整 wrapper 仍独立批准，预算取交集。
新增 managed host 14、Manager 6、HostIdentity 3、真实 Windows managed Wasm 2 分别通过；
原 host 1+13、Manager 11+1、strict 6、process 18+5 回归另计，重复不累计。
修正提交前取消、effect 后 Unknown 与错配收尾；新 host strict Clippy/限定格式通过。
旧 327 SDK／57 冻结输入、wire/schema 与封存工件保持；新候选不继承旧冻结 pin。
完整 wrapper 持久审批、Workbench/ProtectedSession owner、sandbox/认证/其他平台仍
OPEN/NOT_RUN；SDK26/G04 未冻结。见 [C13 接线与实际边界](../reports/reconstruction-2026-10-06/codex-sdk-c13.md)。
下方 C12 及更早检查点保留历史结果与当时身份；本轮现状以 C13 为准。
<!-- C13-CURRENT-END -->

<!-- C12-CURRENT-BEGIN -->
当前 C12 接口增量（2026-10-06，本地未提交）：已补会话 Rust/Wasm 客户端、类型化进程控制、
严格 single-import 组合运行入口和原 R2 执行权实时复验。process 合同 23、R2 客户端 10、
旧入口真实 Wasm 6、process 客户端 10、组合 host 14、运行入口 6、定向 runtime 回归 21、
原 R2 回归 106 分别通过，重复方法不累计。Windows 原生实际执行 7 项、纯注册表 3 项、真实 proposal Wasm→Windows→process Wasm 耦合 3 项分别通过；封存 Wasm 未重建，Unknown 不重放。
327 SDK／57 冻结输入及旧封存归档保持；新候选不继承旧冻结 pin。
原生 close-input／PTY resize 尚 Unsupported；完整 SDK26／G04、产品 GUI、受保护内容库、
OS sandbox、认证 transport 和其他平台仍 OPEN／NOT_RUN。详见 [C12 接口与限定证据](../reports/reconstruction-2026-10-06/codex-sdk-c12.md)。

下方 C11 及更早日期的文字保留当时的身份、结果与未运行范围；本轮现状以 C12 为准。
<!-- C12-CURRENT-END -->

当前C11检查点（2026-10-06，本地未提交）：Windows R2接口106通过；原Linux R2归档已用新外层校验器通过封存核验，旧pin不变。真实新Rust/C/C++目录Wasm在原managed owner上12方法通过；原件与目录回归70、只读映射7分别通过。四Python组99通过/3 FIFO跳过，新构建门禁15通过。SDK327/冻结57保持；完整SDK26/G04、产品批准链、ProtectedSession、picker、真实Windows执行器与其他平台仍OPEN/NOT_RUN。详见[C11限定资格](../reports/reconstruction-2026-10-06/windows-sdk-c11.md)。

以下带日期检查点保留当时身份和未运行范围；C11后续结果以上述报告为准。

当前C10检查点（2026-10-05）：本次开发分支更新收录C08–C10。C10已新增独立 `fs-directory-request-v1`、严格单import、包feature及只读discovery，复用原已批准selection和owner，不导出路径、句柄或新授权。Windows新Rust34（codec12/helper4/profile9/owner9）、既有回归191及frame5分别通过；Python33和两个原生C/C++消费者另计。新Rust Wasm仅编译通过，真实新Rust guest、C/C++ Wasm、Workbench产品／GUI、受保护Session与其他平台仍未验收。旧Core IO FileList保持Unsupported，SDK26／G04仍OPEN，无新Release。 详见[接口与实测边界](../reports/reconstruction-2026-10-05/directory-request-sdk.md)。

## 当前开发检查点（2026-10-05）

本次源码同步目标为 `codex/windows-sdk-convergence-20261005`，承接云端 `468ef2e` 并保存 C02–C07 Windows 复验与 SDK 增量。应用源码为 `0.1.9-test.58+62`；已发布下载仍为 test.56 测试预览，本次不构建或发布新安装包。

C07 原 owner 九组 115 方法、原始 SDK 定向回归 42 方法及网络 100 方法分别通过；115 已含取消7 / 目录owner14 / native16，42 保留过滤及 child helper 边界。C07整库 Clippy 保留10处既有诊断（exit101）、C07 owned诊断0；C08三文件fmt与strict库Rustc通过；C08当时library Clippy仍exit101／10既有诊断／owned0，不是整库lint通过。327 SDK / 57 冻结输入保持原字节，完整 SDK26 / G04 继续 OPEN。

目录捕获、分页、结束和清理已进入原 IoWorker；原授权、时钟、预算与 Unknown 无重放规则保持。C08已新增可信工作线程 `capture_directory_fresh(File, CaptureLimits)`及所持缓冲Zeroizing，使用锁定getrandom0.4.3并在生成前后复验；Windows结果为 `PASS（Windows限定）`，见 [阶段说明](../reports/reconstruction-2026-10-05/directory-secret-factory.md)。旧显式seed接口保持。C08不提供picker或祖先证明；当前C09的限定选择接线见下文，公开目录协商、blob 耐久后端、生产入口及其他平台资格继续推进。

C09已完成限定Windows Release／locked／offline资格，见 [C09阶段说明](../reports/reconstruction-2026-10-05/directory-selection-owner.md)。可信宿主 `capture_directory_under(anchor, relative, limits)`只保留并核验原opened anchor到relative leaf的raw UTF-16句柄链，复用原worker FileList、原时钟、取消和预算；root加N个分量共享原8资源，32段语法上限不是可用深度。新selection_path8＋directory_selection12、C08 factory14、原owner九组115和原件42分别当前实际PASS；原件42为base9／dependency3／region7／reader主9／shared14，reader raw10含child helper1不加方法，region保留84过滤。17个credited测试进程合191 meaningful方法（raw192含child1），zero-match失败进程保留且不计功；这些数字不能作为SDK冻结。Workbench第二次Release x86_64 `--locked --offline --lib` check通过，首次缺offline asn1-rs0.7.2的exit101保留；只是编译检查，ProtectedSession／GUI／picker以上provenance和non-Windows产品执行NOT_RUN。C09 network100和Clippy明确NOT_RUN，不继承C08历史通过或lint结果。这不证明picker时刻、anchor以上来源或传入anchor的sharing策略，不增加guest FileList、目录guest或公共UI，blob耐久后端仍缺。SDK26／G04仍OPEN，公开FileList及conditional Replace仍Unsupported。C08/C09历史报告保留当时状态；本次开发分支更新收录C08–C10，无新Release。

当前统一入口为[项目状态](PROJECT_STATUS.md)、[SDK门槛](../reports/reconstruction-2026-10-05/sdk-next-gates.md)和[本次同步范围](../reports/reconstruction-2026-10-05/documentation-sync.md)。下方日期条目保存当时执行与交付状态；其中“未提交／未推送”和旧“下一项”不覆盖本次检查点。

2026-10-05 **C07 Windows 原 owner 目录命令有界通过**：原 `IoWorker` 的可信 selected-directory 捕获／分页／结束、单命令取消、未读交付与 idle 清理已接线；九组实际115方法通过，其中新取消7、新目录owner14和保留native16已包含在115内。原件42和network100分别fresh通过，保留过滤与child helper范围；不与C06历史、旧114、首个负向、子案例、时钟样本或命令数累计。格式检查及strict Rustc通过；整库Clippy仍exit101，10处既有源诊断逐一保持bootstrap字节，本轮owned诊断0。见 [C07范围／原始证据摘要](../reports/reconstruction-2026-10-05/directory-owner-sdk.md)。

原Authority／Control时间、授权和交付校验方法保持，Control新增有界目录Admission记账。每个原clock样本与验证在同一短step中完成，native查询／私有解析、编码、取消谓词和实际root／lease／spool drop均在clock锁外；不能抢占同步OS，不当缓慢的可信clock closure仍可能阻塞共享时钟。global8包含queued／resident／retired tombstone，真实资源drop后才释放额度；累计费用不退，metadata allowance不是总RSS上限，Unknown不自动重放。

C07封存时的历史门槛（C08–C10进展以本页最新检查点为准）：**SDK26／G04继续OPEN**。生产protected owner9／真实session／StorageIoWorker／GUI、trusted-secret factory、picker／祖先来源、workspace task入口、新独立Dir request/schema与包feature/import/helperprofile协商、blob durable backend/history及其它平台尚未闭合；原公开FileList及conditional Replace保持Unsupported。普通合成Store和临时目录不替代生产owner。下方C06及更早日期说明保留其历史范围；C07当时最终代码资格SHA256为`773e36fa49013d79071a6cb9f9de4500cb74d99c5aec8042b70e39290ccdea7f`。

2026-10-05 **C06 目录观察与分段字节 SDK**：独立Rust/C/C++17目录库13、blob库23、Windows native DirectoryBroker16分别通过；138共享wire vectors（目录76=16接受/60拒绝、blob62=15接受/47拒绝）、独立Rust7方法与19去重检查families通过，C++复执行8个C families不另加方法。当前接线重新实际执行原件42与网络100；42为精确parent名单的定向suite，有过滤/显式child helper排除，不能写整体零过滤。新增消费者均Windows native，没有新增directory/blob Wasm/guest协商；原SDK327/冻结57及旧客体不重建。完整runtime strictClippy真实FAIL101旧style保留，两处新增诊断修正后native16与该范围strictcompilePASS。原Core FileList继续Unsupported，Ticket取消/原owner队列与idle维护/picker祖先证明/fresh trusted-secret factory缺失，wholeblob backend/watch/rename/upload/conditionalReplace仍OPEN或Unsupported，Unknown不重放。G04/SDK26仍OPEN，未改前端/app版本，未commit/push/CI/发布。见 [C06实测](../reports/reconstruction-2026-10-05/directory-blob-sdk.md)、[接口](PLUGIN_DIRECTORY_BLOB_SDK.md)与[下一门槛](../reports/reconstruction-2026-10-05/sdk-next-gates.md)。

2026-10-05 **C05 SSE SDK 与独立分发**：新增独立Rust／C／C++17事件库，完整UTF-8／NUL与retry None/0/MAX；最终Windows库6、网络100（C04原98＋新增2）、准备策略11分别通过，原生83语料／19检查实现与跨语言编码核对通过。90文件源码包的27个分发测试、项目外两库／六newguest真实编译及WS/SSE四方法新产物执行通过；四方法属于100的再验证，旧客体不重建。首因竞态、UNLOCALIZED失败、驱动失败和修补均保留。原SDK327／冻结57和Windows分支保持，当前本地未提交／推送／CI／发布；生产owner/GUI、账户/TLS、普通token、其它平台及完整SDK26门槛仍OPEN。见 [SSE实测](../reports/reconstruction-2026-10-05/sse-event-sdk.md)、[分发资格](../reports/reconstruction-2026-10-05/channel-payload-distribution.md)、[后续门槛](../reports/reconstruction-2026-10-05/sdk-next-gates.md)。

2026-10-05 **C04 WebSocket 类型化 SDK**：新增独立 Rust／C／C++17 payload 库与新三语言 Wasm 示例，只复用原 channel import／包特性，不产生 socket 或 URL 权限。Windows Release 下原件42、网络98（原96＋新增2）、独立库6及准备策略11项分别通过；原生参照与 C／C++消费者另核对109个 wire vectors（38接受／71拒绝）、65536个 close code 和各37份编码产物，这些不是新增业务方法数。新夹具的握手失败、最小修补和复跑原样保留。原 SDK327／冻结57、C03能力契约和生产网络源码保持；当前仍为本地候选，未提交／推送／CI／发布，生产批准入口、账户／TLS、普通用户 token、GUI、其它平台及完整 SDK 冻结仍 OPEN。见 [实际结果与边界](../reports/reconstruction-2026-10-05/ws-message-sdk.md)、[SDK 接入](PLUGIN_WS_MESSAGE_SDK.md)和[后续门槛](../reports/reconstruction-2026-10-05/sdk-next-gates.md)。

2026-10-05 **C02 Windows 复验与 C03 SDK 推进**：在独立候选中检出云端 `468ef2e`，先修复 Windows 文件身份／工具布局、超时首因、背压及 registry 失败夹具问题，再完成原件42、网络96、changes runtime32（production30为子集）、Core18及指定回归；原 SDK327／冻结57恒同，所有初始失败保留。随后新增独立版本的 changes payload discovery 与严格消费者，Python182通过／2 POSIX skip、discovery9／preflight8及实际新旧 host×consumer 兼容通过；生产 route 仍为空／false、authority none。C02 已生成可恢复有界包，C03结果独立保存。原 Windows 分支／工作树保持；当前本地候选未提交／推送／CI／发布，protected owner／普通用户 token／GUI／其它平台和完整 SDK 冻结仍 OPEN。见 [Windows复验](../reports/reconstruction-2026-10-05/windows-sdk-revalidation.md)、[SDK新增阶段](../reports/reconstruction-2026-10-05/changes-sdk-discovery.md)与[接入指南](PLUGIN_CHANGES_METADATA_SDK.md)。

2026-10-04 **Windows 开发支线源码检查点**：目标分支 `codex/windows-sdk-qualification-20261003`，基线 `63f38d4a`。本次保存云端 Linux 汇合源码及累计 Windows SDK 工具、IO 操作历史、受管 SSE 实现和 W15 WebSocket 待验证草稿。C01 当前无密钥 Windows 定向验证为 42/42，原 SDK327／冻结57保持不变；WebSocket transport 与 19 项新测试尚未编译／运行，受保护 owner、普通用户 token、GUI、其它平台和完整 SDK 冻结仍 OPEN。此前报告中的未提交／未推送为各次验证归档时的历史状态；本条记录此次源码同步范围，不扩大验收结论。见 [汇合核验与范围](../reports/reconstruction-2026-10-04/windows-convergence-recheck.md)。


2026-10-04 **C01 云端汇合输入复核与 Windows 原件验证**：原 17 路径 Linux 增量已在 W11 汇入，当前全部字节相同，未重复应用补丁。当前源码重新执行原 dependency 3、base 9、映射 7、共享对象 14、owned reader 9，共 42 个唯一方法通过；原件／SDK327／冻结57及 827 项输入前后一致，实际只读写入探针记录 0xC0000005。三份内部子代理审查及最终运行复核已完成，全部本地修改保留。protected owner 9 项、生产绑定与完整 SDK 冻结仍 OPEN；W15 WebSocket 保存为待验证源码。见 [当前汇合复核](../reports/reconstruction-2026-10-04/windows-convergence-recheck.md)。未提交／推送／CI／发布。

2026-10-03 **W14 原宿主批准的 POST SSE 通道**：新增显式 `managed-channel` 原生桥接，原 Store 严格 claim 成功后仅一次 POST，真实 Receive／exact ACK 后才读取下一事件，完整元数据使用独立 Cap’n Proto envelope；撤权与期限复核贯穿入队、交付和原 ACK 事务。Windows Release／离线／锁定最终134方法通过（72传输／SSE／新桥接＋62原通道／执行器／原件），新TEMP零残留。Rust／C／C++新 guest 实际消费完整事件；原9基础＋3依赖及 provider 保持原字节，SDK327／冻结57与云端Linux源码未修改。两轮编译失败、一轮48通过／2失败的原始运行保留；定向修复后完整复验，不清除历史。HTTP EOF／ACK／实际HTTP和producer join分别记录，业务仍OutcomeUnknown。未接Workbench生产批准UI／目录，WS／认证／TLS／真实外部API／普通用户token／受保护owner／GUI／其它平台及完整SDK仍OPEN。见 [宿主接口指南](PLUGIN_SSE_CHANNEL.md)与[实现、实测及剩余范围](../reports/reconstruction-2026-10-03/windows-sse-channel.md)。未提交／推送／CI／发布。

2026-10-03 **W13 只读HTTP历史查询**：原QueryOperation在显式managed路由接通；当前单operation批准、原帧／payload摘要核验和Ready／read复核，不重发Unknown、不暴露正文。最终Windows Release离线锁定144方法通过（42新增＋原件回归），SDK327／冻结57原件／Schema／Linux代码恒同。三次fixture失败及定向修正原始保留；unit仅选10项，普通用户token／新三语言query guest／protected owner／GUI／其它平台未验。G07仅此范围推进，完整SDK继续OPEN。见 [接口指南](PLUGIN_OPERATION_HISTORY.md)与[实施／证据](../reports/reconstruction-2026-10-03/windows-operation-history.md)。本阶段未提交／推送／CI／发布。


2026-10-03 **W12 有界 SSE 传输**：新增独立 decoder／SseLease，保持旧 HTTP/SDK327/57原件/schema/pins 和 Linux 汇合代码；审查修复解析首因及 EOF-invalid 截断标记。唯一 Windows Release 离线锁定运行55方法通过（34新增SSE＋21原传输），零fail/ignore/filter、新TEMP零残留，独立审查按源码和实际日志分别记录。只验证合成localhost与传输生命周期，公开guest网络源/channel bridge、TLS/外部账号/其他平台及完整SDK继续OPEN；G07历史查询仍待窄只读授权实现。见 [W12范围与证据](../reports/reconstruction-2026-10-03/windows-sse-transport.md)。本阶段未提交／推送／CI／发布。


2026-10-03 **W08–W11 本地推进与 Linux 汇合**：channel 45 个无密钥方法通过，最小释放顺序修复后相关 36 方法复验、新目录零残留；独立 SDK 的原 Rust/C/C++ task 均真实离线编译 exit0；W05 原 native 14 个产物在受限 medium token 复跑通过、67 输入恒同。用户提供的 Linux 17 路径源码增量已按原 SHA/blob 合入，原 25 项本地修改保留。云端 Close199 是外部汇总，本包缺完整 Linux 测试日志；当前没有新增 Linux/GUI/受保护 owner 运行资格，整个 SDK 仍 OPEN。见 [夹具清理](../reports/reconstruction-2026-10-03/windows-channel-fixture-cleanup.md)、[独立编译](../reports/reconstruction-2026-10-03/windows-sdk-standalone-builds.md)、[受限 token](../reports/reconstruction-2026-10-03/windows-sdk-restricted-token.md)、[汇合范围](../reports/reconstruction-2026-10-03/linux-windows-convergence.md)。本轮未提交／推送／CI／发布；下方历史记录保持原范围。

2026-10-03 **Windows 原生 SDK 与独立开发工具**：Clang／MSVC 七个原 fixture 各编译运行一次，14 build＋14 run exit0，独立1293项证据复核无阻塞；实际 child-handle token观察限定为管理员，未另证取样瞬间 liveness。新增 source-only SDK 导出／校验与显式 `--sdk-only` 项目工具，原宿主契约检查、source lock 和冻结原件保持；100个相关文件／元数据工具回归通过、0 skip，首次fixture前置清单失败及修复单独保留。SDK独立目录的最终模板／锁验收另记，不把元数据验证当模板业务执行或整个SDK冻结。见 [原生矩阵](../reports/reconstruction-2026-10-03/windows-sdk-native-codecs.md)、[独立分发](../reports/reconstruction-2026-10-03/windows-sdk-distribution.md)。

后续按 [当前26需求／10组门槛](../reports/reconstruction-2026-10-03/sdk-scope-freeze-gates.md) 推进：生产 channel 产品资格、公开 POST SSE／WS、文件目录／blob、异步组合、重启核对与多平台。已有权限复核／Stop／Linux低层基础不再列作未实现；当前完整范围仍 OPEN。原 committed-tool runner 未运行且不绕gate，本轮未提交／推送／CI／发布；下方历史记录保持各自身份。

2026-10-03 **当前 Rust SDK 库复验**：Windows x64 独立离线锁定测试报告92项通过、失败／忽略／过滤均0，真实exit0；其中1项按需夹具导出分支未启用，不作为新增功能证明。327 SDK／57原件／8 pending源码、锁与工具前后不变。管理员recorder及默认子进程token继承范围已记录；普通token、原bounded runner、其他原生codec全集、正式模板／分发及生产UI尚未获资格。见 [库测试证据与边界](../reports/reconstruction-2026-10-03/windows-sdk-library.md)，整个SDK未冻结。

2026-10-03 **Windows SDK 续验**：原 transport 六包本机真实服务／HTTP 的2＋4项通过，runtime服务7套／76个唯一测试与SDK-only Rust／C／C++消费者通过。HTTP fixture清理先复现失败，再通过删除回归及原四项复验，零临时残留；早期72个残留文件和更正原样保留。精确LF C／C++重新编译运行通过，C的NDEBUG按预期拒绝。原件、runtime、消费者与产品资格分别记录，整个SDK未冻结，未提交／推送／CI／发布。见 [续验与剩余范围](../reports/reconstruction-2026-10-03/windows-sdk-transport-consumers.md)；下方条目保留历史身份。

下一步补其他原生codec全集、模板与分发、准确committed-tool身份的完整bounded runner、生产channel／GUI及平台矩阵。已有Stop／权限核验源码不等于生产全链通过，Linux测试不计Windows资格。

2026-10-01 **0.1.9-test.57+61 开发检查点与 Linux 迁移**：用户授权保存本轮代码/文档到原 `codex/m03-stream-revocation-backpressure` 分支，保留全部历史证据，不合并 main、不创建 Release、不运行 Actions/CI。生产 Windows supervisor/host/Flutter 链已落实；SDK014 最新默认 Release 构建通过，真实 normal/host/UI/supervisor 四类故障及 WM_CLOSE 通过。当前生产 channel 20 项实测为 11 通过、9 失败/错误、0 跳过；catalog UI 在 available 检查处失败，旧 native 产品 4 项通过，profile 查询未运行。不能把此前 managedRunner/SDK 库验证当作生产通道全面通过。源码/状态迁移目标已确认 Linux 云电脑，Linux 监督、IPC、权限与工作台需独立实现和实证。详见 [SDK014 精确状态](../reports/codex-morrow-v1.1/windows-sdk-014-2026-10-01.md) 与 [test.57 检查点](../reports/0.1.9-test.57-checkpoint.md)。


2026-10-01 [Windows SDK 013候选](../reports/codex-morrow-v1.1/windows-sdk-013-2026-10-01.md)：新生产host只读profile及developer profiles工具已实现；643 runtime、三语言新15包/实际12基础执行、旧工作台4/4与原6 transport包真实回归通过，IO/service仍experimental。公开扩展规则和26需求矩阵已整理，3项真实拒绝/隔离fixture及独立复核通过，公共stream/events/Cloud尚未实现，整个SDK未冻结。工具捕获异常已保留并fixture修复后226 clean复验；遗漏guest输入的完整host失败保留、新完整host200、默认Release构建及新Release正常/host/UI/sup故障已实测通过，原精确窗口失败/未观察保留、最终独立审计PASS_SCOPED，统一源/产物/原始证据封存完成。CC/Codex暂停；无CI/提交/推送/发布。

2026-10-01 [Windows SDK 013 候选](../reports/codex-morrow-v1.1/windows-sdk-013-2026-10-01.md)：最新优先级为现有插件兼容与 Windows SDK 稳定验收。CC Switch／Codex 获取和构建暂停，不作为 Windows 冻结门槛；此前 G0 记录保留历史范围。基础 Wasm guest ABI2/runtime7/task3/UI1/dependency1 与实验 IO/service/mutation 分开，当前尚未冻结。统一原件、三语言构建、生产兼容、权限／预算／故障回归和独立复核进行中。无 CI／推送／发布。

2026-10-01 [G0 前置准备 012](../reports/codex-morrow-v1.1/g0-preparation-012-2026-10-01.md)：新增同源 CC Switch pure core + 既有 ABI2 Transform adapter；native 11/11、真实 Wasmi 与 Morrow API/PluginLibrary widget focused 2/2 通过，两份独立复核完成。CC Switch 原锁 Windows metadata 首次 0，仅证明依赖解析；原 Tauri caller/公开 broker 接入和完整 G0 未完成，SDK 未冻结。011 生产源码/历史密封证据保持；未提交、CI、推送或发布。


2026-10-01 [Windows生产监督接入011](../reports/codex-morrow-v1.1/m03-product-integration-011-2026-10-01.md)：默认Flutter→独立supervisor→原Rust业务host/Wasmi链完成；修复早host崩溃Data写失败绕过回收，可信再预览和手动Retry接入。统一新候选1515源恒同：UI24/native9、完整host199、默认Release构建、真实host/UI/sup故障/35.6ms早故障、commit→exit窗口、实际WM_CLOSE及共享核心15检查/16故障通过。Unknown/原失败不清除，缺证明继续拒绝；历史失败和未观察保留。G0产品图0/2、SDK冻结、跨平台/短写完成/断电/恶意同用户隔离仍未验收。未提交、无CI/推送/部署。


2026-10-01 [生产 owner 显式恢复 010](../reports/codex-morrow-v1.1/m03-owner-recovery-010-2026-10-01.md)：Windows v2身份/原文历史 CAS、只读 preview及 Flutter显式 Unknown确认已接入；新候选本地 ledger10、gate1、原worker5、UI/session15、实际生产8项通过。实际默认 Windows Release及 UI崩溃→显式资源恢复→新代次渲染已通过；sup缺原proof仍保守拒绝，Recovered不是正常业务成功或direct host放行。完整198项host回归及Release补充故障验证执行中；G0双产品图/SDK冻结/跨平台/同用户隔离/真实短写/断电仍未验收。未提交、未CI、未推送或发布。原历史保留。


2026-10-01 Windows 生产接入 009 已完成本轮限定验证：默认 Flutter → 独立 supervisor → 原 Rust workbench host/进程内 Wasmi；9 条生产命令退出 0、28 项 focused 测试通过，平台 13 项、更新后 Release 桌面正常/UI 崩溃、共享 M03 15 条检查及 16 项真实故障回归通过。已修复失联与正常证明裁决、死 UI 输出中断回收证明、M03 提前 Released 竞态。真实 UI 崩溃 0.101617 秒回收全部持有句柄，资源证明完整但 owner/业务仍 Closing/Unknown，真实重开拒绝；绝不将 exit 2 或资源证明当业务成功。 跨平台/恶意同用户隔离/成功 OS 短写/断电持久性/G0/SDK 冻结及 supervisor 崩溃后的可信 owner 修复未验收。见 [009 实现、证据与剩余边界](../reports/codex-morrow-v1.1/m03-production-supervisor-009-2026-10-01.md)。分支和 HEAD 保持，修改未提交，未 CI/安装/提权/发布。

完整 Workbench Rust host 库扩展回归 **193/193** 通过，零失败/忽略，独立复核完成；首次缺真实 guest 路径的失败仍保留。限定范围及新证据见 [完整 host 回归补充](../reports/codex-morrow-v1.1/m03-production-supervisor-009-host-regression-2026-10-01.md)。

当前 M03 共享核心另外完成 [3 项真实 Core/HTTP 故障复验](../reports/codex-morrow-v1.1/m03-shared-supervisor-core-009-http-2026-10-01.md)：每例真实 supervisor 0、恰好 1 POST/无额外 POST，Close/ACK 与最终资源证明分开，业务 Unknown 保留；仍不等于完整 M03/G0 或 SDK 冻结。

2026-09-30 [暂停同步检查点](../reports/codex-morrow-v1.1/m03-pause-checkpoint-2026-09-30.md)：按用户要求暂停开发，三协作会话空闲。network-abort 单次本机运行观察通过；authority-deadline 已启动批次收尾为 unconfirmed（guest evidence writer join 未确认、缺 Close/ACK），原证据保留；pipe-partial-close 未启动，真实运行后独立复核未进行。fixture003源码和回执已逐字节归档到companions；仅同步当前开发分支与 Drive 单个ZIP，不合并主线、不发Release。SDK未冻结。

2026-09-30 [M03 被动故障实现](../reports/codex-morrow-v1.1/m03-passive-fault-005-2026-09-30.md)：fixture003、原期限观测与测试专用12B管道接缝、运行入口005已实现并冻结；宿主52次定向调用、guest Core15/native35通过，runner独立审查的后验与封存阻塞已修并冻结003，13项纯检查通过。修复真实控制失败后空等RequestClosed的清理路径，原预算与失败保留。正在运行前独立复核，三个HTTP故障场景尚未运行，SDK未冻结；未推送／发布。

2026-09-30 [M03 当前宿主生命周期复验](../reports/codex-morrow-v1.1/m03-lifecycle-004-2026-09-30.md)：新增期限触发和 peer 断开同进程 pipe 测试，Windows probe 15/15、当前原生库 28/28；新固定宿主与冻结 real Core fixture 对撤权/OS 背压各运行 1 次本机 POST，生产方均通过，独立只读整链复核已限定通过。首次证据路径拒绝（0 POST）保留失败。原期限自然到达、网络/残帧断开的完整被动 fixture003 在设计中，SDK 未冻结；未推送／发布。

2026-09-30 [Drive 合入与 Windows 复核](../reports/codex-morrow-v1.1/drive-review-2026-09-30.md)：接入 partial-write-003 和插件打包前一致性检查；本机网络 21 项、纯写状态 8 项、Windows probe 13 项、原生库 26 项及 SDK 工具 62 项通过，原生二进制重新构建成功。上述集合有重叠，不能相加；仍未覆盖真正非零部分 OS completion 故障、完整 Core/child/HTTP 竞态、产品 G0 和跨平台。SDK 未冻结，本轮未推送／发布。

2026-09-29 [本支线当前状态](DEVELOPMENT_BRANCH_STATUS.md)：保留 2026-09-27 文件变更与 Windows 本地预览的专项边界，新增 M03 A010 失败、A011/B011 各一次真实运行与独立只读复核；OS 部分写入、系统选择器人工验收、后续修复成品、产品 G0、全平台与 SDK 冻结均未完成。下列记录按各自日期及产物保留，不能把分项计数相加或把本地 ZIP 等同公开 Release。

2026-09-27 源码同步检查点：本提交汇总文件变更底座、C/C++/Rust SDK、审批到期修复、Windows 构建装配和验收记录，目标为 `codex/io-safety-refactor` 开发分支，不创建标签或 Release。下方“未提交／推送／发布”为各次验收当时的历史状态；SDK 未冻结，系统选择器与人工审批、其他平台资格继续保留。

2026-09-27 [Guest 审批期限与收尾修复](../reports/guest-mutation-expiry-2026-09-27.md)：首次 Start 前按单调时间计时，到期后禁止新 Prepare/Chunk/Commit/Execute 和提交重试，保留已有回执核对及 Stop→实际退出→必要 Repair/ACK。弹窗到期禁用确认，确认返回后重新观察原任务；撤权／换库精确关闭旧弹窗，不误关其他对话框。九语言提示同步。最终会话 18、Widget 9、真实原生 4，共 **31/31** 通过，包含两次真实 30 秒到期后的独立 Discovery/Reconcile。修复尚未纳入上一轮 Windows ZIP；下一步为系统选择器与人工审批验收、新应用产物及其他平台资格。SDK 未冻结，未提交／推送／发布。

2026-09-27 [完整 Windows 本地预览验收](../reports/windows-mutation-preview-2026-09-27.md)：应用 `0.1.9-test.56+60`、工作台包 `0.1.9-test.54.7` 与主题 `1.0.2` 完整构建并装配。最终目录原生集成 **6/6**、实际 Release 应用自检 **4 项 PASS**、501 项文件摘要及两份 ZIP 完整性通过；旧输出覆盖拒绝有效。修复 MSBuild 环境中的主题 SHA-256 命令不可用问题。此前 20/52 项资格仍绑定各自旧产物，不视为新包重跑。下一步为审批弹窗跨越 30 秒后的过期提示与安全收尾、系统选择器人工验收及其他平台；SDK 未冻结，未提交／推送／发布。下方历史记录按阶段保留。

2026-09-27 [最大正文内容事务资格完成](../reports/guest-mutation-content-crash-2026-09-27.md)：原计划的四个 Core 提交点矩阵 **20/20** 通过，覆盖三语言、Rust 恢复 Widget、普通宿主对照及独立正文／LiveStaging 回执校验。各故障恢复后都保持 Prepared、无结果与无目标文件；提交前回滚和提交后持久化分别得到证明。后续顺序：① 完整 Windows 应用构建及内置／主题插件装配核验；② 系统选择器、真实点击审批和 30 秒权限窗口的人工验收；③ 其他平台能力与资格。保留旧 transport 原件兼容门槛；Debug 最大正文期限及条件 Replace 边界未关闭。SDK 未冻结，未提交／推送／发布。下方同日条目作为历史阶段记录保留。

2026-09-27 [最大正文文件效果资格](../reports/guest-mutation-max-crash-2026-09-27.md)：16 MiB／274 个逻辑块的三语言真实链路及恢复 Widget **52/52** 通过，并修复实际历史读取预算缺口；累计预算与只读授权边界保持。下一阶段顺序：① 四个 Core 内容事务提交点，三语言与 Rust Widget 加对照共计划 **20 项**，独立核验正文与 LiveStaging 回执，Prepared 文案不能替代回执证明；② 完整 Windows 构建、系统选择器与 30 秒人工审批验收；③ 其他平台。Debug 最大正文 Deadline 未据此消除；条件 Replace 仍拒绝无保证后端，SDK 未冻结，未提交／推送／发布。

2026-09-27 [非空 Guest 真实故障恢复与 Widget](../reports/guest-mutation-crash-ui-2026-09-27.md)：本机三语言 Create／Delete 故障、正常对照及 Rust 恢复 UI **32/32** 通过，恢复入口回归 **13/13**。此阶段完成后按顺序推进：① 16 MiB 全故障组合，含暂存／分块写入／flush／publish；② 完整 Windows 构建、系统选择器与 30 秒审批时限人工验收；③ 其他平台资格。条件 Replace 继续拒绝不具备保证的后端。普通进程故障不能替代断电验收，SDK 未冻结；本轮未提交／推送／发布。

2026-09-27 [扩展预算包只读历史恢复](../reports/guest-mutation-history-binding-2026-09-27.md)：独立 History 绑定与 Guest→恢复页交接已落地，保持普通计费、当前授权、原记录及无重放边界。三语言正常重启、重复只读核对和外部目标摘要保护已验证。后续顺序更新为：① 非空 guest 在真实进程故障后的发现／核对与产品恢复 UI；② 16 MiB 全故障组合；③ 完整 Windows 构建、系统选择器与 30 秒审批时限人工验收；④ 其他平台。普通重启不替代崩溃资格，SDK 未冻结，未提交／推送／发布。

2026-09-27 [Guest 独立会话与实际审批界面](../reports/guest-mutation-execution-ui-2026-09-27.md)：实际预算、计划审阅、Prepare 与第二次 Execute、页面外生命周期和撤权防护已接通。真实三语言会话 3/3、Rust Widget 1/1、Dart 会话／客户端 58/58、新面板 4/4、原有面板／设置 45/45 通过。下一步按顺序推进：① mutation-budget-v1 当前授权下的跨重启只读发现／核对；② 非空 guest 故障到恢复界面的整链；③ 完整 Windows 应用、系统选择器与 30 秒审批时限可用性；④ 16 MiB 故障组合及其他平台资格。SDK 未冻结，未提交／推送／发布。

2026-09-27 [Guest 私有协议与 Dart 客户端](../reports/guest-mutation-private-wire-2026-09-27.md)：五个独立调度动作、原始回帧交付、显式批准预算及持有原命令回执的类型化客户端已接入。Windows Release 三语言 private-wire 3/3、宿主及目录／IO 199/199、runtime 常规 40/40 与真实 SDK 普通／16 MiB 6/6 通过；客户端边界与实际回帧测试见专项报告。下一步为独立 guest 会话、实际预算／计划审阅／第二次 Execute UI，再完成真实 Flutter 整链及预算扩展包跨重启只读恢复。SDK 未冻结，未提交／推送／发布。以下为历史阶段记录。

2026-09-27 [Workbench guest 编排与目录声明](../reports/workbench-mutation-guest-2026-09-27.md)：已完成 Rust 宿主独立 guest 任务、显式预算绑定、两阶段审批、真实 Wasm 执行、保守历史核对和九语言目录声明。Windows Release 三 SDK **3/3**，宿主及目录／IO **196/196**，Dart／Flutter **107/107** 通过。下一步：独立 guest 私有协议及 Dart 类型 → 实际批准预算和两次确认 UI → Flutter 至原 owner 全链路 → mutation-budget-v1 包当前授权下的跨重启只读恢复。目录展示不代表 guest UI 已接通；完整 16 MiB 故障组合、平台资格和条件 Replace 继续开放。SDK 未冻结，未提交／推送／发布。

2026-09-27 [非空多块 guest 回执与崩溃恢复](../reports/mutation-guest-recovery-2026-09-27.md)：Rust／C／C++ 实际 SDK 的 Stage／Commit／Execute 丢回执 3/3、内容事务与 native Create 的独立进程退出案例 36/36、普通构建故障开关无效控制 3/3 通过。重开原 Store 显式核对审计回执、正文／文件摘要和 Core claim 防重放；原 owner／opt-in／核对回归 40/40。新增可重复资格脚本及部分写入测试边界，没有改变生产恢复语义。下一步优先接产品侧插件动作、实际预算展示与两阶段审批，再验证恢复 UI 的真实非空 guest 链路；完整 16 MiB 故障组合、其他平台与条件 Replace 仍开放。SDK 未冻结，未提交／推送／发布。

2026-09-27 [显式 mutation 预算与最大内容资格](../reports/mutation-budget-2026-09-27.md)：新增 `mutation-budget-v1`，精确实例显式批准、两层累计计费和签发前余量预检，旧 16／64 MiB IO 限额不变。Windows Release 三语言普通／16 MiB 实际流程 **6/6**，最大内容约 1.87–2.50 秒；原 owner／opt-in／核对 **39/39**，旧 SDK 原件运行 12 项、HTTP／服务 6 项及宿主 30 项回归通过。Debug Rust 最大内容另行复现 Deadline，不能泛化为所有构建模式均合格。SDK 未冻结；下一步补非空故障／跨进程恢复矩阵，再接入产品侧预算与两阶段审批。本轮未提交、推送或发布。以下为此前阶段记录，其当时限制以本条和专项报告中的后续证据为准。

2026-09-27 [三语言 mutation Wasm 与原 owner 接入](../reports/mutation-guest-wasm-2026-09-27.md)：独立包协商／import、Stage 与 Execute 模式、单请求完成绑定、有界语义去重、原期限与真实结果核对已接入。Rust／C／C++ 实际临时目录创建／删除 3/3、原 owner／opt-in／核对 37/37 通过；旧任务／UI／转换 9 项、依赖 3 项、HTTP／服务 6 项原件运行及宿主变更 30 项回归通过。64 MiB 累计硬预算仍不能覆盖协议最大 16 MiB 内容的完整流程；非空故障矩阵、产品审批整合及平台资格继续开放，SDK 未冻结，未提交或推送。

2026-09-27 [原 owner 审批凭证与一次性执行许可](../reports/mutation-guest-approval-2026-09-27.md)：明确授权后接入精确计划审批、已领取回执检查、Prepared／持久内容校验及旧 Execute 的 opt-in 单向许可；丢回执不降级，Release 不复用引用。Windows 临时目录 runtime 33/33、宿主 mutation 30/30；严格 Clippy 仍有两类旧警告，限定例外通过。每 worker 生命周期签发上限 128。尚无 guest import，下一步接协商／显式 job／原 owner 分派及三语言 Wasm；SDK 未冻结，未提交或推送。

2026-09-27 [文件变更 C／C++ SDK 编解码接口](../reports/mutation-ffi-2026-09-27.md)：补齐版本化 C ABI、C++17 自有数据／只可移动结果封装及可重复原生验证脚本。SDK 全量 76/76、严格 Clippy、Windows C／C++ 两项实际程序（21 个同源向量与八动作）、Rust SDK wasm32 编译检查通过；契约／枚举／向量与锁定工具 22 项通过。该阶段没有 guest import 或文件执行能力，下一步为原 owner 分派与可信审批／执行许可；SDK 未冻结，未提交或推送。

2026-09-27 [独立文件变更 guest 契约与 Rust 编解码](../reports/mutation-guest-codec-2026-09-27.md)：新增独立 mutation Cap’n Proto 草案、Core／Rust SDK 有界编解码及 21 个共享 CLI 正反向量；旧 IO v1 保持不变。Core 新增＋旧 IO 测试 30、SDK 全量 71、工具与锁定 20 项通过，SDK 严格 Clippy 通过。修复 Unicode 校验与 Unknown 响应的两端差异。C／C++ 接口、独立 import、原 owner 分派及三语言真实 Wasm 仍待实施；本轮仅编解码，不启用插件文件效果，SDK 未冻结，未提交或推送。

2026-09-27 [文件创建／删除界面与独立执行会话](../reports/mutation-execution-ui-2026-09-27.md)：Windows IO 设置已接选择、范围预览、准备、独立执行确认及退出／恢复交接；九语言补齐。修复停止与迟到回执门控、ACK 身份校验，以及真实宿主保留历史提交导致后续操作禁用的问题。执行会话 19/19、Flutter 组合 43/43（含非空三块内容的真实原生与真实控件链路）、本地化 4＋7 通过，严格分析无诊断。系统选择器由测试注入路径；未做完整应用构建或人工验收。下一步为版本化 guest 扩展、三语言 SDK／Wasm 和非空中间故障点；SDK 未冻结，未提交或推送。

2026-09-27 [真实故障后的恢复界面联调](../reports/mutation-crash-widget-2026-09-27.md)：真实时钟 Flutter 控件＋Windows 宿主完成 Create／Delete 各三阶段故障及普通宿主对照，7/7 通过；实际按钮完成发现、选择、离页返回、退出确认及独立核对，Unknown／Observed 文案分离，最终等 ACK 与宿主 close。既有面板及竞态回归 12/12，严格分析无诊断。新增三语言文件变更 SDK 接入计划；正式变更编辑／审批、guest SDK、非空内容中间故障点及其他平台继续开放，SDK 未冻结，未提交或推送。

当前下一实施面见 [文件变更 guest SDK 接入计划](PLUGIN_MUTATION_SDK_PLAN.md)：真实恢复界面与可信 Create／Delete 操作入口已完成本机链路，接下来推进版本化三语言扩展、原 owner 调度和真实 Wasm；正式系统选择器人工验收与非空故障点并行补验；旧 IO／transport 原件保持不变。该计划不表示 SDK 已接通或冻结。

2026-09-27 [真实宿主故障退出与恢复](../reports/mutation-crash-recovery-2026-09-27.md)：Create／Delete 各三个阶段的真实进程退出及普通构建对照共 7 项通过；新宿主重开临时受保护库，发现原计划并两次独立只读核对，Unknown 不重放。修复发送尚未完成时 EOF 导致未处理 Future 错误的竞态；确定性红绿回归和传输／关闭组合 38 项通过，严格分析无诊断。仍待真实崩溃后的 UI 按钮整链、正式变更编辑流程与 guest SDK；不代表断电或外部强杀验收，SDK 未冻结，未提交或推送。

2026-09-27 [文件变更恢复界面](../reports/mutation-recovery-ui-2026-09-27.md)：Windows IO 设置已接只读发现、显式翻页／续扫、所选计划核对及退出／修复／确认，新增有界展示模型保留页面外状态。九语言 30 条文案与资源同步；Dart 组合 66、真实宿主与既有设置组合 27、i18n 4＋7 项通过，新增 widget 11 项通过（含 320×640 窄屏），严格分析无诊断。后续为强制崩溃整链、正式文件变更编辑流程及 guest SDK；SDK 未冻结，未提交或推送。

2026-09-27 [同宿主断点续扫](../reports/mutation-checkpoint-2026-09-27.md)：Core／runtime／私有协议／Dart 已接通有界 checkpoint，可在旧任务退出与 ack 后以当前授权显式开启新任务续扫。位置限定同一存活 Store 与范围，宿主只保留最近 64 份，仍受共用 512 个启动身份限制，不跨进程持久化。Core 19、runtime 26、Dart 62、Flutter／真实宿主 27 项通过；宿主全量首次 171 通过、1 服务短期限失败，单独复跑通过，限制并发全量 172 项通过。下一步为恢复 UI 与真实崩溃整链；SDK 未冻结，未提交或推送。

2026-09-27 [独立恢复会话与真实进程恢复](../reports/mutation-recovery-session-2026-09-27.md)：Dart 会话稳定持有有界发现页／核对结果，补齐精确提交重取、丢回执、停止意图、实际退出／确认及失败准入放弃。真实 Windows 新进程重开临时库验证原计划一致、Observed／Prepared 分别核对及丢页后的明确重扫。Dart 组合 61、Flutter／真实宿主 27、打包工具 16 项通过，0 失败（Dart 1 既有文件夹具跳过），严格分析无诊断。下一步为恢复 UI、预算耗尽续扫与真实崩溃整链；SDK 未冻结，未提交或推送。

2026-09-27 [原计划发现协议与 Dart 客户端](../reports/mutation-discovery-wire-2026-09-27.md)：有界持久发现已接私有启动／翻页／领取／关闭协议和类型化客户端，补齐请求去重、混合字段拒绝与发现任务隔离。宿主全量 171、Dart 组合 46、Flutter 受控传输 26 项通过，0 失败（Dart 1 项既有文件夹具跳过）；7 份真实 Mutation 回帧已验证。下一步为独立恢复会话、丢回执处理、预算耗尽续扫和真实跨进程恢复；SDK 未冻结，未提交或推送。

2026-09-27 [持久原计划发现与原生任务](../reports/mutation-plan-discovery-2026-09-27.md)：Core 有界键集扫描、受保护原件限额解码及 Store 绑定游标已接入 runtime／Workbench 原生任务；发现与完整核对、文件执行隔离。Core 全量 744 通过／15 ignored，runtime 25、宿主全量 167 通过，0 失败；最终专项另见报告。私有发现协议、Dart／恢复 UI、跨进程闭环及预算耗尽续扫仍开放，SDK 未冻结，未提交或推送。

2026-09-27 [宿主文件变更计划构造](../reports/mutation-plan-builder-2026-09-27.md)：原 owner 从保留的选择构造规范草稿，私有协议及 Dart 类型化入口已接通；不写 Store、不执行文件效果，显式 Prepare 前允许修改。宿主全量 165、runtime 专项 23、Dart 组合 42、Flutter 受控传输 23 项通过（Dart 1 既有夹具跳过）；最终 Dart JSON 分析无诊断。下一步为持久计划发现、独立恢复会话／UI 与跨进程恢复；SDK 未冻结，未提交或推送。

2026-09-27 [文件变更核对协议与类型化客户端](../reports/mutation-client-2026-09-27.md)：只读核对已接入私有启动协议，Dart 类型化客户端覆盖变更命令与核对，RustWorkbench 原生接线及敏感发送缓冲清理完成。宿主全量 163、Dart 39、Flutter 受控传输 23 项通过，0 失败（Dart 1 项既有夹具跳过）；Dart 分析无问题，宿主 Clippy 保留 12 条既有警告。下一步为 Rust 侧计划构造、持久计划发现、可恢复会话／UI 与跨进程恢复；SDK 未冻结，未提交或推送。

2026-09-27 [原 worker 退出后的只读历史核对](../reports/mutation-reconciliation-2026-09-27.md)：新增独立原生核对任务，以当前权限读取完整原计划及已保存的 OS 结果；不重新选择目标，不重放效果，Unknown 保持待核对。Workbench 全量 159、runtime 相关 232 项通过（runtime 2 ignored）；严格 runtime Clippy 通过，宿主仍有 12 条既有警告。下一步接私有核对启动协议、类型化 Dart／UI、持久原计划索引及跨进程恢复；SDK 未冻结，未推送。

2026-09-27 [文件变更私有协议与跨语言绑定](../reports/mutation-wire-2026-09-27.md)：追加启动／提交／状态／领取／命令取消，有界去重阻止重复派发，明确 Core 阶段与 OS 结果投影；Dart 绑定由标准工具生成。Workbench 全量 152、Dart 31 项通过，0 失败（Dart 1 项既有夹具跳过）；Clippy 仍有 12 条既有警告。下一步优先补 worker 退出后的重新授权历史／效果核对，再接类型化客户端与 UI；SDK 未冻结，未推送。

2026-09-27 [Workbench 原生文件变更任务](../reports/workbench-mutation-task-2026-09-27.md)：创建／分块／执行／查询／持久取消接入原工作台任务，逐命令身份隔离迟到回执，保留选择元数据并区分待核对与终态；真实 Windows 受保护存储验证通过。Workbench lib 全量 147 项，最终状态修正后专项 4 项通过（重叠）；Clippy 仍有 12 条既有警告。下一步接私有协议、Dart 与 UI／guest SDK；未打包、推送。

2026-09-27 [文件计划显式持久取消](../reports/file-plan-cancel-2026-09-27.md)：原 owner 队列新增 cancel_mutation_plan，精确 Prepared 可持久取消，保留证据并清除未提交缓冲；已派发结果不可改写，丢回执只读核对。runtime 组合 232 项通过，最后预算修正后专项 16 项通过，0 失败（分项重叠）；Clippy 与宿主编译检查通过。SDK／工作台界面接线、分块恢复和跨重启核对继续开放，未打包或推送。

2026-09-27 [文件变更原 owner 队列](../reports/file-mutation-owner-2026-09-27.md)：Windows 创建／删除接入原后台队列，补齐 60 KiB 分块、取消、一次领取、历史查询与资源释放；Core 先匹配请求再核验正文，终态查询避免重复正文核验。Core 全量 739、runtime 相关 228 项通过，0 失败；Clippy 与实际宿主编译检查通过。下一步接 Workbench 协议／任务模型、三语言 SDK 与 UI，持久取消／跨重启核对仍开放；条件替换继续明确不支持，未打包或推送。

2026-09-27 [文件变更取消与同 owner 接口](../reports/file-mutation-control-2026-09-27.md)：完成单操作取消、持锁取时／准入及原 Manager／Runtime 分借用；修复取消遮蔽已派发历史的问题。Windows runtime 相关回归 216 项通过、0 失败，Clippy 与实际宿主编译通过。下一步为原 owner 队列、分块正文及 SDK/UI 接线；条件替换仍明确不支持，未打包或推送。

2026-09-27 [条件替换协议与平台边界](../reports/file-replace-boundary-2026-09-27.md)：补齐 Replace 专用持久 claim／观察、历史证据与崩溃测试。Windows 原型证实独占句柄下替换被拒绝，放宽共享会覆盖并发对象；生产明确返回 Unsupported，不降级。Core 全量 737、runtime 相关 162 项通过，0 失败；两个显式原型是限制证据。下一步为 Create／Delete 补取消与持锁时钟接口，再接原 owner 队列及 SDK/UI；未打包或推送。

2026-09-27 [Windows 文件创建执行](../reports/file-create-execution-2026-09-27.md)：接通 Unknown claim 后的原目录临时写入、同步、不覆盖发布和 Response＋Observed 原子保存；真实 Windows、名称碰撞、撤权及七处进程崩溃已验证。Core 全量 724、runtime 组合 158、最终创建专项 14 项通过，0 失败（分项重叠）。条件替换、完整 Unknown 核对、owner 队列与 SDK/UI 仍开放；未打包或推送。

2026-09-27 [文件创建持久执行边界](../reports/file-create-claim-2026-09-27.md)：Create 增加原计划/内容/回执齐备后的严格一次性 Unknown claim，历史拒绝证据双失和回执晚于派发；补堵通用文件 Response 写入与响应预留释放旁路。Core 完整 715 项、runtime 组合 145 项通过，0 失败。实际临时文件写入、不覆盖发布与 Create 观察尚未接通，完整 IO/SDK/UI 继续开放；未打包或推送。

2026-09-27 [Windows 创建目标目录链](../reports/file-create-target-binding-2026-09-27.md)：原目录句柄逐段打开并拒绝重解析点、固定全部祖先、全链预算先行，Create 已接 Prepared/审计内容暂存；不触碰最终文件。Windows runtime 组合 145 项、Core 定向 28 项通过，0 失败。实际创建/不覆盖发布/Unknown 核对、条件替换与 SDK/UI 继续开放，未构建安装包。

2026-09-27 [Windows 原句柄删除](../reports/file-delete-execution-2026-09-27.md)：底层 FileDelete 已接通持久 Unknown claim、原句柄删除、Response＋Observed 原子保存及一次性选择；实际文件/硬链接/只读拒绝/并发门/真实崩溃已验证。Core 完整 706 项、runtime 组合 135 项通过，0 失败；最终定向复验另见报告。创建/条件替换、完整 Unknown 业务核对、owner 队列与公共 SDK/UI 尚未完成，未构建安装包。

2026-09-27 [Windows 文件目标与准备接线](../reports/file-target-binding-2026-09-27.md)：保留可信选中文件的原句柄与实例租约，将计划/内容暂存接到实时授权；明确提交前拒绝与提交后交付拒绝。Windows 运行时组合 122 项通过、0 失败。实际创建/替换/删除、owner 队列与 SDK/UI 接线仍开放，未重新构建安装包。

2026-09-27 [暂存回执与审计迁移](../reports/file-content-receipt-2026-09-27.md)：新增 v23 暂存回执、同事务 kind 6 审计事件、来源/序号校验与正反闭包，内容或回执单边丢失可检测；完整 Core 故障注入回归 698 项通过、0 失败，11 个子进程入口由父测试调用。实际文件效果仍关闭，未重新构建安装包。

2026-09-27 [文件内容持久暂存](../reports/file-content-staging-2026-09-27.md)：新增有界内容原件、v22 迁移、Prepared 阶段双授权暂存/读取、共享额度及取消留存；完整 Core 故障注入回归 685 项通过，0 失败。实际文件效果、执行就绪承诺和 SDK/界面接线尚未开放。

2026-09-27 [文件变更原子准备](../reports/file-mutation-preparation-2026-09-27.md)：新增有界路径与独立变更计划契约、同事务 Prepared/原件/容量预留、开库与快照闭包、取消恢复；通用 IO 入口禁止绕过文件派发门槛。Windows 核心组合回归 119 项通过（含真实进程崩溃），详见报告。实际写入、内容暂存、现场句柄授权和 Unknown 核对尚未接通，完整文件 IO 与 SDK 冻结继续开放。

2026-09-27 [文件任务生命周期与恢复](../reports/file-task-lifecycle-2026-09-27.md)：修复 Finished 与实际退出同回执导致成功任务误报中断；补充隐藏/返回/切库/Unknown 控制互斥及真实取消、丢启动回执只读恢复。Dart 30、UI 66、最终成品真实任务 3 项通过，Windows Release 与临时库自检通过；本地未提交/推送。原生选择器人工验收、真实磁盘故障、目录/写入及跨重启业务核对仍开放。

2026-09-27 [Drive 增量合并验收](../reports/drive-merge-acceptance-2026-09-27.md)：在独立工作树合并检查点 `807b9cd` 与本地主题/构建检查点 `008db00`；补齐文件任务九语、Windows 测试夹具及真实 Dart→宿主文件捕获验证。新增能力、最终 Windows 构建与合并结果以验收记录为准；不变更应用版本、不推送、不发布。以下日期条目保留历史验证范围，不能据旧条目判断后续工作尚未完成。

2026-09-26 [SDK 库源码锁](../reports/plugin-sdk-source-lock-2026-09-26.md)：新增可迁移源码摘要锁、显式更新与严格锁要求；预检和构建前后核对源码／锁身份，漂移停止打包。118 项 Python、21 模板真实锁定预检、契约及 17 固定原件检查通过；不计为真实编译／执行资格，不锁定完整工具链。SDK 独立分发、Windows／Flutter 验收等继续开放，未使用 CI、推送或发布。

2026-09-26 [SDK 工程离线预检](../reports/plugin-project-preflight-2026-09-26.md)：新增 validate 的 TOML 声明摘要、与正式构建共享的预检、21 模板批量预检模式；修复出站服务编译遗漏 IO 契约／版本检查。Python 106 项、21 工程真实 CLI、契约同步检查及 17 固定原件通过；不计为编译／运行资格。独立 SDK 分发与 Windows／Flutter 产品验收仍开放，未使用 CI、推送或发布。

2026-09-26 [文件会话与选择器接线](../reports/plugin-file-session-2026-09-26.md)：新增跨页面保留的文件会话、按 offset／EOF／最终 SHA-256 增量校验、Unknown 不重放及取消迟到回复隔离；原生 IO 页接入选择器和控制，HTTP 面板隔离文件任务。独立 Dart 28 项、定向分析及 17 个固定原件通过。Flutter 页面仅源码／解析检查，Windows、完整 Flutter 验收和九语言文案仍开放；未推送、发布或使用 CI。

2026-09-26 [文件私有协议与 Dart 客户端](../reports/plugin-file-wire-2026-09-26.md)：启动、块请求、结束和一次领取接入原任务控制；可信路径只在原 worker 打开，重复身份及嵌套调度拒绝。修复二进制文件／引用 seed 错用 schema 文本摘要。runtime 415、Workbench 64、IO SDK 5、Dart 7、严格 Clippy 与固定原件检查通过，计数有重叠。Flutter 启动的云元数据访问被自动审批拒绝；独立 Dart 真实帧验证通过，系统选择器／页面和 Windows 产品资格仍开放。未推送、发布或使用 CI。

2026-09-26 [文件后台队列与工作台原生接线](../reports/plugin-file-owner-2026-09-26.md)：类型化捕获／分块／结束复用原 owner 队列、授权、停止与实际 join；工作台新增文件任务入口和一次领取。共享时钟检查串行化，宿主 ceiling 与实例实际预算同时生效。runtime 412、Workbench lib 60、network 134、三语言 IO 5 和服务原包 6 通过，分项有重叠；Linux 测试维护仍失败并保持 RecoveryRequired。私有协议、系统选择器、Flutter 与 Windows 实机待接，未推送、发布或使用 CI。

2026-09-26 [选中文件真实句柄入口](../reports/plugin-selected-file-2026-09-26.md)：新增 FileBroker 原句柄有界捕获，读取前共享额度预留、块间／最终撤权检查、固定字节摘要及失败释放；旧 Vec 计费保持。最终文件 23、三语言模块 5、runtime 常规 403、network 常规 134 与严格 Clippy 通过，范围有重叠。工作台选择器／任务协议／Flutter 接线、Windows 真实文件和完整文件系统继续开放；未使用 CI、推送或发布。

2026-09-26 [配置服务统一接线与原包复验](../reports/plugin-configured-service-2026-09-26.md)：从 Google Drive 恢复 `7d08b95`，核对主线未变。工作台改用同一宿主接线对象生成所选资源依赖、重放 scope、router 与目录；三语言原包覆盖配置服务撤权后的缓存隔离、等待中终止、重开不重发、空选择及错 worker 归还。网络 134、原包专项 6、SDK 66、工具 97 与旧原件回归通过，新增无需重编 guest 的恢复验收入口。Windows／Flutter／真实 DPAPI/TLS 产品验收仍开放，未推送、发布或使用 CI。

2026-09-26 [公共 SDK 服务出站闭环](../reports/plugin-service-http-sdk-2026-09-26.md)：新增三语言 `--service-http` 原包模板、原帧摘要接口和显式四资源槽声明。真实网络三项覆盖 21 场景，验证敏感头隔离、宿主凭据、等待期间原 owner 写入、断线／停止／撤权 Unknown 与同库重开不重发；SDK 66、工具 94、打包 CLI 15、原生及旧原件回归通过。源码与证据保存 Google Drive。Windows／Flutter、实际 DPAPI/TLS 与完整业务核对仍开放，未使用 CI、推送或发布。

2026-09-26 [插件底座本地稳定候选收尾](../reports/plugin-foundation-closeout-2026-09-26.md)：显式长时服务 profile、三语言资源目录 SDK、真实原包续租／耗尽／撤权／历史重开完成；新增 transport-v1-rc1 六对固定 IO／服务原件，保留旧 guest-v1-rc1。core 645、runtime 397、network 134、SDK 66、Python 92 及专项通过，分项有重叠。Windows 专属依赖、Flutter 与凭据／TLS 实际入口仍阻断整体收尾；本轮未发布或使用 CI。

2026-09-26 [入站服务 SDK 与进度校准](../reports/plugin-service-sdk-2026-09-26.md)：基于最新主线 `d9c0431`（test.56），新增 Rust／C／C++ 服务 codec、原帧关联与一次完成、六类项目模板中的 service 原包流程和显式服务打包声明。Linux 本地已验证三语言原包→真实 TCP 节点的七方法、二进制正文、未批准绑定、错误认证及撤权；SDK、工具、核心打包与独立原生 codec 结果见报告。完整文件系统、长时/持久恢复、Windows／Flutter 和 SDK 冻结仍开放；未使用 Actions/CI，不创建 Release。

2026-09-25 [开发分支同步与本地 Windows 编译](../reports/development-sync-2026-09-25.md)：开发分支快进主线 `d9c0431`，应用 `0.1.9-test.56+60`；纳入 Web 本地 Rust 工作台、媒体持久化、受限主题包导入及 Windows 交互/瀑布流/退出修复。当前状态以 [插件系统状态](PLUGIN_SYSTEM_STATUS.md) 与 [Web 对齐表](WEB_PARITY.md) 为准。以下旧条目保留当轮事实，不代表当前仍未发布。

近期优先级：

1. 排查 CI 36115685547 的孤立数据场景导航中断；重新验证 fresh/legacy/orphan/media/theme 全部浏览器门禁后再推进 Pages 部署。最新主线尚无全绿证据。
2. 继续三语言入站服务 SDK、完整文件 IO、异步续接与跨重启核对；现有 IO codec 不等于完整 SDK 冻结。
3. Web 推进旧库迁移、备份恢复、持久草稿/附件/字体、大库与配额，再补通用外部插件和网络/服务平台能力；正式 UI 完整自动保存仍开放。

2026-09-24 [源码同步与插件系统状态](PLUGIN_SYSTEM_STATUS.md)：归档下列两轮三语言 IO SDK、真实网络及项目工具增量；主线与开发分支同步，不创建 Release、不变更版本或旧兼容原件。最新完成范围和七类开放工作以此状态页为准，下面“未推送”保留各轮报告完成时的历史状态。

2026-09-24 [三语言 IO SDK 真实 HTTP](../reports/plugin-io-network-sdk-2026-09-24.md)：Rust／C／C++ guest 经受管 worker 与 TCP 验证七种方法、二进制／重复参数头、凭据注入和 429、发送前拒绝、断线 Unknown 不重发及同库重开保留。29 项既有 HTTP 与四项三语言专项、严格 Clippy 和前置 IO／旧插件门槛通过；新增 IO 项目模板、显式能力/预算输出，31 项工具及九项打包 CLI 用例通过，三个生成 HTTP 原包以工作台 forward profile 再次通过真实网络专项；公网服务商、入站服务 SDK、流式与完整跨进程恢复仍开放。未推送发布。

2026-09-24 [三语言 IO SDK](../reports/plugin-io-sdk-2026-09-24.md)：C／C++／Rust 受管 IO codec、单次 Wasm 调用与原帧完成已接入；53 项 Rust SDK、15 项 Python、12 项旧原件兼容及五项覆盖三语言的 IO 专项通过，原生 C／C++ 和真实核心适配器通过。HTTP 此轮仅证明核心回执穿过真实 guest，真实 transport／服务节点 SDK 接入与项目模板列为下一步；完整文件系统、流式、跨重启 Unknown 与 SDK 冻结仍开放。未构建应用、提交或推送发布。

2026-09-24 [test.55 Windows 测试预览](../reports/0.1.9-test.55-release.md)：应用升为 0.1.9-test.55+59，汇总七风格/深度、控件过渡、设置修复、编辑恢复底座和安全后台关闭。168 项发布回归与 4 场景双进程阶段验证、协议/语言生成检查及成品自检通过；正式 UI autosave、SDK 冻结及其他平台资格继续开放。

2026-09-24 [组件分离与安全后台关闭](../reports/surface-shutdown-fixes-2026-09-24.md)：限制七风格外投影范围，收紧高深度阴影与卡片悬停，增加紧密按钮间隔；专用关闭页可后台等待，失败/长期未完成重新提示，真实退出才释放所有权。监听器与 worker 并行收尾，保留实际 join 门槛。47 项 Dart、31 项 Rust、静态分析与 Windows Release 通过；没有产品退出耗时或实机点击资格结论，正式 autosave 仍开放。未推送/发布。

2026-09-24 [草稿捕获完整性门槛](../reports/editor-capture-completeness-2026-09-24.md)：附件元数据捕获失败现传播至会话，停止旧快照自动保存并阻止关闭/驱逐；原号 Unknown 核对不会清除当前未捕获状态，部分恢复和同步重入不会误清失败。73 项组合（含真实宿主持久字节/重开验证）及静态分析通过。正式 UI autosave、原始值/附件/富捕获的工作台所有权和 S1 清理门槛仍待接线；未重建应用或推送发布。

2026-09-24 [设置保存与控件视觉修复](../reports/settings-style-fixes-2026-09-24.md)：停用内置插件后应用设置通过宿主受控事务保存，卡片权限不变；字体页进入共享画布导航；粗野主义侧影/按钮轮廓打磨；补齐开关图标、拖动投影及工作台/插件页复选框过渡。106 项不同 Dart 测试（含 14 项成品宿主验证）、Rust 18 项、静态分析及 Windows Release 通过。既有开关图标/拖动投影/复选框开放项本轮关闭；GPU Profile、正式 autosave 与 SDK/平台资格继续开放。未推送/发布。

2026-09-24 [风格深度滑块](../reports/style-depth-2026-09-24.md)：六种非扁平风格支持 0–200% 深度，默认 100%，主设置实时预览/重置，组件独立覆盖或跟随主题/其他组件。新增字段贯通私有 wire、Protobuf、JSON 迁移和 Rust 校验，旧数据缺省维持原外观。67 项组合（含 7 项真实宿主）与 1 项对照图、插件 9 项及宿主 10 项通过，12 目标分析/语言/协议检查通过，Windows Release 已更新；内置插件 test.54.6，应用版本不变。未推送/发布。

2026-09-24 [开关轨道与凹槽交接](../reports/switch-style-transition-2026-09-24.md)：复现并修复新拟态开关装饰瞬间替换；加入可中断轨道/凹槽与明暗过渡，保留原生交互、主题颜色优先级及高对比/Apple 自适应。62 项相关回归、两目标分析、原金图及 Windows Release 通过。图标/拇指几何、拖动投影对齐、复选框、首绘 Raster 和正式 autosave/SDK 仍开放；未推送/发布。

2026-09-24 [七风格滑块连续变形](../reports/style-slider-morph-2026-09-24.md)：主设置、组件设置、调色罗盘与随身听九处滑块保持原生身份，尺寸/圆角/轨道/光照连续过渡，反向从当前画面继续；处理离散端点留白、减少动画与隐藏收敛。57 项相关回归、七目标分析及 Windows Release 通过；本轮无 GPU 性能结论。未推送/发布；开关等控件全路径、首绘 Raster、正式 autosave 与 SDK 仍开放。

2026-09-24 [风格滑块交互与绘制](../reports/style-slider-contract-2026-09-24.md)：修复新拟态轨道跳过启用动画、遗漏缓冲段与覆盖调用方颜色，自绘滑块按钮遵循启用/禁用颜色及主题线条透明度。43 项相关回归、三目标分析和 Windows Release 通过；实际拖动中切换七种风格保留 State/焦点和单次起止回调。未推送/发布；滑块形状连续变形、开关风格过渡、首绘 Raster 与正式 autosave/SDK 仍开放。

2026-09-24 [按压过零与半透明填充](../reports/style-press-continuity-2026-09-24.md)：六种非扁平风格的浮雕边缘由正负两侧连续淡出，零深度保留显式填充与焦点/高对比提示；修复新拟态 40% 底色重复绘制成约 64% 的问题。39 项回归、三目标分析与 Windows Release 通过；两张金图仅修改经审查的凸起示例圆角像素。未推送/发布，不宣称 GPU 提升；首次 Raster、其他控件全路径及正式 autosave/SDK 继续开放。

2026-09-24 [七风格输入边框过渡](../reports/style-input-transitions-2026-09-24.md)：复现并修复新拟态与实验/扁平风格切换时凹槽消失、新拟态明暗中点跳变及标签缺口问题。14 个风格/主题端点的 182 条像素路径、IME/选区/焦点保留与原金图纳入 36 项回归，五目标分析和 Windows Release 构建通过；补齐上一轮提交证明的四项成品包原生验证。未推送/发布，不宣称 GPU 改善；首绘性能、正式 autosave owner 与 SDK 资格继续开放。

2026-09-24 [历史提交证明与延迟确认](../reports/editor-commit-proof-deferred-ack-2026-09-24.md)：新增私有只读 inspectEditorCommit，V1 新卡原会话可取得真实首提交证明，V2 支持只读观察与显式延迟清理；正式 UI 默认行为未切换。真实测试覆盖新卡父子交接/删源/重开、证明回执损坏后的只读重试及历史确认兼容，52 项 Dart 回归与十目标分析通过。下一步由工作台 owner 将最新完整草稿、提案持久与清理条件接入正式界面；SDK/富捕获/关闭与平台资格仍开放，未推送发布。

2026-09-24 [父子草稿附件目录](../reports/editor-draft-handoff-assets-2026-09-24.md)：完整父 pin/原始值与 ParentLink 校验、子首保存后来源晋升、释放后拒绝重选、重启后重新导出预览已由目录与真实宿主测试验证。修复显式 MIME 绕过 TextureKind 校验。29 项组合无跳过、五目标分析通过；本轮未重建完整应用或推送发布。正式 autosave 仍开放，下一步区分 V2 延迟确认清理与 V1 首 Create 提交证明接口。

2026-09-24 [草稿交接恢复入口](../reports/editor-draft-handoff-recovery-ui-2026-09-24.md)：工作台历史菜单可发现/核对持久交接并分别确认建立后继、退休父草稿或取消；coordinator 随工作台保留 Unknown 与在途请求。写前刷新插件/来源，切库移除旧恢复/确认路由。41 项组合（含两个真实宿主工作台流程）、分析、九语言检查及 Windows 构建通过；正式编辑器自动保存仍待接线，未推送/发布。

2026-09-24 [持久交接私有协议与独立客户端恢复](../reports/editor-draft-handoff-proposal-wire-2026-09-24.md)：六个动作和 Dart codec/native 已连接；四类丢回执在两个独立客户端进程中完成只读发现与恢复，另有四项在线损坏回复清理验证。真实端到端测试修正了单条/分页字段及 absent 操作号不一致；新旧路径回归与 Windows Release 构建通过。正式编辑器自动保存、富捕获来源、关闭/切库、满容量及跨平台仍开放，未推送/发布。

2026-09-24 [风格列表透明过渡对照](../reports/style-collapse-experiment-2026-09-24.md)：两轮候选与相邻恢复基线未证明取消淡入淡出的稳定收益，已撤回候选并精确恢复生产源码。新增真实停画/恢复、焦点语义与反转状态回归，最终 51 项 UI 通过。既有 Release 摘要匹配且正式入口配置已恢复，不重复构建相同产物；首次浮雕编译峰值继续开放，未推送/发布。

2026-09-24 [风格选择 Profile 与模糊核优化](../reports/style-selection-profile-2026-09-24.md)：真实设置入口完成三种玻璃材质 39 次风格切换采样，定位首次 shader 编译峰值；固定浮雕模糊核并让下沉阴影随深度淡入。48 项 UI 回归通过；磨砂首次新拟态峰值由两轮基线 85/101 ms 降至本轮最终 69 ms，但首次 Raster 及超透热切换仍有超预算帧，不宣称全程 60 Hz。Windows Release 结果见专项清单，未推送/发布，SDK 与跨平台开放项不变。

2026-09-24 [明暗光照连续性](../reports/style-lighting-2026-09-24.md)：修复共用表面动画中点的深浅光照跳变，快速反向切换沿用当前光照；输入凹槽从最多四组边缘降为两组。47 项 UI 回归与四目标分析通过，Windows 构建结果见专项产物清单。未提交/推送/发布，GPU 性能与跨平台验收仍开放。

2026-09-24 [风格切换过渡](../reports/style-transitions-2026-09-24.md)：共用表面改为可中断的七项权重与几何插值，固定子节点，扁平淡出、减少动画、隐藏及零深度场景收敛；实验输入边框增加凹槽/明暗插值与 90 条端点像素检查。最终 45 项 UI 回归通过，本地 Windows 预览更新，未推送或发布。GPU 性能、InputDecorator 端到端时序及跨平台资格仍待验收。

2026-09-24 [五种实验风格](../reports/experimental-styles-2026-09-24.md)：纸感、黏土、Fluent、轻量粗野与精密器物接入卡片和主要控件，保持扁平默认及风格列表默认收起；九语言、透明玻璃、编辑状态和 Rust 保存重开验证完成。内置插件升至 `.54.5`，应用版本不变；本地 Windows 预览更新，未推送或发布。新风格仍实验性，平台性能、第三方自绘控件覆盖与风格间过渡继续打磨；旧程序读取新风格前需先恢复已支持的风格。

2026-09-23 源码同步检查点：收录 test.54 发布后本工作树累计的版本化内容、编辑恢复与持久草稿底座、UI 生命周期、新拟态/组件跟随、外置语言与字体、跨版本迁移工具及对应验证记录。同步开发分支和主线，不创建标签或更新 Release；应用仍为 `0.1.9-test.54+58`，内置包 `0.1.9-test.54.4`。各专项报告中的“未推送”是当轮验收时的历史状态；开放项和平台验收边界继续有效。

2026-09-23[新拟态控件打磨与跨版本迁移](../reports/neumorphism-migration-2026-09-23.md)：按钮、输入、滑轨、开关、复选框、导航/材质选项及播放器分层凸起/下沉，风格列表默认收起，保留焦点和编辑状态；浅色/深色视觉核验完成。新增 Windows 双格式迁移工具，旧 JSON 与受保护 SQLite 默认预检并输出新目录，逐卡/审计/历史/媒体核对，失败目标标记隔离；修复重复迁移的媒体操作冲突。56 项测试通过（44 UI、8 迁移、4 最终宿主），14 目标分析及九语言资源检查通过，Windows Release 已构建且成品迁移 smoke 通过。范围与待决提案、旧字段、插件旁置配置及托管字体边界见报告和[用法](MIGRATE_LIBRARY.md)。未运行真实用户库迁移；未提交/推送/发布，原 SDK/编辑器待办保持开放。

2026-09-23[宿主持久草稿交接提案](../reports/editor-draft-handoff-proposal-2026-09-23.md)：发送前固定完整child/ParentLink/退休号，提供准备、只读发现核对、建子、条件退休和取消；已备/取消提案阻止旧入口绕过，原操作封存防复活。修复父退休后暂存清理被误挡的问题。Rust组合50项和最终Windows旧客户端13项通过无跳过，包含三个独立Rust进程退出/恢复，Windows Release重建。新私有wire/Dart和正式autosave尚未接线，按[接线计划](EDITOR_HANDOFF_PROPOSAL_WIRE_PLAN.md)继续；未提交/推送/发布。

2026-09-23[风格、玻璃动效与组件跟随](../reports/ui-style-follow-2026-09-23.md)：修改前源码与证据已独立归档；主设置新增扁平/新拟态，改进玻璃过渡、按压与贴合展开、部分组件右键菜单。组件材质支持多叉树与跟随链，UI/Dart/Rust 拒绝循环，解除后恢复自身参数，默认主题/特殊设置不变。37 项 UI、12 项 Rust、7 项真实宿主测试通过无跳过；17 目标分析、九语言与协议生成检查通过，Windows Release 已重建。内置包 test.54.4，应用 test.54+58 不变。原深度审查/SDK 开放项继续保留，未提交/推送/发布。

2026-09-23[宿主父子草稿交接](../reports/editor-draft-handoff-host-2026-09-23.md)：完整父快照/附件pin复制、真实S1提交证明、唯一持久后继、条件退休与只读关系发现已落地；修复双活动时先删子及祖先未退休导致的死结，阻止冻结父继续新写/导入。新集成13项、原回归57项、单元19项通过；Windows Release构建及新宿主客户端7项通过。私有wire/Dart尚不表达新关系，旧入口明确拒绝而不丢字段；正式编辑器autosave、交接Unknown恢复、历史维护及SDK冻结继续开放。未提交/推送/发布。

2026-09-23[草稿会话归属与附件来源](../reports/editor-draft-ownership-2026-09-23.md)：工作区持有会话、单活动视图租约、有界关闭保存和失败恢复已实现；附件来源显式登记，已确认pin采纳不制造新编辑，S1未提交时在途S2继续保存。真实测试发现并修复MIME与界面分类混淆。最终 **60项通过无跳过**、9目标分析无问题、Windows Release增量构建成功。删源附件、S1迟到回执、视图分离、宿主重开与后续保存已验收；正式编辑器autosave和跨draft父子交接仍按[接线计划](EDITOR_UI_PERSISTENCE_PLAN.md)推进。未提交/推送/发布。

2026-09-23[工作台附件决定恢复入口](../reports/editor-draft-import-recovery-ui-2026-09-23.md)：现有历史按钮新增九语言附件决定恢复页，包含失活草稿，只读查看/关闭不执行业务；操作前精确核对，确认取消封存原请求。新增全程串行扫描阻止页间写入；已确认动作后刷新失败不误报Unknown，陈旧操作封禁且手动刷新可用。原编辑器仅选区/方向变化被S1回执关闭的问题已修。最终组合 **68项全过**、11目标分析和语言检查通过，Windows Release重建。正式正文/附件autosave继续按[接线计划](EDITOR_UI_PERSISTENCE_PLAN.md)推进，未提交/推送/发布。

2026-09-23[附件放弃决定与跨客户端进程恢复](../reports/editor-draft-import-decisions-2026-09-23.md)：独立决定 journal 固定原操作/基线；取消原子封存旧操作号，精确提交与自动失活分开判定。私有动作 109–113、分页全局范围发现及 Dart 只读恢复已接入，包含失活草稿与满额度旧提交确认。Rust 最终组合 **59 项**、Windows 客户端组合 **34 项**通过，另两独立进程对 **5 种场景**完成准备/恢复验收；Windows Release 重建、9 个分析目标及生成检查通过。正式编辑器/自动保存/恢复 UI、扫描并发边界和长期历史维护仍待接入；未提交/推送/发布。

2026-09-23[持久导入私有协议与客户端](../reports/editor-draft-import-client-2026-09-23.md)：七个类型化动作接入固定提案、删源重试、分段核对、精确缓冲释放及导出不覆盖；新旧草稿共享整段传输队列。独立 Dart 会话保留 Unknown/原放弃意图，修复本地失败清除旧 Unknown 和绕过待决放弃的问题。Rust 集成 **50 项**、协议单元 3 项、门禁 1 项通过；最终 Windows 客户端 **28 项通过**，静态 8 目标及生成检查通过，Release 重建。UI 和应用进程重启后的待决决定恢复仍未接线，不能将保留调用方意图的宿主重开测试标成完整应用恢复。未提交/推送/发布。

2026-09-23[宿主持久附件导入与清理](../reports/editor-draft-staging-host-2026-09-23.md)：固定 Pending/Ready 提案、可释放字节归属、重启列举/导出、精确消费与显式放弃已接入 Rust 库。正文与消费标记原子保存，失败清理仍计费；修复自动清理和显式放弃的历史回执混淆，预留达到修订上限前的清理余量。最终宿主组合 **46 项通过**，Windows Release 重建及既有原生客户端 **15 项通过**；包含真实子进程崩溃、SQLite 失败、跨草稿共享释放和 256 修订边界。私有协议/Dart/恢复 UI 尚未切换，长期历史与全局容量压力验收继续开放；未提交/推送/发布。

2026-09-23[附件可释放持久归属原语](../reports/editor-draft-retained-staging-2026-09-23.md)：Core 原子提交字节与 Snapshot owner，精确重试只读核对原字节，支持唯一归属恢复及合法未知字段保留；未选附件不通过 CardRecord 引入永久事件引用。Core 附件/真实崩溃组合 30 项通过，Windows Release 重建及实际客户端附件/编辑组合 15 项通过。宿主 draft_staged 仍为内存归属，本轮没有宣称未选附件已可在 UI 恢复；下一步按[接入方案](EDITOR_DRAFT_STAGING_PLAN.md)补固定导入 Pending/Ready、消耗/放弃、列表导出和 Unknown。完整审查及 SDK 门槛保持开放，未提交/推送/发布。

2026-09-23[新建卡片的独立草稿来源](../reports/editor-draft-new-card-2026-09-23.md)：显式区分已有卡片与新卡草稿，首次目标冲突拒绝，不提前创建正式内容；后发生的正式内容保留，草稿基线不变。先建活动 journal 再导入附件，重启恢复不写业务；空白新卡可显式初始化。Rust 库 12 项、存储/协议/恢复 29 项通过，最终 Windows 客户端组合 106 项通过无跳过，7 目标分析及 Windows Release 构建成功。主编辑器默认自动保存尚未接线；来源交接、持久暂存/历史维护、富捕获和恢复冲突界面继续推进。未提交/推送/发布。

2026-09-23[独立编辑草稿会话与视图绑定](../reports/editor-draft-session-2026-09-23.md)：当前输入、已确认代次、冻结操作与 Unknown 分开管理，完整选区/IME 变化进入草稿；视图分离不销毁会话或静音后台观察，历史/迟到回执不覆盖当前值。V2 确认后保留精确 S1 来源描述，真实宿主验证后续草稿可引用源/S1 两代附件。最终组合 101 项通过，最后绑定边界专项 8 项通过（有重叠），9 文件分析无问题，Windows Release 重建。主编辑界面尚未默认启用此自动保存：新卡来源类型、在途来源切换/身份退休、富捕获持久来源、暂存/历史维护、恢复冲突界面仍须完成。完整审查与 SDK 冻结门槛保持开放；未提交/推送/发布。

2026-09-23[编辑草稿私有协议与原生客户端](../reports/editor-draft-protocol-2026-09-23.md)：新增独立 Cap’n Proto 契约、8 MiB/32 KiB 有界分片及固定操作回执核对，保存/读取/放弃的首片丢失可通过精确关联清理缓冲，EOF 后重开使用原操作核对。Rust 协议 6 项、最终客户端组合 81 项全部通过；Windows Release 已重建，原传输和存储恢复回归通过。当前未接自动保存和恢复界面，未配对 UTF-16 单元明确拒绝；持久暂存归属、历史维护、富捕获来源、语言/字体提案、迁移入口和完整错误协议仍开放。未提交/推送/发布。

2026-09-23[宿主持久草稿记录](../reports/editor-draft-journal-2026-09-23.md)：独立草稿代次 CAS、完整原始编辑值、固定历史操作核对、草稿级附件暂存/持久引用、S1 历史证据与明确放弃已在 Rust 宿主库实现。模型/计费 9 项、新旧真实宿主组合 19 项通过（含两个子进程夹具入口）；Windows Release 重建，最终宿主的既有客户端组合 92 项全过。私有协议、Dart 自动保存与恢复 UI 尚未接线；历史/失败暂存维护、256 身份限额后的处理及严格 Clippy 既有告警继续开放。未提交/推送/发布。

2026-09-23[编辑保存与新草稿共存](../reports/editor-draft-coexistence-2026-09-23.md)：保存等待/Unknown 期间允许继续文字输入，原 S1 固定核对；确认后以精确修订打开 V1/V2 后续会话，保留 S2 与选区，须再次明确保存。已保存附件按原身份继承，原文件删除后仍能继续；修复别名失败的会话丢失、旧保存竞争和九语言窄屏溢出。最终 Release 产物组合 **92 项全过、无跳过**，11 个分析目标无问题，语言目录 7 项/22 生成产物通过；Windows Release 重建。S2 仍只保留在当前窗口，跨重启草稿/来源恢复、语言字体提案、迁移入口和统一错误协议继续开放；未提交/推送/发布。

2026-09-23[V2 编辑持久恢复与放弃](../reports/editor-recovery-2026-09-23.md)：固定原操作、捕获证据和附件在内容提交前持久保存；重启只读核对，显式恢复不重跑 guest。冲突提案可明确放弃，原操作在同一事务中封存，防止名额复用后的迟到重放。已补切库、展示失败和确认失败保护。最终 Windows 组合 61 项通过、Rust 恢复专项 9 项通过、九语言窄屏与词条检查通过，Windows Release 重建。活动16槽/64MiB不等于历史磁盘上限；S2/提交前草稿、语言字体提案、损坏隔离、容量压力矩阵、迁移入口和完整错误协议仍待推进。未提交/推送/发布。

2026-09-23[版本化编辑未提交判定](../reports/versioned-no-commit-2026-09-23.md)：直接 V2 待办/卡片编辑仅在提交前失败且按原卡片/操作确认无提交时解除冻结；提交阶段、回执丢失、历史重试及通用错误保持待核对。打开的详情同步当前源并保留草稿，已发生的未知操作不会因本地未发出的重试被清除。真实 SQLite 故障和原生代理测试验证边界，最终组合 37 项全过、Rust 2 项通过、静态检查 9 目标无问题，Windows Release 已重建；结果与边界见报告。该分类未扩展为迁移或捕获编辑的完整 ErrorEnvelope；跨重启编辑恢复、正式迁移门槛及 SDK 冻结仍未完成。未提交/推送/发布。

2026-09-23[主工作台混合格式与 TaskId](../reports/mixed-workspace-ui-2026-09-23.md)：正式主界面已接混合加载/查询、三态待办、普通字段捕获编辑及分类/收藏/删除恢复。V2 拒绝旧 JSON 保存，原操作重试与内容所有者隔离；修补窄屏溢出、Undo 和附件重复缓存。Windows Release 重建，最终组合 29 项全过（含真实混合库重开挂载主UI），18 项静态目标无问题、语言测试 7 通过；此前 UI 组合 47 通过/1 跳过，边界见报告。迁移按钮未开放；下一步可靠 NoCommit 分类、迁移门槛及跨重启编辑恢复。未提交/推送/发布。

2026-09-23[混合格式读取与展示准备](../reports/versioned-workspace-read-2026-09-23.md)：新增完整只读扫描与纯展示模型，保留 V2 TaskId/歧义/BigInt，核对并发已知修订后交付。扫描 5、展示 3、真实客户端 5、捕获编辑 2，共 15 项通过，6 个分析目标无问题。真实混合库缺包重开仅验证新读取入口，旧主界面尚未切换；[入口矩阵与接线门槛](../reports/versioned-workspace-integration-plan-2026-09-23.md)已记录。迁移入口保持未开放，未提交/推送/发布。

2026-09-23[V2 捕获编辑保存](../reports/captured-cards-editor-2026-09-23.md)：普通文本、附件和真实剪贴板转换观察接入同一 V2 提交；固定原操作重试，保留 TaskId/歧义/来源，不改旧投影。纯投影 4、实际 owner 4、旧路径 15、最终 Windows 原生组合 13 项通过；4 个静态目标与生成检查通过，Windows Release 已构建。主 UI 与跨重启编辑恢复仍待接入；版本不变，未迁移用户内容、未提交/推送/发布。

2026-09-23[版本化私有客户端](../reports/versioned-content-client-2026-09-23.md)：接通 V1/V2 读取、分页、固定迁移、TaskId/普通卡片编辑与混合查询；修复迁移后仍用 V1 启动探测的问题。历史回执与当前读取分离，提交后刷新失败保留原回执及修订；查询按原 operation 分页，无效续页不能创建新查询。Rust 消息/分页 5、真实路由 1、Dart codec 7、最终原生组合 11 项通过，6 个分析目标无问题，Windows Release 重建成功。应用 `.54+58`、插件 `.54.3` 不变；下一步 V2 编辑器来源与正式 TaskId/歧义界面。迁移入口未开放，未标 SDK 冻结，未提交/推送/发布。

2026-09-23[混合格式查询](../reports/query-v2-capture-2026-09-23.md)：独立query.v2 guest与真实Workbench入口支持V1/V2同库筛选，V2阶段保持显式值，待确认TaskId纳入待办；所有筛选/排序/归并有实际观察与固定快照来源。新规则5、旧插件12、新旧调度/guest18、新旧捕获及故障注入23、最终原生客户端7项通过；130张混合卡片归档在备份恢复且临时源库移除后重放通过。Windows Release已重建，插件`.54.3`，应用仍`.54+58`。旧查询与已有内容契约摘要未变；下一步私有客户端协议、新版编辑器来源和Flutter TaskId接线；未开放迁移、未标SDK冻结、未提交/推送/发布。

2026-09-23[V2 普通卡片操作](../reports/cards-v2-owner-2026-09-23.md)：接通正文/标题、附件、收藏、明确类别阶段、删除与恢复的独立规则及真实 Workbench 宿主入口，保留 TaskId、来源和未知字段；补上已删除卡片的新任务写入门禁。新插件规则 5、纯投影 2、实际 guest/独立回放 4、owner 5 项通过，原 TaskId 7、旧宿主兼容 20 项通过；Windows Release 重建及最终产物客户端 7 项通过。第一方插件 `.54.2`，应用仍为 `0.1.9-test.54+58`。下一步为混合查询、私有客户端协议、新版编辑器来源和 Flutter TaskId/歧义接入，迁移按钮未开放；T31–T34 与 SDK 冻结仍未完成。未提交/推送/发布。

2026-09-23[TaskId 实际宿主读写](../reports/tasks-owner-and-edit-2026-09-23.md)：Workbench 库已接固定基线迁移、七命令任务编辑和显式 V1/V2 读取，实际 Rust Wasm 证据进入原核心事务；旧操作跨重开不重执行，缺包/禁用仍可只读，附件保持可导出。读取 3、纯投影 3、真实执行证据 3、实际 owner 4 项通过；旧投影/迁移/CLI 兼容 17 项通过。Windows Release 重建及新宿主客户端 7 项通过。下一步为私有客户端协议、V2 普通卡片动作、混合查询和 Flutter TaskId/歧义界面；未开放迁移按钮，不标记 T31–T34 完成。未提交/推送/发布。

2026-09-23[历史回复与工作台隔离](../reports/late-content-replies-2026-09-23.md)：成功回执后读取当前卡片，修订缓存按 unsigned u64 单调更新；旧编辑/删除回复不能覆盖更新内容，旧工作台回复不能进入新工作台。切库清旧卡、刷新核对缺失 ID/修订，已确认提交后的刷新失败不生成新业务操作。Windows Release 完整重建成功，新产物原生组合 14、Flutter 相关组合 33、语言资源 7 项通过；未执行原生页面扫描故障注入和完整窗口验收；后续 TaskId 正式界面仍未接通。未提交/推送/发布。

2026-09-23[TaskId 宿主投影与旧写入防线](../reports/tasks-migration-projection-2026-09-23.md)：实际 Rust Wasm 捕获绑定完整原件、选定包、固定迁移器和确定 operation；原库重开、独立重放及固定历史样本通过。旧实时 SetContent 仅允许 format 1，schema 2 版本感知编辑绑定完整源 Card；旧历史解释和已提交操作重试保留。核心组合 65、宿主组合 34 项通过；Wasm 仅 core 编译检查通过。正式迁移 UI、格式感知读写与歧义确认仍未接通，不代表 T31–T34 完成。本轮按用户要求使用原生 GPT-6-sol/high 多代理，未使用 SubagentBridge；未提交/推送/发布。

2026-09-23[核心格式迁移事务](../reports/content-migration-atomicity-2026-09-23.md)：新增独立版本化命令，固定完整源 Card，原子提交格式/正文/证据并保留附件、关联和未知字段。核心相关 59 项通过，涵盖 SQLite 空间耗尽、证据失败回滚、并发基线拒绝和跨重开重试。实际 TaskId 的宿主证据投影、旧写入者门禁及 Flutter 入口仍待接通，用户卡片尚未迁移，T31–T34 不标整项完成。

2026-09-23[TaskId / 待办 V2 引擎](../reports/tasks-v2-engine-2026-09-23.md)：新增独立 V2 格式、稳定任务身份、歧义来源、明确任务动作与插件投影，实际 Rust Wasm 在原预算下验证迁移和阶段独立性。第一方插件升至 `0.1.9-test.54.1`。核心迁移事务的后续进展见上条；证据投影、旧写入者提交防线和 Flutter 接入仍待完成，主卡片继续使用 V1；不得将纯引擎测试记作 T31–T34 全链路通过。

2026-09-23[深度复核 v1.2 核验与兼容修补](../reports/deep-review-ruling-2026-09-23.md)：接入单飞关闭/恢复协调，原进程退出前禁止重开；偏好保存固定原操作并阻止失败后排队草稿自动覆盖，区分提交成功与回读待确认。后续加入原库中的有界恢复提案、基线绑定和历史查询，重启不自动重发；已提交提案只核对，不重新上传。再补入九语言恢复界面、基线冲突的明确放弃决定及原操作 ID 封存，旧决定无法清理新提案，新草稿不回滚或自动提交。修复核心客户端 JS Uint64 访问器及旧待办 UI 的阶段/进度分叉。V1 联动操作增加明确确认，未重写冻结投影。TaskId/V2 迁移、通用远端 Unknown、新草稿持久恢复和完整 ErrorEnvelope 仍未完成，不能记作 SDK 冻结；验证矩阵及未执行范围见报告，未推送/发布。

2026-09-22[外置语言配置与自定义字体](../reports/language-registry-and-fonts-2026-09-22.md)：语言清单、自称和回退规则收归独立 JSON，统一生成 Dart／Rust 清单及编译语言资源；增加语言需补齐 ARB 后重新构建。字体设置支持系统字体族、TTF／OTF 本地导入、重开恢复与重置，切换保留编辑状态，九语言文案齐备。验证与平台边界见报告；私有宿主协议有新增，公共插件 SDK 不变，SDK 冻结任务继续按原计划推进，未推送／发布。

2026-09-21[工作台 UI 生命周期优化](../reports/ui-lifecycle-optimization-2026-09-21.md)：基于 test.54，主卡片改为稳定身份和惰性瀑布流，同页查询等待时隐藏保留视图、经宿主确认后更新；高频设置有限保活，支持内存压力回收。增加有界查询操作引用、插件目录快照及 HTTP 草稿恢复，隐藏展示停止轮询，插件异步关闭失败继续跟踪。后续重点是万卡首次数据准备、长期 Dart／原生／GPU 资源计费与真实媒体／插件负载测量，再推进玻璃绘制专项；本轮不代表全部生命周期路线或 SDK 已冻结。验证数字与本地 Windows 构建状态见报告，未推送／发布。

2026-09-21[test.54 Windows 测试预览](../reports/0.1.9-test.54-release.md)：应用升级至 `0.1.9-test.54+58`，将下列三轮开库优化汇入本次源码与 Windows 预览发布；保留旧条目的当轮未发布状态作为历史记录。每次开库仍完整校验，本版不改变存储格式、内置插件包或 SDK 契约。后续先测量归档增长来源与剩余串行热点，再评估读取／计算流水线及历史分层；自动清理、跨启动验证缓存与 SDK 冻结均未包含。

2026-09-21[开库读取与关联校验优化](../reports/library-read-verification-2026-09-21.md)：当前格式在同一只读校验事务中复用已验证证据大小与归档摘要，减少重复解码和分块遍历；每类最多 4,096 条，超限回退完整检查，旧格式保留原校验顺序。实际库副本按 AB/BA 各四次启动，宿主就绪中位数 1.810→0.888 s、遮盖移除 2.222→1.299 s，整体再缩短约 42%。Core 568、Audit 97、原生集成 2 项通过，Windows Release 已更新；既有测试 lint 单独记录，未推送/发布，版本与 SDK 计划不变。

2026-09-21[开库并行校验对比](../reports/library-open-parallel-2026-09-21.md)：数据库仍由原线程持锁读取，最终证据校验按两份有界数据并行计算，小库／单核／Web 回退串行。实际库副本按 AB/BA 顺序各四次启动，宿主就绪中位数 2.005→1.799 s、遮盖移除 2.420→2.213 s，约再缩短 0.21 s；保留优化。Core 565、Audit 97、原生集成 2 项通过，Windows Release 已重建，既有 Core 测试 lint 仍单独记录；未推送/发布，版本与 SDK 计划不变。

2026-09-21[内容库开库流程优化](../reports/library-open-optimization-2026-09-21.md)：当前格式、已绑定身份的原生 WAL 活动库将四次完整校验合并为持锁事务内一次，每次重开仍完整验证；旧格式迁移和其他后端保留原路径。实际约 101.9 MB 内容库副本各三次进程启动，遮盖移除中位数 7.677→2.393 s（Dart 入口起），加载页到遮盖移除 7.558→2.271 s。Core 564、Audit 含故障注入 97、真实宿主集成 2 项通过，Windows Release 已重建；Core 全目标 Clippy 的既有测试 lint 与个别绘制峰值仍保留。未推送/发布，版本与 SDK 后续任务不变。

2026-09-21[test.53 发布准备](../reports/0.1.9-test.53-release.md)：应用升级至 `0.1.9-test.53+57`，汇总当前开发线与设置/多语言/保存/启动改动；Windows Release 构建及 356 项常规回归、2 项真实宿主集成、4 项语言资源、6 项生成器和 3 项 Rust 定向回归通过。64 项需额外环境的常规测试跳过，另保留 3 条既有 info；不将此视为全平台或完整 SDK 验收。发布 Windows 包、源码和 SHA-256，标记测试预览版；后续仍按 Unknown、文件系统、三语言 IO SDK 与平台资格门槛推进。

2026-09-21[启动等待与切换跟进](../reports/windows-startup-transition-2026-09-21.md)：窗口初始化与宿主打开并行，固定启动根节点保留遮盖至工作台首帧，再以 100 ms 淡出；揭示开始即可交互，支持减少动画。三次 Windows Release 中位数：窗口可见 443→319 ms，工作台首帧 818→706 ms（Dart 入口起），遮盖完全移除 830 ms。9 项回归、4 文件分析与最终真实进程启动通过，Release 已重建；约 18–22 ms 绘制峰值及大库加载仍待优化。未推送/发布，SDK 任务不变。

2026-09-21[Windows 启动响应修正](../reports/windows-startup-2026-09-21.md)：轻量启动窗口与后端加载并行，数据恢复前不挂载可写工作台；重复安装同摘要插件包改为完整校验后直接复用，避免多余写盘。三次 Release 进程实测窗口可见中位数从 1.215 s 降至 0.443 s；复用玻璃绘制后，正式工作台首帧中位数从 0.928 s 降至 0.818 s（Dart 入口起），不混同启动页与加载完成。7 项界面、21 项 Rust、2 项真实宿主回归及相关分析通过，Windows Release 已重建。大库/附件按需加载与首帧 GPU 开销仍待后续优化；未推送/发布，SDK 后续任务不变。

2026-09-21[设置首开卡顿修正](../reports/settings-cold-navigation-2026-09-21.md)：设置导航入口去除昂贵水波纹，页面统一无快照淡入淡出。Windows 300 卡片冷启动复测，磨砂列表/编辑 Raster 峰值从 24.776/23.752 ms 降至 4.864/15.394 ms，六轮连续切换无超 16.7 ms 帧；液体玻璃仍有约 20–21 ms 的个别峰值，保留为后续性能观察项。39 项回归和相关分析通过，Windows Release 已重建至 `build/windows-corners/x64/runner/Release`；未调用 SubagentBridge，未推送/发布，SDK 后续任务不变。

2026-09-21[设置切换性能与九语言界面](../reports/settings-navigation-locales-2026-09-21.md)已完成：空间外观标题左对齐；组件列表按可见区域创建，普通材质跳过液体折射工作。当前 Windows 300 卡片 Profile 六轮切换的 UI 最大帧由 82.21 ms 降至 6.362 ms，超 16.7 ms 帧由 6 降至 0（限定设备/流程）。新增俄/法/德/西/日/韩/葡七语言共 6,195 条译文，接通原库语言保存及重开恢复，保留草稿/选区/查询身份。39 项界面组合、语言资源与真实原库/进程回归、七语言 Windows 窗口流程和相关分析通过；Windows Release 已更新至 `build/windows-corners/x64/runner/Release`。未调用 SubagentBridge，未推送/发布；SDK 后续任务不变。

2026-09-21[设置宽屏排版与统一标题栏](../reports/settings-width-and-headers-2026-09-21.md)已完成：设置图标与展开按钮等宽、对称；所有设置页保持单列，内容居中并限制 920 逻辑像素宽度，画布仍铺满。主设置、插件、IO 与组件页共用无组件材质的简洁标题栏。33 项 Flutter、8 文件分析和 Windows 设置视觉流程通过，覆盖超宽、窄屏、主题、返回动效及草稿保留。Windows Release 已更新至 `build/windows-corners/x64/runner/Release`；未调用 SubagentBridge，未推送/发布，SDK 后续任务不变。

2026-09-21[设置单列与窗体圆角](../reports/settings-single-column-corners-2026-09-21.md)已完成：展开完整设置按钮移到右侧；所有设置区块默认单列并铺满；Flutter 统一抗锯齿裁剪，原生命中区域留出边缘覆盖空间，磨砂层独立圆角裁剪。16 项相关 Flutter、2 项原生窗口回归、1 条设置视觉流程与实际屏幕像素检查通过，100% 缩放下验证 8/20/32 圆角及透明/磨砂恢复。因旧预览窗口占用，Windows Release 构建到独立 `build/windows-corners/x64/runner/Release`，原窗口未关闭。未调用 SubagentBridge，未推送/发布；SDK 后续任务不变。

2026-09-21[插件设置与保存修复](../reports/settings-plugins-and-save-repair-2026-09-21.md)已完成：整套插件管理归入独立页；子页响应式铺满，桌面主设置可展开；移除自动滚动条，独立组件新增玻璃类型/圆角/继承/重置。原内容库副本复现同版本不同摘要导致的 RevisionConflict，只读阻断外观与卡片保存；内置包改用 guest 独立版本并阻止同版本覆盖，保留显式启用规则，原库未修改。140 项 Flutter、21 项 Rust、3 项真实宿主、3 条 Windows 流程与双语/生成检查通过。Windows Release 已重建并核对配套宿主/插件摘要。未调用 SubagentBridge，未推送/发布；SDK 后续任务不变。

2026-09-21[设置分区与共享画布](../reports/settings-layout-and-canvas-2026-09-21.md)已完成：网络/文件 IO 管理移到独立设置页，统一模块间距；底部异常提示可关闭，tips 默认透明悬浮并支持独立材质。组件编辑与标题栏共用工作台画布，主题动态更新；服务界面草稿按后端保留，重挂仍刷新权限且不自动执行。138 项相关测试、21 文件分析、3 条 Windows 流程及双语资源验证通过，Windows Release 已重建。未调用 SubagentBridge，未推送/发布。SDK 后续任务保持不变。

2026-09-21[TLS 身份管理界面与 Windows 验收](../reports/application-service-tls-identity-ui-2026-09-21.md)已完成：双语身份选择/保存/替换/禁用、后端会话草稿、冻结修订与 Unknown 核对接入工作台。86 项客户端组合、2 条完整 Windows 窗口流程、4 项语言包与 6 项目录测试、9 文件分析通过；Windows Release 已重建。真实窗口验证保存后删除源 PEM、HTTPS、运行中轮换停服、原拥有者回收后刷新并显式选择新修订、旧证书拒绝、禁用与同库重开。下一项转向跨重启 Unknown 持久核对，再推进完整文件系统、三语言 IO SDK 与平台资格；未推送/发布，SDK 尚未冻结。

2026-09-21[TLS Dart 管理与真实进程闭环](../reports/application-service-tls-client-2026-09-21.md)已接通：不可变身份元数据、严格分页/回执校验、保存/禁用与冻结引用启动沿原拥有者通道执行。77 项客户端组合、9 项真实进程用例及 11 文件分析通过；实际 HTTPS 轮换/禁用、原库重开、保存提交后回执损坏/EOF 保持 Unknown 且不重复创建均已验证。原生宿主 Release 已重编译，完整 Flutter Windows 应用尚未重建。下一项是已保存身份选择与轮换 UI、草稿/Unknown 状态保留及真实窗口验收；未推送/发布。

2026-09-21[TLS 原拥有者管理与受保护引用启动](../reports/application-service-tls-control-2026-09-21.md)已接通：私有分页/保存/禁用命令仅返回引用、修订、摘要和禁用状态；启动绑定原 Store 的独立身份依赖，不占用 8 个出站端点名额。真实 HTTPS 验证源 PEM 移除后启动、无关保存不断连、替换/禁用停服与旧缓存拒绝、显式新身份重启。宿主/协议 85+8、最终 TLS 6（重叠）、运行时 24、客户端 67 项通过，限定严格 Clippy/分析和生成一致性通过。下一项为 Dart 类型化管理接口、已保存身份选择/显式轮换 UI 与真实窗口验收；本轮未重建完整 Windows 应用，未推送/发布。

2026-09-21[TLS 原资料库存储](../reports/application-service-tls-store-2026-09-21.md)已完成：Schema 21 原子迁移、原库身份绑定、精确修订 CAS、128 条上限/共享配额、稳定分页、SQLite 快照保留密文及独立 TLS 撤销依赖。core 常规 556、历史迁移组合 65、最终 TLS 定向 9、audit 26、宿主/协议 91 项通过（有重叠，子进程 harness 另计）；最终严格 Clippy 通过。下一步接原拥有者管理命令、受保护引用启动和真实服务撤销/轮换 UI；当前没有应用监听接线，未推送/发布。

2026-09-21[TLS 受保护封装与原生加载](../reports/application-service-tls-protection-2026-09-21.md)已完成：独立 Protobuf＋LZ4 密文格式和 Windows 当前用户保护绑定资料库/引用/修订/摘要，源 PEM 移除后可恢复身份；旧审计/HTTP 用途域与限额保留。core 31（另 1 原有 harness 忽略）、audit 14、宿主/协议 91 项通过，严格 Clippy 与 core Wasm 编译通过。下一步必须完成原 Store 迁移/CAS/配额/快照、精确撤销依赖及私有控制与轮换 UI；当前封装尚未接入持久管理或应用启动，未推送/发布。

2026-09-21[应用 TLS 有效期与运行授权](../reports/application-service-tls-validity-2026-09-21.md)已完成：证书链共同区间在实际启动时收窄原授权，到期/回拨失效沿原路径停服，缓存交付也受约束；双语 UI 显示 UTC 区间并要求失效后重新检查。89 项宿主/协议、20 项 runtime、8 项网络、67 项不同客户端用例、3 项真实进程、1 项 Windows 窗口与 4 项语言资源测试通过。最终窗口已修复并验证日期顺序。下一项为受保护密钥存储与续期，再推进 Unknown 核对、文件系统和三语言 IO SDK；未推送/发布。

2026-09-21[应用 TLS 协议与 Windows 界面](../reports/application-service-tls-ui-2026-09-21.md)已接通：已批准 TLS 发布可选择本地 PEM 并检查，启动冻结路径与证书摘要，Rust 重验实际文件。85 项宿主/协议、64 项客户端组合、2 项真实进程、1 项完整 Windows 窗口和 4 项语言资源测试通过；相关分析、lib 严格 Clippy、生成一致性及完整 Windows Release 构建通过。窗口流程验证 HTTPS 202、停止回收及原库重开，文件选择为确定性注入，不代表系统对话框验收。下一项为证书有效期/过期策略和保护存储/续期，再推进 Unknown 核对、文件系统与三语言 IO SDK；未推送/发布。

2026-09-21[应用服务 TLS 原生入口](../reports/application-service-tls-native-2026-09-21.md)已完成：显式选择有界本地 PEM，启动前重验摘要与密钥配对，TLS/明文模式严格匹配批准，活动身份不随磁盘文件自动更换。78 项宿主回归、增强后的 10 项 TLS 定向测试及 lib 严格 Clippy 通过；真实 TLS、原拥有者保存、停止回收与同库重开已验证。私有协议和 Flutter 证书选择界面尚未接通，这是下一项；证书续期/保护存储、Unknown 核对、文件系统与三语言 IO SDK 仍开放。未重建完整 Windows 应用、未推送/发布。

2026-09-21[服务期间完整私有请求分段](../reports/segmented-owner-frames-2026-09-21.md)已完成：64–128 KiB请求以32 KiB块进入同一原拥有者，完整摘要校验后一次执行；原64 KiB命令额度、停止/Unknown与旧插件ABI保留。实际71,408字节配置保存、认证HTTP共存和同库重开通过；68项宿主测试、37项客户端组合（新增Unknown后路由文件9项）与4项真实进程测试通过，严格Clippy/生成一致性和完整Windows Release通过。下一项为应用服务TLS/证书生命周期，再推进Unknown核对、文件系统与三语言IO SDK；不等于guest流式IO或SDK稳定，未推送/发布。

2026-09-21[按资源依赖撤销](../reports/resource-scoped-revocation-2026-09-21.md)已接入：无关配置/批准/端点写入保持原服务和缓存，匹配端点及共享凭据撤销只影响依赖者；原Store锁、CAS、回滚不复活和全局失效边界保留。主应用服务依赖所有明确选择的端点，选中任一资源失效仍退出整个依赖服务，以阻止旧缓存越过授权。700项Rust测试及2项真实Windows窗口测试通过，严格Clippy和完整Windows Release通过；Dart分析被本机perf清理异常中断，未记作通过。下一项为有界分段帧与应用服务TLS，再推进Unknown核对、文件系统、三语言IO SDK和平台资格；不声明逐请求隔离或SDK稳定，未推送/发布。

更新：2026-09-21。基线：test.52 开发线 457e023 与隔离分支 `codex/io-safety-refactor` 的 Track A 修补／重构；应用版本已更新为 `0.1.9-test.54+58`。当前实现与发布范围以本摘要最新报告为准，早期记录保留其原始基线；本看板随代码提交维护，是当前任务状态入口；总架构与退出门槛见 [主路线](FUTURE_ROADMAP.md) 和 [执行路线](ROADMAP_UPDATE_2026-09-15.md)。版本号、编译和测试数量不替代产品验收。

状态含义：已验子集＝对应限定实现通过；下一项＝可开始编码；待前置＝须先通过列出的门槛；可并行＝不修改正在整合的核心契约；研究＝不得作为运行后端上线。本轮隔离修正与验证证据见 [修正报告](../reports/io-safety-refactor-2026-09-19.md)；前置提交608ccc3已同步到同名远端开发分支；本轮后续改动的本地验证不代表已发布或主应用端到端验收。

## 早期状态摘要（历史；当前状态见顶部）

编码协作自2026-09-21采用SubagentBridge辅助：主代理负责规划、审核、验证和本地整合；模型接收有清晰边界的编码任务与必要上下文，不直接提交或推送。后续按用户要求扩大SubagentBridge承担的实现与测试范围，两个Flash模型均可使用max；主代理保留规划、关键审核、验证与合并。首个GLM max候选经纠正一处断言后，服务会话17项回归及限定分析通过，见[接入记录](../reports/subagentbridge-coding-2026-09-21.md)。该首轮仅新增回归；后续窗口修正与新构建见下方最新报告。

生产实现证据：[服务故障验证](../reports/service-run-faults-2026-09-20.md)。已接凭据/端点、HTTP任务、API节点配置/有限运行面板及原工作台业务路由；当轮162项组合回归（含8项真实Windows故障）、9文件分析和Windows预览构建通过，冻结SDK完整性为36文件/13原包对。最新[启动回执丢失验证](../reports/service-run-reply-loss-2026-09-21.md)新增真实管道损坏/EOF两项，相关35项组合及2文件分析通过；本轮仅增加测试，未重建生产产物或重复累计历史测试。

2026-09-21又补[普通业务提交后回执故障](../reports/service-command-reply-loss-2026-09-21.md)：实际语言设置写入后损坏回执或EOF，原命令身份/Unknown保留；重开同一原库证明修订只增加一次，明确再次保存也不重复提交。相关37项回归及2文件分析通过，不把API调用算作真实窗口点击。

2026-09-21新增[真实封存竞争与修复](../reports/service-seal-repair-2026-09-21.md)：独立SQLite写锁使原宿主停止时封存失败；读取仍可用，写入/启动/确认被阻止，释放竞争后须显式修复。修复保留原身份/旧封存段和历史诊断，操作数不增加，原库可关闭重开；最终相关38项回归与限定分析通过。

2026-09-21新增[Windows完整应用集成](../reports/service-window-integration-2026-09-21.md)：修复异步目录加载不可选、设置滚动/折叠状态冲突及共享会话在布局过渡中重建冲突。93项相关回归、1项原生窗口集成、9文件分析与Windows Release构建通过；真实宿主下经UI启动服务、创建内容/Markdown预览、切换语言和窗口尺寸、停止确认，并重开原库核对。输入为Flutter框架注入，capture为固定转换输入，不代替系统鼠标键盘/剪贴板验收。最新Flutter实现与产物以该报告为准，旧41e6431是之前实现基线。

2026-09-21又补[慢拥有者回调停止](../reports/service-slow-owner-2026-09-21.md)：真实监听与同步回调分开退出，已开始操作保持Unknown、排队操作不执行，正常/捕获panic均取回原owner。9项原生回收回归与严格Clippy通过；同步回调仍不可强制打断。

2026-09-21[可暂停IO S0原型](../reports/suspendable-io-s0-2026-09-21.md)已验证：原wasmi调用两次恢复、入口不重跑，真实TCP等待期间原HostRuntime可提交卡片并重开原库验证；6项测试和严格Clippy通过。探针未接生产Manager/IoBinding/Broker，不能宣称现有插件已可暂停。

2026-09-21[owned Runner改造](../reports/owned-runner-2026-09-21.md)已将真实Runner的输入、待处理调用和continuation改为自有状态，宿主回调只保留在Store外的同步驱动。内部恢复使用同一原实例/调用；公开入口仍同步，受管网络等待仍会占用worker。验证与原包兼容证据见报告；本轮未更新Windows产物。

2026-09-21[broker阶段拆分](../reports/broker-phases-2026-09-21.md)已接入现有同步入口：认领、独立执行、原owner提交分离，结果绑定原broker/宿主/连接；7项新阶段测试包含实际backend等待时原宿主提交与原库重开核对。相关117项运行时/故障注入、33项网络回归及严格Clippy通过；公开驱动仍同步，未重建应用。

2026-09-21[package执行状态与worker逐import驱动](../reports/owned-package-frame-2026-09-21.md)已接入真实路径；完成帧核验仍集中在原worker。489项含故障注入的运行时回归、33项网络回归和严格Clippy通过。路由与HTTP等待仍同步，尚未实现网络等待期间处理普通命令。

2026-09-21[受管HTTP等待与原拥有者调度](../reports/deferred-http-owner-2026-09-21.md)已接入真实HTTP适配器：有界传输任务持有自有数据，原worker等待期间可处理拥有者命令，异常路径取消后join实际传输再回收。真实TCP屏障下原库卡片提交、同库重开，以及回调提交后panic保留Unknown均通过；491项运行时/故障注入、35项网络回归及两包严格Clippy通过。未重建Windows应用。下一步补入站服务同时出站HTTP的组合、停止竞争及应用业务路由验收，S2/S3与新IO SDK尚未整项完成。

2026-09-21[入站服务与出站等待组合](../reports/service-outbound-wait-2026-09-21.md)补齐真实认证HTTP→内容读取→出站HTTP→持久完成路径。原拥有者等待期间写入、同键重放/查询不重发、端点撤权、保留期到期，以及慢回调期间停止均通过；38项相关网络回归与严格Clippy通过。另修正工作台HTTP包装器未转发begin的问题；重新构建真实Rust HTTP插件，6项工作台HTTP回归与lib严格Clippy通过。测试使用WAT包，不代替Rust SDK资格；主应用服务仍为DenyOutbound，下一步接明确的服务出站资源与原批准/撤销流程。

2026-09-21[新会话与多端点路由](../reports/subagentbridge-sessions-and-routes-2026-09-21.md)已完成：更新后的SubagentBridge通过GLM/DeepSeek两个max会话各两轮真实调用，验证revision续写和供应商缓存命中，后续按模块复用上下文。新增最多8个已批准端点的精确路由集合，并接入原生服务组合测试；42项相关网络回归及严格Clippy通过。外来/撤销/同库重连实例不能通过旧集合获得授权。主应用服务仍拒绝出站IO，持久端点选择、原批准/撤销及允许引用交付仍是下一项。

2026-09-21[原生服务持久出站选择](../reports/service-outbound-selection-2026-09-21.md)已接入可信 Rust 启动入口：最多8个明确端点/修订，原实例重新批准，稳定引用与本次授权分离；请求历史绑定完整策略/凭据摘要。实际 Rust 测试插件完成入站→出站→完整响应，同配置重批准不重发，变更配置冲突；78项相关回归通过。现有私有协议/Flutter仍选择空集合，下一项为端点选择界面、原会话绑定、允许引用交付及运行中撤销验收；未重建Windows应用或宣布IO SDK稳定。

2026-09-21[服务出站端点面板与协议](../reports/service-outbound-ui-2026-09-21.md)已接通：Flutter明确多选最多8个当前插件端点，启动请求冻结引用/修订；刷新变化不替换旧选择，Unknown与重挂不重发。私有协议交给原生重新批准，实际Rust插件/真实HTTP测试通过；71项Dart组合、14项原生服务、4项语言资源通过。下一项为允许资源引用交付、运行中撤销/竞争和完整Windows窗口验收；本轮未重建完整应用或推送。

2026-09-21[服务允许资源目录](../reports/service-resource-directory-2026-09-21.md)已接通：声明 service-resources-v1 的包从独立、带摘要的目录获取实际批准端点/凭据引用、方法与限额；宿主剥离外来伪造头，旧包和空选择不接收目录。实际 Rust/Wasm 插件以目录替换错误请求体引用，真实 HTTP、同政策重放、变更冲突、撤权及原拥有者回收通过；79项相关测试与限定严格Clippy通过。下一项为完整 Windows 窗口出站/运行中撤销及停止竞争；未重建完整应用、未推送，公共 IO SDK 仍未稳定。

2026-09-21[Windows资源发现与撤销验收](../reports/service-resource-window-2026-09-21.md)完成：实际Rust/Wasm插件在完整窗口选择端点后发现引用并执行真实HTTP；等待时停用端点和手动停止分别验证取消/回收、原身份及同库重开。新窗口2项、既有窗口1项、相关Dart组合71项通过；完整Windows Release已重建。当前批准写入会保守撤销全部恢复的入站/出站授权，单端点停用会回收整个服务；下一项按依赖资源精确撤销，再接分段帧及应用服务TLS。未推送/发布，公共IO SDK仍未稳定。

当前资源撤销及应用私有64–128 KiB帧分段已验证，主线下一项为应用服务TLS与证书生命周期。系统输入/真实剪贴板、更多编辑路径、其余维护故障和外部效果核对继续开放，可独立推进。启动/普通语言写入回执损坏及EOF、SQLite竞争下显式修复已验，完整故障恢复仍未完成。文件系统可独立推进；跨重启Unknown、因果证据、三语言IO SDK和全平台资格未完成。应用服务当前只准入已批准的单个回环HTTP有限运行。

下面的总表是当前状态；“本轮恢复边界验收”之后为累计阶段记录，其“下一项”“尚未接入”保留当时语境，不覆盖本摘要与总表。

## 已验子集

| 编号 | 范围 | 证据及仍未覆盖的范围 |
| --- | --- | --- |
| IO-A | 当前 IO 声明、Registry 批准、Manager／Pool 实例绑定 | [准入报告](../reports/road-07-io-admission.md)；声明不是资源授权 |
| IO-B1 | 当前协议 Read/Finish/Cancel、raw Runner IO、固定字节 FileBroker、预算／撤权／回收 | [整合验收](../reports/track-a-integration-2026-09-16.md)；15 codec + 12 raw + 15 managed 回归包含在核心396／运行时271项内；不是异步作业或选择器 |
| IO-C0 | Store v16 意图历史及后续意图／审计逻辑预留 | [意图记录](IO_INTENT_RECORDS.md)；该阶段本身不包含受保护材料，后续材料/执行证据见IO-C1/C2，完整效果核对仍待完成 |
| ROAD-04a | test.52 宿主中英文界面 | [i18n 范围](I18N_PREVIEW.md)；插件消息、RTL、业务值迁移未整项通过 |
| SDK-BASE | 冻结 C／C++／Rust 原包兼容 | 冻结基线为36固定文件／13原包对；历史原包执行见修正报告，最新完整性检查见服务故障报告；新 IO 仍实验性 |

## 编码队列

| 顺序／编号 | 状态／优先级 | 模块与前置 | 可评审产物与退出证据 |
| --- | --- | --- | --- |
| 1 / IO-C1 | 已验存储子集 / P0 | core 证据存储；依赖 IO-C0 | Store v17 受保护原件、原容器身份和共享容量预留；读取／幂等重试有界校验；满额、真实满盘、撤权、崩溃重开、材料缺失均可解释，旧签名原件不改写 |
| 2 / IO-C2 | 已验 broker 子集 / P0 | runtime broker＋core，沿用 IoBinding | 同 operationId 唯一活跃执行、请求匹配、原代次退休及恢复核对；现用 Store 严格认领，两个独立宿主竞争同一操作时仅新提交成功者可外发；重复提交、并发绑定、发送边界中断不导致重发，历史记录不恢复授权 |
| 3 / IO-B2 | 已验调度＋托管准入＋持久子调用 / P0 | runtime 作业调度＋独立契约路由 | 有界 submit/poll/read/cancel、Ready 最终交付撤权、声明预算、温和排空已验；已接真实 Manager/IoBinding 的撤权与原实例共享 job/bytes；[托管证据](../reports/managed-io-jobs-2026-09-19.md)。同一作业子调用已贯通 Prepared／发送边界／Observed，无重复计费；[接线证据](../reports/brokered-io-jobs-2026-09-19.md)。HTTP端点与原实例资源批准已接真实传输，持久批准和主应用HTTP/有限服务任务已接入；后续补长IO可暂停、其它资源及故障闭环 |
| 4 / IO-D1 | 已验本机 HTTP/TLS 出站子集 / P0 | guest→Manager/IoBinding→broker→network_node | [托管 HTTP](PLUGIN_MANAGED_HTTP.md)：原实例端点批准、精确 origin/方法/凭据引用、真实 POST/状态/重复头/原件、发送后断线不重发与 Ready 撤权已验；[持久端点批准与Windows系统保护凭据](PLUGIN_OUTBOUND_AUTHORITY.md)已接线，主应用凭据/端点管理与HTTP任务页面已接入；仍待真实提供者核对、路径范围和更多平台 |
| 4 / IO-D2 | 已验本机受管服务子集 / P0 | broker＋network_node 受管服务 | 独立 service 帧／声明 tag 7、真实 Manager 的发布与监听批准、同 worker 路由及 Principal service scopes 已接线；这是宿主显式发布，非 guest 动态注册。本机 HTTP/TLS 的认证／冲突／额度／撤权／节点关闭已验；[持久请求](PLUGIN_SERVICE_HISTORY.md)已接同一 Store 的原子准备／唯一认领／响应原件重试／TTL，真实断线重启恢复已验；[内容权限交集](PLUGIN_SERVICE_CONTENT.md)已通过实际 HTTP 读写与重放验证；[只读状态查询与稳定配置](PLUGIN_SERVICE_RECOVERY.md)已接线；[入站批准与认证解析](PLUGIN_SERVICE_AUTHORITY.md)已接原Store及真实HTTP/TLS；主应用配置/有限运行面板和原业务路由已接；完整Unknown核对、实际窗口验收、跨平台凭据提供者与新三语言SDK未完成，见 [实现合同](PLUGIN_MANAGED_SERVICE.md) |
| 4 / IO-D2a | 已验原生内容子集 / P0 | 远端主体与内容权限交集；依赖 IO-D2 | 原逐对象 grant probe＋service policy＋真实 principal scope；7类命令保留原事务授权，Ready／重放复验，范围变化同key冲突，HTTP实际读／改名／重启重放已验；持久配置/入站批准及主应用服务配置与运行面板已接线；实际窗口与完整内容/UI/capture共存仍待验收，见 [合同](PLUGIN_SERVICE_CONTENT.md) |
| 4 / IO-D2b | 已验配置／查询子集，整体进行中 / P0 | 持久服务配置与恢复操作 | 原Store v18保存稳定namespace、主体／批准引用与修订CAS；新实际grant恢复journal；原worker只读查询不认领、不执行，真实HTTP重启与响应边界已验，见[合同](PLUGIN_SERVICE_RECOVERY.md)。Store v19入站认证摘要／发布批准、原拥有者写锁与撤销、原worker配置修改和HTTP/TLS绑定已验；出站受保护凭据已接Store v20及原worker；主应用配置与有限运行、启动诊断及8项实际故障已验；后续补真实窗口和其余故障流程、Unknown核对、因果关系、跨进程时钟高水位及证据退休 |
| 4 / IO-D3 | 下一项，可独立推进 / P0 | 平台文件适配＋broker | 系统选择、目录枚举、创建／替换／删除，资源越界／替换冲突／撤权／崩溃结果核对；固定读取保留兼容测试 |
| 5 / IO-E1 | 待 B2/D1/D2/D3 契约验收 / P1 | sdk/rust、sdk/c、sdk/cpp | 三语言类型化 IO、同一正负向量与独立仓库插件；旧原包原样执行；新扩展单独形成兼容候选 |
| 5 / IO-E2 | 已验HTTP任务、服务配置/运行与诊断子集，整体进行中 / P1 | workbench_host＋Flutter 管理界面 | [管理接口](PLUGIN_IO_MANAGEMENT.md)已接私有协议与真实Registry：声明／批准分离、明确保存／撤销、修订校验与重启恢复；原Store有界分页、Windows凭据与[具体端点批准](PLUGIN_ENDPOINT_MANAGEMENT.md)录入／替换／停用已验。[Rust应用任务状态](PLUGIN_APP_IO_TASKS.md)已接原Storage所有权、Busy与恢复；[HTTP任务消息与关闭](PLUGIN_APP_HTTP_TASKS.md)已接私有通道和Dart接口；[HTTP任务页面](PLUGIN_APP_HTTP_TASKS.md)已验Windows真实Rust guest/凭据/响应与设置重挂；API节点配置/有限运行面板及原工作台业务路由已接，启动诊断与真实故障证据见[最新报告](../reports/service-run-faults-2026-09-20.md)；下一项实际窗口、其余故障与Unknown证据核对，用户资料无隐式迁移 |
| 5 / ROAD-08-IO | 待 C1/C2 与实际后端 / P0 | 录制证据与独立验证器 | A→B→IO→内容提交→封存→删除安装来源→隔离重放；真实故障、合法退休与缺材料分类；重放禁止实际外发 |
| 6 / IO-E3 | 待基础双向 IO / P1 | NET-2–8／NODE-4–7 按各自依赖 | OAuth／多账号、上传下载、分页限流、流/SSE/WebSocket、webhook、持久服务与 TLS 运维；每个 profile 单独验收 |

## 可并行及研究

| 编号 | 状态 | 下一步与边界 |
| --- | --- | --- |
| ROAD-01b | 可并行 / P0 | 补主应用／测试／平台固定源码构建回执；不把本轮源码整合写入旧预览归档 |
| ROAD-02/04b | 可并行设计 / P0 | LiteralText／MessageRef、命名空间、任务语言上下文、坏包和RTL向量；固定接口后才接插件 UI；不新增 TS/JS 或动态 Dart 插件 |
| ROAD-12/AND-01 | 可并行探针 / P0 | Android Rust/Wasm 引擎提取与执行域能力；编译、设备、隔离分别记证据，不能继承 Windows 通过状态 |
| ROAD-05/06 | 可并行固定接口验证 / P0 | 真实跨进程 A/B 故障、证据容量与退休；存储／schema 变更需与 IO-C1 串行整合 |
| QUIC-RESEARCH | 研究 / P1 | 先统一迁移开关、IPv4/IPv6共享端口计数及层次依赖；再提交真实 socket/TLS/传输原型，下载的计数器／布尔模型不接产品 |

## 每次合入检查

记录基线和实际范围，复核 schema／数据库／授权唯一权威；跑改动相关回归、冻结原包完整性及执行兼容。影响共享模块时检查 wasm32 编译，原生 IO 另报平台资格。完成一项只移动该子项状态，ROAD-07、完整 SDK 和 M0–M7 不因局部通过整体勾选。测试版继续沿 test.x 推进，0.2.0 仅在声明范围达到既定门槛后评审。

## 本轮恢复边界验收

持久入站与跨宿主唯一外发的限定结果见 [报告](../reports/service-history-2026-09-19.md)：core 499、runtime 356、network 72 项通过，均无失败；5个 ignored 为父测试实际启动的崩溃子进程入口。三 crate 严格静态检查、默认 wasm32 库编译与冻结 SDK 原件检查通过。不是全平台运行、新 SDK 稳定或完整插件产品验收。

## 本轮内容权限验收

[内容服务报告](../reports/service-content-2026-09-19.md)：核心510、运行时370、网络78项通过，均无失败；主应用宿主116个测试入口通过（含3个既有子进程入口，不将父测试内的子进程输出重复累计）。实际 HTTP→Wasm→core 读取／改名、主体隔离、权限收窄后缓存拒绝和重启原回执恢复已验；旧 dispatch 采时保持兼容，新增 guarded 入口执行最终授权检查。IO-D2b的配置／只读查询进展见下；IO-D3 文件系统后端可沿固定 Broker 边界独立推进。

## 持久配置与恢复查询进展（2026-09-20）

原Store的配置修订、原worker查询及实际HTTP入口已贯通，证据见[本轮报告](../reports/service-recovery-2026-09-20.md)。所有查询保留原授权、容量与TTL；Missing不写记录、Prepared不认领、Unknown不重发，Observed返回原件。配置仅为期望状态，主体认证与资源批准引用不能恢复旧权限。下一编码顺序：主应用授权／凭据与任务配置 → 有证据的Unknown核对与完整因果链 → 证据退休；三语言SDK和主应用UI依赖这些契约继续推进。

## 持久入站授权进展（2026-09-20）

[批准合同](PLUGIN_SERVICE_AUTHORITY.md)与[验证报告](../reports/service-authority-2026-09-20.md)：原Store v19记录认证摘要和精确发布批准，原实际实例重新准入；配置或认证更新通过原worker执行并阻断迟到交付。数据库副本、独占模式与显式原生VFS遵守同一授权锁。仅Windows本机HTTP/TLS及存储故障测试通过；没有主应用发布UI、出站秘密保险库或全平台运行结论。

## 持久出站授权进展（2026-09-20）

[出站批准合同](PLUGIN_OUTBOUND_AUTHORITY.md)：端点批准和系统保护凭据保存在原Store v20，Windows使用与审计密钥隔离的DPAPI域；实际插件实例与凭据使用权限在解密前复核，修订更新继续走原worker并撤销旧活动授权。后续主应用需提供端点批准、凭据录入／轮换、任务与恢复界面；其他平台凭据提供者、OAuth及完整网络SDK继续独立验收。验证结果见[本轮报告](../reports/outbound-authority-2026-09-20.md)。

## 主应用 IO 类别管理进展（2026-09-20）

[接口合同](PLUGIN_IO_MANAGEMENT.md)与[验证报告](../reports/plugin-io-management-2026-09-20.md)：主应用支持独立保存／撤销网络和文件类别，保持内容批准与启用状态；真实 Flutter→Rust 进程重启恢复通过。切换工作台后的迟到回包与旧关闭失败已隔离。类别批准不代替资源授权，也没有接通实际网络任务。

本节当时的下一项为原Store有界元数据列表与凭据录入／轮换；完成状态见下节。禁止另建运行时或数据库绕过唯一权威；Unknown核对、因果链、文件系统及完整SDK继续保持原退出门槛。

### Web 构建阻断及修复

上一轮 Flutter Web JavaScript Release 因 Cap'n Proto 反射代码的64位schema ID无法精确表示为JavaScript数值而失败，历史记录见[故障报告](../reports/plugin-io-management-2026-09-20.md)。本轮生成器保留原生反射，并为Web提供精确十六进制／BigInt身份侧表；Web关闭可选int反射，不修改消息布局。UiEvent三个UInt64字段使用两个UInt32传递，真实Chrome边界向量和完整Web JS Release均通过，见[修复报告](../reports/credential-admin-web-2026-09-20.md)。这不代表所有运行期UInt64路径、浏览器存储或Web插件IO均已验收。

## 主应用凭据管理进展（2026-09-20）

原Store v20在同一读事务进行有界分页和快照核对；主应用Windows凭据面板完成新建、替换、停用和进程重启恢复，仅返回元数据。原DPAPI保护、原Store CAS和撤权协调器继续是唯一权威；保存凭据不会启用插件或创建活动网络授权。[报告](../reports/credential-admin-web-2026-09-20.md)记录真实Flutter→Rust、缓冲区清理、页外损坏、混合记录空页及Web修复证据。IO-E2仍为部分完成，新SDK未冻结。

下一编码顺序：完整Storage交接底座（进展见下节）→ 主应用可恢复任务状态机 → 原端点批准管理与短响应start/poll/read/cancel → 真实主应用HTTP链路、撤销与重启核对。文件系统后端可沿固定broker边界独立推进；凭据期限不代替活动授权，Unknown结果不能自动重发。

## 受保护存储的 IO 所有权交接（2026-09-20）

[所有权合同](PLUGIN_IO_OWNERSHIP.md)：IoWorker 可移交完整 HostOwner，Windows Storage 保留原审计 Session、签名身份、数据库／身份／Registry 租约。独立准入 IO 实例而不拆取 Pool 根；异常结束归还原容器、执行／断连／维护状态及必要的待清理实例。原运行时默认 API 保留，维护失败不把已发生的 HTTP 效果改写为未执行。

这属于 IO-B2／IO-E2 的宿主底座子集。真实受管 Wasm→本机HTTP→原审计Store→封存／重开已验，详见[报告](../reports/io-owner-2026-09-20.md)。CLI／Flutter 主应用命令循环尚未使用新交接入口，不能标记主应用网络任务完成。下一项是在主应用中建立显式的存储在线程中／停止待退出／已取回状态，再接端点批准和短响应任务协议；文件系统、API节点管理、Unknown核对与完整SDK保持原门槛。


## 2026-09-20 主应用任务所有权状态

[应用合同](PLUGIN_APP_IO_TASKS.md)：Workbench Rust 入口可使用原 Manager／Store 独立准入 IO 实例，启动一项受管任务并非阻塞查询、读取、取消、回收和恢复；StorageSlot 对现有内容方法显式提供 Busy，协议提前拒绝依赖存储的文件／上传／批准副作用。Ready 仍须最终读取授权校验，停止请求不冒充线程退出，退出诊断与实际修复分开保存。

本轮验证记录见[报告](../reports/app-io-tasks-2026-09-20.md)。这不是 CLI／Flutter 已有网络任务界面：下一项具体端点批准管理 → 私有任务消息、界面状态与 EOF／关闭待退出 → 实际用户路径、撤销和重启核对。文件系统、API节点、Unknown核对、因果链、三语言SDK与平台资格保持原有退出门槛。原任务句柄不持久化，不恢复旧授权，不自动重试外部效果。

## 主应用端点批准进展（2026-09-20）

[端点管理合同](PLUGIN_ENDPOINT_MANAGEMENT.md)与[验证报告](../reports/endpoint-admin-2026-09-20.md)：原Store完整政策分页／保存／停用、私有协议、Dart原生适配、中英文界面及进程重启恢复通过。保存只记录批准，不启用插件、不解密秘密、不建立连接；冻结SDK不变。下一项明确为CLI／Flutter短响应任务消息、关闭待退出及实际用户请求路径；IO-E2整体仍进行中。

## HTTP任务与关闭接线进展（2026-09-20）

[接线合同](PLUGIN_APP_HTTP_TASKS.md)：真实Rust HTTP-forward guest仅允许原输入帧的一次转发；原Store解析端点与凭据、原实例授权、非阻塞私有任务控制及Dart接口已接入。CLI关流等待实际存储回收，Dart不再五秒强杀。下一项为用户任务表单、状态及错误恢复页面；随后实际用户路径、API节点管理与文件系统，不把本阶段解释为完整网络UI或SDK稳定。验证见[本轮报告](../reports/http-task-control-2026-09-20.md)。

## HTTP任务页面进展（2026-09-20）

明确提交、状态、结果、取消、原实例恢复及完成确认已接插件库。当前包handler来自原目录，不从权限类别推断。设置收起保留原后端结果和未知状态，恢复观察只查询；新鲜Local/无TaskKey时才可显式归档未知尝试，不代表远端回滚。[页面报告](../reports/http-task-ui-2026-09-20.md)记录85项Dart回归、真实Windows凭据/HTTP表单及Web构建。下一顺序：API节点配置与发布控制 → 跨重启Unknown证据核对/因果链 → 文件系统后端 → 三语言IO SDK候选；整个IO-E2仍进行中。

## API节点管理底座进展（2026-09-20）

已接Workbench Rust配置、一次性认证令牌、精确发布批准与停用，原Store提供整表校验的稳定分页；保存不启用、不监听。新15项定点测试、宿主完整165项及原存储40项回归通过，见[报告](../reports/service-admin-2026-09-20.md)。尚未接私有消息或Flutter管理页面。

下一编码顺序细化为：[管理合同](PLUGIN_SERVICE_MANAGEMENT.md)中的消息/模型/双语表单 → 常驻监听租约与单请求预算分离 → ServiceHost保留完整原Storage的泛型化与统一停止回收 → 真实入站/查询/故障用户路径。现有30秒实例寿命和长期占用内容库的Busy不能作为常驻节点最终方案，也不能通过自动反复绑定或旁路数据库绕过；解决后再验Unknown持久证据、文件系统及三语言IO SDK。IO-D2b与IO-E2整体保持进行中。

## API节点私有消息进展（2026-09-20）

七个管理动作已连接Rust宿主、私有Cap'n Proto与Dart原生适配；一次令牌独立所有权、回复缓冲清理、连接失败封锁和历史IPv6 scope读取已有回归。真实Windows进程证明配置/批准在原库重开后保留、轮换和修订冲突有效，保存不会自动监听。详见[消息接线报告](../reports/service-wire-2026-09-20.md)。

下一项收敛为双语配置/认证/批准页面及一次令牌呈现；之后仍按管理合同完成常驻租约、原Storage调度和监听/worker真实回收。当前没有新增服务管理页面、主应用监听入口或三语言IO稳定承诺。版本保持不变，本阶段仅本地提交。

## API节点管理页面进展（2026-09-20）

双语配置/认证/发布批准表单已嵌入插件库，支持逐对象范围编辑、一次令牌呈现与清理、目录变更保留草稿和明确重绑定。未知写入留在原后端会话，重新打开页面不自动重发或解锁。Windows真实表单已验证创建、编辑、轮换、批准更新、停用及原库重开；115项相关Dart回归、语言包检查与Web构建通过，见[页面报告](../reports/service-ui-2026-09-20.md)。

下一项是管理合同中的常驻监听租约与每请求预算分离，随后完成ServiceHost完整Storage所有权、统一访问调度、监听监督/worker真实退出与恢复，再连接启动/停止及真实入站用户路径。现有页面只管理期望配置和批准，IO-D2b/IO-E2整体仍进行中；Unknown证据核对、文件系统和三语言IO SDK继续保持原门槛。

## 服务运行所有者适配进展（2026-09-20）

独立前置ServiceHost完整所有权适配已落地：泛型HostOwner、构造失败归还原worker、非阻塞停止请求、真实join后返回完整WorkerExit，以及可取消等待但不丢失owner的shutdown_owned。原HostRuntime构造和bind空路由调用保持源码兼容。真实Windows原Storage上的HTTP执行、端口冲突和封存失败修复已验；监听关闭与worker归还分别检查，见[所有权报告](../reports/service-owner-2026-09-20.md)。

后续按[常驻运行方案](PLUGIN_SERVICE_RUNTIME_PLAN.md)实施：显式版本化运行租约与每请求预算 → 包含原Pool/Manager/内容状态的WorkbenchState和统一有界调度 → 长耗时等待可暂停与工作台共存 → 主应用启动/停止及完整故障用户路径。本轮没有放宽30秒旧声明，也没有新监听按钮；常驻期间内容Busy仍未解决，不能标记常驻API节点完成。

## 服务有限运行租约进展（2026-09-20）

`service-run-v1`独立版本化声明与可信宿主 `bind_service_run`已实现：有限时长上限暂为一小时，每请求仍受原IO短期限；原实例一次签发、真实时钟截止、回退/过期失效及原累计字节账本保持。真实同一监听器31秒后执行第二个不同请求，旧30秒绑定不再是新profile的限制；旧包行为不变。详见[有限运行报告](../reports/service-run-2026-09-20.md)。

这是常驻节点的前置原型，主应用尚无启动入口。一小时稳定性、累计作业总额、显式续租与完整WorkbenchState调度仍未完成；不能用原型替代服务期间内容界面的可用性验收。下一步继续完整工作台所有权和调度，同时收敛版本化运行预算与续租；IO-D2b/IO-E2整体保持进行中。

## 服务累计任务与字节预算进展（2026-09-20）

`service-run-budget-v1`增加明确的累计任务保留次数和字节声明，可信宿主通过新入口批准更小额度；原IoContext统一计费，取消/丢弃/取结果不退款，单独资源占用不计任务。最后一个获准任务不会因任务额度耗尽而失去交付资格；四条字节计费路径均执行宿主上限。旧profile不重解释，新字段和feature成对验证。详见[累计预算报告](../reports/service-run-budget-2026-09-20.md)。

下一项是显式续租的原身份/修订/额度更新规则，以及完整WorkbenchState和有界调度；主应用监听按钮、服务期间内容界面的响应和未知结果核对仍未完成。IO-D2b/IO-E2继续进行中，当前没有SDK稳定承诺或发布动作。

## 原声明内显式续租进展（2026-09-20）

预算profile的可信宿主续租入口已接原IoWorker与ServiceHost：固定原Manager/Control/包、当前批准、原grant和两级期望修订；不创建新实例，不清除累计账本。更新只能扩大当前批准并保持在首次签发确定的声明总时长、任务和字节上限内。所有旧绑定副本共享新期限，单请求、认证、发布和历史结果期限保持独立；停止/排空、撤权、到期和时钟回退不能恢复。详见[续租报告](../reports/service-run-renewal-2026-09-20.md)。这是有限原型，不提供无限续租或稳定SDK承诺。

当前下一编码顺序：抽出持有原Storage/Pool/Manager/内容会话/undo/暂存的完整WorkbenchState → 原执行者的有界命令队列与管理容量预留 → 长IO等待期间的可暂停执行和工作台共存 → 主应用显式启动/状态/停止/修复及真实HTTP/TLS用户路径。前一步不能通过只给IO worker转移裸Storage或在旁路Store写内容替代。随后继续Unknown证据核对、完整文件系统、三语言IO SDK候选及各平台资格；IO-D2b/IO-E2整体仍进行中。

## 宿主命令预留通道进展（2026-09-20）

原生执行者已增加独立8项宿主命令保留，覆盖排队、执行和Ready未读；取消后的未知结果、原时钟复验与两条队列轮转都有专项。`ManagedHostOwner`允许原Manager随整个拥有者移动，失败仍归还原对象。真实Windows组合拥有者在同一HTTP监听期间保留Storage/Pool/Manager，查询原Store已执行记录并完整回收，见[验证报告](../reports/owner-commands-2026-09-20.md)。这是调度前置，不是全部工作台内容方法已在线程中运行。

下一步收敛为：内部Manager的续租/撤权管理命令 → 完整WorkbenchState提取及现有内容命令接入 → 长IO等待可暂停 → 主应用界面和故障路径。内部Manager目前不能供外部旧续租方法借用，这一缺口不能通过复制管理器解决。宿主handler与旧IO router仍同步执行，预留队列不等于长任务期间的响应时间保证；主应用内容Busy尚未消除，IO-D2b/IO-E2及完整SDK门槛保持未完成。

## 内部Manager续租命令进展（2026-09-20）

原Manager随owner移动后，现在可通过原执行者的类型化续租命令更新同一运行租约。该入口共用8项保留容量和外部续租的身份/修订/CAS规则；待完成、明确拒绝、成功和Unknown分别处理。取消与CAS串行化，已更新账本不因回执丢失回滚。Windows同一监听器以两个不同持久请求身份完成真实HTTP/Wasm执行，第二次在旧期限之后、更新期限之内，原Store两条Observed记录与保护身份均保留；详见[内部续租报告](../reports/owned-service-renewal-2026-09-20.md)。

上一节的内部续租缺口已关闭。下一项：完整WorkbenchState提取（原Storage/Pool/Manager/内容会话/undo/附件导入暂存/capture）→ 内容与批准/撤权等应用管理命令接入 → 长IO等待可暂停 → 主应用启动/停止/恢复与真实用户路径。socket关闭、授权撤销与worker回收仍是独立步骤，应用关闭流程必须分别完成。主应用内容Busy、Unknown持久证据核对、完整文件系统和三语言IO SDK稳定门槛仍未完成；IO-D2b/IO-E2保持进行中。

## 完整工作台状态提取（2026-09-20）

原业务状态已集中到WorkbenchState，直接持有Storage/Pool/Manager/内容及UI会话/undo/附件与上传暂存/capture。短IO从移动Storage改为移动完整State；外围仅持有StateSlot和HTTP提交关联。原内容、查询与证据提交实现迁到State，并由外围显式借用转发。短任务封存与最终应用清理分离，线程回收不关闭编辑器或Pool根。验证细节见[状态报告](../reports/workbench-state-2026-09-20.md)。

这关闭了“Pool/Manager和编辑状态仍在另一个线程”的结构缺口。后台业务调用当前仍返回Busy：下一项要在同一个State上建立有界业务/管理命令与原协议派发，接入常驻节点准入，再实现可暂停长IO和主应用启动/停止/恢复。不能把本次提取标作服务期间编辑已可用，也不能以外围缓存或第二份Store替代后续接线。IO-D2b/IO-E2、Unknown证据核对、文件系统和完整三语言SDK继续进行中。

## 原工作台业务命令派发（2026-09-20）

WorkbenchState已实现原生CommandOwner，本地与worker共用业务协议校验、错误码、部分结果/令牌清理和响应预算。原State明确拒绝调度动作；外围保留Busy及显式修复门槛。排队输入和未读回执使用Zeroizing，取消/停止/失效清理缓冲而不提前返还容量。实际Rust guest创建、HTTP、编辑及读取沿同一State完成，旧修订失败，Ready写取消后的Unknown保留一次真实提交，见[验证报告](../reports/workbench-commands-2026-09-20.md)。

下一切片：持久服务配置下的有限运行准入与原State移交 → 主应用异步业务命令回执及启动/停止/修复 → 长IO等待可暂停和界面响应。现有短IO自动drain，主应用常驻服务尚未接入，不能把测试专用Running worker标为完整产品路径。随后继续Unknown证据核对、文件系统、三语言SDK及全平台资格；IO-D2b/IO-E2继续进行中，版本和SDK冻结基线不变。

## 原生应用服务准入与监督回收（2026-09-20）

Workbench已提供start_service、service_status和submit_service_command；原持久配置/发布/认证解析、明确有限预算和原State移交接入同一个StateSlot。服务监督线程拥有Tokio runtime，监听与worker都实际结束并join后才归还原State；Drop只请求停止，监督线程保留清理资源。绑定、监听、监督和存储退出结果独立，端口冲突也保留原拥有者并要求确认。审查发现的配置先检查后pin竞态已改为先固定原授权锁、再比较预期修订和地址。见[验证报告](../reports/application-service-admission-2026-09-20.md)。

本切片限定单个已批准loopback HTTP有限服务；不自动续租，不把未批准出站调用变成权限。下一项：私有服务/命令调度协议和有界句柄表 → Flutter启动/状态/停止及内容命令异步交付 → 可暂停长IO、TLS与出站资源接线。Unknown持久核对、文件系统、三语言SDK和平台资格继续原验收；IO-D2b/IO-E2整体未完成，版本和发布状态不变。

## 应用服务命令协议与Dart低层接口（2026-09-20）

原有限服务现通过私有协议显式启动、观察、停止、修复与确认；同一原State上的业务命令分成提交、状态、一次性读取和取消。每服务8项未消费句柄及512项提交历史保留相同身份去重，冲突拒绝，历史不淘汰；丢回执不自动执行。内层请求64KiB、业务回复128KiB，仅外层命令读取封装允许256KiB。Dart提供拥有参数/结果的低层接口，敏感内层回执可显式清除。验证证据见[命令协议报告](../reports/service-command-protocol-2026-09-20.md)。

下一项：在RustWorkbench现有业务调用中接异步命令路由（等待结果不得占住传输队列）→ 缩小上传块并保留一次性令牌/Unknown语义 → Flutter服务启动/状态/停止/修复与原内容工作台并用的实际用户路径 → 可暂停长IO、TLS和出站资源适配。此切片尚无页面入口或全部业务自动路由，不宣布服务期间编辑体验完成；IO-D2b/IO-E2、Unknown持久核对、文件系统、三语言SDK与平台资格仍按原门槛推进。版本保持test.52+56，未推送或发布。


## Dart服务期间业务自动路由（2026-09-20）

RustWorkbench新增独立业务队列，在实际管道发送槽按最新已观察服务选择原task。普通业务通过原命令通道完成；状态/停止不被业务等待阻挡，整个请求按同一截止执行。未知/损坏结果保留原身份且不重发，敏感结果与借用Reader分别管理寿命；传输失败用EOF等待原进程实际清理，原错误不被关闭状态覆盖。运行中IO轮询不再误设业务只读。现有上传分块已经32KiB，完整内层帧超过64KiB仍明确拒绝。见[验证报告](../reports/service-business-routing-2026-09-20.md)。

真实Windows原生宿主、有限服务WAT夹具与原Rust工作台guest共同验证：两个实际HTTP请求之间正常创建/编辑/读取卡片和保存语言，停止/回收/确认及关闭重开后保留数据。受控管道另覆盖启动期间排队、Pending状态/停止、Unknown、损坏回执、查询错误、令牌清理与EOF。

下一项：Flutter服务运行页面（显式有限预算、当前身份/绑定/监听/回收状态、停止/修复/确认）→ 实际页面与内容/UI/capture并用、故障用户路径 → 可暂停长IO、TLS及出站资源。69项相关Dart回归及限定分析见报告；不把底层入口标为完整页面，不宣称所有业务尺寸已完成分段。IO-D2b/IO-E2、文件系统、Unknown持久核对、三语言SDK与全平台资格继续原门槛，版本仍test.52+56，无发布动作。

## Flutter有限服务运行面板（2026-09-20）

插件设置已接入显式启动、有限预算、绑定/监听/监督/内容回收状态，以及停止、修复和确认。后端绑定会话保留在途启动及Unknown身份；写前核对原task，失败不自动重启，真实退出回执是回收确认的前提。运行候选绑定配置/发布/插件/Registry修订，语言和目录刷新保留草稿；短HTTP面板在服务期间转为提示并保留原请求草稿。只读运行面板不延长认证令牌寿命。

12项会话测试、19项新面板测试及真实Windows原生控制会话/HTTP/卡片持久化路径通过，最终相关组合144项通过，Flutter限定分析无问题。原生release与完整Windows构建通过，随附release宿主/插件的真实服务流程1/1通过；已修复依赖下载失败后CMake重试误用系统安装目录的问题。具体限制见[运行面板报告](../reports/service-run-ui-2026-09-20.md)。不把widget验证等同真实窗口视觉或全平台资格。

下一项：真实故障流程（端口冲突、明确准入拒绝诊断、撤权、到期、丢回执、慢回调停止和封存修复）→ 更多内容/UI/capture组合与超限请求分段 → 可暂停长IO、TLS及出站资源。Unknown持久核对、文件系统、三语言SDK和全平台资格仍按原门槛；IO-D2b/IO-E2尚未整体完成，版本保持test.52+56。

## 启动诊断与真实服务故障（2026-09-20）

已区分合法宿主错误与丢失/损坏响应；错误后按原提交身份只读核对实际任务，保留诊断和清理入口，不自动重启。真实Windows Release宿主通过端口占用、期限到期、运行中撤销发布/认证及四类准入错误，共8项、无跳过。工作线程准入拒绝确实可能保留清理任务，只有退出并确认后才能再次启动。组合162项通过，9文件分析通过，Windows预览构建已更新；详见[故障验证报告](../reports/service-run-faults-2026-09-20.md)。

| 顺序 | 待完成工作 | 退出证据 |
| --- | --- | --- |
| 1 | 实际窗口与内容/UI/capture共存 | 真实输入、页面切换及原内容身份/代次正确；widget和原生控制测试不能替代窗口验收 |
| 2 | 真实丢回执、慢回调停止、封存失败修复 | 原任务/操作可核对，不自动重发；实际线程退出与原内容库修复分别证明 |
| 3 | 长IO可暂停及超限业务帧分段 | 等待网络时普通授权命令有界响应，完整原请求不截断，不复制运行时/数据库 |
| 4 | 应用服务TLS及出站资源 | 沿同一持久批准、凭据、worker和预算链路验证实际网络 |

文件系统可沿固定broker边界独立推进；跨重启Unknown、因果证据、三语言IO SDK和全平台资格保持原门槛。版本仍test.52+56，未推送或发布，IO-D2b/IO-E2不整体勾选。

## 草稿交接私有协议与客户端（2026-09-23）

[交接协议报告](../reports/editor-draft-handoff-wire-2026-09-23.md)记录新增的完整父子关系、宿主原始请求摘要、固定交接/退休操作及只读分页发现。Rust 22 单元 + 22 集成、Dart 14 编解码 + 8 真实客户端组合通过，Windows Release 已构建；三代 B 同时具有父链接和退休标记的错误限制已修复。各阶段测试有重叠，不累计为全产品验收。

下一项为发送前固定交接提案及原退休操作的跨应用重启持久发现与明确核对，再接正式编辑器默认 autosave、关闭和恢复流程。当前调用者仍需持有原提案，只读已提交关系不等于未知结果恢复完成。富捕获来源、历史维护、语言/字体待决提案、迁移 UI、统一错误/私有诊断、远端 Unknown、SDK 与其他平台资格保持开放；未提交、推送或发布。
