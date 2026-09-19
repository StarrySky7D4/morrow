# 托管 IO 作业准入与交付

日期：2026-09-19。基线：`ac3e8274260dd51f6a710279475ef9402d37df5d`；隔离分支 `codex/io-safety-refactor`。本轮继续 IO-B2 的真实托管接线，完整插件网络／文件执行链仍未完成。

## 已实现

- `IoWorker::spawn_managed` 在调用线程核对 Manager、HostRuntime、完整 ManagedInstance 与原始 IoBinding。错误身份在时钟回调之前拒绝，工作线程持有完整实例，Manager 留外侧。
- 管理器收窄 IO 批准、停用、移除或释放会撤销原实例控制。与后台同步回调之间没有跨调用持有的 Manager、作业状态或时钟锁，阻塞回调不会阻止宿主撤权。
- 新私有 IoJobLease 使用原实例 IoContext。排队时按完整输入计 jobs/bytes，请求与响应增量计费，Ready 与 ReadBound 继续持有共享并发槽。read/drop 归还 job，累计 bytes 不退；丢弃执行中句柄须等真实回调退出才释放。
- 运行中调用和最终 read 使用原绑定时钟域重新校验；取时与校验串行，实际实例取消与每个作业取消相连。过期托管执行器进入收尾。
- 以真实请求检查能力交集：读取类要求 FileRead，HTTP要求HttpRequest，非空凭据引用额外要求CredentialUse。失败请求不进入可信Router，也不产生额外请求／响应计费；已准入的任务输入仍保留计费。
- JobReport 新增 http_response，与原 response 互斥，HTTP状态／响应头／正文按原提交帧验证，头与正文纳入领取大小限制。失效交付清除两种结果。

## 实际验证

新增 `plugin_runtime/tests/io_jobs_managed.rs`，使用真实 Catalog/Registry/Manager/ManagedInstance 和实际 Wasm 调用，外部后端仍为明确标记的可信脚本。12项覆盖：身份错误、审批与凭据能力交集、共享额度竞争、精确增量计费与不退款、Ready撤权／到期／Manager释放、HTTP结果与ReadBound、阻塞期间立即撤权、未结束调用的槽位保持、响应超共享限额后的Unknown。

首批9项托管＋17项原作业专项已通过。最终运行时全量（all-features、release、locked、offline）**316通过、0失败、1个由父测试调用的子进程入口ignored**，日志 `build/managed-io-runtime-full.log`。12项托管回归包含在全量内，不重复累加。全目标严格Clippy（-D warnings）通过，日志 `build/managed-io-clippy.log`。默认wasm32库检查通过（仅编译边界），日志 `build/managed-io-wasm.log`。冻结SDK完整性36文件／13原包对再次通过，日志 `build/managed-io-sdk-baseline.log`；9项旧包执行＋3项依赖执行也包含在全量内。结论为 **PASS_SCOPED**。

## 接线边界与下一步

**这已经是实际管理器与实例共享配额的接线，但还不是完整资源执行链。** IoBinding只给能力类别与实例配额；路径、origin、方法、凭据身份、监听地址仍需资源批准。现有Router仍为低层可信接口，本轮没有声称发起真实网络／文件效果。

下一步实现不可伪造的作业子调用预留，并由Broker接受、消费该预留。每次操作需要绑定operationId、实际包和请求摘要；资源与最坏响应容量在外发之前预留，避免把现有Broker.begin嵌套进作业后重复计job/bytes。然后将请求材料、OutcomeUnknown发送边界、Observed事实与最终交付相连，再引入真实HTTP/API节点/文件后端。

本轮不修改固定schema或冻结SDK，不改Store格式、应用版本，不迁移用户资料。低层spawn为既有可信入口，产品托管路径应使用spawn_managed；没有将历史396/468等核心测试或宿主证据计作本轮重复执行。源码留在隔离分支，未合并开发线、未推送和发布。
