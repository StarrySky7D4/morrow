# 最新开发支线状态

<!-- C28-CURRENT-BEGIN -->
## 当前开发检查点（2026-10-10，C28 阶段 21）

严格 checkout 第二轮来源与固定元数据验证已实际通过，并完成独立有限读回。匹配 helper 第三轮离线构建的 Cargo 与外层均退出 0，原始输入、物理集合和完整新 Cargo home 后置守卫通过，保存当前库、runner 与 setup 三个新产物；独立读回已核对三个产物与 169 个源码文件当前字节，以及日志、严格守卫报告和真实外层终端记录的交叉绑定。未运行两个程序或真实沙箱。

公开 guest 第一次 metadata 因解包时路径规范化权限拒绝退出 101，第二次 metadata 子进程退出 0，外层因原 27 包锁与 wasm32 过滤后 25 节点图的契约不符退出 1；仅排除两个指定目标不适用包的第三次后继已准备，metadata、编译、Wasm 隐私与行为资格均未运行。两次外层退出 1 的已知失败保留；没有新 Wasm 身份接入公开 harness。

本次仅更新脱敏状态文档，已选产品源码与已推提交 `58391f3c` 一致，版本保持 `0.1.9-test.58+62`。阶段 18 仍为 Cargo 退出 0、外层退出 1；公开 Git 树仍缺 session 夹具，完整构建保持 `NOT_RUN_MISSING_SESSION_FIXTURE`。原生 Start 的 Unknown 不重放；owner finish、factory release、cleanup/join、真实断连、生产沙箱及后续 11 项原始库测试仍待验收。`SDK26_G04=OPEN`，`release_eligible=false`，SDK 未冻结。

下一步完成公开 guest 新字节与会话资格，再开展授权范围内的 Windows 生命周期复验。会话层与安全执行层验收后暂停准备测试预览，不等待扩展执行层。见 [阶段 21 进展记录](../reports/reconstruction-2026-10-10/windows-agent-sdk-c28-stage21.md)、[阶段 20 准备记录](../reports/reconstruction-2026-10-10/windows-agent-sdk-c28-stage20.md)与[阶段 18 编译记录](../reports/reconstruction-2026-10-09/windows-agent-sdk-c28-stage18.md)。下方内容保留历史时点。
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

C07 原 owner 九组 115 方法、原始 SDK 定向回归 42 方法及网络 100 方法分别通过；115 已含取消7 / 目录owner14 / native16，42 保留过滤及 child helper 边界。整库 Clippy 仍有 10 处既有诊断（exit101），本轮 owned 诊断0。327 SDK / 57 冻结输入保持原字节，完整 SDK26 / G04 继续 OPEN。

目录捕获、分页、结束和清理已进入原IoWorker；C08新增可信宿主 `capture_directory_fresh(File, CaptureLimits)`，生产生成固定使用getrandom0.4.3，原身份／clock／取消前后复核和所持缓冲Zeroizing保持。新Windows Release／locked／offline实际factory14（69filtered）、原owner115／九组、原件42／五组、网络100／十一组分别通过，42保留region14与reader1过滤。限定三文件fmt及strict库Rustc通过；整库library Clippy仍exit101／10旧诊断／owned0，不是全SDK冻结。C08历史限定证据保留，本次开发提交收录C08–C10，详见 [C08实测](../reports/reconstruction-2026-10-05/directory-secret-factory.md)。C08不提供picker或祖先证明；新的C09限定链见下文，公开目录协商、blob耐久后端、生产入口及其它平台仍OPEN。

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

2026-10-01 **0.1.9-test.57+61 开发检查点与 Linux 迁移**：用户授权保存本轮代码/文档到原 `codex/m03-stream-revocation-backpressure` 分支，保留全部历史证据，不合并 main、不创建 Release、不运行 Actions/CI。生产 Windows supervisor/host/Flutter 链已落实；SDK014 最新默认 Release 构建通过，真实 normal/host/UI/supervisor 四类故障及 WM_CLOSE 通过。当前生产 channel 20 项实测为 11 通过、9 失败/错误、0 跳过；catalog UI 在 available 检查处失败，旧 native 产品 4 项通过，profile 查询未运行。不能把此前 managedRunner/SDK 库验证当作生产通道全面通过。源码/状态迁移目标已确认 Linux 云电脑，Linux 监督、IPC、权限与工作台需独立实现和实证。详见 [SDK014 精确状态](../reports/codex-morrow-v1.1/windows-sdk-014-2026-10-01.md) 与 [test.57 检查点](../reports/0.1.9-test.57-checkpoint.md)。


