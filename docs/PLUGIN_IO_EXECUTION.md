# IO-C2：唯一活跃执行与恢复核对

状态：broker 层已完成本轮生命周期修正，验证范围见 [整合修正报告](../reports/io-safety-refactor-2026-09-19.md)，依赖 [IO-C1 受保护原件](IO_PROTECTED_EVIDENCE.md) 的存储与预留。本层在可信宿主会话内约束一个 operationId 只跨越一次持久发送边界，并让任何中断都只能走核对、不能走重发。托管 HTTP 作业已通过子调用预留接入，见 [接线报告](../reports/brokered-io-jobs-2026-09-19.md)。真实 HTTP／文件后端和资源授权属于 IO-D。

## 组成

- `plugin_runtime::io_execution::Broker`：内存中的 operationId → 活跃执行注册表。注册表不持久化，进程重启后由持久历史单独决定还能不能开始新的尝试。
- 每条活跃执行持有：该次绑定的实时 `IoBinding`（`plugin_runtime/src/io_binding.rs`）、独立并发作业租约或原作业子调用租约、主体与命令快照、以及 `Prepared`／`Dispatched` 状态。
- 持久侧复用 `io_intents` 历史与 `io_evidence` 原件：发送边界提交 `OutcomeUnknown`，成功后提交 `Observed` 并保留响应原件。
- 后端由可信宿主以同步回调提供；本层自身不产生任何网络、文件或凭据效果。

## 前置检查与请求匹配

`begin` 必须全部通过才建立活跃执行，且顺序保证先用实时权限、再读持久数据：

1. 命令包摘要必须等于当前托管包的真实摘要；提供的 subject 与 capability 必须等于命令自身声明；capability 必须在这次绑定的批准集合内，实例、管理器、主机与时钟都必须是当前代次。
2. 持久历史必须是该主体持有的、与该命令完全相等的 `Prepared` 记录；`OutcomeUnknown` 一律拒绝，`Observed`／`CancelledBeforeDispatch` 分别报告已终态。
3. 受保护请求原件必须已持久且与命令绑定一致：载荷摘要等于 `request_sha256`，字节数等于 `request_bytes`。缺原件报告 `EvidenceUnavailable`。
4. 在跨越发送边界前预留完整收尾容量（后续意图／审计事件与请求＋响应材料），预留幂等、按 operationId 绑定，并与其他写入共享同一逻辑字节额度。
5. 同一 operationId 已有活跃执行时拒绝，不建立第二个。

重复提交、并发绑定同一个 operationId 都只会得到一个执行；后到的请求得到 `Duplicate`，不会调用后端。

## 发送顺序与中断语义

`dispatch` 的顺序固定，任何一步失败都不会产生第二次外发：

1. 实时绑定与租约检查；已过期、已停止、所有权被替换的实例在触碰持久状态前就被拒绝。
2. 重新读取持久历史，必须仍是同一 `Prepared` 命令，且请求原件仍在。
3. **先提交发送边界**（`OutcomeUnknown`），再调用后端。
4. 调用前再次复核租约与实时代次，拒绝迟到代次执行。
5. 后端恰好调用一次。
6. 返回有界响应后保留原件并提交 `Observed`，这些是历史事实。交付前再次采样宿主时钟、核对原实例和活跃条目；停止、过期或退休后返回错误，不交付成功载荷。最后释放活跃执行；运行中的租约由持有者保留至回调结束。

`dispatch` 接受宿主时钟闭包，在入场、Store 最终提交授权、实际调用前、最终交付前分别采样。该新接口仍属实验性运行时 API，未修改冻结 guest SDK。

由此得到的中断分类：

| 中断点 | 持久状态 | 允许的动作 |
| --- | --- | --- |
| 边界提交前 | `Prepared` | 可用新的实时绑定重新开始，或取消 |
| 边界提交后、后端调用前 | `OutcomeUnknown` | 只能核对，禁止重发 |
| 后端报错或结果不明 | `OutcomeUnknown` | 只能核对，禁止重发 |
| 响应原件已保留、`Observed` 前 | `OutcomeUnknown`＋原件 | 核对时直接使用已保留原件 |
| 响应原件保留失败 | `OutcomeUnknown` | 核对摘要收尾，不假装有原件 |

后端返回超限响应、材料容量不足或保留失败都不会被当成成功；这些情况保留 `OutcomeUnknown`，由核对决定结论。

## 代次退休与迟到结果

- `maintain(now)` 退休已失效执行；`retire(operation)` 可显式退休并撤销交付。运行中的同步回调不可被强制中断，租约持续到回调返回，不能通过退休提前腾出并发槽位。
- 活跃执行持有原始绑定的身份快照，旧实例、旧连接或换主后的绑定都无法通过复核；即使执行已经开始，迟到结果也不会被当作当前代次交付。
- 重启后注册表为空，但 `OutcomeUnknown` 的历史永远不能开始新尝试；只有 `Prepared` 才能用全新实时授权重新开始。

## 恢复核对

`recovery` 返回纯历史分类（`AwaitFreshAuthorization`／`ReconcileOnly`／`AlreadyObserved`／`CancelledBeforeDispatch`）与是否保留请求／响应原件。它是诊断结论，不是授权，也不恢复任何实时句柄。

`reconcile` 只接受 `OutcomeUnknown`：

- 已保留响应原件：以原件摘要和 `OriginalResponse` 来源收尾，调用方不得给出不同摘要。
- 没有原件：调用方必须给出核对摘要；Store 释放从未接纳的材料预留，再以 `Reconciliation` 来源提交 `Observed`。已接纳的原件不会被删除，也不会被伪装成核对结果。

核对提交后历史终态化，活跃执行退休，材料预留归零。

## 预算

`begin` 为一次执行申请一个并发作业租约，按 `request_bytes + response_limit` 计入该实例的 IO 字节预算；累计字节只增不退，释放执行只归还并发作业槽位。Store 材料预留与 outbox 写入共享同一逻辑字节额度，真实磁盘空间仍由 SQLite 的 `StorageFull` 报告。

托管 `submit_brokered` 使用私有 `begin_in_job`，只接受原作业签发的单次 `IoCallLease`。响应完整帧额度及一个 resource 在发送前预留；父作业已经计 input/request/jobs，不再走独立 `begin` 重复计费。原作业取消、Manager 撤权和绑定到期贯穿 Store 提交前、实际回调前及最终交付。记录 Observed 后撤权仍保留事实，但拒绝载荷交付。

最终时钟回调在注册表锁外调用，之后重新核对条目与身份；允许时钟回调退休该操作，不会自锁。此约束与托管作业的纯取时时钟要求不同，后者见 IO-B2 文档。

## 仍未覆盖

出站持久资源批准与主应用、监听／服务发布与文件变更（IO-D1–D3）、真实后端效果核对来源与远端身份、录制式隔离回放（ROAD-08-IO）、凭据账户服务、按配额退休及三语言 SDK。原实例端点批准、凭据头注入与真实HTTP/TLS出站已由 [托管 HTTP](PLUGIN_MANAGED_HTTP.md) 接入；单独使用Broker不授予资源权限。
