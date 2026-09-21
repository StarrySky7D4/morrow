# 常驻服务运行与工作台调度实施方案

当前实现：`41e6431`；2026-09-20，版本 `0.1.9-test.52+56`。已实现完整WorkbenchState、原执行者命令预留、内部续租、本地/worker共用业务派发，以及原生应用的持久配置服务准入和监督回收。当前应用服务准入限定明确批准的单个loopback HTTP有限运行；私有异步命令协议、有界身份表及Dart业务自动路由已接入；Flutter有限服务运行面板及启动诊断已接入，真实端口占用/到期/撤销发布与认证/准入拒绝8项通过；实际窗口与其余故障、应用服务TLS和出站资源适配仍待完成。现有短IO自动drain的行为不变。

## 当前限制的具体来源

| 位置 | 当前行为 | 不能直接当成常驻方案的原因 |
| --- | --- | --- |
| `core/src/plugin_package/io.rs` / `core/schemas/io_manifest.proto` | IO v1声明包含最长30秒 `max_duration_ms` 和累计字节额度 | 旧包同一字段约束绑定寿命与单作业期限；新service-run-v1仅通过独立profile延长运行 |
| `plugin_runtime/src/io_binding.rs` | 旧绑定按IO声明限期；service-run-v1在原IoContext签发一次有限运行，原请求上限不变 | 重复绑定不应改变同实例账本；释放作业只返还并发容量，不退款 |
| `plugin_runtime/src/io_jobs.rs` | `spawn_session_owned` 从声明建立单作业timeout，`submit_routed`校验；工作线程同步执行一项guest/broker调用 | 监听长期运行需要独立运行期限；仅添加UI队列仍可能等待当前阻塞请求结束 |
| `network_node/src/managed_service.rs` | 监听监督和执行worker有各自退出路径 | socket关闭不证明worker已退出；必须分别观察并真实join |
| `workbench_host/src/lib.rs` / `io_tasks.rs` / `service_tasks.rs` | StateSlot用执行者变体保持原State独占，原生服务准入、命令提交与双退出已接线；短IO仍独立drain | 私有命令/回执协议已接；Dart业务路由已接；已接Flutter运行交互，尚需实际故障流程、TLS和出站资源适配，不能通过第二个Store绕过独占 |

## 一、完整所有者适配

`ServiceHost<O: HostOwner = HostRuntime>` 和 `ManagedRoute<O>`保留原拥有者类型；克隆只共享同一个worker。新建失败通过 `new_owned` 返回原 `IoWorker<O>`，调用方仍负责停止与回收。`request_stop`只发出取消，`try_reclaim`只有线程真实结束后返回一次完整 `WorkerExit<O>`，`shutdown_owned`等待实际结束，无固定成功超时。

WorkerExit中的执行结果、原instance断连结果和维护/封存结果独立保留；维护失败不撤销已经发生的内容或网络效果。原Storage包含审计身份、原库保护租约和Registry租约，不能退化成裸Runtime。旧HostRuntime专用 `shutdown`保留兼容语义，但新的工作台服务入口必须使用保留owner的接口。

调用方保留ServiceHost直到回收完成。Drop只请求取消，不能向已经消失的调用方交还owner；本阶段不增加阻塞析构或强杀。监听节点先请求关闭并等待监督任务结束，同时请求worker停止，然后取得WorkerExit恢复原拥有者。端口绑定失败或取消启动也执行同样回收。

## 二、显式版本化的服务运行租约

第一步已实现独立的实验性有限运行profile：`IoDeclaration.service_run`（tag 8）与必需feature `service-run-v1`成对出现；profile版本1声明运行时长，要求Listen、Publish和正确服务schema。原IO v1每请求最长30秒与累计字节额度不变；缺少profile的旧包仍只能使用旧绑定。规范编码检查继续覆盖整个manifest字段18，未知/重复/非规范嵌套字段均拒绝。旧原包和冻结SDK不重打包。