2026-10-01 [Windows SDK 013候选](../reports/codex-morrow-v1.1/windows-sdk-013-2026-10-01.md)：新生产host只读profile及developer profiles工具已实现；643 runtime、三语言新15包/实际12基础执行、旧工作台4/4与原6 transport包真实回归通过，IO/service仍experimental。公开扩展规则和26需求矩阵已整理，3项真实拒绝/隔离fixture及独立复核通过，公共stream/events/Cloud尚未实现，整个SDK未冻结。工具捕获异常已保留并fixture修复后226 clean复验；遗漏guest输入的完整host失败保留、新完整host200、默认Release构建及新Release正常/host/UI/sup故障已实测通过，原精确窗口失败/未观察保留、最终独立审计PASS_SCOPED，统一源/产物/原始证据封存完成。CC/Codex暂停；无CI/提交/推送/发布。

2026-10-01 [Windows SDK 013 候选](../reports/codex-morrow-v1.1/windows-sdk-013-2026-10-01.md)：最新优先级为现有插件兼容与 Windows SDK 稳定验收。CC Switch／Codex 获取和构建暂停，不作为 Windows 冻结门槛；此前 G0 记录保留历史范围。基础 Wasm guest ABI2/runtime7/task3/UI1/dependency1 与实验 IO/service/mutation 分开，当前尚未冻结。统一原件、三语言构建、生产兼容、权限／预算／故障回归和独立复核进行中。无 CI／推送／发布。

2026-10-01 [G0 前置准备 012](../reports/codex-morrow-v1.1/g0-preparation-012-2026-10-01.md)：新增同源 CC Switch pure core + 既有 ABI2 Transform adapter；native 11/11、真实 Wasmi 与 Morrow API/PluginLibrary widget focused 2/2 通过，两份独立复核完成。CC Switch 原锁 Windows metadata 首次 0，仅证明依赖解析；原 Tauri caller/公开 broker 接入和完整 G0 未完成，SDK 未冻结。011 生产源码/历史密封证据保持；未提交、CI、推送或发布。


2026-10-01 [Windows生产监督接入011](../reports/codex-morrow-v1.1/m03-product-integration-011-2026-10-01.md)：默认Flutter→独立supervisor→原Rust业务host/Wasmi链完成；修复早host崩溃Data写失败绕过回收，可信再预览和手动Retry接入。统一新候选1515源恒同：UI24/native9、完整host199、默认Release构建、真实host/UI/sup故障/35.6ms早故障、commit→exit窗口、实际WM_CLOSE及共享核心15检查/16故障通过。Unknown/原失败不清除，缺证明继续拒绝；历史失败和未观察保留。G0产品图0/2、SDK冻结、跨平台/短写完成/断电/恶意同用户隔离仍未验收。未提交、无CI/推送/部署。


2026-10-01 [生产 owner 显式恢复 010](../reports/codex-morrow-v1.1/m03-owner-recovery-010-2026-10-01.md)：Windows v2身份/原文历史 CAS、只读 preview及 Flutter显式 Unknown确认已接入；新候选本地 ledger10、gate1、原worker5、UI/session15、实际生产8项通过。实际默认 Windows Release及 UI崩溃→显式资源恢复→新代次渲染已通过；sup缺原proof仍保守拒绝，Recovered不是正常业务成功或direct host放行。完整198项host回归及Release补充故障验证执行中；G0双产品图/SDK冻结/跨平台/同用户隔离/真实短写/断电仍未验收。未提交、未CI、未推送或发布。原历史保留。


2026-10-01 Windows 生产接入 009 已完成本轮限定验证：默认 Flutter → 独立 supervisor → 原 Rust workbench host/进程内 Wasmi；9 条生产命令退出 0、28 项 focused 测试通过，平台 13 项、更新后 Release 桌面正常/UI 崩溃、共享 M03 15 条检查及 16 项真实故障回归通过。已修复失联与正常证明裁决、死 UI 输出中断回收证明、M03 提前 Released 竞态。真实 UI 崩溃 0.101617 秒回收全部持有句柄，资源证明完整但 owner/业务仍 Closing/Unknown，真实重开拒绝；绝不将 exit 2 或资源证明当业务成功。 跨平台/恶意同用户隔离/成功 OS 短写/断电持久性/G0/SDK 冻结及 supervisor 崩溃后的可信 owner 修复未验收。见 [009 实现、证据与剩余边界](../reports/codex-morrow-v1.1/m03-production-supervisor-009-2026-10-01.md)。分支和 HEAD 保持，修改未提交，未 CI/安装/提权/发布。

完整 Workbench Rust host 库扩展回归 **193/193** 通过，零失败/忽略，独立复核完成；首次缺真实 guest 路径的失败仍保留。限定范围及新证据见 [完整 host 回归补充](../reports/codex-morrow-v1.1/m03-production-supervisor-009-host-regression-2026-10-01.md)。

