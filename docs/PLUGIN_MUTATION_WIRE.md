# 私有文件变更调度协议

这是可信原生应用与 Windows Workbench 的私有 Cap’n Proto 协议，不是 guest SDK、HTTP API 或任意路径授权。所有新动作只能在最外层应用执行，原 owner 的嵌套业务调用、服务命令帧和 Web／其他未适配平台不能执行文件变更。外层帧上限仍为 128 KiB。

## 身份与调度

`mutationStart @128`、`mutationSubmit @129`、`mutationStatus @130`、`mutationRead @131`、`mutationCancelCommand @132`、`mutationReconcile @133`、`mutationDiscover @134` 追加在旧动作后，不改既有编号。完整布局以 `workbench_host/schemas/host.capnp` 为准；生成 Dart native/web 绑定及 `hostDigest` 同步更新，guest IO schema 未变。

Start 绑定非零 32 字节 submission、包 ID／摘要、Registry revision、Create／Replace／Delete、可信选中路径、Create 相对路径、subject、审批摘要和期限。路径上限 4096 字节，包 ID／subject 最多 256 字节，期限 1–30000 ms 且不超过包声明。审批 scope 来自可信调用者；协议本身不能证明路径来自系统选择器。

Reconcile 单独携带非零 submission、包 ID／摘要、当前 Registry revision、完整原 RequestRecord 容器及 1–30000 ms 期限，没有路径字段。它与 Start 共用下述有界身份表，但摘要域不同；换用同一 submission 跨不同启动类型也属于冲突。正式任务启动仍要求上一个任务真正 join、必要恢复及 ack，重复同一申请只读取原任务身份，不再创建核对任务。

一个 Workbench 会话最多保存 512 个启动身份。有效格式且没有其他未确认任务的申请，在目录／预算／实例准入前保留身份；后续失败也不能重用。Busy、格式错误、身份表已满等尚未准入的请求不占新身份。相同身份和相同规范字段只能取得原任务 key；字段改变拒绝；原任务已经 ack 后旧 key 失效，不能重启。启动失败留下的清理 Task 不作为成功 MutationTask 缓存，沿 ioStatus／repair／ack 处理。

Submit 绑定 TaskKey、独立非零 submission 和精确命令内容。每个任务最多 512 个成功入队的命令身份，一次只有一个待完成／待领取结果。相同 submission 和相同内容返回原 commandId，即使已领取也不重新入队；改变命令内容、正文或 offset 拒绝。准入失败未入队的命令不保留身份。状态响应包含当前命令；响应顶层 `mutationCommandId` 是此次提交对应的原命令 ID，重复旧提交时两者可能不同。

命令编号 Select=0、Prepare=1、Chunk=2、CommitContent=3、Execute=4、Query=5、CancelPlan=6、Release=7；原生只读核对任务的状态种类 Reconcile=8（不属于 Submit 命令）、BuildPlan=9、Discover／NextPlans=10。Select 的 commandId 为 1，之后只在成功入队时递增。Read／CancelCommand 必须精确匹配 TaskKey 和 commandId，旧请求不能领取或取消新命令。Status 不领取回执。块最多 60 KiB；只接收对应命令的字段，拒绝混入其他计划／正文／offset。

## 状态、结果与不确定性

MutationState delivery：0 Pending、1 Ready、2 Consumed。Ready 仍需要最终实时授权后才能领取。`reconcileRequired` 表示待核对；`terminal` 表示已确认 Observed／Cancelled、已释放，或发现扫描完成／无法继续；它不是所有任务共用的文件成功标记。二者不同。已领取的选择元数据可从状态恢复，只在原现场选择有效，不是跨重启身份或新权限。terminal 与 reconcileRequired 可以同时为真：已确认原计划终态后，后续 Query／Release 的回执仍可能失败；客户端不能把这两个维度错误地互斥。

MutationResult kind：0 Pending、1 Selected、2 Prepared、3 Staged、4 Created、5 Deleted、6 History、7 Released、8 PlanCancelled、9 Failure、10 Reconciled、11 Planned、12 Plans。Core Record／Outcome 原件保留为 Data，并提供以下权威显示字段：

- phase：0 无记录、1 Prepared、2 OutcomeUnknown、3 Observed、4 CancelledBeforeDispatch。
- operationId：记录或实际结果绑定的原操作身份。
- effect：0 不提供效果判断、1 OS 确认成功、2 OS 拒绝；osCode 仅在拒绝时提供原数值。

