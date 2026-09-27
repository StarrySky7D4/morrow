# 原工作线程退出后的只读变更核对（2026-09-27）

在 `codex/io-safety-refactor`、基线 `b9225f64f6c62584ad7243e30249d8a088bcb155` 继续补文件变更底座。本轮添加 Windows runtime 与 Rust Workbench 的独立历史核对任务；没有添加 UI 或 guest SDK 入口。

## 行为

- `IoWorker::reconcile_mutation(RequestRecord)` 不需要现场 MutationSession，不构造 TargetBroker，不打开目标路径。完整原计划先与持久命令逐项匹配，再验证内容及证据；缺失历史明确返回空。
- `Workbench::start_mutation_reconciliation(StartOptions, RequestRecord)` 要求上一任务真正 join、必要恢复及 ack，并从已归还的同一内容库 owner 重新准入。重新检查当前包摘要、启用状态、IO 权限与 Registry revision；旧审批摘要仅作为原计划身份，不恢复原目标执行权。
- Reconciled 保留 Prepared／Unknown／Observed／Cancelled 阶段。仅 Observed 读取与原观察摘要一致的 Core 效果原件，区分系统成功与 OS 拒绝。读取并不解决仍为 Unknown 的外部事实，也不新写观察记录。
- 读取预算在进入 Store 前预留，两次验证正文分别计费；领取仍检查实时授权。独立核对任务没有选择会话，不能 Prepare／Chunk／Commit／Execute／CancelPlan／Release；领取成功或失败后请求停止，实际 join 和维护／断开规则保持原样。
- 既有私有回复编码新增状态 kind=8、结果 kind=10，支持结果原件和效果投影；没有新增私有启动动作。本轮无需重新生成 Schema 绑定，详见 [协议边界](../docs/PLUGIN_MUTATION_WIRE.md)。

## 审查与验证

只读审查发现查询原先仅计正文或响应的较大值，漏计请求与另一类证据。现按 `content_length + request.container().len() + MAX_RESPONSE_BYTES` 的 checked sum 预留；Observed 第二次读取重新计费。它沿用现有执行链的逻辑字节口径，不声称是所有容器／解码内存的精确峰值计量。严格 Clippy 另发现 Reconciled 枚举增加了大块内联存储，已将效果原件改为 `Option<Box<MutationOutcome>>`，不压制新警告。

| 范围 | 结果 | 证据 |
|---|---|---|
| Windows Workbench lib 全量，真实三份 Wasm | 159 通过、0 失败、0 ignored | `build/mutation-reconciliation-host-full.log` |
| Workbench lib Clippy | 通过；12 条其他模块既有警告 | `build/mutation-reconciliation-clippy.log` |
| Runtime 组合回归 | 232 通过、0 失败、2 ignored | `build/mutation-reconciliation-runtime.log` |
| Runtime lib＋新增专项严格 Clippy | 通过，保留既有 collapsible_if 例外 | `build/mutation-reconciliation-runtime-clippy.log` |
| 定向 rustfmt、git diff --check | 通过 | 本轮工具结果 |

新增工作台六项集成用例覆盖实际创建后删除目标目录、Prepared、Cancelled、模拟持久 claim 后的 Unknown、空历史、原计划失配／包摘要／过期 revision／停用权限。额外检查未 ack 拒绝、状态位和禁止后续效果操作。协议投影用例覆盖 Unknown、空历史与 Observed OS 拒绝；其中 Unknown 是测试直接写入 Core claim，不是强杀进程的崩溃资格。

Runtime 新增四项核对测试覆盖空历史／异包摘要、预算先行拒绝、过期准入拒绝及结果 Ready 后到领取前过期的最终拦截。早期过期测试误以为可以先入队再收到域错误；实际队列在准入时正确返回 Closed，已修正断言并单独增加 Ready 后过期测试。组合测试使用 `--features packages`，没有启用 fault-injection；不与旧报告不同 feature 的测试数量直接比较。

## 剩余任务

下一步将重新授权核对接入私有启动协议及类型化 Dart 会话，持久保存／发现原计划身份，覆盖跨进程重启与真实崩溃，并完成 UI 的审批／进度／结果闭环。接着推进 C／C++／Rust guest SDK 以及正文分块恢复。Windows 条件替换继续明确不支持，不降级覆盖；SDK 尚未冻结。

本轮不提交、推送、打包或发布；原生测试不代表其他平台、真实进程崩溃或 UI 验收。