未声明新累计预算的profile由可信宿主显式调用 `Manager::bind_service_run`，核对实际Manager、原Host/instance、当前修订、包摘要和已批准能力。声明本身不授权；读取持久发布配置也不自动签发运行租约。签发在原IoContext锁内一次完成，无效请求不消耗首次签发机会。相同实例不能通过丢弃句柄、再次绑定、旧bind_io入口或到期后重绑续期。新实例需要重新显式批准，不恢复旧租约。

实验性运行声明上限暂为一小时，宿主可批准更短期限；这是有限原型边界，不是吞吐测量结论、长期稳定性资格或SDK冻结承诺。运行同时受可信单调毫秒与真实Instant截止约束，回退或到期后永久失效。配置化监听仍取发布、认证、配置有效性与运行租约的共同限制。真实31秒监听及独立请求预算证据见[有限运行报告](../reports/service-run-2026-09-20.md)。

累计预算扩展已实现：profile可选 `budget`（tag 3）与独立必需feature `service-run-budget-v1`成对出现，同时保留父feature。预算版本1声明累计任务保留次数及累计字节上限。宿主必须使用 `bind_budgeted_service_run` 显式批准两项额度，不能超过声明；旧绑定入口拒绝预算profile，不能以旧默认值绕过。没有预算字段的旧profile编码和运行语义保持不变。

原IoContext保存累计任务数和原字节账本，IoBinding与IoWorker的 `service_run_usage` 仅提供诊断。任务计数是已成功保留任务预算的次数，包括独立IO准备或文件选择，不是成功业务响应数；资源单独保留不计任务。取消、丢弃、读取以及已保留后入队失败不退款；预算保留之前的拒绝不计费。并发Usage.jobs仍是另一个维度。

四条字节记账路径——任务保留、worker准入、IO响应预留、增量/完成帧计费——均检查宿主总额度。累计任务数耗尽只拒绝新的任务保留，不使存活检查失效；已获准任务仍可在剩余字节额度内完成，已完整预留的响应仍可读取。声明最多1,000,000次任务保留，字节数受原IoBudget上限约束；这些是有限原型边界，不是吞吐性能承诺，宿主可进一步收紧。实现及验证见[累计预算报告](../reports/service-run-budget-2026-09-20.md)。

预算profile现可通过可信宿主 `IoWorker::renew_service_run` 或 `ServiceHost`同名入口显式续租。调用携带原Manager、原ServiceGrant、期望Registry修订与运行修订，以及新的绝对截止和累计上限。运行修订从1开始；同一修订并发更新只有一次成功。原Manager/Control/连接、当前启用包摘要和批准能力重新校验；错误Manager或grant在采样原时钟之前拒绝。诊断 `service_run_snapshot`不产生授权。

续租只支持已显式批准的预算profile，不增加schema或guest接口。新期限与两项预算不能缩小，至少一项变化，且不得超过原包声明。时间上限始终从首次签发的tick与Instant计算，不把每次续租变成新的滚动窗口。所有绑定、监听器和资源副本读取同一运行期限；累计任务和字节不清零。旧绑定仍禁止重复签发，非预算profile不能借此隐式升级。

更新在原worker准入/停止锁、原时钟和原IoContext锁的顺序下完成。已到期、时钟回退、停止、排空或撤权不能通过续租恢复；既有每请求截止和被取消结果不变。配置解析生成的grant继续核对原Store的配置/认证/发布探针；低层手动签发的grant仍由可信宿主负责显式撤销，不能宣称其具有持久配置探针。独立认证、发布、出站资源和历史结果期限不会被续租延长。实现证据见[续租报告](../reports/service-run-renewal-2026-09-20.md)。

独立宿主命令容量预留已在原生底座实现，工作台命令接线仍待完成；没有自动续租、无限服务时长或超过原声明的补充额度路径。

新的声明、宿主批准与实际运行准入分别保存并核对以下维度：

