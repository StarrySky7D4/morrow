# Agent 原生 owner 与可信 Rust 入口

2026-10-06，C16 本地实验性候选。接口位于 Windows `workbench_host::agent_tasks`；本页描述可信 Rust 调用路径，不新增 guest 管理权限或 Flutter 运行按钮。接口仍 experimental，未作为正式冻结 SDK 发布，未提交或推送。实际证据见 [C16 报告](../reports/reconstruction-2026-10-06/codex-sdk-c16.md) 与 [原始 validation](../reports/reconstruction-2026-10-06/codex-sdk-c16-validation.json)。

## 原 owner 与执行准入

`AgentContext` 持原 `SessionExecHost`、ProcessHost、BorrowedNativeResources、可选 BorrowedWindowsExecutionPort，以及原 executor `Arc<Connection>` 和 Admission。`AgentStart` 固定 package ID、完整包装 SHA、catalog/Manager 双 revision、存活期限和预算。静态审批只是 capability ceiling；连接仍须来自原 Manager、当前完整包装、fresh admission 与同一原 host 身份。

`Workbench::start_agent` 是可信 Rust 生产入口。它检查原 StateSlot 可写和 product gate、exact owner binding、现有 native 资源清理状态、期限和预算，并连接原 catalog/Manager。若附 native port，必须为同 owner 的 production port；普通 qualification port 不能进入此入口。会话层-only 上下文可以没有 port，但仍需 clean 原资源。普通测试使用泛型 AgentWorker 与普通 SQLite owner，不能当作真实 ProtectedSession 运行证明。

完整原 WorkbenchState 移到独立 OS worker；原 Core、Store、HostRuntime、Manager、ProtectedSession 和密钥 lease 不复制、不重开。独立 Tokio scheduler 仅驱动借用 native backend，没有第二 Core 或保护库 owner。旧 IoWorker carrier 审批和原 SDK/wire 合同保持原行为。

## 生命周期入口

| 可信 Rust 方法 | 行为 |
| --- | --- |
| `start_agent(options, context)` | 准入后移原 owner，返回 TaskKey；OS spawn 失败保存原 owner/context 与清理状态，不自动启动重试。 |
| `submit_agent(key, command)` | 有界提交单条操作，返回 AgentCommandHandle；不能依据 Unknown 重发写或控制。 |
| `poll_agent(key)` / `agent_status()` | 检查 task identity，观察清理和实际 join，返回分开的 execution/disconnect/maintenance 结果；不是无条件恢复可写。 |
| `cancel_agent(key)` | 请求原 lease、guest、executor 与 native stop，再观察；stop 不代表 exit、EOF 或 join 完成。 |
| `recover_agent(key)` | 显式重试同 owner 的清理/事实对账；不重放 guest、启动或不确定效果。 |
| `acknowledge_agent(key)` | 仅按原 StateSlot 清理/维护门禁确认已结束任务，不能跳过资源退役。 |
| `agent_scheduler_status()` / `reap_agent_schedulers()` | 观察和显式清理有界 scheduler 债务；不恢复丢失 owner 或执行授权。 |

单帧最多 128 KiB，最多 16 条命令，总预留最多 4 MiB，import 预算最多 16384，存活最长 60 秒；实际限额取调用者与原包/Manager 上限。每次 Wasm import 经 `run_owned` 调原 owner 维护，再复核原 managed/catalog/Core 存活权限；时钟采样前后也复验。catalog 变更、Manager disable/remove/升级、取消、stop 或原连接退役拒绝续用。

## native facts 与同步回收

BorrowedNativeResources 绑定原 HostBinding，同 owner 的 port 共用原 16 个 native 进程预留与事实 mailbox。真实子进程只由原固定 intent、review/claim、同 executor Admission 和 backend 启动。持久 wrapper 审批不直接批准任意命令。

取消后仍保原 owner 至真实 exit、stdout/stderr EOF、provider/jobs 退役、完整 facts 历史确认与实际 OS worker join。事实 CAS Conflict/Unknown 保留计费并停止自动重写，需要显式 recover；Unknown CAS 只读核对精确既有事实，不重放效果。没有 port 但已有 charged native 资源时，pre-spawn 返回原 owner 与清理上下文，零 guest import。

native clean 计数不是 Tokio task join。真实 worker join 后先释放 retired port/context，保留同 Runtime 的独立 anchor；仅在同步非 Tokio 线程 `Arc::try_unwrap` 成功后同步 Runtime Drop/shutdown。外部 Arc 未释放或 Tokio context 内收尾都保持 pending 原 owner。unused 上下文也用相同唯一所有权检查，不能因为 worker 尚未 started 或资源计数为空就丢 anchor。

scheduler active token/debt 上限 16，非唯一或不确定退出保留已预留 slot，显式 reap 在 native clean 且唯一所有权时回收；满额拒绝新启动。没有 detached 回收线程、无限 forget 或 timeout 假装已 join。同步 Runtime Drop 可能等待异步任务，不能承诺有界回收延迟。未完成事实或丢失保护 owner 的债务仍 OPEN，reap 不能重新赋权。

## 资格边界

当前三项组合测试使用原封存 Rust Wasm 和真实普通 Windows 子进程，验证 original owner 移动、写效果/失权 Unknown、charged 无 port 拒绝与 trusted 清理。环境明确无沙箱，owner 是合成普通 SQLite，非真实 ProtectedSession/DPAPI。可信生产 Rust 入口已接线并通过限定编译/预检查；真正桌面产品调用、保护库、生产沙箱/helper/network policy、Linux 与其他平台、完整 SDK26/G04 仍 OPEN。GUI 无执行按钮，不宣称 GUI 可执行或正式 SDK 已冻结。
