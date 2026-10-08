# SDK 后续门槛与实施顺序

<!-- C28-CURRENT-BEGIN -->
## 当前开发检查点（2026-10-08，C28）

本次向 `codex/windows-sdk-convergence-20261005` 同步 C11–C28 的 Agent/SDK 开发源码、相对路径构建入口与限定进度说明；应用版本保持 `0.1.9-test.58+62`，不更新 main、标签或 Release。

原 protected owner 的会话链已有有界验证，H008 编译与 8 项纯诊断测试通过；Windows 原生 Start 仍为 Unknown，清理和显式修复仍 pending，不重放不确定操作。新诊断版完整 Windows 链尚未运行，移植副本的依赖检查与既有隔离测试分别记录。Agent 接口仍 experimental，安全执行层、SDK26/G04、完整 Codex IPC、PTY/stdin/resize、完整网络隔离及其他平台资格仍 OPEN。

优先完成新 Windows 链实测、定位 Start Unknown 并复验真实回收；会话层与安全执行层验收后暂停准备测试预览。完整范围见 [C28 同步说明](../reconstruction-2026-10-08/windows-agent-sdk-c28.md)。下方报告保留各自历史时点，旧“未推送”不覆盖本次同步。
<!-- C28-CURRENT-END -->

<!-- C15-CURRENT-BEGIN -->
2026-10-06 C15本地候选：工作台完整会话插件管理已接独立admin协议/owner与设置界面，
借原Manager/持久catalog，保留原busy/lost/恢复门禁和顶层native管道，不改旧Core/IO。
admin10、owner18、旧host18/14/13/1回归分别通过；Dart18（含实际Rust五对向量互验
和原管道Busy路由）通过。新库strict Clippy、Dart fatal-infos/限定格式和Windows宿主check
通过；不是新OS执行/真实桌面/ProtectedSession资格。327/57/旧合同/Linux/封存工件保持。
实际native port借原runtime/worker、生产GUI、认证/sandbox、交互控制及全SDK26/G04仍OPEN。
没有commit/push/Release/CI。详见 [C15报告](../reconstruction-2026-10-06/codex-sdk-c15.md)；下方C14及更早内容保留其历史范围。
<!-- C15-CURRENT-END -->

<!-- C14-CURRENT-BEGIN -->
2026-10-06 C14本地候选：完整Agent wrapper持久catalog/approval接口已实现，原Manager撤销seam
和同原connection/freshadmission接线保持。新catalog18、Manager新3与其原6、原回归11/1/6、
旧managed/route/schema14/13/1分别通过；实际Windows新1通过（helper1过滤，sandbox=None）。
大包装不受原快照512KiB限制，static审批不恢复livegrant；保存故障/Unknown即时撤旧Core，
旧效果不重放。327/57/旧合同/封存工件保持；Workbench新协议/GUI、ProtectedSession/native
owner、认证transport、sandbox、交互控制/其他平台及SDK26/G04仍OPEN。未commit/push/Release。
详见 [C14报告](../reconstruction-2026-10-06/codex-sdk-c14.md)；下方C13及更早文字为各日期历史，不替代本条实际范围。
<!-- C14-CURRENT-END -->


<!-- C13-CURRENT-BEGIN -->
当前 C13（2026-10-06，本地未提交）：会话/进程新入口已接原 Catalog/Registry/Manager，
共用原实例限额、Control 与连接；基础包与完整 wrapper 仍独立批准，预算取交集。
新增 managed host 14、Manager 6、HostIdentity 3、真实 Windows managed Wasm 2 分别通过；
原 host 1+13、Manager 11+1、strict 6、process 18+5 回归另计，重复不累计。
修正提交前取消、effect 后 Unknown 与错配收尾；新 host strict Clippy/限定格式通过。
旧 327 SDK／57 冻结输入、wire/schema 与封存工件保持；新候选不继承旧冻结 pin。
完整 wrapper 持久审批、Workbench/ProtectedSession owner、sandbox/认证/其他平台仍
OPEN/NOT_RUN；SDK26/G04 未冻结。见 [C13 接线与实际边界](../reconstruction-2026-10-06/codex-sdk-c13.md)。
下方 C12 及更早检查点保留历史结果与当时身份；本轮现状以 C13 为准。
<!-- C13-CURRENT-END -->

