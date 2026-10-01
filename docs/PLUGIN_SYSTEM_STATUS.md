# 插件系统当前状态

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

2026-09-30 [M03 被动故障实现](../reports/codex-morrow-v1.1/m03-passive-fault-005-2026-09-30.md)：fixture003及宿主原期限／12B测试接缝已实现；宿主52次定向调用、guest Core15/native35通过。运行入口独立审查的后验与封存阻塞已修并冻结003，13项纯检查通过；正在冻结实现复核，三个HTTP故障场景尚未运行。修复真实控制失败后空等RequestClosed，保留原预算、Unknown和独立data错误。SDK未冻结，未推送／发布。

2026-09-30 [M03 当前宿主生命周期复验](../reports/codex-morrow-v1.1/m03-lifecycle-004-2026-09-30.md)：新增期限触发和 peer 断开同进程 pipe 测试，Windows probe 15/15、当前原生库 28/28；新固定宿主与冻结 real Core fixture 对撤权/OS 背压各运行 1 次本机 POST，生产方均通过，独立只读整链复核已限定通过。首次证据路径拒绝（0 POST）保留失败。原期限自然到达、网络/残帧断开的完整被动 fixture003 在设计中，SDK 未冻结；未推送／发布。

2026-09-30 [Drive 合入与 Windows 复核](../reports/codex-morrow-v1.1/drive-review-2026-09-30.md)：接入 partial-write-003 和插件打包前一致性检查；本机网络 21 项、纯写状态 8 项、Windows probe 13 项、原生库 26 项及 SDK 工具 62 项通过，原生二进制重新构建成功。上述集合有重叠，不能相加；仍未覆盖真正非零部分 OS completion 故障、完整 Core/child/HTTP 竞态、产品 G0 和跨平台。SDK 未冻结，本轮未推送／发布。

2026-09-29 [本支线当前状态](DEVELOPMENT_BRANCH_STATUS.md)：保留 2026-09-27 文件变更与 Windows 本地预览的专项边界，新增 M03 A010 失败、A011/B011 各一次真实运行与独立只读复核；OS 部分写入、系统选择器人工验收、后续修复成品、产品 G0、全平台与 SDK 冻结均未完成。下列记录按各自日期及产物保留，不能把分项计数相加或把本地 ZIP 等同公开 Release。

2026-09-27 源码同步检查点：本提交汇总文件变更底座、C/C++/Rust SDK、审批到期修复、Windows 构建装配和验收记录，目标为 `codex/io-safety-refactor` 开发分支，不创建标签或 Release。下方“未提交／推送／发布”为各次验收当时的历史状态；SDK 未冻结，系统选择器与人工审批、其他平台资格继续保留。

2026-09-27 [Guest 审批期限与收尾修复](../reports/guest-mutation-expiry-2026-09-27.md)：首次 Start 前按单调时间计时，到期后禁止新 Prepare/Chunk/Commit/Execute 和提交重试，保留已有回执核对及 Stop→实际退出→必要 Repair/ACK。弹窗到期禁用确认，确认返回后重新观察原任务；撤权／换库精确关闭旧弹窗，不误关其他对话框。九语言提示同步。最终会话 18、Widget 9、真实原生 4，共 **31/31** 通过，包含两次真实 30 秒到期后的独立 Discovery/Reconcile。修复尚未纳入上一轮 Windows ZIP；下一步为系统选择器与人工审批验收、新应用产物及其他平台资格。SDK 未冻结，未提交／推送／发布。

2026-09-27 [完整 Windows 本地预览验收](../reports/windows-mutation-preview-2026-09-27.md)：应用 `0.1.9-test.56+60`、工作台包 `0.1.9-test.54.7` 与主题 `1.0.2` 完整构建并装配。最终目录原生集成 **6/6**、实际 Release 应用自检 **4 项 PASS**、501 项文件摘要及两份 ZIP 完整性通过；旧输出覆盖拒绝有效。修复 MSBuild 环境中的主题 SHA-256 命令不可用问题。此前 20/52 项资格仍绑定各自旧产物，不视为新包重跑。下一步为审批弹窗跨越 30 秒后的过期提示与安全收尾、系统选择器人工验收及其他平台；SDK 未冻结，未提交／推送／发布。下方历史记录按阶段保留。