**Created／Deleted 是结果种类，不等于 OS 成功；Observed 仅表示保存了观察结果。** 当前 Query 返回历史与暂存状态，不返回效果原件，因此其 effect=0；不得从 Observed 推断成功。原生独立 Reconciled 回复可携带已验证的效果原件；没有原件时仍为 effect=0。

Failure 分层，避免暴露原生路径或将交付失败误作回滚：

| failureLayer | failureCode |
|---|---|
| 1 Delivery | 1 Busy、2 Closed、3 Limit、4 Cancelled、5 Unknown、6 Consumed |
| 2 Target | 1 Admission、2 CommittedButDeliveryDenied、3 RestartRequired、4 Busy、5 OutcomeUnknown、6 AlreadyDispatched、7 UnsupportedConditionalReplacement、8 CancelledBeforeDispatch、9 CommittedButDeliveryCancelled、10 InvalidSelection、11 Missing、12 Mismatch、13 Changed、14 Io、15 Limit、16 Persistence、17 CommitUnknown、18 RevisionConflict |

取消某条命令、持久取消计划与停止任务分别处理。命令错误后不自动重放，原 worker 存活时可显式 Query／CancelPlan／Release。成功领取 Released 才请求停止；`ioCancel` 可显式停止。实际 join、存储维护和断开成功后才能 ack。

## 尚未接通的恢复边界

原生 Select 回执在领取前丢失时，runtime 会回收未交付选择；Query／Release 可返回 Missing，应停止、实际 join、ack 后重新选择。宿主成功领取 Select 后上层回包丢失，则可从状态恢复保留元数据。

原 worker 超时／停止／崩溃退出后，旧 Mutation Query 无执行载体。原生调用者现可在实际 join、维护／断开恢复及 ack 后，显式调用 `start_mutation_reconciliation(StartOptions, RequestRecord)`，以当前包审批与 Registry revision 重新准入。它接收完整原计划，使用归还的原 owner，只读匹配历史及 Observed 效果原件，不打开目标路径、不恢复现场引用、不重放效果。不存在记录返回空；Prepared／Unknown／Cancelled 不附带 OS 成功推断；失配拒绝。领取成功或失败都请求停止，仍须真正 join 后才能 ack。

独立核对现已接通私有 mutationReconcile 启动动作与类型化 Dart 客户端。`RustWorkbench.mutationTasks` 暴露 NativeMutationTaskClient；`supportsMutationTasks` 仅在 Windows 原生进程通道下为真。共享的 poll／stop／repair／ack 仍在 WorkbenchIoTaskControl。所有七类 mutation 动作走最外层调度，不封装为服务业务调用。

客户端完整保留 UInt64 为 BigInt，检查任务／提交／命令身份、阶段与效果组合，并在清理回帧前取得自有字节副本。原计划和 Core 记录仍是有界不透明容器，解析与持久权威仍在 Rust。相同 Start 再提交时，顶层 commandId=1，但当前任务可能已前进到其他命令；客户端接受这种合法恢复，不重新执行。

传输层为单次明确调用，不自动重试；mutationStart／mutationSubmit／mutationReconcile／mutationDiscover 的构造及发送帧沿用现有尽力清理机制，调用者原始数据与系统副本不在清理保证内。独立只读恢复会话和正常关闭后的真实进程重启已验证，见下节；编辑会话、真实崩溃整链和 UI 端到端恢复仍待完成。不可把退出、ack、空记录或重新选择当作执行许可。

Dart 当前完成生成绑定、类型化原生客户端与真实帧解码测试；只读恢复 UI 已接入；正式文件变更选择审批、编辑进度页和 C／C++／Rust guest SDK 仍待接入。Windows 条件替换继续在 claim 前明确 Unsupported。


## 宿主构造草稿计划

Submit kind=9 仅接收 operationId、contentLength 和可选 contentSha256；不可混入 plan、块正文或 offset，其他 Submit 种类也不可混入这三个字段。操作 ID 遵循 Core 身份规则（UTF-8 1–256 字节，无控制字符及路径分隔符／冒号），内容最多 16 MiB 且不超过任务预算。摘要有值时须为非零 32 字节，零长度内容须采用 SHA-256(empty)；无摘要仅允许零长度，最终仍根据原选择的 Create／Delete／Replace 校验。

实际构造在原 owner 队列内执行。TargetBroker 从原句柄租约取得 target reference、expected identity、相对路径、subject、审批摘要、包摘要及 disposition；调用者不能重新指定这些身份。构造前后走现有实时权限、期限与取消检查，不重新打开路径。队列／回复预算沿用现有有界通道。