<!-- C12-CURRENT-BEGIN -->
当前 C12 接口增量（2026-10-06，本地未提交）：已补会话 Rust/Wasm 客户端、类型化进程控制、
严格 single-import 组合运行入口和原 R2 执行权实时复验。process 合同 23、R2 客户端 10、
旧入口真实 Wasm 6、process 客户端 10、组合 host 14、运行入口 6、定向 runtime 回归 21、
原 R2 回归 106 分别通过，重复方法不累计。Windows 原生实际执行 7 项、纯注册表 3 项、真实 proposal Wasm→Windows→process Wasm 耦合 3 项分别通过；封存 Wasm 未重建，Unknown 不重放。
327 SDK／57 冻结输入及旧封存归档保持；新候选不继承旧冻结 pin。
原生 close-input／PTY resize 尚 Unsupported；完整 SDK26／G04、产品 GUI、受保护内容库、
OS sandbox、认证 transport 和其他平台仍 OPEN／NOT_RUN。详见 [C12 接口与限定证据](../reconstruction-2026-10-06/codex-sdk-c12.md)。

下方 C11 及更早日期的文字保留当时的身份、结果与未运行范围；本轮现状以 C12 为准。
<!-- C12-CURRENT-END -->

C11后续（2026-10-06）：真实新三语言目录guest已在Windows普通合成Store的原owner上12方法通过，新C/C++构建/静态门禁也已补齐。R2 Windows106及旧Linux归档跨平台封存核验通过；旧合同pin未改变，真实Windows进程执行器/生产接线仍开放。下一项推进产品批准链、ProtectedSession/picker来源、第三方分发、blob耐久/恢复及平台矩阵；完整SDK26/G04不关闭。见[最新限定资格](../reconstruction-2026-10-06/windows-sdk-c11.md)。


Codex基础接口R2完成后的顺序（2026-10-05）：原六项缺口已修补，收尾预留、可信事实递进、退休／换代、准入回收和独占owner具备实际回归。专用包／Wasm／native路由、Linux固定输入执行器与实际ThreadStore通过限定验收；正式Rust156／Python32／外部Codex2及移地客户端编译通过，573来源／617成员归档独立核验。下一步为实际生产Exec进程／事件provider、完整Codex注入、认证批准链与平台资格；PTY／交互输入输出／resize／signal／terminate延期，SDK26仍OPEN。R1历史保持，见[修补与整体核查](session-exec-v1-r2-repair.md)和[当前合同](../../docs/PLUGIN_AGENT_SESSION_EXEC.md)。

此前内容片段已实现（2026-10-05）：独立agent-content-v1四请求与原HostRuntime接线、可信批准／单次CAS、完整读取消费者当时通过60个方法，原Core回归24及示例分别实际通过；本轮v25后内容60项再次回归。后续继续新native准入与内容外发交集、view、账户和native bundle。完整M05／SDK26保持OPEN；[当前缺口与证据](codex-sdk-interface-gaps.md)区分已有实现及尚未闭合的接线。

Codex候选冻结门禁推进（2026-10-05，Linux云端）：当前宿主新增[候选验证入口](../../tool/verify_codex_sdk_candidate.py)，核验consumer review与kit摘要、180个完整文件、当前canonical全部12个源码文件及wire身份；19对向量、1项版本拒绝和命令证据也须完整。Python门禁40／兼容与同步22、canonical Rust qualification14、默认feature check、目录静态profile9实际通过，各自计数。输出仍为 `qualification_only`、SDK `OPEN`、P-02 `NOT_RUN`。后续消费该候选先运行此宿主门禁，再按独立门槛执行真实插件资格；归档build_plan和历史receipt不能替代当前检查。详见[本轮证据与未闭合项](codex-sdk-freeze-gate.md)。

当前C10检查点（2026-10-05）：本次开发分支更新收录C08–C10。C10已新增独立 `fs-directory-request-v1`、严格单import、包feature及只读discovery，复用原已批准selection和owner，不导出路径、句柄或新授权。Windows新Rust34（codec12/helper4/profile9/owner9）、既有回归191及frame5分别通过；Python33和两个原生C/C++消费者另计。新Rust Wasm仅编译通过，真实新Rust guest、C/C++ Wasm、Workbench产品／GUI、受保护Session与其他平台仍未验收。旧Core IO FileList保持Unsupported，SDK26／G04仍OPEN，无新Release。 详见[接口与实测边界](directory-request-sdk.md)。

## 2026-10-05 源码同步后的执行顺序

当前开发线为 `codex/windows-sdk-convergence-20261005`，版本 `0.1.9-test.58+62`；[项目状态](../../docs/PROJECT_STATUS.md)统一区分源码、限定资格和已发布下载。C02–C07 源码与报告在本次检查点同步，不重跑历史测试、不产生新 Release。

