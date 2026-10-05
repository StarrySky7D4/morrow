# SDK 后续门槛与实施顺序

## 2026-10-05 源码同步后的执行顺序

当前开发线为 `codex/windows-sdk-convergence-20261005`，版本 `0.1.9-test.58+62`；[项目状态](../../docs/PROJECT_STATUS.md)统一区分源码、限定资格和已发布下载。C02–C07 源码与报告在本次检查点同步，不重跑历史测试、不产生新 Release。

1. 已封存 C07：115 / 42 / 100 分别计数，42 保留过滤；原件输入不变，整库 Clippy exit101 的 10 处既有诊断保留。
2. C08 优先实现原 owner 工作线程上的新鲜 secret factory 与持有期零化；原 checked worker/session serial 参与域分离。当前只完成设计审查，尚无实现或执行资格。熵错误关闭、取消不重放，OS 随机调用不持原 clock / Control 锁。
3. 继续完整 G04：picker / 祖先证明、工作台任务入口、独立目录 request/schema、包 feature/import/helper profile 协商、blob durable backend/history、upload/watch/rename 与恢复矩阵。
4. 并行规划 G05/G06/G07 的异步组合、长期变化与恢复身份，再按授权分别验证生产和平台矩阵。完整 SDK26 / G04 仍 OPEN，公开 FileList 与 conditional Replace 继续 Unsupported。

下方 C02–C07 及更早记录保留当时语境；旧“下一片段”已经完成时，以本节顺序和最新证据为准。

2026-10-05 **C07 Windows 原 owner 目录命令有界通过**：原 `IoWorker` 的可信 selected-directory 捕获／分页／结束、单命令取消、未读交付与 idle 清理已接线；九组实际115方法通过，其中新取消7、新目录owner14和保留native16已包含在115内。原件42和network100分别fresh通过，保留过滤与child helper范围；不与C06历史、旧114、首个负向、子案例、时钟样本或命令数累计。格式检查及strict Rustc通过；整库Clippy仍exit101，10处既有源诊断逐一保持bootstrap字节，本轮owned诊断0。见 [C07范围／原始证据摘要](directory-owner-sdk.md)。

原Authority／Control时间、授权和交付校验方法保持，Control新增有界目录Admission记账。每个原clock样本与验证在同一短step中完成，native查询／私有解析、编码、取消谓词和实际root／lease／spool drop均在clock锁外；不能抢占同步OS，不当缓慢的可信clock closure仍可能阻塞共享时钟。global8包含queued／resident／retired tombstone，真实资源drop后才释放额度；累计费用不退，metadata allowance不是总RSS上限，Unknown不自动重放。

**SDK26／G04继续OPEN**。生产protected owner9／真实session／StorageIoWorker／GUI、trusted-secret factory、picker／祖先来源、workspace task入口、新独立Dir request/schema与包feature/import/helperprofile协商、blob durable backend/history及其它平台尚未闭合；原公开FileList及conditional Replace保持Unsupported。普通合成Store和临时目录不替代生产owner。下方C06及更早日期说明保留其历史范围；最终代码资格SHA256为`773e36fa49013d79071a6cb9f9de4500cb74d99c5aec8042b70e39290ccdea7f`。

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
| G04 文件目录／附件 | C06历史目录13/blob23及codec/state/conformance保留原范围；C07原IoWorker可信目录队列九组115（含7取消/14owner/16native）、fresh原件42和network100通过；公开guest FileList仍Unsupported、整库Clippy仍101 | 保持完整目标：生产trusted-secret factory/picker/ancestor proofs、workspace task入口、独立Dir request/schema与SDK feature/import/helperprofile协商、blob durable backend/history/upload/watch/rename与平台矩阵；conditional Replace Unsupported且不退化overwrite |
| G05 异步依赖组合 | 原依赖图和有限同步调用已有；原 import 组合限制保留 | 新组合 profile：根 deadline／取消、双方撤权、原子预算预留、队列／重入、实际终态与 join；不能简单放宽旧 factory |
| G06 内容变化／Cloud | 有限固定集合 changes source、三语言 metadata SDK、精确 ACK 和独立 discovery 已有限定验证 | 长期 watch、retention／gap、完整 cursor 与跨进程恢复、工作区／关系、服务插件更新和当前授权；Cloud 仍为可选独立服务 |
| G07 历史与恢复 | 有界单操作核对、HTTP 历史、内容回执和 resource-only 恢复各有独立合同 | 跨进程真实故障窗口、各扩展恢复身份与资源释放矩阵；Unknown 不自动重放，历史 ACK 不恢复活授权 |
| G08 UI／第三方产品 | 基础声明式 UI 与冻结原件有旧有界接口和运行证据 | 独立开发者安装、批准、内容动作、撤权与关闭；富 UI profile／控件 fallback／资源租约逐项验证。C／C++／Rust 为入口，当前不新增 TS／JS 或动态 Dart 插件 |
| G09 平台 | 本阶段是 Windows x64 的限定资格；Linux 云端与旧平台报告各保留来源 | OS×架构×profile 的 backend／owner／存储／凭据／UI／生命周期矩阵；Android、Web、macOS、iOS 不继承 Windows PASS |
| G10 冻结与分发 | SDK327／冻结57字节保持，原 Rust／C／C++ guest 与 provider 不重建；C03 交接补丁可恢复 | 扩展独立分发与模板、第三方接入、原 committed-tool runner 身份和完整目标验收；提交／公开分发须在授权范围内，ZIP 或版本号不代表冻结 |

## 建议次序

1. 保留C04/C05已完成的payload与分发资格及可恢复交接，不重复增加方法或改写历史失败。
2. 在已封存C07原owner可信目录队列范围上，推进生产trusted-secret factory、picker/ancestor proofs、workspace task入口与独立Dir request/schema及package feature/import/helperprofile协商；继续blob durable backend/history/upload/watch/rename与完整平台矩阵。SDK26/G04冻结目标不能缩为现有guest补丁或当前PASS子集。
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