Planned=11 只包含 `plan@13` 的规范 Core RequestRecord 容器；没有 Record／Outcome，没有 Prepared 或成功效果投影。生成草稿不写 Store、不产生 OS 文件效果、不改变 resource.request 的准备状态。显式 Prepare 之前可以修订草稿，Query 仍返回无记录；Prepare 之后相同计划可在实时校验通过时重建，改变计划拒绝；已释放／已消耗的现场选择不能重建。一次领取丢失不意味着已持久化，不能自动重放。

`MutationTaskBackend.submitBuildPlan` 已接通对应字段及计划回帧，Dart 仅复制有界不透明容器。有界持久计划发现另见下节；真实崩溃恢复闭环、界面审批及 guest SDK 尚未完成；独立只读恢复会话已接通。证据见 [计划构造验收](../reports/mutation-plan-builder-2026-09-27.md)。


## 持久原计划的私有分页协议

2026-09-27：`mutationDiscover`（Action 134，Request 字段 56）接通 Store／runtime／Workbench 原计划发现。请求指定提交 ID、包 ID／摘要、Registry revision、可信宿主主体、Create／Replace／Delete 范围、扫描上限 1–8 和期限 1–30000 ms。主体不是 guest 自报授权；包和能力仍须当前审批。结构校验通过且无活动任务后，提交 ID 在权限／预算准入前占用，失败后不可换参数重用；原请求重试只取回原任务身份，不再扫描。

第一页 commandId=1、state.kind=Discover(10)。继续页使用 Submit kind=10，仅允许 submission／kind／scanLimit；其他命令拒绝 scanLimit。发现任务仅允许 NextPlans／Release，目标变更任务不能用 NextPlans。所有请求走外层宿主调度，嵌套服务业务路径拒绝。

`Plans=12` 包含 `plans@14` 的原始 RequestRecord 容器、`scanned@15`、`done@16` 和非完成页的 `checkpoint@17`。每页最多扫描八个候选，最多八个计划；每份容器受 Core 同等上限约束。空页可以尚未完成，非完成页必须扫描了候选。没有 Record／Outcome／phase／effect，不可由原计划推断成功或重新获得文件执行权。只有单独核对才验证完整历史与效果原件。

Dart 的 `startDiscovery`／`submitNextPlans` 接通这些类型；复制前检查列表数量、单项及总字节上限，清理回帧前取得不可变嵌套副本。Plans.done 必须对应 state.terminal；发现状态不能包含选中目标或 reconcileRequired。重复 Start 的回执仍是 commandId=1，当前状态可以已经前进。任何一步都没有自动重发。

上一页必须领取后才可继续；同一提交 ID 重试 Next 只返回原命令 ID，不再次前进。领取后的传输回执丢失无法再次领取相同页，上层必须显式处理这份不确定性；不能以新令牌悄悄推进。发生错误后只关闭／停止，待真正 join、维护成功及 ack 后才开启新任务。Release 复用现有种类 7 和 IO 生命周期。

发现游标仍由 worker 持有，但成功领取的非完成页现在提供一个非零 32 字节 checkpoint。`MutationDiscover.checkpoint@8` 可在旧任务实际退出、恢复及 ack 后，由新的 submission 显式带入下一次启动。Core 将位置绑定到同一个存活 Store、主体、包摘要和操作类型；新 worker 仍须通过当前审批、目录修订、期限和预算校验。完成页、失败页和未领取页不提供 checkpoint，status 也不能恢复丢失页。

私有 checkpoint 只是宿主查找凭据，不序列化内部操作 ID，不代表授权；宿主最多保留最近 64 个已领取页的位置，先进先出淘汰。过期、篡改、其他宿主或范围失配均明确拒绝，不自动从头扫描。该缓存跨任务 ack 保留，但不跨宿主进程重启；启动仍受每个 Workbench 会话共用的 512 个提交身份上限约束。每页是新的 SQL 视图，较早排序位置的并发插入不保证被本次扫描覆盖，不能将续扫宣称为一致性全库快照。

Dart 会话在不确定翻页期间保留上一份成功页的位置。调用者应在清理前保存该 checkpoint，完成实际退出及 ack 后显式启动续扫；从上一已交付位置重读可能重复只读结果，不会重放文件效果。恢复 UI、跨进程持久续扫和真实宿主崩溃恢复整链继续开放，SDK 未冻结。证据见 [断点续扫验收](../reports/mutation-checkpoint-2026-09-27.md)。


## 独立恢复会话

