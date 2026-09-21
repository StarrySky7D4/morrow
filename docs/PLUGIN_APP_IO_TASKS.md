# 主应用 IO 任务与内容库访问

`Workbench` 的 Rust 应用入口使用显式 `StateSlot` 承接完整原 `WorkbenchState`。该状态直接拥有Storage、Manager、Pool、内容/编辑会话、undo、上传暂存和capture范围；内部不保存worker句柄或可空StorageSlot。外围负责一次任务的准入、交付、停止、真实线程回收与恢复，不创建第二份数据库。端点批准页面见[端点管理](PLUGIN_ENDPOINT_MANAGEMENT.md)，HTTP任务消息及界面见[HTTP任务接线](PLUGIN_APP_HTTP_TASKS.md)与[页面验证](../reports/http-task-ui-2026-09-20.md)。

## 原存储与应用状态

| 状态 | 含义与允许的操作 |
| --- | --- |
| Local | 原内容库在调用线程，允许既有内容操作 |
| Running | 原内容库由任务线程持有；状态、读取结果和取消不等待网络返回 |
| Stopping | 已请求停止，原线程尚未归还存储；不能声称取消已经完成 |
| Reclaimed | 实际线程已结束并归还原存储；保留上一任务身份、交付与退出诊断，待明确确认 |
| RecoveryRequired | 原存储已取回，但清理或封存需要恢复；失败清理保留确切实例，封存失败仍可读取既有内容 |
| Unavailable | 线程未归还原容器，不能自动重开内容库或重新执行任务 |

StateSlot的执行者现区分短IO与持久配置服务，两者共用同一owner、清理/修复和确认规则。原生 `start_service` 返回后可能仍在绑定；`service_status`保留原提交身份、实际地址、绑定/监听/监督结果及通用存储阶段。服务工作台命令通过 `submit_service_command`进入原ServiceHost，短IO仍自动排空。私有协议现已开放显式服务启动/状态与有界命令提交/查询/读取/取消，Dart提供低层接口并自动路由服务期间的普通业务；页面启动入口已接入，Windows完整应用路径见[窗口集成报告](../reports/service-window-integration-2026-09-21.md)。

StateSlot 不实现 Deref；现有业务方法先显式借用完整状态，缺席返回类型化Busy。调用顺序在文件创建、计数、捕获、上传完成与注册表变更之前检查。IO使用内部原Manager在同一HostRuntime上独立准入实例，再通过 `spawn_managed_owner` 将完整状态移交，不拆取Pool根。短IO退出只封存原Storage；应用最终关闭才关闭Pool、编辑器和capture范围。HttpTasks的已用提交身份与当前提交关联留在外围，移交和回收均不重置它们。

## 受信任应用接口

- `start_io(StartOptions, prepare)` 核对原选中包摘要、Registry 修订、启用及 IO 类别批准，创建一份随机任务身份。可信准备回调只能在原 Manager／HostRuntime／ManagedInstance／IoBinding 上构建资源适配器，不发送请求；准备失败或 panic 尝试清理原实例。具体 origin、方法、凭据与额度仍必须由资源适配器核对。
- 当前一份 Workbench 同时承载一项任务。输入与结果仍受原执行器预算约束；任务提交后进入 drain，Ready 保留到真实读取、取消或到期，避免先断连使有效结果失去交付条件。
- `poll_io` 与 `io_status` 只尝试非阻塞回收。`read_io` 保留执行器的最后授权检查；读取上限不足不会消费结果。结果被取消后不能转回成功。
- `cancel_io` 立即撤销该任务执行器的授权并请求停止，不等待同步网络回调。旧任务身份、另一工作台的身份不能操作当前任务。
- `repair_io` 仅重新清理原连接和封存同一内容库，不重发请求。原执行、断连、维护结果分别保留；成功修复不会改写历史错误或将已发生的效果改为未发生。
- `acknowledge_io` 明确放弃已结束任务的剩余句柄与诊断，未归还存储或仍需修复时拒绝。确认并不重试任务。新任务必须经过新的准入和资源检查。

任务身份和控制状态只属于当前进程；进程重启不会恢复旧句柄或旧授权。已有 IO 意图、Observed／Unknown 与证据仍由原 Store 保存，重启核对功能依原有路线继续接入。

## 既有私有协议与关闭

合法私有请求开始时尝试回收已结束线程，然后在依赖状态的分支前检查访问。导出不能因Busy创建文件，上传完成不能因Busy消费令牌。Manager、编辑器及缓冲区已随原状态移交，因此目录、插件状态、界面/捕获关闭及上传追加/取消也必须显式借用状态。短IO后台阶段这些入口返回Busy，有限服务期间已通过原执行者命令通道接线；两条路径都不能静默丢弃关闭操作或编造插件修订。Rust `plugin_status`、`ui_close`、`close_capture_scope` 改为Result，私有协议传播既有错误码。原执行者命令通道仍需更多实际UI/capture组合验收，不能把Busy作为常驻服务的最终交互方案。

沿用现有错误字段与 `ui_code`：110＝Busy，111＝需要恢复，112＝原存储不可用，113＝任务身份失效，114＝上一任务尚未确认。任务所有权阶段未修改消息布局；后续端点管理扩展了私有协议并同步生成 Dart，冻结 SDK 保持不变。旧 Dart 错误处理可接收普通错误，查询不能将 Busy 误判为终止。

`finish()` 请求停止，只在实际取回存储后关闭 Pool 和封存；未退出返回 Busy。`Drop` 不阻塞等待，同步回调尚未结束时原内容库租约仍由线程持有。CLI 现已将 EOF／截断输入／读写错误统一送入停止待退出流程，Dart 关闭等待实际进程退出而不再五秒强杀。清理或封存失败仍明确返回；进程被系统强制结束时不承诺完成。

