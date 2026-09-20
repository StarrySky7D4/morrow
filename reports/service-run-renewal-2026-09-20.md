# 服务运行显式续租验证

基线 `89294fdef0b84fc08e720017bb4da4a83b6a97c8`，隔离分支 `codex/io-safety-refactor`，应用版本保持 `0.1.9-test.52+56`。**PASS_SCOPED**：本阶段实现原包声明内的可信宿主显式续租，不提供无限常驻或自动扩额，也没有主应用监听入口。

## 实现合同

- `IoWorker::renew_service_run` 与 `ServiceHost` 同名入口接受原Manager、原ServiceGrant、期望Registry/运行修订、新绝对截止和累计任务/字节上限。只支持已批准的预算profile；无新增schema、guest权限或HTTP管理路由。
- 原实例Control、连接、包摘要、当前启用与批准能力重新验证。错误Manager/grant在采样原时钟前拒绝；当前grant再检查自身撤销、监听器和独立授权。配置解析的grant具有原Store配置/认证/发布探针；手动签发的低层grant仍由可信宿主负责明确撤销。
- 同一运行从修订1开始，以期望修订比较并更新；过期修订、无变化、缩小和超声明更新拒绝。累计用量不清零、不退款；扩大批准使用绝对总额，不能跨运行转移。
- 首次签发的tick和Instant固定最长窗口。续租的新期限仍从该起点计算，所有旧绑定副本读取共享期限，包括文件资源清理。预算profile没有新的滚动时长；旧非预算profile不能借续租升级，重复绑定仍拒绝。
- 原worker的准入/停止锁覆盖续租，随后取原时钟及原IoContext锁。停止、排空、管理撤权、时钟回退和到期均不能通过续租恢复。每请求截止、已取消结果、认证/发布、出站资源和历史结果期限保持独立。
- `service_run_snapshot`只暴露运行修订、当前截止、获准上限和累计诊断，不构造或恢复授权。文件资源清理仍是原有非永久失效的清理观察，不能使用跨绑定清理时钟污染其他运行；真正存活与续租检查保留永久失效语义。

## 验证

| 范围 | 结果 | 本地证据 |
| --- | --- | --- |
| Runtime旧绑定、托管作业、累计预算及响应预留 | 49 passed / 0 failed | `build/run-renewal-runtime-regression.log` |
| Runtime服务、内容权限、执行与固定文件后端 | 63 passed / 0 failed；1个崩溃子进程入口ignored，由父测试实际调用 | `build/run-renewal-runtime-final.log` |
| Runtime新增续租专项 | 12 passed / 0 failed | `build/run-renewal-runtime-fixed.log` |
| Network plugin-adapter完整回归 | 115 passed / 0 failed；文档编译通过 | `build/run-renewal-network-final.log` |
| Windows原受保护Storage的服务退出与回收 | 3 passed / 0 failed | `build/run-renewal-storage.log` |
| Runtime、Network所有目标严格Clippy | 均通过，`-D warnings` | `build/run-renewal-runtime-clippy.log`、`build/run-renewal-network-clippy.log` |
| Runtime默认wasm32库 | 编译检查通过；不包含原生续租模块，不作为Web服务资格 | `build/run-renewal-wasm-check.log` |
| 冻结SDK原件完整性 | 36固定文件、13原Wasm/包对通过 | `tool/verify_plugin_sdk_baseline.py`本地输出 |

共242项不同测试最终通过，其中新增15项（Runtime 12、HTTP 3）。崩溃子进程入口不重复计数。所有Rust测试使用release；Runtime使用all-features，Network使用plugin-adapter，Storage使用all-features下的 `--lib storage::service_tests`。本轮没有Flutter或完整工作台回归；差异格式与所改Rust文件的rustfmt检查通过。

真实HTTP新增三项：同一socket与原实例在旧期限后执行第二个不同请求，累计任务从1到2且字节连续，最终额度耗尽返回429；配置/认证/发布失效拒绝续租且关闭原监听；续租后1ms请求期限仍返回503。原31秒持续监听回归再次通过。专项用可控单调tick验证旧授权期限边界，不能把它写成额外的真实长时间运行；真实Instant固定窗口另有3秒专项。

Runtime新增12项还覆盖：旧监听/服务副本、Ready交付、固定文件字节资源清理、两个并发同修订更新仅一个成功、错误身份不采样调用方时钟、预算/期限收缩及超限拒绝、管理禁用/撤回后再启用不能恢复、停止/排空和独立请求截止。

## 验证中修正

1. 新HTTP夹具的时钟偏移字段初次放错到配置辅助函数参数，编译失败。移到实际Running结构并完成完整网络回归。首轮输出保留在 `build/run-renewal-http.log`；不是运行时续租缺陷。
2. Runtime专项最初只允许Clock/Expired错误，后台10ms监测可能先观察坏时钟并停止实例，此时续租正确返回Denied。测试接受这三种拒绝，仍验证初始运行有效、期限/用量未更新、回退后不能恢复。错误身份的采时断言改为只计调用测试线程，保留后台独立监测，避免用全局回退tick制造无关竞态。首轮11通过/1失败保留于 `build/run-renewal-runtime-final.log`，修正后12项全通过，不重复累计先前通过项。

独立代码复审未发现阻断问题；复审与实际测试分别记录。没有改变用户持久数据、冻结SDK原包或旧profile编码，也没有将有限续租解释为全平台资格、SDK稳定或服务期间工作台已可交互。

## 下一步

按[常驻运行方案](../docs/PLUGIN_SERVICE_RUNTIME_PLAN.md)抽出完整WorkbenchState，连同原Storage、Pool、Manager、undo、内容会话及暂存交给原执行者；增加有界管理命令和容量预留，再处理长IO等待期间的调度与工作台共存。随后接主应用显式启动、状态、停止、修复及HTTP/TLS用户路径。Unknown证据核对、完整文件系统、三语言IO SDK及平台资格保持原门槛。

本轮仅本地编码、验证与提交，没有推送、发布或关机。使用内置子代理协助绑定实现、测试和只读审查；没有实际调用DeepSeek。
