# 文件变更 guest SDK 接入计划

2026-09-27，基于当前 `codex/io-safety-refactor` 工作树核验。本文记录实验接入状态和后续门槛，SDK 尚未冻结。用户范围仍包含 C、C++、Rust；不增加 TS／JS guest 路线。

## 当前差距

### 已完成：最大正文的内容事务退出

文件效果阶段之外，单独补齐 16 MiB 正文在 `file-content-after-bytes`、`file-content-after-receipt`、`file-content-before-commit`、`file-content-after-commit` 四点退出的产品链路。故障发生在 Prepare 的 Commit 内，不获得或调用 Execute 许可。

- 三语言原生会话各四个故障点，加各一个普通宿主故障开关无效对照；Rust 实际恢复 Widget 同样覆盖四点及一个对照，完整矩阵 **20/20 通过**。
- 新宿主显式发现原计划并重复核对：原操作保持 Prepared、效果未指定、没有结果，目标始终不存在；不能显示成功、恢复旧许可或自动续做。
- 前三点的正文与回执一起回滚，after-commit 的完整正文与 LiveStaging 回执均经独立只读 Store 核验。现有恢复 DTO 的 Prepared 阶段无法证明这一区别，新增测试专用校验器读取原计划、正文与回执，不增加生产或 Guest 权限。
- 普通宿主对照只完成 Prepare，保持等待独立执行确认。保留原期限、包／权限检查、退出证明及临时库清理保护。

上述门槛已按同一源码和固定产物完成，见 [内容事务专项报告](../reports/guest-mutation-content-crash-2026-09-27.md)。主代理独立核对全部用例、32 项源码摘要与产物摘要；它与前一轮 52 项文件效果资格分别保留，不拼接计数。

### 下一验收面：完整应用与平台边界

1. **已完成 Windows 完整本地预览**：最终目录原生集成 6/6、真实 Release 自检 4 项 PASS、501 项摘要和两个 ZIP 通过，见 [完整构建报告](../reports/windows-mutation-preview-2026-09-27.md)。此前 20/52 项故障矩阵仍绑定旧产物，尚未对新包重新执行。
2. **审批到期自动化修复完成**：本地单调截止、到期禁重试、确认后重新观察原任务、精确关闭旧弹窗、Stop／实际退出／Repair／ACK，以及原生真实 30 秒后的独立核对，共 31/31 通过，见 [期限报告](../reports/guest-mutation-expiry-2026-09-27.md)。系统文件／目录选择器和实际人工审批仍待验收；修复尚未进入上一轮不可变 Windows ZIP，后续需新应用产物。不扩大授权期限。
3. 按平台证明保护密钥、文件效果、历史恢复能力；不将 Windows 进程退出扩大为断电或全平台资格。

此前 [16 MiB 文件效果故障与恢复资格](../reports/guest-mutation-max-crash-2026-09-27.md) 为 **52/52**，并已修复独立历史读取的有界元数据预算缺口。内容事务门槛本轮补齐，但 Debug 最大正文仍有期限限制，条件 Replace 继续拒绝无保证后端，SDK 未冻结。以下日期相同的段落按历史阶段保留，其未完成项以上述最新状态为准。

2026-09-27 更新：独立 guest 会话与产品审批面板已完成本轮自动化验收，见 [Guest 界面报告](../reports/guest-mutation-execution-ui-2026-09-27.md)。实际预算、原计划摘要、两次确认、撤权及丢回执门控已接入，Rust／C／C++ 实际会话与 Rust 实际 Widget 均通过。当前主要缺口调整为扩展预算包跨重启只读恢复、非空 guest 故障恢复界面、最大内容故障组合、完整应用与系统选择器验收和其他平台资格。授权仍最长 30 秒，本轮未证明人类审批的时间可用性；SDK 尚未冻结。

原生 owner 已有选择、规范计划构造、Prepare、连续分块、内容提交、Execute、Query、计划取消、Release 及独立只读发现／核对。应用私有进程协议和 Dart 类型化客户端已经接通，Windows 恢复界面已通过真实故障后的七项按钮联调，见 [验收报告](../reports/mutation-crash-widget-2026-09-27.md)；可信 Create／Delete 选择、准备、二次执行确认和恢复交接也已接入，见 [界面验收](../reports/mutation-execution-ui-2026-09-27.md)。真实宿主联调已通过，系统选择器仍待人工验收。

guest 采用与旧 IO v1 分离的接入路径：

