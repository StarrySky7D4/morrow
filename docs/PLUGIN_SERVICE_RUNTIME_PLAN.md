# 常驻服务运行与工作台调度实施方案

基线：`834f6809dcb642dee3f21a79db213256e0c308a7`；2026-09-20。本文区分已实现的完整所有权、有限服务运行租约原型，累计任务/字节预算，以及仍待实现的续租和工作台调度。主应用常驻节点尚未完成。

## 当前限制的具体来源

| 位置 | 当前行为 | 不能直接当成常驻方案的原因 |
| --- | --- | --- |
| `core/src/plugin_package/io.rs` / `core/schemas/io_manifest.proto` | IO v1声明包含最长30秒 `max_duration_ms` 和累计字节额度 | 旧包同一字段约束绑定寿命与单作业期限；新service-run-v1仅通过独立profile延长运行 |
| `plugin_runtime/src/io_binding.rs` | 旧绑定按IO声明限期；service-run-v1在原IoContext签发一次有限运行，原请求上限不变 | 重复绑定不应改变同实例账本；释放作业只返还并发容量，不退款 |
| `plugin_runtime/src/io_jobs.rs` | `spawn_session_owned` 从声明建立单作业timeout，`submit_routed`校验；工作线程同步执行一项guest/broker调用 | 监听长期运行需要独立运行期限；仅添加UI队列仍可能等待当前阻塞请求结束 |
| `network_node/src/managed_service.rs` | 监听监督和执行worker有各自退出路径 | socket关闭不证明worker已退出；必须分别观察并真实join |
| `workbench_host/src/lib.rs` / `io_tasks.rs` | Storage交给IO worker，但Pool、Manager、undo、附件暂存和UI会话仍在Workbench调用线程 | 在线内容操作不能通过第二个Store绕过独占，也不能只给worker加原始读卡片请求 |

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

显式续租、管理队列预留与工作台调度仍待实现；累计预算没有自动续租或补充额度路径。

新的声明、宿主批准与实际运行准入分别保存并核对以下维度：

- **运行租约**：原run身份、管理修订、配置摘要、包摘要、单调期限和绝对期限；真实原Store的发布/认证有效期仍是更早的上限。
- **每请求上限**：执行时长、请求/响应/帧大小、host调用数；不因监听寿命延长而放宽。
- **并发容量**：排队、执行、Ready未取走结果分别有界；UI管理命令预留容量。
- **累计额度**：在原实例账本上累计的作业/字节总额。取消、读取、释放、续租、换路由都不清零，不以新worker掩盖累计消耗。

固定有限运行租约和显式累计预算已实现，下一步实现保留原身份及账本的显式续租。续租必须绑定同一run、原账本和期望修订，核对当前Manager/配置/认证批准；以新的绝对累计上限和期限替换批准，不把“增加额度”解释为抹去已经使用的额度。自动轮换绑定、时钟回退、过期后复活及跨run转移批准均拒绝。续租额度修改尚未实现；后续长期数值上限需经原型测量确定并版本化，不能把当前有限原型当作续租、管理队列和工作台共存均已完成。

需要同时修改并测试的入口：包声明验证、Manager准入、IoBinding时间/计费、IoWorker每请求deadline、ListenerGrant/ServiceGrant及持久授权解析。只改其中一处会造成旁路或仍在30秒后失效，不作为完成。

退出证据：同一服务实际超过旧30秒窗口仍可接新请求；单个请求仍按原短期限停止；到期、撤权、额度耗尽、时钟回退与重启均拒绝恢复旧授权；续租前后的累计账本连续。采用可控单调钟做边界测试，同时保留至少一次真实持续监听测试。

## 三、把完整工作台状态放入单个执行者

抽出 `WorkbenchState`，直接持有原Storage、Pool、Manager、内容会话、undo、附件/导入暂存与capture状态；不包含指向自身的worker句柄或可空StorageSlot。它实现HostOwner，runtime始终来自同一Storage。外层Workbench成为运行状态与命令回执的管理者。

工作台层定义有界且完全拥有参数的命令，分别覆盖Page/Read、Create/Apply、Query、Preferences、Capture、Import/Export及管理操作。Create/Apply继续走现有 `run_observed`、Pool、逐对象授权和证据提交，不允许直接写Store替代。响应是拥有的结果或受控结果句柄，不跨线程传递借用、指针或临时UI对象。

Runtime通过独立可信owner消息trait调度，避免反向依赖Workbench；该入口不导出给guest，也不复用远端service principal授权。输入、排队及Ready结果均计容量，准入失败与已提交但回执未知分开。服务请求不能耗尽工作台管理保留队列，停止/撤权可在队列外即时阻断新的交付。修复仅重试断连或封存，不重跑业务命令。

第一步允许同一执行线程串行处理UI和服务命令，这是过渡阶段，不宣称即时响应。下一步需要将长耗时网络等待和guest续执行改为可暂停的作业阶段，使原宿主在等待期间能处理其他已授权命令；不能在仍持有 `&mut HostRuntime` 的同步guest/broker调用中重入工作台。保留命令顺序、operation身份及最终授权检查，不能为了交互响应复制Runtime或数据库。

退出证据：服务运行时普通内容浏览/编辑、附件和设置修改可完成；请求洪泛下UI命令有界等待；内容命令与入站操作竞争仍受原修订/权限/审计控制；停止、关闭、已提交后取消和未知回执均可解释。仅返回Busy或等整个服务关闭后再访问不满足最终产品要求。

## 四、启动、停止与恢复顺序

1. 原库加载配置/批准，验证实际原包、原instance、对象grant及新运行profile；创建服务run身份。
2. 移交完整WorkbenchState；spawn失败恢复 `SpawnFailure.owner`，再清理其原instance。适配器选项失败从 `ServiceHostFailure.worker`回收。
3. 绑定经过批准的地址与TLS；端口冲突或取消启动必须归还原owner。Tokio runtime持续存活到监听和worker都结束。
4. 停止时先阻断新请求、撤权与取消，再分别等待监听监督、worker真实join；不以五秒超时或socket关闭替代证明。
5. 恢复原owner，报告执行/断连/封存各自状态；故障保持可修复，CLI EOF沿相同路径等待。
6. 接入主应用启动/状态/停止/修复页面，验证实际认证HTTP/TLS、内容读写、持久请求查询、撤权、端口冲突、故障重启与工作台共存。

本方案不改变Unknown持久证据核对、文件系统后端、三语言IO SDK和各平台资格的原门槛。其余网络能力也不会因常驻监听子项通过而一并标为完成。