- **运行租约**：原run身份、管理修订、配置摘要、包摘要、单调期限和绝对期限；真实原Store的发布/认证有效期仍是更早的上限。
- **每请求上限**：执行时长、请求/响应/帧大小、host调用数；不因监听寿命延长而放宽。
- **并发容量**：排队、执行、Ready未取走结果分别有界；UI管理命令预留容量。
- **累计额度**：在原实例账本上累计的作业/字节总额。取消、读取、释放、续租、换路由都不清零，不以新worker掩盖累计消耗。

固定有限运行租约、显式累计预算和原声明内续租已实现；完整WorkbenchState、有界调度和应用有限运行界面也已接线。后续长期数值上限需经原型测量确定并版本化，不能把当前有限续租原型当作管理队列、无限常驻和工作台共存均已完成。

需要同时修改并测试的入口：包声明验证、Manager准入、IoBinding时间/计费、IoWorker每请求deadline、ListenerGrant/ServiceGrant及持久授权解析。只改其中一处会造成旁路或仍在30秒后失效，不作为完成。

退出证据：同一服务实际超过旧30秒窗口仍可接新请求；单个请求仍按原短期限停止；到期、撤权、额度耗尽、时钟回退与重启均拒绝恢复旧授权；续租前后的累计账本连续。采用可控单调钟做边界测试，同时保留至少一次真实持续监听测试。

## 三、把完整工作台状态放入单个执行者

`WorkbenchState`已从外围Workbench抽出，直接持有原Storage、Pool、Manager、内容及内外部UI会话、undo、附件/上传暂存与capture状态；不包含worker句柄或可空StorageSlot。它实现HostOwner/ManagedHostOwner，runtime始终来自同一Storage。外层Workbench通过StateSlot管理完整状态的移交/回收，并保留HTTP提交去重与回执关联。

原内容、查询、证据、设置、插件目录、凭据、端点和服务批准逻辑都在State上运行，外围使用显式借用转发。短IO的finish_io只封存原Storage，不关闭Pool和编辑器；应用finish在真实回收后才执行完整清理。状态提取本身不证明业务调度；后续已接原worker有界命令和有限服务应用准入。现有短IO仍进入drain，服务期间普通业务则使用命令路由。验证范围见[状态提取报告](../reports/workbench-state-2026-09-20.md)。

工作台层定义有界且完全拥有参数的命令，分别覆盖Page/Read、Create/Apply、Query、Preferences、Capture、Import/Export及管理操作。Create/Apply继续走现有 `run_observed`、Pool、逐对象授权和证据提交，不允许直接写Store替代。响应是拥有的结果或受控结果句柄，不跨线程传递借用、指针或临时UI对象。

原生底座已经通过显式 `CommandOwner` trait 和 `submit_owner_command` 实现独立宿主通道，避免反向依赖Workbench；ServiceHost仅向本地可信调用方转发，不新增HTTP路由、guest ABI或远端principal权限。并发保留上限8项，单项输入至多64KiB、回复批准至多1MiB，输入加回复上限在准入时一起保留；这些是有限原型参数。排队、执行及Ready未读结果均占容量，丢弃运行中句柄不会提前释放队列持有的保留。调用方取走回复后负责自己的内存；handler仍须限制内部工作和分配，回复大小复核不能充当强制内存沙箱。

每次循环最多执行一项宿主命令，随后仍处理一项原IO消息，服务请求占满原作业容量不影响这8项保留。处理前后、正式进入handler前及读取结果时均复核原运行有效性。取消/关闭发生在handler启动前不会调用handler；启动后的取消、停止、错误或不合格回复按Unknown处理，不报告回滚，也不自动重试。panic沿原worker失败回收，执行/断连/维护状态继续分别保存。修复仅重试断连或封存，不重跑业务命令。