| 边界 | 当前事实 | 接入位置 |
| --- | --- | --- |
| guest IO 解码 | FileCreate／Replace／Delete 仍返回 Unsupported | `core/src/io.rs` |
| Wasm 分派 | 旧 IO 入口保持原边界；独立 `mutation-v1` job 在暂停点调用原 owner | `plugin_runtime/src/io_jobs.rs` |
| Rust SDK | 独立 mutation 编解码与 Wasm helper；不会自动重试效果 | `sdk/rust/src/io.rs`、`sdk/rust/src/mutation.rs` |
| C／C++ SDK | 独立 mutation 描述符／类型化工厂、结果视图及 Wasm helper | `sdk/c/include/morrow_plugin_mutation.h`、`sdk/cpp/include/morrow_plugin_mutation.hpp` |
| 原生执行 | 原 owner 队列和 TargetBroker 已实现 Create／Delete；Replace 明确拒绝不具备条件保证的后端 | `plugin_runtime/src/io_jobs/mutation_commands.rs`、`plugin_runtime/src/file_target/` |

不能把已有 schema 中预留的 fileCreate 字段，或 Dart MutationTaskBackend，当成 guest SDK 已实现。

## 接入原则与版本策略

采用独立版本化 mutation 扩展契约和固定 Wasm import，保留现有 IO v1 原件。独立 `mutation.capnp` 实验草案及 Core／Rust SDK 编解码已经落地，见 [扩展说明](PLUGIN_MUTATION_GUEST.md) 与 [验证报告](../reports/mutation-guest-codec-2026-09-27.md)。`mutation-v1` 协商精确 schema 摘要、guest ABI 2 和 IO 能力，固定 import 为 `morrow_mutation_v1.call`；本草案未冻结，不修改既有冻结候选和摘要。若后续转为 IO v2，必须显式双版本分派和旧原件验证，不能原地刷新 IO v1 的 pin。

`transport-v1-rc1` 已保留三语言 HTTP／服务原 Wasm 和包。新增 guest 实现需要继续运行这些旧原件，不能重编后宣称兼容。

目标选择、主体、包摘要、当前批准、审批摘要、预期文件身份（如适用；Create 没有既存目标身份）及平台句柄由可信宿主产生并绑定原 owner。guest 只获得当前实例／代次／租约范围内的有界不透明引用；不能提交任意本机路径、主体、审批摘要或 RequestRecord 来签发权限。新进程不复活旧选择句柄。

## 实施顺序与完成门槛

1. **可信选择与变更编辑流程（Windows 自动化链路已完成）**：用户显式选择目标、查看操作和内容范围、批准；原 owner 持有选择租约，界面只持有草稿与任务身份。明确区分取消未派发计划、停止 worker、实际退出、修复和确认。保存失败必须保留原错误与待核对状态。SDK 原型可先由可信测试宿主选择目标并签发受控引用；Create／Delete UI、取消／撤权和真实非空执行链路已覆盖；正式产品交付仍须补系统选择器人工验收与完整应用构建，不能把测试注入路径当作选择器验收。
2. **扩展契约与三语言一致向量（三语言编解码及 C／C++ 原生验证已完成）**：规范操作 ID、长度／哈希、命令身份、结果阶段、预算与错误；旧 schema 不变。guest 提供内容与操作意图，由 owner 构造规范计划；Prepare／Commit／Execute 分别显式发生。Core、Rust SDK 与原生 C／C++ 程序已通过 21 个同源正反向量，见 [C／C++ 验证](../reports/mutation-ffi-2026-09-27.md)。该报告只证明编解码；独立 import 与真实 Wasm 的后续验证应单独记录，不能混用两阶段证据。
3. **原 owner 内 guest 分派（已接入）**：复用当前任务队列、授权前后检查、字节账本、预留管理容量和取消信号。不得从正在执行的 owner 同步回调并等待它自己的队列，不能另开 Store 或复制 Manager 绕过所有权；需要在可暂停 guest 边界直接调用原 State，或对现有调度增加可证明不自锁的命令切换。
4. **Create／Delete 完整闭环**：构造计划、Prepare、Create 连续分块与 Commit、Execute、历史 Query、CancelPlan、Release、实际 join／ack；同一提交令牌只重取原身份，领取丢失保留不确定性，Unknown 不重发 Execute；原 worker 存活时显式查询原历史并按阶段清理，退出后重新批准并独立只读核对。Create 非空内容及写入／刷新／发布故障点必须补验，不能只凭零长度恢复矩阵关闭。
5. **三语言真实 Wasm 与平台资格**：Rust／C／C++ 真正经过 Wasm import 执行临时目标操作；旧 transport 原件不重编通过。验收报告分开标明本机 Windows、Linux、其他原生平台与 Web 的实际能力，不作全平台推断。冻结门槛仍按 SDK 兼容文档执行。

## 必须覆盖的反例

