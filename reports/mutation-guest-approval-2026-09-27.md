# 原 owner 文件变更审批凭证与一次性执行许可

2026-09-27，`codex/io-safety-refactor`，基线 `b9225f64f6c62584ad7243e30249d8a088bcb155`。用户明确授权受控创建／删除审批及临时目录验证后实施。本增量未提交、推送或发布。

## 已实现

- `issue_mutation_guest` 由可信宿主调用。必须先领取原选择回执；从 TargetBroker 的保留目标重建规范计划，并精确核对完整原件及解析字段。仅接受 Create／Delete，将原 worker、session、独立非零引用及计划 SHA-256 绑定到私有字段凭证。签发本身不写 Store、不执行文件效果。
- `authorize_mutation_guest_execution` 是第二个独立可信命令。要求已领取审批回执、精确计划摘要、原 Prepared 历史；Create 还要求实际持久内容的长度、内容哈希及请求摘要匹配。许可只有在成功回执被领取后才生效。
- 原有 Execute 入口对已加入 guest 审批的资源强制检查许可及当前授权，完成前置检查后、进入 Core claim／OS 路径前单向消费。重复执行、重新签发已用许可均拒绝。丢失审批／许可回执不能恢复成无审批的旧路径。既有未 opt-in 的可信 UI 流程保持不变。
- Prepare／BuildPlan 不能替换已批准的操作与内容；Query、派发前 CancelPlan、Release 保留原有语义。票据领取后的正常 cancel/drop 不误撤销已领取许可；未领取就取消仍拒绝。
- 每个 worker 整个生命周期最多签发 128 个引用。Release 不返还数量、不删除引用使用记录；第 129 个拒绝。这是明确的有限签发预算，达到上限后须正常结束原 worker 并以当前权限开启新任务，不能继承旧凭证。
- Workbench 私有 UI 协议明确拒绝两个新增内部响应，既不泄露引用，也不把审批响应编码成旧操作成功。旧 wire schema 不变。

## 验证

| 范围 | 最终结果 | 日志 |
| --- | --- | --- |
| Windows runtime 变更组合 | 33/33：owner 27（含新增 guest 7）、opt-in 2、reconciliation 4 | `build/mutation-guest-runtime.log` |
| Workbench mutation 回归，实际编译的原 Rust 工作台 Wasm 夹具 | 30/30；最终源代码重跑通过 | `build/mutation-guest-approval-host-final.log` |
| Runtime Clippy 原始严格检查 | 未通过：旧代码 3 处 collapsible_if、1 处 let_and_return | `build/mutation-guest-runtime-clippy-strict.log` |
| Runtime Clippy 限定上述两类例外 | 通过；不是原样严格全通过 | `build/mutation-guest-runtime-clippy-allowed.log` |

新增测试在临时目录实际验证非空 Create、Delete、取消保留原文件；覆盖无许可、错误摘要、未领取／取消回执、计划替换、跨 worker、期限失效、许可消费后重放、Release 后复用引用以及 128／129 次边界。Create 在 Prepare 前、Prepare 后及传块后但 Commit 前均不能取得许可，完整 Commit 后可显式授权并执行。

首次宿主运行有 29 项通过、1 项因缺少 `MORROW_WORKBENCH_WASM` 环境变量失败，原日志保留在 `build/mutation-guest-approval-host.log`。补齐现有真实夹具路径后通过，未跳过测试。首次新增原生断言试图在独占选择句柄释放前读文件，已移至 Release 后验证；没有放宽目标句柄限制。新 IssueGuest 计划采用 Box，消除本轮新增 large_enum_variant 警告；新增条件也已消除自身 collapsible_if。

独立只读审核未发现 P1 授权或重放绕过，并指出 128 次生命周期额度的可用性限制，已记录上述约束。验证未涉及用户实际文件。

复验命令：

```powershell
cargo test --locked --offline --manifest-path plugin_runtime/Cargo.toml --features packages --test mutation_owner --test mutation_owner_opt_in --test mutation_reconciliation
$env:MORROW_WORKBENCH_WASM = (Resolve-Path build/first-party-plugins/wasm32-unknown-unknown/release/morrow_workbench_plugin.wasm).Path
cargo test --locked --offline --manifest-path workbench_host/Cargo.toml --lib mutation -- --test-threads=2
cargo clippy --locked --offline --manifest-path plugin_runtime/Cargo.toml --features packages --tests -- -D warnings -A clippy::collapsible_if -A clippy::let_and_return
```

## 尚未完成

这只是原 owner 的可信控制入口，**未接通 Wasm guest import，也不是三语言 guest 文件变更验收**。下一步需要独立 import／协商、显式 mutation job、原 owner 暂停／恢复受检分派、预算及 submission 去重，再验证 Rust／C／C++ 真实 Wasm 链路与非空内容故障恢复。优先采用准备 job 结束、可信宿主审批、独立执行 job 的流程，避免等待自己的队列。Replace 及其他平台仍按既有限制处理，SDK 未冻结。