2026-09-27 [16 MiB 内容事务故障与恢复资格](../reports/guest-mutation-content-crash-2026-09-27.md)：四个 Core 内容提交点的三语言实际 Guest、普通宿主对照及 Rust 实际恢复 Widget **20/20** 通过。前三点正文与回执一起回滚，after-commit 两者完整持久化；新宿主两轮只读核对保持原 Prepared／无结果，目标目录始终为空，不继承执行许可。另由只读 Store 校验原计划、正文摘要和 LiveStaging 回执，不能用界面阶段代替存储证据。测试清理改为明确确认宿主退出、校验与清理结果，失败保留现场。下一步为完整 Windows 应用／系统选择器与 30 秒人工审批验收，以及其他平台资格；进程退出不等价于断电，Debug 最大正文期限限制仍开放，SDK 未冻结，未提交／推送／发布。下文同日记录保留各阶段当时的边界，以本条为最新进度。

2026-09-27 [16 MiB 文件效果故障与恢复修复](../reports/guest-mutation-max-crash-2026-09-27.md)：最大正文实测暴露并修复了 Reconcile 因计划／结果元数据超出普通单次上限而失败的问题。只读历史增加 12,448 字节有界元数据空间，正文上限和原累计预算不变，低预算小记录路径保留，不能借用写权限。最终 Windows Release 三语言文件效果故障／正常对照及 Rust 实际恢复 Widget **52/52** 通过；运行时边界 **6/6**、相关回归 **40 项**、Release 宿主 **45/45** 通过。另修复独立 package-management 的编译条件遗漏。下一步为 16 MiB 四个内容事务提交点的恢复及独立回执验证（计划 20 项）、完整应用／选择器与人工审批验收、其他平台。Debug 最大正文期限限制仍存在，SDK 未冻结；未提交／推送／发布。

2026-09-27 [非空 Guest 故障到恢复界面](../reports/guest-mutation-crash-ui-2026-09-27.md)：三语言 Create／Delete 的真实进程故障、新普通宿主只读核对及 Rust 实际恢复按钮流程 **32/32** 通过，包含普通宿主故障开关无效对照。恢复页新增工作流快捷选项，仅预填历史范围；页面回归 **13/13**。Unknown 不推断成功，重复核对不重放文件效果。当前覆盖 184,393 字节四块 Create；最大 16 MiB 与部分写入／flush／publish 故障、完整应用／系统选择器人工验收及其他平台仍开放。SDK 未冻结，未提交／推送／发布。

2026-09-27 [扩展预算插件只读恢复](../reports/guest-mutation-history-binding-2026-09-27.md)：新增独立历史绑定，以普通读取额度核对当前获准包的原记录，不批准扩展写预算。运行时拒绝 guest、通用 Broker、目标资源与非历史 owner 命令；产品入口在真实退出／ACK 后交接只读恢复页。三语言非空 Create 后正常重启已验证原计划及结果逐字节一致、撤权拒绝与外部目标不被重放覆盖。下一步为非空 guest 真实崩溃到产品恢复界面的整链、16 MiB 故障组合及完整应用／选择器验收。最终测试范围和日志见报告；SDK 未冻结，未提交／推送／发布。

2026-09-27 [Guest 独立会话与实际审批界面](../reports/guest-mutation-execution-ui-2026-09-27.md)：产品 IO 设置已接实际预算、原计划审阅、独立 Prepare／Execute 确认和页面外会话；撤权使旧确认失效，丢回执不自动续传或重放。Windows Release Rust／C／C++ 会话 3/3、Rust 真实 Widget 1/1、会话／客户端组合 58/58、受控新界面 4/4、旧界面及设置 45/45 通过。九语言同步。下一步为预算扩展包跨重启只读恢复、非空 guest 故障恢复界面和完整应用／系统选择器验收；SDK 未冻结，未提交／推送／发布。下文为历史阶段记录，其当时限制以后续报告为准。