1. 已封存 C07：115 / 42 / 100 分别计数，42 保留过滤；原件输入不变，整库 Clippy exit101 的 10 处既有诊断保留。
2. C08 已安装原 owner 工作线程的新鲜 secret factory 与所持缓冲零化实现：`capture_directory_fresh(File, CaptureLimits)`只在原身份／FileList检查后调用锁定 `getrandom 0.4.3`，原checked worker/session serial参与域分离，前后复核原clock／取消／授权；OS随机调用不持原clock／Control／Ticket锁。熵错与全零关闭，Unknown不重放，legacy显式secret接口保留。新Windows执行结果为 `PASS（Windows限定）`，见 [C08说明](directory-secret-factory.md)，各组重新实际执行，不继承或累计C07通过。
3. C09可信opened anchor相对选择已完成限定Windows新20／factory14／owner115／原件42资格，见 [C09说明](directory-selection-owner.md)。原worker在FileList预算内保留anchor至leaf全部句柄并复核，Workbench `start_directory`仅Release locked/offline库check通过，首次缺offline依赖exit101保留；产品NOT_RUN，当前network100和Clippy NOT_RUN。
4. 继续完整G04：anchor以上／native picker时刻来源和传入anchor的sharing策略、Workbench产品执行、C10独立Dir request已实现，真实三语言guest与产品profile资格仍待验证、blob durable backend/history、upload/watch/rename与恢复矩阵。
5. 并行规划 G05/G06/G07 的异步组合、长期变化与恢复身份，再按授权分别验证生产和平台矩阵。完整 SDK26 / G04 仍 OPEN，公开 FileList 与 conditional Replace 继续 Unsupported。

C08/C09新增仍本地未commit／push，已有772466文档／源码发布检查点保持。下方 C02–C07 及更早记录保留当时语境；旧“下一片段”已经完成时，以本节顺序和最新证据为准。

2026-10-05 **C07 Windows 原 owner 目录命令有界通过**：原 `IoWorker` 的可信 selected-directory 捕获／分页／结束、单命令取消、未读交付与 idle 清理已接线；九组实际115方法通过，其中新取消7、新目录owner14和保留native16已包含在115内。原件42和network100分别fresh通过，保留过滤与child helper范围；不与C06历史、旧114、首个负向、子案例、时钟样本或命令数累计。格式检查及strict Rustc通过；整库Clippy仍exit101，10处既有源诊断逐一保持bootstrap字节，本轮owned诊断0。见 [C07范围／原始证据摘要](directory-owner-sdk.md)。

原Authority／Control时间、授权和交付校验方法保持，Control新增有界目录Admission记账。每个原clock样本与验证在同一短step中完成，native查询／私有解析、编码、取消谓词和实际root／lease／spool drop均在clock锁外；不能抢占同步OS，不当缓慢的可信clock closure仍可能阻塞共享时钟。global8包含queued／resident／retired tombstone，真实资源drop后才释放额度；累计费用不退，metadata allowance不是总RSS上限，Unknown不自动重放。

C07封存时的历史门槛（C08–C10进展以本页最新检查点为准）：**SDK26／G04继续OPEN**。生产protected owner9／真实session／StorageIoWorker／GUI、trusted-secret factory、picker／祖先来源、workspace task入口、新C10独立Dir request已实现，真实三语言guest与产品profile资格仍待验证、blob durable backend/history及其它平台尚未闭合；原公开FileList及conditional Replace保持Unsupported。普通合成Store和临时目录不替代生产owner。下方C06及更早日期说明保留其历史范围；C07当时最终代码资格SHA256为`773e36fa49013d79071a6cb9f9de4500cb74d99c5aec8042b70e39290ccdea7f`。

2026-10-05 **C06 当前推进**：目录codec/state13、blob codec/state23、Windows native broker16、138共享vectors与独立Rust7/去重19families通过；当前原件42定向parent名单及network100均fresh执行PASS。CPP复执行8个C families不另增方法，42过滤/child helpers另记。新增payload消费者是Windows native，无directory/blob Wasm或guestnegotiation。完整runtime strictClippy FAIL101保留，新增两处诊断修正后native16/该范围strictcompilePASS。G04/SDK26继续OPEN；见 [C06](directory-blob-sdk.md)。