`ManagedHostOwner` 与 `spawn_managed_owner` 在移动之前借用拥有者内部的原Manager完成同一准入校验，解决借用Manager同时移动整个状态的问题；缺失/错误Manager仍返回原owner与原instance。Windows组合拥有者实测包含原Storage、Pool和Manager，可在同一真实HTTP监听期间查询原库执行事实并完整回收，见[宿主命令报告](../reports/owner-commands-2026-09-20.md)。该报告的组合测试不是主应用WorkbenchState，不能单独证明undo、内容/UI会话或暂存移交；后续完整State和应用路由证据分别见对应报告。

内部Manager续租现通过 `IoWorker<ManagedHostOwner>::queue_service_run_renewal` 和ServiceHost本地转发进入原8项保留队列，无需实现字节命令handler。执行时借用原owner内部Manager，与外部续租共用身份、当前批准、两级修订、首次期限和累计额度校验。取消在worker状态锁下与CAS串行化；开始后的取消/停止只压制回执为Unknown，不能回滚或据此自动重试。`ServiceRunRenewalHandle::read`区分待完成、明确的续租批准/拒绝和交付不确定；排入队列不是续租成功。见[内部续租报告](../reports/owned-service-renewal-2026-09-20.md)。

该入口不增加guest ABI、HTTP管理路由或持久操作记录。诊断快照仍不产生授权。完整State业务命令已接线，不复制Manager、不另开Registry、不在handler中重入worker；全部管理页面和实际撤权故障流程仍需逐项验收。

第一步允许同一执行线程串行处理UI和服务命令，这是过渡阶段，不宣称即时响应。下一步需要将长耗时网络等待和guest续执行改为可暂停的作业阶段，使原宿主在等待期间能处理其他已授权命令；不能在仍持有 `&mut HostRuntime` 的同步guest/broker调用中重入工作台。保留命令顺序、operation身份及最终授权检查，不能为了交互响应复制Runtime或数据库。

退出证据：服务运行时普通内容浏览/编辑、附件和设置修改可完成；请求洪泛下UI命令有界等待；内容命令与入站操作竞争仍受原修订/权限/审计控制；停止、关闭、已提交后取消和未知回执均可解释。仅返回Busy或等整个服务关闭后再访问不满足最终产品要求。

## 四、启动、停止与恢复顺序

业务派发前置现已落地：WorkbenchState实现CommandOwner，借用同一个私有协议业务处理器；外围只保留调度与StateSlot访问门槛。worker内拒绝全部调度动作，不允许递归start/repair。原本地只读访问、服务错误脱敏和修订检查继续共用；输入及未读回执增加明确擦除责任。原State上的实际Rust guest写入与HTTP交错、旧修订拒绝，以及Ready写取消后的单次提交已接入测试，详见[业务命令报告](../reports/workbench-commands-2026-09-20.md)。

持久配置/发布批准下的有限服务准入、原State独占移交、私有命令提交/查询/读取/取消及Dart业务路由现已接入原ServiceHost；Flutter运行面板沿该路径控制服务。短IO继续使用原排空规则，监听监督、Tokio runtime及原worker保留到实际退出。该接线不证明同步长IO期间能够有界响应UI，也不替代实际窗口及完整故障恢复验收。

1. 原库加载配置/批准，验证实际原包、原instance、对象grant及新运行profile；创建服务run身份。
2. 移交完整WorkbenchState；spawn失败恢复 `SpawnFailure.owner`，再清理其原instance。适配器选项失败从 `ServiceHostFailure.worker`回收。
3. 绑定经过批准的地址与TLS；端口冲突或取消启动必须归还原owner。Tokio runtime持续存活到监听和worker都结束。
4. 停止时先阻断新请求、撤权与取消，再分别等待监听监督、worker真实join；不以五秒超时或socket关闭替代证明。
5. 恢复原owner，报告执行/断连/封存各自状态；故障保持可修复，CLI EOF沿相同路径等待。
6. 接入主应用启动/状态/停止/修复页面，验证实际认证HTTP/TLS、内容读写、持久请求查询、撤权、端口冲突、故障重启与工作台共存。