2026-09-27 [Guest 私有协议与 Dart 客户端](../reports/guest-mutation-private-wire-2026-09-27.md)：独立 guest Start／Submit／Status／Read／CancelCommand、实际批准预算、原始 Core 回帧及类型化回执已接入，保持与原生直接执行路径分离。Windows Release 三 SDK private-wire 3/3、宿主及目录／IO 199/199、runtime 常规 40/40 与 SDK 普通／16 MiB 6/6 通过。真实失败响应与 Dart 关联验证见专项报告。产品 guest 独立会话／审批 UI、预算扩展包跨重启只读恢复及其他平台仍开放；SDK 未冻结，未提交／推送／发布。

2026-09-27 [Workbench guest 编排与目录声明](../reports/workbench-mutation-guest-2026-09-27.md)：独立宿主 guest 入口接入显式预算、精确计划审批与第二次 Execute 确认，真实 Wasm 使用原 owner，旧 native 入口不能处理 guest TaskKey。修复内部回执竞争、取消／释放及历史不确定状态投影；目录九语言显示声明上限而不授予权限。Windows Release 三语言 SDK **3/3**，宿主及目录／IO 回归 **196/196**，Dart／Flutter **107/107**。完整回归首次因缺少三类实际 Wasm 路径而失败，重新编译并配置夹具后通过，原失败记录保留。新 guest 私有协议、Flutter 审批／执行入口、扩展预算包跨重启只读恢复与其他平台仍待接入；SDK 未冻结，未提交／推送／发布。

2026-09-27 [非空多块 guest 回执与崩溃恢复](../reports/mutation-guest-recovery-2026-09-27.md)：Rust／C／C++ 实际 SDK 的 Stage／Commit／Execute 丢回执 3/3、内容事务与 native Create 的独立进程退出案例 36/36、普通构建故障开关无效控制 3/3 通过。重开原 Store 显式核对审计回执、正文／文件摘要和 Core claim 防重放；原 owner／opt-in／核对回归 40/40。新增可重复资格脚本及部分写入测试边界，没有改变生产恢复语义。该门槛仅覆盖非空四块和显式进程退出；产品审批／恢复界面、完整 16 MiB 故障组合、其他平台与条件 Replace 仍开放。SDK 未冻结，未提交／推送／发布。

2026-09-27 [显式 mutation 预算与最大内容资格](../reports/mutation-budget-2026-09-27.md)：新增 `mutation-budget-v1`，精确实例显式批准、两层累计计费和签发前余量预检，旧 16／64 MiB IO 限额不变。Windows Release 三语言普通／16 MiB 实际流程 **6/6**，最大内容约 1.87–2.50 秒；原 owner／opt-in／核对 **39/39**，旧 SDK 原件运行 12 项、HTTP／服务 6 项及宿主 30 项回归通过。Debug Rust 最大内容另行复现 Deadline，不能泛化为所有构建模式均合格。SDK 未冻结；下一步补非空故障／跨进程恢复矩阵，再接入产品侧预算与两阶段审批。本轮未提交、推送或发布。以下为此前阶段记录，其当时限制以本条和专项报告中的后续证据为准。

2026-09-27 [三语言 mutation Wasm 与原 owner 接入](../reports/mutation-guest-wasm-2026-09-27.md)：独立包协商／import、Stage 与 Execute 模式、单请求完成绑定、有界语义去重、原期限与真实结果核对已接入。Rust／C／C++ 实际临时目录创建／删除 3/3、原 owner／opt-in／核对 37/37 通过；旧任务／UI／转换 9 项、依赖 3 项、HTTP／服务 6 项原件运行及宿主变更 30 项回归通过。64 MiB 累计硬预算仍不能覆盖协议最大 16 MiB 内容的完整流程；非空故障矩阵、产品审批整合及平台资格继续开放，SDK 未冻结，未提交或推送。

2026-09-27 [原 owner 审批凭证与一次性执行许可](../reports/mutation-guest-approval-2026-09-27.md)：明确授权后接入精确计划审批、已领取回执检查、Prepared／持久内容校验及旧 Execute 的 opt-in 单向许可；丢回执不降级，Release 不复用引用。Windows 临时目录 runtime 33/33、宿主 mutation 30/30；严格 Clippy 仍有两类旧警告，限定例外通过。每 worker 生命周期签发上限 128。尚无 guest import，下一步接协商／显式 job／原 owner 分派及三语言 Wasm；SDK 未冻结，未提交或推送。