历史 C06 计划（原 owner 队列已由 C07 完成）：原 `IoWorker<O:ManagedHostOwner>` 的Windows private typed directory command lane：同原Manager/HostRuntime/ManagedInstance/IoBinding、原clock/Control/Ticket/worker退出与join。capture移动可信已选择File，单页pending/累计预算/取消/迟到或未领取Unknown/idle清理必须进入原owner循环；不得创建替代Store/owner或复用FileRead来授权FileList。DirectoryBroker现有root object证明不含ancestor/picker provenance；fresh unpredictable per-broker secret factory也未实现。trusted host factory、protected owner资格和公开guest request/import/feature协议随后独立审查，当前不直接新增单import或放宽旧组合gate。

以下C05/C04dated记录保留其当时边界；旧计划的未完成措辞不覆盖本条C06已完成的有限codec/native范围。

2026-10-05 **C05 当前推进**：类型化SSE与独立payload源码分发的限定Windows资格完成。库6、网络100、准备策略11、原生83vectors、27分发方法和项目外六新guest实际执行各有独立证据；重复四运行方法不增加100。原SDK327／冻结57恒同，当前仍本地未提交／推送／发布。见 [SSE](sse-event-sdk.md)及[分发](channel-payload-distribution.md)。

历史 C04/C05 顺序（codec/state 已由 C06 有限完成）：按G04推进独立fs-directory/blob-transfer codec与状态校验，再接严格版本化单import资源入口。已有channel::Directory只列已批准通道；FileRead不推导FileList／递归／rename。当前Core/Runner拒绝channel+IO和多个extra imports，不能直接放宽旧factory。原ReadAttachment授权／card revision／32KiB片段可先补完整下载摘要；真实文件目录broker、watch／rename／upload、原owner接入和各类恢复仍需实现／验收。条件Replace继续Unsupported。生产受保护owner／真实文件／其它平台保持独立门槛，不改完整26目标。

2026-10-05。本文更新后续实施次序，不改变完整 SDK 目标。历史 [26 项需求／10 组门槛](../reconstruction-2026-10-03/sdk-scope-freeze-gates.md) 保留其原始观察身份；当前进展以 [C02 Windows 复验](windows-sdk-revalidation.md)、[C03 能力发现](changes-sdk-discovery.md)及本阶段实际记录为准。

SDK 的接口实现、编解码一致性、实际插件执行、宿主批准链、生产界面和平台资格是不同的门槛。Windows 合成内容库中的真实执行可以证明该限定流程，不能代替用户内容库、账户或其他平台的验收。完整 SDK 仍为 OPEN。

## 当前能力与下一步

| 门槛 | 当前证据 | 下一项实现／验收 |
|---|---|---|
| G01 生产 channel 与批准入口 | 原公共 channel 的队列、Receive／ACK／Send、精确身份、撤权、期限及 join 已有限验证；C02 补齐 Windows 复验 | 对当前最终候选运行真实 catalog／owner／Flutter 批准链和产品故障矩阵。当前用户范围排除 GUI／受保护 owner，不能以普通 Store 的测试关闭此项 |
| G02 网络流与三语言 SDK | 受管 POST SSE、WS 及普通 Store guest 链已有限定验证；C04/C05两种类型化payload与独立分发／六新客体实际执行通过 | 补真正第三方安装／审批与生产入口、账户／TLS及完整网络恢复。解码消息或分发清单不产生网络授权 |
| G03 完整 HTTP／账户 | 原有有界 HTTP、服务、请求／响应字节接口及合成端点测试存在 | 独立 Account／OAuth、流请求正文、认证租约、cookie／signer、TLS 与目的地策略资格；既有成功不证明账户与公开服务可用 |
| G04 文件目录／附件 | C06历史目录13/blob23及codec/state/conformance保留原范围；C07九组115、原件42和network100保留各自资格；C08原owner fresh-secret入口已实现，复验 `PASS（Windows限定）`；公开guest FileList仍Unsupported | C09可信相对选择已限定验证、Workbench宿主入口仅check通过；继续补anchor以上／picker时刻来源、Workbench产品执行、C10独立Dir request已实现，真实三语言guest与产品profile资格仍待验证、blob durable backend/history/upload/watch/rename与平台矩阵；conditional Replace Unsupported且不退化overwrite |
| G05 异步依赖组合 | 原依赖图和有限同步调用已有；原 import 组合限制保留 | 新组合 profile：根 deadline／取消、双方撤权、原子预算预留、队列／重入、实际终态与 join；不能简单放宽旧 factory |
| G06 内容变化／Cloud | 有限固定集合 changes source、三语言 metadata SDK、精确 ACK 和独立 discovery 已有限定验证 | 长期 watch、retention／gap、完整 cursor 与跨进程恢复、工作区／关系、服务插件更新和当前授权；Cloud 仍为可选独立服务 |
| G07 历史与恢复 | 有界单操作核对、HTTP 历史、内容回执和 resource-only 恢复各有独立合同 | 跨进程真实故障窗口、各扩展恢复身份与资源释放矩阵；Unknown 不自动重放，历史 ACK 不恢复活授权 |
| G08 UI／第三方产品 | 基础声明式 UI 与冻结原件有旧有界接口和运行证据 | 独立开发者安装、批准、内容动作、撤权与关闭；富 UI profile／控件 fallback／资源租约逐项验证。C／C++／Rust 为入口，当前不新增 TS／JS 或动态 Dart 插件 |
| G09 平台 | 本阶段是 Windows x64 的限定资格；Linux 云端与旧平台报告各保留来源 | OS×架构×profile 的 backend／owner／存储／凭据／UI／生命周期矩阵；Android、Web、macOS、iOS 不继承 Windows PASS |
| G10 冻结与分发 | SDK327／冻结57历史字节证据保持，原 Rust／C／C++ guest 与 provider 不重建；当前Codex候选kit／review／canonical源码门禁通过，57个冻结输入重新核验；C03 交接补丁可恢复 | 候选门禁只证明输入身份与完整性，不能关闭SDK26；继续扩展独立分发与模板、第三方接入、原 committed-tool runner 身份和完整目标验收；提交／公开分发须在授权范围内，ZIP 或版本号不代表冻结 |

