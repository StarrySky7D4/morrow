# 服务累计任务与字节预算验证

基线 `834f6809dcb642dee3f21a79db213256e0c308a7`，隔离分支 `codex/io-safety-refactor`；应用版本保持 `0.1.9-test.52+56`。**PASS_SCOPED**：在原实例运行账本中增加显式累计任务保留次数和宿主字节额度，并验证真实HTTP预算拒绝。续租和主应用常驻节点尚未完成。

## 实现合同

- `ServiceRunProfile.budget`使用可选tag 3，预算版本1声明max_jobs/max_bytes；与必需feature `service-run-budget-v1`双向匹配，仍要求父service-run-v1。六种已知feature可共存，未知或非规范嵌套编码拒绝。省略预算时旧profile编码保持一致。
- 声明最多1,000,000次任务保留，字节上限不超过原IoBudget。可信宿主通过 `Manager::bind_budgeted_service_run` 显式批准更小或相同的额度；预算错误、身份/修订错误、使用错误入口都不消耗首次签发。旧bind_service_run不能绕过新预算，新入口也不为旧profile猜测默认额度。
- 原IoContext保存累计任务数、原字节账本和获准额度。原Usage.jobs仍是并发量；新增IoBinding/IoWorker的service_run_usage只读取累计诊断，不产生权限。
- 累计任务数在成功保留任务预算时增加，包括独立IO准备及文件选择；不是成功API响应数。单独资源占用不增加任务数。取消、丢弃、结果读取以及保留后的入队失败不退款；保留之前的拒绝不计费。
- 四条字节路径均核对宿主上限：直接任务保留、worker任务准入、IO响应预留、增量/完成帧计费。累计任务耗尽不进入存活检查；已获准任务仍可在剩余字节额度内完成，已完整预留响应仍可读取。输入获准不承诺所有后续响应预算均足够。
- HTTP作业预算拒绝返回429及固定错误文本；报文本身超限保留原校验。没有虚构Retry-After或自动补充额度/重试。

这些是有限原型边界，不是百万次任务吞吐资格、长期压力测试或SDK稳定承诺。没有修改原请求短期限、原运行一次签发规则、已发生副作用的保守恢复语义或用户数据格式。

## 验证证据

| 范围 | 结果 | 本地证据 |
| --- | --- | --- |
| Core新预算、旧profile、IO包验证 | 25 passed / 0 failed | `build/run-budget-core.log` |
| Runtime绑定、作业、预算与响应预留 | 49个不同测试最终通过 | `build/run-budget-runtime.log`、`build/run-budget-runtime-fixed.log` |
| Runtime服务、内容权限、执行、固定文件后端回归 | 62 passed / 0 failed | `build/run-budget-runtime-regression.log` |
| Network plugin-adapter完整回归 | 112 passed / 0 failed / 0 ignored，文档编译通过 | `build/run-budget-network-final.log` |
| Windows受保护原Storage回收专项 | 3 passed / 0 failed / 0 ignored | `build/run-budget-storage.log` |
| Core、Runtime、Network所有目标严格Clippy | 均通过，`-D warnings` | `build/run-budget-core-clippy.log`、`build/run-budget-runtime-clippy-final.log`、`build/run-budget-network-clippy.log` |
| 冻结SDK与差异格式 | 36固定文件、13原Wasm/包对通过；rustfmt及diff检查通过 | 本地校验输出 |

共251项不同测试，其中新增17项：Core 7、Runtime预算7、响应预留2、真实HTTP 1。Runtime首组的42项通过和修正后的7项合并计49，未重复计入首轮失败夹具中的通过项。不是完整Flutter/工作台或全平台回归。

真实HTTP测试分别耗尽累计任务数和字节额度：第一项返回202及精确结果，下一项返回429，运行授权仍存活；原31秒监听回归也再次通过。真实Wasm用例验证最后一个获准任务在下一项被拒后仍正常交付，完成帧超过宿主额度报Limits。受控出站后端测试在精确字节上限读取已预留响应，在少一字节时后端调用次数为零；没有把合成后端当作真实出站网络资格。

## 发现与修正

1. 新测试夹具把task read_input容量写成65536，而ABI要求131072，导致成功用例报TaskProtocol。修正夹具容量及内存区域，并将完成帧超额用例收紧为具体Limits断言，防止以其他协议失败冒充预算测试。失败日志保留，7项复验全部通过。
2. HTTP适配器将JobError::Limit统一映射成413，导致合法请求在服务额度耗尽时被误报为报文过大。改为在作业准入处返回429；完整网络回归通过。首轮失败见 `build/run-budget-network.log`。
3. 严格检查发现一处嵌套条件可合并，已按Rust写法修正，Runtime和Network复验通过。初始静态检查日志保留。

独立只读复审确认四条记账路径、两条任务保留路径、首次签发、旧入口拒绝和诊断锁序，未发现必须修复的问题。入队失败不退款沿用原保留点的代码语义；本轮没有另外注入队列断连故障，不将其描述为新故障注入验收。

## 后续

继续[常驻运行方案](../docs/PLUGIN_SERVICE_RUNTIME_PLAN.md)：显式续租绑定原运行身份、当前修订和原累计账本；完整WorkbenchState、有界调度与长IO等待期间的界面响应；随后接入启动/停止/修复和实际用户路径。管理队列预留、Unknown证据核对、完整文件系统、三语言IO SDK和跨平台资格仍未完成。

本轮仅本地修改与提交，没有推送、发布、关机、Dart/UI变更或SDK原包重打包。使用内置子代理协助契约、测试与审查，没有实际调用DeepSeek。