当前 M03 共享核心另外完成 [3 项真实 Core/HTTP 故障复验](../reports/codex-morrow-v1.1/m03-shared-supervisor-core-009-http-2026-10-01.md)：每例真实 supervisor 0、恰好 1 POST/无额外 POST，Close/ACK 与最终资源证明分开，业务 Unknown 保留；仍不等于完整 M03/G0 或 SDK 冻结。


2026-09-30 独立 supervisor Windows 008 已实现：[证据与边界](../reports/codex-morrow-v1.1/m03-independent-supervisor-008-2026-09-30.md)。最终 check014 的 15 项指定命令通过、278 源指纹稳定；新 run005 的 16 项真实故障/恢复/并发案例通过。exit 2 负测不计作 Close/ACK 协议成功；supervisor 崩溃后的耐久 owner 保持 fail-closed。真实 Core/HTTP 三场景均 expected_fault_observed（各一次 POST、host 0、完整终态与回收证明）；Flutter 接入、跨平台、恶意同用户隔离、成功 OS 短写及产品 G0/SDK 冻结尚未验收。改动未提交，无 CI/安装/提权/发布/合并。下方等待架构选择的内容仅为历史记录。

2026-09-30 恢复更新：[M03到期终态006](../reports/codex-morrow-v1.1/m03-expiry-terminal-006-2026-09-30.md)已修生产host到期Stop过早关闭输入，立即撤权/禁止新业务并固定有界完成Close/ACK；新三故障场景终态闭环，业务Unknown保留。[Windows owner/lifecycle007](../reports/codex-morrow-v1.1/m03-lifecycle-owner-007-2026-09-30.md)补充真实用户锁namespace下崩溃后拒绝、继承stdio ClosingUnconfirmed、错PID拒绝、四实际会话重叠及同profile StorageBusy；独立只读复核通过这些限定子项。短成功OScompletion、完整崩溃恢复/恶意同用户隔离、跨平台及G0/SDK冻结仍未完成；新增supervisor等待用户架构选择，未实现或安装。当前分支HEAD04b060e，修改未提交，未CI/合并/部署/发布。下文旧状态保持历史记录，最新边界以链接报告为准。


2026-09-30 [暂停同步检查点](../reports/codex-morrow-v1.1/m03-pause-checkpoint-2026-09-30.md)：按用户要求暂停开发，三协作会话空闲。network-abort 单次本机运行观察通过；authority-deadline 已启动批次收尾为 unconfirmed（guest evidence writer join 未确认、缺 Close/ACK），原证据保留；pipe-partial-close 未启动，真实运行后独立复核未进行。fixture003源码和回执已逐字节归档到companions；仅同步当前开发分支与 Drive 单个ZIP，不合并主线、不发Release。SDK未冻结。

2026-09-30 [M03 被动故障实现](../reports/codex-morrow-v1.1/m03-passive-fault-005-2026-09-30.md)：fixture003、宿主H1/H2及运行入口已实现；当前runner003保留001／002原审查阻塞及快照，13项纯检查通过。宿主default／feature构建与52次定向调用、guest Core15/native35通过；三个HTTP故障场景尚未运行，正在冻结实现复核。SDK未冻结，未提交／推送／发布。

2026-09-30 [M03 当前宿主生命周期复验](../reports/codex-morrow-v1.1/m03-lifecycle-004-2026-09-30.md)：新增期限触发和 peer 断开同进程 pipe 测试，Windows probe 15/15、当前原生库 28/28；新固定宿主与冻结 real Core fixture 对撤权/OS 背压各运行 1 次本机 POST，生产方均通过，独立只读整链复核已限定通过。首次证据路径拒绝（0 POST）保留失败。原期限自然到达、网络/残帧断开的完整被动 fixture003 在设计中，SDK 未冻结；未推送／发布。

2026-09-30 [Drive 合入与 Windows 复核](../reports/codex-morrow-v1.1/drive-review-2026-09-30.md)：接入 partial-write-003 和插件打包前一致性检查；本机网络 21 项、纯写状态 8 项、Windows probe 13 项、原生库 26 项及 SDK 工具 62 项通过，原生二进制重新构建成功。上述集合有重叠，不能相加；仍未覆盖真正非零部分 OS completion 故障、完整 Core/child/HTTP 竞态、产品 G0 和跨平台。SDK 未冻结，本轮未推送／发布。

当前工作区基于远端文档检查点 `21f84aaebca31c8ef8d3bdfb3b9788f2a4cffb54`。下文保留 2026-09-29 历史基线，最新验证和后续顺序以上述报告为准。