本方案不改变Unknown持久证据核对、文件系统后端、三语言IO SDK和各平台资格的原门槛。其余网络能力也不会因常驻监听子项通过而一并标为完成。

## 已实现接线与剩余验收

以下原生准入前置现已实现：`Workbench::start_service`核对同一原授权锁内的配置摘要/修订、发布修订和监听政策，签发新有限运行并非阻塞启动监督线程。`service_status`区分绑定、监听、监督和worker退出诊断，保留提交身份；`submit_service_command`只向已运行的原ServiceHost提交业务。取消、修复与确认复用StateSlot任务身份；原State只在监听和worker结束、监督线程真实join后回到本地。ManagedNode新增可取消等待的borrowed join，句柄保留到终态。实际验收见[应用服务准入报告](../reports/application-service-admission-2026-09-20.md)。

私有调度协议与有界命令句柄表、Dart低层模型和接口现已接入，见[命令协议报告](../reports/service-command-protocol-2026-09-20.md)。Dart普通业务自动路由现已通过真实HTTP与原Rust工作台卡片/语言操作验证，见[路由报告](../reports/service-business-routing-2026-09-20.md)。Flutter启动/状态/停止/修复/确认面板现已接入，使用backend绑定会话保留在途尝试。页面与真实宿主的验证范围见[运行面板报告](../reports/service-run-ui-2026-09-20.md)。后续重点为实际故障流程、长IO可暂停、TLS和出站资源及完整Unknown核对。

已实现的可信应用ServiceStart准入复用 `service_authority::ResolvedService::resolve/issue`、`Manager::bind_budgeted_service_run`、`IoWorker::spawn_managed_owner`、`ServiceHost::new_owned/bind_configured`。第一验收限定原持久发布配置中的单个loopback HTTP服务，使用明确的有限期限和累计预算，不自动续租；未批准的出站调用明确拒绝。复用StateSlot的唯一owner与修复/确认规则，以执行者变体区分短IO和常驻服务，避免两个槽分别持有同一库。

服务监督器持有Tokio runtime、ServiceHost和监听直到实际退出。ManagedNode已提供可取消等待的borrowed join，由监督器保留完成路径；监听和worker都结束后才将原State重新暴露为本地。端口绑定失败保留原ServiceHost回收，构造失败保留原worker回收，不能丢弃拥有者后旁路重开。

私有调度与Dart双队列已实现：发送槽内选择原task，普通业务按顺序提交/等待/读取，控制命令绕过业务等待。现有上传块实查32KiB，完整内层请求仍限64KiB；超限明确失败，不回退本地或截断。有限运行页面已通过双语窄屏、预算/身份绑定、卸载重挂、Unknown及停止/回收门槛测试。

启动错误诊断现已接通：仅合法宿主错误转为类型化启动失败，随后按原submission观察实际任务；无任务才归档，有清理任务则保留原控制门槛。损坏/丢失响应或观察失败保持Unknown，不自动重发。真实Windows Release宿主的端口占用、到期、撤销发布/认证及四类准入错误8项通过；调用额度与并发数分别遵循host_calls和max_jobs。详见[故障验证报告](../reports/service-run-faults-2026-09-20.md)。

2026-09-21新增真实管道启动回执损坏/EOF两项验证：原宿主先接受启动，再由代理损坏或丢弃响应；分别验证按原身份恢复控制，以及实际退出后重开同一内容库且不自动启动。相关35项回归通过，详见[回执故障报告](../reports/service-run-reply-loss-2026-09-21.md)。

普通业务语言保存现也通过真实管道损坏/EOF验证：保留原命令Unknown，按原身份核对Consumed，重开原库及后续明确确认保存均维持单次修订增量，见[业务回执报告](../reports/service-command-reply-loss-2026-09-21.md)。相关37项回归通过，不外推到任意业务或第三方网络副作用。