2026-09-27 [文件变更 C／C++ SDK 编解码接口](../reports/mutation-ffi-2026-09-27.md)：补齐版本化 C ABI、C++17 自有数据／只可移动结果封装及可重复原生验证脚本。SDK 全量 76/76、严格 Clippy、Windows C／C++ 两项实际程序（21 个同源向量与八动作）、Rust SDK wasm32 编译检查通过；契约／枚举／向量与锁定工具 22 项通过。该阶段没有 guest import 或文件执行能力，下一步为原 owner 分派与可信审批／执行许可；SDK 未冻结，未提交或推送。

2026-09-27 [独立文件变更 guest 契约与 Rust 编解码](../reports/mutation-guest-codec-2026-09-27.md)：新增独立 mutation Cap’n Proto 草案、Core／Rust SDK 有界编解码及 21 个共享 CLI 正反向量；旧 IO v1 保持不变。Core 新增＋旧 IO 测试 30、SDK 全量 71、工具与锁定 20 项通过，SDK 严格 Clippy 通过。修复 Unicode 校验与 Unknown 响应的两端差异。C／C++ 接口、独立 import、原 owner 分派及三语言真实 Wasm 仍待实施；本轮仅编解码，不启用插件文件效果，SDK 未冻结，未提交或推送。

2026-09-27 [文件创建／删除界面与独立执行会话](../reports/mutation-execution-ui-2026-09-27.md)：Windows IO 设置已接选择、范围预览、准备、独立执行确认及退出／恢复交接；九语言补齐。修复停止与迟到回执门控、ACK 身份校验，以及真实宿主保留历史提交导致后续操作禁用的问题。执行会话 19/19、Flutter 组合 43/43（含非空三块内容的真实原生与真实控件链路）、本地化 4＋7 通过，严格分析无诊断。系统选择器由测试注入路径；未做完整应用构建或人工验收。下一步为版本化 guest 扩展、三语言 SDK／Wasm 和非空中间故障点；SDK 未冻结，未提交或推送。

2026-09-27 [真实故障后的恢复界面联调](../reports/mutation-crash-widget-2026-09-27.md)：真实时钟 Flutter 控件＋Windows 宿主完成 Create／Delete 各三阶段故障及普通宿主对照，7/7 通过；实际按钮完成发现、选择、离页返回、退出确认及独立核对，Unknown／Observed 文案分离，最终等 ACK 与宿主 close。既有面板及竞态回归 12/12，严格分析无诊断。新增三语言文件变更 SDK 接入计划；正式变更编辑／审批、guest SDK、非空内容中间故障点及其他平台继续开放，SDK 未冻结，未提交或推送。

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

更新：2026-09-27。应用源码仍为 `0.1.9-test.56+60`。本轮将 Drive 检查点 `807b9cd` 与本地 Windows/主题检查点 `008db00` 组合验收；未创建标签、Release 或远端推送。已发布安装包不包含本轮开发增量。

## Windows 合并候选验收

文件任务面板已接入九语外置 ARB 和生成语言包，新增选择取消/切换语言保留状态测试。Windows C/C++ 原生协议测试、Dart→真实宿主捕获/分块摘要/退出回收/撤权拒绝已补验，静态检查提示已清理。最终构建、分项结果与保留限制见 [验收记录](../reports/drive-merge-acceptance-2026-09-27.md)。这些结果补充下列历史报告，不代表完整 SDK 冻结；系统原生选择对话框的人工操作、跨平台与真实 DPAPI/TLS 生命周期仍须单独验收。

## 2026-09-26 SDK 库源码锁定

新增可选 `sdk.lock.toml`、显式 lock-sdk 创建／更新及严格要求锁定的参数。锁以相对路径记录 SDK 库源码、契约、Cargo 声明／锁和构建脚本；预检、构建前后检测新增／删除／修改和锁身份变化。118 项 Python 工具测试、21 模板真实锁定／严格预检通过，冻结原件不变。范围不含完整工具链或传递依赖源码，不是 SDK 独立发行或可重现构建资格；见 [源码锁报告](../reports/plugin-sdk-source-lock-2026-09-26.md)。

## 2026-09-26 SDK 工程离线预检