更新：2026-09-29。分支：`codex/m03-stream-revocation-backpressure`；M03 源码检查点 `c8074fef4d4849e4130f54f1b5bd1e6bf7c78494`。后续仅修改文档，未重新运行产品、M03 或 CI 测试。

## 与主线及发布的关系

`main` 在本次检查前为 `d9c0431`，应用版本 `0.1.9-test.56+60`，对应公开 Windows [test.56 测试预览](../reports/0.1.9-test.56-release.md)。本分支基于 `codex/io-safety-refactor` 的 `8855791`，追加一项 M03 检查点；写本文时其代码检查点相对原主线领先 16 个提交、落后 0。主线后来增加的文档提交不改变这个代码基线对比。本分支未合并、未创建新标签或公开 Release。代码版本号与公开下载包版本相同，不表示下载包含本分支的文件变更或 M03 实验。

## 已验证范围及限制

| 范围 | 分支上已有的限定证据 | 仍需完成 |
| --- | --- | --- |
| 文件读取与会话 | 受管文件任务、增量校验与原生选择器／Flutter 接线，见[文件 owner](../reports/plugin-file-owner-2026-09-26.md)及[会话阶段](../reports/plugin-file-session-2026-09-26.md) | 当时的阶段缺口须以后续文件变更报告更新，不把早期“尚未 UI 接线”继续当作整个分支的现状 |
| 文件创建／删除和 Guest | 开发分支已接独立 mutation 契约、C/C++/Rust Wasm、显式预算、双阶段审批、持久计划/效果核对和恢复 UI；[文件变更状态记录](PLUGIN_SYSTEM_STATUS.md)及[开发看板](DEVELOPMENT_BOARD.md)列出 2026-09-27 各阶段证据。16 MiB 文件效果故障/恢复专项 52/52，四个内容提交点专项 20/20；计数各自绑定场景和产物 | 条件 Replace、系统选择器/人工审批、Debug 最大正文期限、断电级故障及其他平台资格未关闭；SDK 未冻结 |
| Windows 本地预览 | [本地预览报告](../reports/windows-mutation-preview-2026-09-27.md)记录完整构建装配、最终目录原生集成 6/6、应用自检 4 项 PASS、501 项摘要与两份 ZIP 核对 | 它是开发分支本地产物，不是公开 test.56 Release；后续审批期限修复未包含在该 ZIP，仍需新产物及人工实窗验收 |
| 审批期限修复 | [专项报告](../reports/guest-mutation-expiry-2026-09-27.md)记录会话 18、Widget 9、原生 4，共 31/31 的定向资格 | 系统选择器与人工审批、装配后成品复验、其他平台仍开放 |
| M02 原生会话与准入 | 固定候选和记录中的限定验收 | 生产审批、恢复和完整产品 G0 |
| M03 撤权 | A010 真实运行失败保留；A011 以新候选完成一次真实 Core 事件抑制运行，另有独立只读原件／持久账本复核 | OS 临界竞态、完整并发矩阵与产品接线 |
| M03 系统背压 | B011 对同一 OS 写操作记录三次未完成及取消前补采样、实际回收；一次真实运行加独立只读复核 | OS 部分写入故障、资源生命周期与跨平台 |

A011、B011 的只读复核不是第二次 HTTP 运行，A010 不因后续通过而翻判。fixture002 的 Core 14 项、native 31 项是 M03 定向回归，不代表原 84 项产品场景已经升级。M03 的原件、时序、限制与下一切片见[交付记录](../reports/codex-morrow-v1.1/delivery-2026-09-29.md)及[计划终局签收](../reports/codex-morrow-v1.1/m03-control-pressure-002-plan-2026-09-29.md)。

## 证据、恢复与近期工作

`companions/morrow-codex` 是独立仓库的源码快照，保留受约束的上游输入和许可证，不是已安装的生产插件。Git 未包含大型生成二进制、数据库与压缩证据；配套 Drive 备份及还原清单由[交付记录](../reports/codex-morrow-v1.1/delivery-2026-09-29.md)指向。恢复到不同机器后应重新验证，不得把旧机器回执改写成新运行记录。

2026-09-25 的主线 Web Actions 在 `ccdf89b` 的孤儿数据 fixture 导航出现 `net::ERR_ABORTED`，后续场景跳过；这不是 M03 的验收结果。见[分支 Web 表](WEB_PARITY.md)和运行 36115685547（历史外部引用）。

下一步先设计并验证 M03 的 OS 部分写入故障与取消临界完成竞态，再补并发、资源回收、同用户隔离、崩溃后 owner 核对、产品 G0 与跨平台。文件变更路径则需以当前分支最新专项为基线，完成系统选择器/人工审批和修复后的成品复验；各范围分开记录真实运行、只读复核与发行资格。
