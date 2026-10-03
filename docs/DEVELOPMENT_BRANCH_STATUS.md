# 最新开发支线状态

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

2026-09-25 的主线 Web Actions 在 `ccdf89b` 的孤儿数据 fixture 导航出现 `net::ERR_ABORTED`，后续场景跳过；这不是 M03 的验收结果。见[分支 Web 表](WEB_PARITY.md)和[运行 36115685547](https://github.com/StarrySky7D4/morrow/actions/runs/36115685547)。

下一步先设计并验证 M03 的 OS 部分写入故障与取消临界完成竞态，再补并发、资源回收、同用户隔离、崩溃后 owner 核对、产品 G0 与跨平台。文件变更路径则需以当前分支最新专项为基线，完成系统选择器/人工审批和修复后的成品复验；各范围分开记录真实运行、只读复核与发行资格。