新增 `morrow_plugin.py validate`，无需编译器即可核对工程声明、SDK 契约及 Rust 库／路径／feature 绑定，输出 TOML 声明摘要。正式构建复用检查，出站服务补齐 IO 契约与版本检查。新增 21 模板批量预检入口；本轮工具回归与真实 CLI 范围见 [预检报告](../reports/plugin-project-preflight-2026-09-26.md)。该功能不构建、不执行或授权插件，不替代原包与平台验收，SDK 独立分发仍开放。

## 2026-09-26 文件会话、增量校验与选择器接线

原生 IO 设置页已接入系统选择器和文件任务控制；共享文件会话保留页面重开进度，按精确 offset、长度、EOF 与 SHA-256 校验，Finished 后仍独立等待实际退出／维护。丢回执不重放，取消隔离迟到字节，HTTP 面板避免领取文件结果。

本轮独立 Dart 28 项通过（会话 21、客户端 7），两测试目标分析与 17 个冻结原件检查通过。客户端使用保存的真实 Rust 帧，没有本轮 Rust 重跑。Flutter 页面仅源码接线与解析检查，完整类型／widget／Windows 窗口及九语言文案仍待验收；详见 [会话报告](../reports/plugin-file-session-2026-09-26.md)。当前仍为本地稳定候选。以下分轮记录保留当时范围。

## 2026-09-26 文件私有协议与 Dart 客户端

追加文件启动／块请求／结束／一次领取私有调度消息和类型化 Dart 客户端，绑定提交身份、精确包／修订／handler 和有界输入；可信选中路径在原 worker 才打开，停止／回收仍复用原所有权流程。真实跨层测试发现并修复二进制摘要误用 schema 文本规范化，以及二进制 secret 派生引用的同类问题。

runtime 常规 415（6 ignored）、Workbench lib 64、显式 IO SDK 5、Dart 7、严格 runtime Clippy、绑定生成检查和 17 个 transport 固定原件验证通过；分项重叠。Dart 解析三语言原 guest 的真实 Rust 帧并独立核对内容和 SHA-256，详见 [协议报告](../reports/plugin-file-wire-2026-09-26.md)。

系统选择器、Flutter 文件任务页面、Windows 及 DPAPI/TLS 实际产品资格继续开放。本轮 Flutter 启动因间接尝试云实例元数据访问被自动审批拒绝；停止该路径，独立 Dart 已验证，不算 Flutter 验收。Linux 工作台生产限制和 RecoveryRequired 语义保持；下一步见 [文件任务合同](PLUGIN_FILE_TASKS.md)。

## 2026-09-26 文件后台任务与原 owner 接线

文件捕获、guest 分块读取和结束进入既有有界 owner 队列；资源留在执行线程，取消与实际 join 继续约束归还。Workbench Rust 新增文件启动／块请求／一次领取／结束入口，复用 TaskKey 和修复／确认。后台共享时钟的取时与授权检查已串行，宿主 ceiling 和实例真实预算同时计费。runtime 412、Workbench lib 60、network 134、IO 三语言 5、服务原包 6 与严格 Clippy 通过，分项重叠；详见 [本轮报告](../reports/plugin-file-owner-2026-09-26.md)。

这仍不包含私有文件协议、系统选择器、Dart/Flutter 或 Windows 实机。Linux Workbench 用测试构造验证接口，生产打开保持不支持，维护失败仍进入 RecoveryRequired。接口及下一步见 [文件任务合同](PLUGIN_FILE_TASKS.md)。

## 2026-09-26 选中文件句柄捕获

FileBroker 新增可信宿主持有的普通文件句柄入口：读取前预扣原实例共享额度，固定实际字节与 SHA-256，分块及最终检查撤权／期限。路径更换不重新打开对象；失败不发布引用，释放并发槽但不退累计费用。最终文件专项 23、三语言既有模块专项 5、runtime 常规 403、network 常规 134 及严格 Clippy 通过；计数范围见 [本轮报告](../reports/plugin-selected-file-2026-09-26.md)。

这不是原子文件快照或持久证据。工作台文件选择／任务页面、Windows 实机及完整文件系统仍待实现和验收；接入约束见 [单文件合同](PLUGIN_SELECTED_FILE.md)。