## 下一项

完整业务状态已提取并接入短IO真实移交。State现在实现CommandOwner，与本地调用共用校验、业务分派、错误码和响应清理；State明确拒绝HttpStart及全部Io调度动作。队列输入上限64KiB，私有响应上限128KiB，不增加guest入口。Dart接线时实查现有上传块为32KiB，无需缩小；不能把所有现有128KiB私有请求直接转入队列，完整内层帧超过64KiB必须明确拒绝。排队与未读回执由Zeroizing保护，输入移入State及回复交给读取方时转交清理责任。取消后不把已发生的写入当作回滚。

本地访问仍先检查StateSlot；待显式修复时只读入口保留，写入和上传消费被拒绝。worker的原身份/有效性和prepare_io检查在业务派发前执行。原生Workbench现在可通过公开服务准入与命令方法调用Running服务，私有协议及Dart业务自动路由已连接，Flutter有限服务运行页面已连接；当前应用准入限定有限loopback HTTP，未配置的出站调用拒绝。下一步验证更多实际故障与UI/capture组合，再实现长IO可暂停、TLS/出站资源和完整服务期间响应验收。准备回调不是插件可提交的任意路由，也不是从UI直接构造授权的捷径。文件系统、Unknown核对、完整因果链和三语言IO SDK仍按原门槛推进；当前契约不承诺SDK稳定或全平台运行资格。

持久端点接线顺序：应用在 Storage 尚在位时，用原 Store 的可变借用调用 `StoredHttpEndpoint::resolve` 取得记录与原授权租约；随后将解析对象移入准备回调，使用 `approve_windows` 在新准入的原实例上验证类别、端点政策并解析系统凭据。解析不是恢复活动授权。Preparation 只暴露只读 HostRuntime，不能为适配持久端点而允许回调替换原核心；UTC 的持久记录期限与任务的单调时钟域也继续分开校验。

## 服务调度与命令身份（2026-09-20）

私有动作61—66承载ServiceRunStart/Status与CommandSubmit/Status/Read/Cancel。启动逐项携带原配置、发布、包与Registry身份/修订及有限预算；启动回执丢失时，可用空任务key查询单个当前服务并核对原submission，不自动重新启动。停止、修复、确认仍走原IoCancel/Repair/Acknowledge。

每项服务最多保留8个未消费命令句柄、512项提交历史。相同非零submission及完整内层请求SHA-256返回原随机command key和当前状态；修改字节冲突，失败准入不消耗历史。历史满额拒绝新增，不淘汰旧身份。丢命令提交回执可用CommandStatus的空commandKey及原commandSubmission只读核对，未知身份不执行请求或创建记录，也不必重发正文。状态查询不消费Ready；read复核原授权后至多交付一次，结果离开后只留摘要、started与终态。成功读回执丢失、开始后取消及Unknown都不能成为重跑依据。任务确认后旧key失效，历史不会跨新任务或进程自动恢复。

内层请求最多64KiB且预先验证契约，禁止嵌套全部调度动作；业务回复保持128KiB。只有已验证外层CommandRead允许256KiB响应封装，以容纳原业务回复与少量状态；其余私有帧仍128KiB。Rust清理内层输入、回复与封装payload；Dart清理发送暂存和外层回执，调用方须dispose拥有的内层结果。这里的命令身份只用于当前运行交付，不替代持久内容事务或Unknown证据核对。

私有协议证据见[服务命令报告](../reports/service-command-protocol-2026-09-20.md)。Dart现已将观察到的服务期间普通业务自动转为命令，见[业务路由报告](../reports/service-business-routing-2026-09-20.md)。业务队列保持请求顺序；独立传输队列仅等单次响应，命令等待时允许状态和停止插入。发送槽内选择并固定原task，开始后遇错误、Unknown或换代不切换新服务、不回退本地、不重发。丢回执保留原身份供只读核对。


业务等待使用同一截止，在排队后真正发送、flush和收取响应时检查剩余时间。超时/传输不明时封闭当前通道并以EOF请求原拥有者回收，不能用kill把超时当作完成。关闭不等待整条业务队列，阻止后续轮询后等待已受理传输和实际进程退出；最初的传输错误在清理期间保留。

服务运行中的IO状态观察不会把原业务界面误置只读；真实回收后恢复本地诊断。内层业务错误与QueryFailure终态保持，损坏的已消费回执转为带task/submission/command身份的Unknown。类型化敏感结果解码后擦除内层帧；返回借用Reader时显式转移其原缓冲寿命。

当前上传块实查为32KiB，已在完整64KiB命令帧预算之内，没有为了接线调整块大小。其他完整请求在64–128 KiB之间使用[私有分段合同](PLUGIN_SEGMENTED_OWNER_FRAMES.md)，每块仍在原64 KiB命令额度内，原拥有者校验全帧后只执行一次；超过128 KiB拒绝，本地路径保留原128 KiB上限。服务运行页面已接入，见[运行面板报告](../reports/service-run-ui-2026-09-20.md)。下一步是更多内容/UI/capture组合、真实故障用户路径与超限业务的显式分段方案，不能把基本卡片/语言实测当作所有UI交互完成。

同步拥有者回调阻塞期间的服务停止已有[原生验收](../reports/service-slow-owner-2026-09-21.md)，监听退出不能替代owner回收；正常或panic返回后才可取回原容器。长IO期间让出执行权的改造按[S0–S4方案](PLUGIN_SUSPENDABLE_IO_PLAN.md)继续推进，当前不宣称同步guest具备该能力。

当前私有分段与失败/Unknown证据见[报告](../reports/segmented-owner-frames-2026-09-21.md)；历史段落不替代当前范围，guest流式IO及应用服务TLS仍未完成。