- 没有批准、撤权、包摘要变化、旧代次、foreign token、其他 worker／Store、超时与时钟异常。
- 提交身份相同但参数不同；发送或领取回执丢失；取消与 claim 竞争；停止后没有真实退出时不得重开。
- 请求解析前后的大小约束、连续 offset、超额内容、错误哈希、重复 Commit、预算及队列饱和。具体上限从当前共享定义引用并验证，不能在三语言各自维护一套互相漂移的数字。
- after-claim／after-effect／after-observe 真实退出后的 Unknown／Observed 分离，不自动重新 Execute；恢复授权使用当前批准。
- Windows Replace 在 claim 和外部效果前明确 UnsupportedConditionalReplacement；不得通过先检查文件 ID 再重命名的竞态实现伪条件替换。

恢复 checkpoint 只限当前宿主有界缓存，不能作为 guest 授权或跨进程续执行令牌。只读发现与核对可单独扩展，但必须保留原计划字节、当前审批和有界分页，不能绕过现有审计关系。

## 原 owner 接入复核（2026-09-27）

运行时已经在每次宿主 import 处保留 Wasm continuation，`io_jobs::run_job` 可以在恢复 guest 前借用原 owner 与 `files.mutations`。扩展采用一次暂停调用对应一个同步结果，不新增 guest 轮询队列。不得在该调用中使用 `IoWorker::mutation_command` 后等待自己的 owner 队列；队列仅在 job 间处理，可能自锁。

当前 `run_job<O: HostOwner>` 与变更分派所需的 `ManagedHostOwner`／Ticket 约束不同。接入时必须加入明确 opt-in 的原 owner 钩子或提取共用受检分派，普通 raw owner 保持拒绝；不能为凑类型而另开 Manager／Store 或伪造 Ticket。逐次使用 `IoJobLease::charge` 对 FileCreate／FileDelete 能力、请求及响应字节收费，并保留前后撤权／取消／期限检查。新 import 与旧 IO v1 分离。

审批不能仅绑定路径引用。可信宿主必须保存完整不可变审批记录：选择租约、原 MutationSession、包／实例／worker／代次、操作类型、操作 ID、内容长度与 SHA-256，以及审批摘要。Prepare 时逐项核对 guest 输入后再构造规范 RequestRecord；`SelectionScope` 中只有一个不透明审批摘要，不足以自行证明 guest 内容已被批准。Delete 同样绑定操作 ID 与目标，guest 不提供路径、主体、审批摘要或预期文件身份。

执行许可与选择租约分开。宿主执行许可绑定已准备 RequestRecord 的精确摘要；guest 紧接 Prepare 发 Execute 时，如无该许可，必须明确拒绝且不产生效果。产品接入可采用在启动 guest job 前明确批准固定操作与内容，或让 continuation 进入专门的待批准状态并从可信 UI 恢复；不允许在暂停 guest 的原 owner 上等待同队列中的审批命令。当前应用的两次确认不能被新增 SDK 隐式绕过。

Release 只清理选择资源，不清除持久历史，也不是 worker 已退出／ACK 的证明。Unknown 只查原历史，重新进程启动不能继承旧租约；宿主重复提交账本必须将令牌与完整参数绑定，不能以 callId 相同推断同一效果。

分派还必须要求显式启用 mutation 的 job／lease。`instance.is_some()` 不等于文件变更权限，ReadOnly router、只读历史及服务持久化只读 job 继续拒绝新动作。请求与最坏响应预算应在持久化或 OS 效果前预留，chunk 除内容外仍计入帧开销；16 MiB 内容上限不是承诺一个同为 16 MiB 的 job 可传满该长度。30 秒是授权／请求期限，不代表同步系统调用一定能在该时刻中断。Store 与 OS 调用期间不得持有控制锁。

## 审批凭证与一次性执行许可（原 owner 已实现）

用户已于 2026-09-27 明确授权受控创建／删除审批及临时目录测试。原 owner 已实现可信审批凭证、独立执行许可与旧 Execute 入口的 opt-in 强制检查；此前撤回的脚手架已按授权重新实现。双模式 guest job 和 import 现已接入，审批前后的独立提交仍是强制边界。原生授权验证范围见 [授权检查报告](../reports/mutation-guest-approval-2026-09-27.md)。

采用独立的准备与执行阶段：每个 Stage job 接收一条宿主绑定的请求并返回，依次完成准备、连续分块与提交；可信宿主随后在原 owner 队列上审核并签发执行许可，再提交 Execute job。每个 job 只允许一次 import，完成帧必须与该调用的原响应相同。不得让 Stage job 停在 import 中等待同一队列处理审批。只读、普通 raw owner 和未显式启用的 job 继续拒绝。