## 2026-09-26 配置服务接线与可恢复验收

工作台服务出站已复用 `SelectedService` / `PreparedService`，将原 Store 的所选端点、凭据实时依赖、重放 scope、HTTP router 和 guest 目录统一生成，并在 attach 时拒绝外来 worker。上一轮 Rust／C／C++ 原包经持久配置服务验证：已选资源撤权阻断正常/缓存/历史交付、未选变更不中断服务、等待期本地写入与停止/撤权、同库重开不重发及原 worker 归还。新增无 guest 重编的原包复验模式，检查完整三语言清单及摘要。

网络 134 项、三语言专项 6 项、SDK 66 项、工具 97 项、原生及旧原件回归通过；工作台宿主 Linux 编译通过，Windows 条件代码与实际产品入口仍待 Windows 环境验证。恢复来源与详细限制见 [配置服务报告](../reports/plugin-configured-service-2026-09-26.md)。

## 2026-09-26 三语言服务出站闭环

新增 `--kind service --service-http`，Rust／C／C++ 原包只依赖公共 SDK，通过宿主资源目录把 POST 正文发往一个批准端点。补齐 C/C++ 精确原服务帧摘要接口与显式资源槽预算，模板声明四个槽，原来的默认两槽保持。三项真实 TCP 专项覆盖三语言共 21 场景：敏感头隔离、宿主凭据、原 owner 等待期间写入、停止／撤权、Observed／Unknown 同库重开且不重发；SDK 66、工具 94、打包 CLI 15、原生接口和旧冻结原包回归通过。已保存 Google Drive 工作检查点。具体证据和首次失败修复见 [服务出站报告](../reports/plugin-service-http-sdk-2026-09-26.md)。

本轮仍是 Linux 本地稳定候选；没有新增 Windows／Flutter／实际 DPAPI 或 TLS 验收，也没有将未知外部效果视为已核对。

## 2026-09-26 底座收尾增量

显式有限长时 profile、三语言资源目录解析、原包续租／累计额度／撤权／重开核对已完成。新增 `transport-v1-rc1` 六对 IO／服务原件和只读回归入口，不修改旧候选。core 645、runtime 397、network 134、SDK 66、Python 92 通过；C/C++ 原生、六原包真实网络和旧内容/UI 原件另验，结果有重叠。

当前为 **Linux 本地稳定候选**。Windows 专属依赖、Flutter 服务入口和凭据／TLS 生命周期没有本轮证据，不能标记整体底座或全平台 SDK 已稳定收尾。结果、失败修复与明确门槛见 [收尾记录](../reports/plugin-foundation-closeout-2026-09-26.md)。以下早先记录保留其当时范围。

## 2026-09-26 入站服务增量

三语言请求解码、原始帧摘要关联响应、一次读取／完成、C 自有句柄和 C++ RAII 已接入；新增 `--kind service`、显式 `--service` 打包与服务 schema pin。生成原包以原声明预算在 Linux 真实 TCP 节点验证七种方法、二进制正文及授权/认证/撤权边界。有限模板不声明 `service-run-v1`，不代表工作台长时服务产品闭环。

本轮 SDK 60 项、Python 工具 32 项与契约/原件 16 项、核心打包 11 项通过；独立 host/SDK codec 2 项与覆盖 C/C++ 的原生用例通过，真实节点一项覆盖三语言及七方法。九项旧内容/转换/UI 原件实跑；Windows 专属依赖测试未运行。分项不相加为产品通过率。完整证据、首次失败与验证限制见 [服务 SDK 报告](../reports/plugin-service-sdk-2026-09-26.md)。

## 当前产品与平台增量