## 建议次序

1. 保留C04/C05已完成的payload与分发资格及可恢复交接，不重复增加方法或改写历史失败。
2. C08原owner fresh-secret factory已完成限定Windows复验，C09可信相对选择已限定验证、宿主任务入口仅check通过；继续核验anchor以上／picker时刻来源、生产secret生命周期、Workbench产品执行与C10独立Dir request已实现，真实三语言guest与产品profile资格仍待验证；继续blob durable backend/history/upload/watch/rename与完整平台矩阵。SDK26/G04冻结目标不能缩为现有guest补丁或当前PASS子集。
3. 独立推进异步组合profile及各能力的恢复身份和预算结算，目录codec不等于目录权限，单个阶段不替代全G04/G05验收。
4. 在相应授权与环境就绪后，分别执行生产批准链、普通用户 token、第三方与逐平台矩阵。
5. 审计完整 26 项需求并确定冻结身份；缺项仍保留 OPEN，不把完成标准改成当前已通过的子集。

每个阶段先封存上一来源，再实现、执行与独立复核。恢复包包含准确基线和可应用的增量；原始失败不能被后来通过覆盖。实际资源 join、已消费 ACK、远端业务结果与内容提交回执各自记录。

## C04 封存时的历史计划

下列清单是 C04 时点尚未实现／运行的计划。C05已完成SSE与独立源码分发的限定Windows资格，见本文件最新条目；保留历史计划，不将其NOT_RUN当作当前状态。

1. 在新的 `extensions/sse-event-v1` 复制原 `sse_event.capnp` version 1／raw SHA256 `bec1e174b7586e46155b60471867900bf91f1fb11849e6761916bfc3ca7c9863`，提供 Rust 自有／借用事件、C 单一合计有界存储和 C++17 owner。保持 data／event／id 的完整 UTF-8、内嵌 NUL及 retry 的 None／0／MAX 区分；共享 pointer 的合计长度在分配前核对。它是 payload codec，不叠加上游 SSE 文本解析器的 id忽略、默认事件名或截断规则。
2. 增加新三语言示例和独立准备工具，复用原 nonduplex Events channel 与精确 ACK。通过真正的原生参照 corpus、严格 C／C++消费者及一次 POST 的实际 guest 流验证字段、预算、撤权、终态和各层 join。历史 helpers／Wasm／package 保持原样；id、retry、EOF及 `[DONE]` 不产生重连、重放或业务完成权限。
3. 使用新导出／验证 profile 保持 `sdk/` 与两个 `extensions/` 的 Cargo 相对路径，不扩大旧 SDK327 分发合同。闭包只含不变库／锁／契约／license、新库／headers／示例和独立验证器；导出核对当前唯一原契约，异地验证器只证明有限清单完整性。
4. 分别测试目录／ZIP、迁移路径、大小、重复／大小写冲突、重解析点、篡改与拒绝覆写，再在仓库外实际离线编译三语言开发示例。旧 SDK-only CLI 和依赖 repo原生契约路径的准备工具，未经适配不能声明已在新闭包可用。

这一步补充第三方开发所需的载荷接口与开发环境，不授予 Store、credential、endpoint、owner 或生产 source approval；SDK327／冻结57、原 discovery 与 route 身份继续保持。