原 owner 应持有私有字段的 guest 凭证，绑定原 worker／session、不可变规范计划及其摘要。可信调用方不能仅提供路径或摘要替代完整计划核对。签发必须建立在原选择回执已经领取的事实之上；引用不能全零，数量有界，并且同一 worker 生命周期内不能因 Release 而重新使用旧引用。

执行许可单独绑定精确计划摘要，只能在准备历史成立后签发；Create 还要求持久内容长度及摘要匹配。授权回执未领取、丢失或取消，不得视为已获执行许可。资源一旦加入 guest 审批约束，现有 Execute 入口也必须执行同样检查，避免从旧入口绕过新约束。未加入这一约束的既有可信 UI 流程保持原有行为。

完成纯检查后、派发效果前单向消费许可；不自动重签，不因丢失响应而重放。Query、派发前 CancelPlan 和 Release 仍按现有状态与授权处理。失败不得伪装为成功或删除持久历史，Unknown 必须走显式查询与恢复。

| 必须验收 | 预期 |
| --- | --- |
| 无选择回执、跨 worker／session、引用重复或超出数量界限 | 拒绝；无持久化及外部效果 |
| guest 提交的操作、内容长度／摘要或规范计划与批准内容不同 | 拒绝；不能用后续 Prepare 替换已批准计划 |
| 尚未准备、Create 内容未持久提交或哈希不匹配 | 不签发执行许可 |
| 无许可、许可回执未领取、已取消或摘要不匹配 | Execute 拒绝；临时目标保持原状 |
| 许可已消费后再次 Execute，或执行结果未领取 | 不重复派发；保留原操作身份和可查询历史 |
| 撤权、包／实例／代次变化、停止／期限失效 | 原 owner 重新校验并拒绝；不凭缓存批准继续 |
| 正确领取许可后的 Create／Delete | 仅在临时目录验证实际效果、原历史与单向消费 |
| 既有 native UI 路径与 host 响应编码 | 原回归通过；新内部审批结果不得伪装成旧协议成功响应 |

当前阶段不增加 Replace，不开放任意路径，不修改旧 IO v1；原生审批验收不能替代真实 Wasm、非空内容故障恢复及平台资格验证；各项证据分别记录。

引用登记最多 128 个，按整个 worker 生命周期计数，Release 不返还额度；达到上限需结束原 worker 并按当前权限重新建立任务，不复用旧租约。该上限是显式的可用性限制，不代表仅允许 128 个并发资源。

三语言真实 Wasm 与旧 SDK 原件的本轮验证见 [接入报告](../reports/mutation-guest-wasm-2026-09-27.md)。

## 尚待完成的交付门槛

- 独立 guest 私有协议及 Dart 类型化客户端已完成，见 [私有协议报告](../reports/guest-mutation-private-wire-2026-09-27.md)。五个动作走外层调度器，返回原始已验证 Core 回帧，读取携带原命令回执；不由 status 重建许可。下一步接独立 guest 会话和 UI，再验证 Flutter 至真实 guest／原 owner 全链路。

- Workbench Rust guest 编排与目录声明的前阶段证据见 [Workbench 接入报告](../reports/workbench-mutation-guest-2026-09-27.md)。当前目录仅显示声明；实际预算审批、精确计划审阅及第二次 Execute UI 仍需接入。既有 native discovery/reconcile 使用普通 binding，尚不能承担 mutation-budget-v1 包的跨重启恢复，需在当前授权下补只读入口。
- 最大内容的预算门槛已在显式 `mutation-budget-v1` profile 下局部关闭：宿主批准单次最多 32 MiB／累计最多 256 MiB，签发前预检，Release 三语言普通／16 MiB 六项通过。旧 16／64 MiB profile 保持不变；Debug Rust 最大流程仍会 Deadline，不能把 Release 结果扩展为所有构建模式与负载的保证，见 [预算报告](../reports/mutation-budget-2026-09-27.md)。后续仍需在产品审批中接入并展示实际批准额度。
- 非空四块 guest 故障门槛已局部关闭：Rust／C／C++ 的回执丢失 3/3、四个内容事务和八个 native Create 边界共 36/36 真实退出案例通过，独立 Store 恢复与 Core claim 防重放已核验，见 [guest 恢复报告](../reports/mutation-guest-recovery-2026-09-27.md)。完整 16 MiB 故障组合、任意断电与产品恢复 UI 仍不在该证明范围内。
- 产品侧插件动作与可信选择／两阶段审批的完整接入、系统选择器人工验收及完整应用构建。
- Linux、其他原生平台与 Web 能力分别验收；Windows 原 owner 测试不等于全平台实现。
- 按 SDK 兼容门槛运行旧 transport 原件、审查新增契约和可用性限制后，再决定冻结候选。
