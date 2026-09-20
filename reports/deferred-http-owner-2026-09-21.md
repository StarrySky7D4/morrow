# 受管 HTTP 等待期间的原拥有者调度

日期：2026-09-21。基线 `ac5b3c8`，分支 `codex/io-safety-refactor`；版本仍为 `0.1.9-test.52+56`。本轮为原生运行时与网络适配器源码改造，未重新构建 Windows 应用、推送或发布。

## 实现

- `BrokerRouter::begin` 为可信原生适配器增加同步／延后两种驱动选择，默认继续调用旧 `route`。guest 的 `morrow_io_v1.call` 字节契约与冻结 SDK 原包不变。
- `RouteContext::defer_dispatch` 在原拥有者上完成容量预留、受保护请求保存和唯一认领。它只存放自有执行任务；适配器返回 Deferred 后，worker 才启动传输。伪造 Deferred、返回与已准备任务不匹配的结果不会产生成功或启动该任务。
- 执行票据持有原请求、身份与 reservation。传输线程没有 HostRuntime／Store 的借用；实际观察回到原 worker 后，原 broker 核验原宿主、连接和票据，再保存响应及 Observed，最后复核交付权限。发送边界后的不确定性仍是 Unknown，不能自动重发。
- 等待期间只服务保留的拥有者命令通道。后续 guest 作业与 ServiceUpdate 不被越序消费；同步拥有者回调本身仍不可抢占。
- `TransportTask` 只有确认线程结束才 join 取结果；异常退出路径先取消再 join，之后才退休 live 记录。取消不是实际退出，也不回滚已提交的卡片或远端效果。回调 panic 时可能没有保存最终网络观察，但已落库的 Unknown 不消失。
- HTTP 适配器共用原授权、凭据、请求及额度准备逻辑；延后路径持有自有端点、请求、Tokio Handle 和原 HttpCallGuard。原响应解析及最终权限检查保留。

## 真实验证

新增两个传输生命周期单测：运行中的 poll 不交付；结果仅消费一次且不误取消；Drop 触发取消但要等真实执行退出。

新增两个真实 loopback TCP／managed Wasm 集成测试：

1. 服务器完整接收二进制 HTTP 请求后等待显式屏障。HTTP 尚未返回时，原拥有者命令在一秒内提交卡片并返回，job 仍 Pending；等待及未读 Ready 期间第二个作业均 Busy。释放服务器后得到正确响应和 Observed；关闭并重开同一原库，卡片和精确响应材料均可核对。
2. 原拥有者命令在卡片提交后 panic。worker 取消并 join 实际传输，再取回原 HostBinding；卡片保留，HTTP 意图为 OutcomeUnknown/ReconcileOnly，作业无成功交付。

最终检查：

| 范围 | 结果 |
| --- | --- |
| `plugin_runtime` 全量、`fault-injection` | 491 passed，0 failed；2 ignored 为由父测试调用的崩溃子入口 |
| `managed_http` | 20 passed |
| `managed_content_service` | 6 passed |
| `managed_service_owned` | 9 passed |
| 两包相关 lib/tests 严格 Clippy | 通过，`-D warnings` |
| 修改文件 rustfmt、diff check | 通过 |

日志位于忽略目录：`build/deferred-runtime-full.log`、`build/deferred-runtime-clippy.log`、`build/deferred-network-final.log`、`build/deferred-network-clippy.log`、`build/deferred-owner-integration.log`。

## 辅助编码与审核

GLM-5.3-flash/max 提供 TransportTask、HTTP 共用准备／延后路径和测试草稿；主代理完成接线与验证。实际修正了恒真断言、错误借用类型、端点类型，以及测试中的占位函数、虚构 API 和二进制请求解析。DeepSeek-flash/max 审阅等待／回收边界；其输出被长度上限截断，且把 Unknown 误解为可重试、忽略 Drop 的 join，不能作为通过证明。主代理依据源码和真实测试独立判断。

本轮曾因结果槽满拒绝一次审阅任务；通过官方 CLI 取消未执行的排队记录、读取已过期结果释放槽后重新提交。未修改服务数据库、取消运行中任务或重放未知结果。

## 下一步与边界

真实 HTTP 生产适配器现已接入可暂停路径；这不等于 S2/S3 的完整应用验收。下一步补入站持久服务调用中同时出站 HTTP 的组合路径、慢拥有者回调与传输完成／停止竞争、原应用业务路由和旧包兼容的专项核验，再进入大帧分段与应用服务 TLS／出站资源。

本轮不证明原 Windows 预览产物已更新、真实系统输入、完整服务并发调度、跨重启 Unknown 核对、全功能文件系统、三语言新 IO SDK 或其它平台资格。新 IO SDK 仍未稳定。