- Windows test.56 包含右键操作、长按排序、tips 编辑、瀑布流布局与退出流程修复，并支持独立主题插件。发布时验证范围见 [test.56 报告](../reports/0.1.9-test.56-release.md)。
- Web 正式入口已经运行设备侧 Rust/Wasm 工作台，内容、身份和附件在本地持久化；导入的背景和音乐可以恢复。OPFS 包登记及共享运行器已接通，不再将这些能力整体列为“尚未接入”。
- Web 对外导入暂限符合 `theme.describe` 的无权限、无 IO/服务、无依赖主题包；导入、启停、重开恢复及卸载已有 Chrome/Edge 线上验收。此范围不包含通用外部业务插件和网络/文件系统平台能力。
- Pages 最近有证据的成功部署为 `52f7345`。test.56 对应的 CI 36115685547 在孤立数据场景的导航初始化报 `net::ERR_ABORTED`，后续媒体/主题场景和部署跳过；`d9c0431` 为 `[skip ci]` 提交，不视为最新主线门禁通过。详见 [Web 对齐表](WEB_PARITY.md)。

## 已有能力与 2026-09-24 增量（历史验证）

基础包管理、权限／实例绑定、内容任务、基础声明式 UI、依赖调用和 guest-v1-rc1 旧二进制兼容候选已建立。宿主具备受管 HTTP/TLS 出站和有限 API 服务节点能力，但这不等于完整 SDK 已冻结。

本次接入 C／C++／Rust 实验 IO codec、单次 Wasm 调用、精确原请求关联及原响应完成；新增 `--kind io` 项目模板、打包与检查时的 IO 声明／预算输出。HTTP 模板声明工作台要求的 `morrow.http.forward.v1`，打包不授予能力，端点和凭据仍由宿主管理。

三个生成的 HTTP 原包已在 Windows 上经真实受管 worker 与本地 TCP 服务执行，未修补包内容或扩大其预算。覆盖七种方法、二进制正文、重复参数与头、宿主凭据注入、429、发送前拒绝、断线 Unknown 不重发及数据库重开保留。该证据不包含公网服务商互操作或新增 Flutter 窗口验收。

| 验证 | 本轮证据 |
| --- | --- |
| SDK | 53 项 Rust 测试，严格 Clippy，C／C++ 原生调用和核心适配器验证 |
| IO 与旧插件 | 五项覆盖三语言的 IO 专项，12 项旧原件运行时检查，36 个固定文件／13 对旧 Wasm 和包保持 |
| 真实 HTTP | 29 项既有回归、四项覆盖三语言的专项；生成原包模式再次通过 |
| 开发工具 | 31 项 Python、九项打包 CLI 测试；三种 HTTP 模板实际构建、打包、检查及运行 |

完整记录见 [IO SDK](../reports/plugin-io-sdk-2026-09-24.md) 和 [真实网络与项目原包](../reports/plugin-io-network-sdk-2026-09-24.md)。没有将这些分项测试相加作为全产品通过率；本轮未重跑全部 15 种模板流程，也未重建应用安装包。

## 后续工作

| 工作包 | 尚需完成 |
| --- | --- |
| 入站服务 SDK | 三语言接口、长时 profile、资源发现、入站→出站公共 SDK 原包及 Observed/Unknown 历史重开已验；Windows/Flutter 产品入口、TLS 与完整业务核对待验 |
| 完整文件系统 | 原句柄捕获、后台任务、私有协议、独立 Dart 会话校验和选择器／页面源码接线已完成；本地九语 widget 与 Windows 真实文件任务已验；原生选择器人工操作、目录、创建／替换／删除、分块写入及各平台授权资源仍开放 |
| 完整网络 | OAuth／多账号、上传下载、SSE／WebSocket、服务适配与适用的专用 profile |
| 长任务与恢复 | 异步续接、完整跨重启核对、端到端隔离重放；持久 Unknown 记录不等于业务已核对 |
| 内容与 UI | 完整内容接口、复杂编辑器、插件语言、会话恢复和依赖管理体验 |
| SDK 稳定化 | IO／服务 transport-v1-rc1 候选、离线工程预检及可选 SDK 库源码锁已建立；跨宿主持续回归、版本承诺、独立开发者接入和独立分发仍开放 |
| 全平台 | 各平台执行、存储、凭据、生命周期的独立能力矩阵与实际验收 |

后续优先完成 Windows/Flutter 实际入口验收，同时推进完整文件 IO、异步与恢复，再扩展认证及流式网络。IO／服务使用独立 transport-v1-rc1 候选，完整 SDK 尚未冻结。正式 UI autosave 等产品开放项继续由 [开发看板](DEVELOPMENT_BOARD.md) 跟踪。
