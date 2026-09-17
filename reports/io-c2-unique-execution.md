# IO-C2：唯一活跃执行与恢复核对

日期：2026-09-17。结果：**PASS_SCOPED**。范围：`morrow-plugin-runtime 0.1.9-test.50` 新增 `io_execution` broker，依赖 Store 格式 17；核心新增材料核对释放 API。真实 HTTP／文件后端、异步作业与远端效果核对不在本次范围；主应用版本、冻结 SDK 原件与其它平台未改动；已推送 `track-a/w1-io-contract`（`d086eaf`），未发布。

## 实际交付

- `plugin_runtime/src/io_execution.rs`：每个 operationId 至多一个活跃执行；`begin` 完成实时绑定、命令匹配、请求原件匹配与全量容量预留；`dispatch` 先提交 `OutcomeUnknown` 发送边界、再调用后端一次、成功后保留响应原件并提交 `Observed`；`cancel` 仅限边界前；`recovery`／`reconcile` 只做历史核对；`maintain`／`retire` 回收失效代次。任何错误路径都不会产生第二次外发。
- `plugin_runtime/src/io_binding.rs`：`duplicate` 提升为 crate 内可见，供 broker 持有实时绑定快照。
- `plugin_runtime/Cargo.toml`：新增 `fault-injection` 透传特性，仅用于真实进程退出测试。
- `core/src/store/io_evidence.rs`：新增 `release_io_material_reconciliation`（仅 `OutcomeUnknown`、仅未接纳的原件、按 kind 释放）；`reserve_io_materials` 改为按 kind 幂等——已接纳原件的槽位不再要求预留，未接纳槽位可补预留，容量只按新增预留计入。
- 测试：`plugin_runtime/tests/io_execution.rs`（7 项常驻 + 1 项真实子进程退出，共 8 项通过、1 子进程忽略）；`core/tests/io_evidence.rs` 新增 `reconciliation_releases_only_never_admitted_quota`。
- 设计说明：[唯一活跃执行](../docs/PLUGIN_IO_EXECUTION.md)；[受保护原件](../docs/IO_PROTECTED_EVIDENCE.md) 同步核对语义。

## 退出证据对应

| 看板要求 | 证据 |
| --- | --- |
| 同 operationId 唯一活跃执行 | 第二个 `begin`（含不同绑定的并发场景）返回 `Duplicate`；后端调用计数始终为 1 |
| 请求匹配 | 历史缺请求原件 → `EvidenceUnavailable`；摘要被改写 → `Conflict`；主体／能力不符 → `Conflict`／`Denied`；存储命令不一致 → `Conflict` |
| 原代次退休 | 实例 `stop` 后 `dispatch` 在触碰持久状态前返回 `Denied`，历史仍为 `Prepared`；`maintain` 回收全部失效执行；新实例重新绑定后可重新开始 |
| 恢复核对 | 重启后新 broker 对 `OutcomeUnknown` 只能 `ReconcileOnly`，`begin` 返回 `OutcomeUnknown`；有原件用原件摘要，无原件用核对摘要并释放未接纳配额 |
| 重复提交不重发 | 成功后重复 `dispatch` 为 `NotFound`，`begin` 为 `Dispatched`；后端调用计数为 1 |
| 并发绑定不重发 | 同 operationId 第二绑定 `Duplicate`，无第二次后端调用 |
| 发送边界中断不重发 | 真实子进程在 `io-intent-after-commit` 退出：重启后历史为 `OutcomeUnknown`、无响应原件、响应配额仍持有、后端闭包从未执行；只能核对收尾 |
| 历史记录不恢复授权 | `begin` 必须有当前实例的实时绑定与批准；`recovery` 只返回分类；`OutcomeUnknown` 永远不能开始新执行 |

## 验证证据

| 检查 | 结果 |
| --- | --- |
| 核心全量 `cargo test --offline` | **412 通过，0 失败**（含新增核对测试） |
| 核心故障注入 `cargo test --offline --features fault-injection` | **451 通过，0 失败** |
| 运行时全量 `cargo test --offline --features packages` | **278 通过，0 失败**（+7 IO-C2） |
| 运行时故障注入 `cargo test --offline --features fault-injection` | **279 通过，0 失败**（含真实子进程退出） |
| 核心 clippy `cargo clippy --offline --all-targets --features fault-injection -- -D warnings` | 无警告 |
| 运行时 clippy（`packages` 与 `fault-injection` 两种特性） | 无警告 |
| wasm32 库编译（core 与 plugin_runtime） | 通过 |

日志：`build/io-c2-core-default.log`、`build/io-c2-core-fault.log`、`build/io-c2-runtime-default.log`、`build/io-c2-runtime-fault.log`、`build/io-c2-test3.log`、`build/io-c2-crash-test.log`。

## 审查发现与修复

- `reserve_io_materials` 原先在任一原件已接纳后拒绝整次调用，导致“先存请求原件、再为响应补预留”的合法流程失败；改为按 kind 幂等并只对新增预留计费。
- `reconcile` 原先固定尝试释放请求与响应两个槽位，遇到已接纳的请求原件会误报冲突；改为先判定每个 kind 是否已接纳，只释放未接纳的槽位。
- 测试夹具时钟在实例重启后回退触发 `Clock`；改为显式按递增时钟重新绑定。

## 产物与边界

核心数据库格式不变（仍为 17）；broker 为纯内存注册表，无独立二进制产物变化。仍未完成：真实第三方 HTTP/HTTPS 与文件系统后端、guest 发布授权路由、异步 submit/poll/read/cancel 与完整任务输入、远端主体与凭据、录制式隔离回放、按配额退休与 GC、主应用 UI 与三语言 SDK。IO-C2 只保证“一个 operationId 至多一次真实外发且中断只走核对”，不表示任何插件已经获得资源授权或能完成真实调用。