真实SQLite竞争导致的封存失败与显式修复也已通过原宿主验证：保留同一任务/身份，待封存材料不因失败丢失，释放竞争后须明确修复，原操作不重做；修复后原库可关闭重开。相关38项回归通过，见[封存修复报告](../reports/service-seal-repair-2026-09-21.md)。这不代替磁盘满、权限或断电类故障资格。

Windows完整应用集成现已通过：经真实窗口中的Flutter框架输入启动原宿主服务，完成内容提交、Markdown预览、语言和宽窄布局切换、停止回收及原库重开核对；同时修复三类实际界面接线问题。相关93项回归、1项原生集成、9文件分析和Release构建通过，见[窗口集成报告](../reports/service-window-integration-2026-09-21.md)。未验系统鼠标键盘/真实剪贴板，渲染截图不是系统截图。

慢拥有者回调期间的真实监听停止与原owner回收也已有[限定证据](../reports/service-slow-owner-2026-09-21.md)：9项原生测试及严格Clippy通过，已开始操作不误报取消成功，排队操作不执行；同步回调仍占用执行线程。S0隔离原型现已通过6项测试，真实等待期间原HostRuntime可提交内容，见[证据](../reports/suspendable-io-s0-2026-09-21.md)；真实Runner现已采用[自有执行状态](../reports/owned-runner-2026-09-21.md)，公开驱动仍同步；[broker内部阶段拆分](../reports/broker-phases-2026-09-21.md)亦已通过117项相关运行时/故障注入与33项网络回归；[package/worker逐import驱动](../reports/owned-package-frame-2026-09-21.md)已接入并通过489项全量运行时/故障注入及33项网络回归；[受管HTTP等待与原拥有者调度](../reports/deferred-http-owner-2026-09-21.md)现已接入真实适配器，491项运行时、35项网络回归及严格Clippy通过；下一主线为[应用服务出站资源与调度验收](PLUGIN_SUSPENDABLE_IO_PLAN.md)（[入站服务与出站等待组合](../reports/service-outbound-wait-2026-09-21.md)的限定原生路径已通过），继而帧分段和TLS/出站资源。系统输入/真实剪贴板、更多编辑/capture、其它维护故障及外部效果核对仍待完成；局部通过不等于持久Unknown核对、完整恢复能力或SDK稳定。

2026-09-21[原生服务持久出站选择](../reports/service-outbound-selection-2026-09-21.md)已接入可信 Rust 启动入口：最多8个明确端点/修订，原实例重新批准，稳定引用与本次授权分离；请求历史绑定完整策略/凭据摘要。实际 Rust 测试插件完成入站→出站→完整响应，同配置重批准不重发，变更配置冲突；78项相关回归通过。现有私有协议/Flutter仍选择空集合，下一项为端点选择界面、原会话绑定、允许引用交付及运行中撤销验收；未重建Windows应用或宣布IO SDK稳定。

2026-09-21[服务出站端点面板与协议](../reports/service-outbound-ui-2026-09-21.md)已接通：Flutter明确多选最多8个当前插件端点，启动请求冻结引用/修订；刷新变化不替换旧选择，Unknown与重挂不重发。私有协议交给原生重新批准，实际Rust插件/真实HTTP测试通过；71项Dart组合、14项原生服务、4项语言资源通过。下一项为允许资源引用交付、运行中撤销/竞争和完整Windows窗口验收；本轮未重建完整应用或推送。

## 2026-09-21 允许资源目录接线

2026-09-21[服务允许资源目录](../reports/service-resource-directory-2026-09-21.md)已接通：声明 service-resources-v1 的包从独立、带摘要的目录获取实际批准端点/凭据引用、方法与限额；宿主剥离外来伪造头，旧包和空选择不接收目录。实际 Rust/Wasm 插件以目录替换错误请求体引用，真实 HTTP、同政策重放、变更冲突、撤权及原拥有者回收通过；79项相关测试与限定严格Clippy通过。下一项为完整 Windows 窗口出站/运行中撤销及停止竞争；未重建完整应用、未推送，公共 IO SDK 仍未稳定。