`MutationRecoverySession` 与页面生命周期分离，`RustWorkbench.mutationRecovery` 和 `forBackend` 返回同一后端配对的稳定会话；挂载、移除监听器不产生 IO。会话没有定时器，刷新、领取、翻页、重试及清理均由调用者明确触发。只保留当前有界发现页和一个核对结果，不缓存全库；不提供 Prepare／Execute 接口。

丢失 Start／Next／Release 提交回执后保留原参数与提交令牌。只允许显式重取完全相同提交的回执；普通 status 的命令编号和种类不能证明提交归属。相同请求由宿主去重。领取回执丢失后，只有同一命令仍未消费时才能显式再领取；如果已经消费，进入 resultLost，不允许继续翻页掩盖缺页。关闭并真正归还 owner 后，可以用新提交明确重开扫描。

停止不等于退出；只有实际 exit 且 reclaimed 才能 acknowledge，recoveryRequired 必须先 repair。ACK 回执丢失后通过权威状态观察区分已归还本地和仍待确认。若准入失败未创建任务，宿主可能仍保留已占用提交令牌；明确 abandon 仅放弃客户端待处理请求，不删除任何持久计划，不释放权限，也不复用失败令牌。

Prepared／Unknown／Observed 等阶段直接保留，独立核对不会把 Unknown 改成成功。正常核对读取后宿主自行停止；停止与确认期间保留已收到的核对结果。ACK 后会话清空，调用者需要展示较长期结果时应自行持有有界结果快照。

Rust `core/examples/plugin_package.rs` 的离线 pack-v2 工具现可显式声明 `file-create`、`file-replace`、`file-delete`；这些名称仅生成包声明，不意味着 guest SDK 已接通文件变更或平台具有条件替换能力。Windows 条件替换限制仍不改变。

本节验证范围与实际进程重启证据见 [恢复会话验收](../reports/mutation-recovery-session-2026-09-27.md)。

## 只读恢复界面

Windows IO 设置中的 `MutationRecoveryManager` 接入上述独立会话，`MutationRecoveryViewState` 仅保留一页原计划、一个选中计划及一个最近核对结果；ack 和离页不会丢弃已选计划。缓存包含原发现范围，不是授权；新查找、续扫和核对仍绑定当前确认目录及实时审批。其他平台不展示此原生入口。

所有业务动作显式点击，展示层仅在可见且本会话任务存活时周期刷新状态，隐藏和退出停止轮询。主体输入必须精确匹配历史标识，计划原件不在 Dart 中解码。所选计划先经退出与 ack，再独立核对；Unknown 不触发重试，不将 Observed 自动显示为成功。协议与 SDK 边界没有因增加 UI 而扩大。详见 [恢复界面验收](../reports/mutation-recovery-ui-2026-09-27.md)。


## 真实宿主故障退出的恢复边界

2026-09-27：Windows 专用 fault-injection 宿主已验证 Create／Delete 在 after-claim、after-effect、after-observe 处直接 exit(86)，由 Dart 等待实际退出后重新打开同一临时受保护库。新宿主发现字节一致的原计划，并执行两次独立只读核对：前两阶段保持 OutcomeUnknown／无已保存效果，after-observe 返回 Observed／已保存 OS 成功。发现和核对不重新取得目标选择，不调用 Execute。普通构建在同样故障环境变量下正常执行，证实测试故障开关不进入默认构建。

该验收覆盖私有进程协议、独立恢复会话及展示模型，不覆盖崩溃后真实 UI 按钮操作整链，也不等同外部强杀或任意断电。检查点仍不跨宿主重启，Replace 能力与 guest SDK 边界保持不变。详见 [故障恢复验收](../reports/mutation-crash-recovery-2026-09-27.md)。


2026-09-27 后续验收：上述独立恢复路径已在真实时钟 Flutter 测试控件中，通过实际发现／领取／选择／退出确认／核对按钮完成七项真实宿主矩阵，含离页重挂载与最终 ACK。它关闭的是恢复面板联调缺口，不包括事故前的正式变更编辑、安装包手工操作、断电或 guest SDK。见 [恢复界面故障联调](../reports/mutation-crash-widget-2026-09-27.md)。

## 可信应用入口

Windows IO 设置通过独立 `MutationExecutionSession` 使用上述私有协议；选择和范围确认只进入准备链，Execute 必须再次明确确认。会话与页面生命周期分离，停止后不接受迟到回复继续推进命令，任务与 Local ACK 均核验提交身份；ACK 后宿主保留的历史提交仅能与本会话最后请求一致。恢复交接只预填范围，不自动开始核对。详见 [本机验收报告](../reports/mutation-execution-ui-2026-09-27.md)；guest SDK 契约保持未接入状态。
