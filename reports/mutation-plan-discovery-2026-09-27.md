# 持久原计划发现与原生任务（2026-09-27）

基线 `b9225f64f6c62584ad7243e30249d8a088bcb155`，分支 `codex/io-safety-refactor`。本轮补齐从已保存内容库发现文件变更原计划的 Core／runtime／Workbench 原生路径；没有变更持久化版本，没有新建第二套计划缓存。**私有发现启动协议、Dart 发现客户端及恢复 UI 尚未接入。**

## 当前实现

- Store 提供 `open_file_mutation_plan_cursor`／`read_file_mutation_plan_page`。游标只属于原 Store 实例，无 SQL 的打开操作不授予读取权限；每页检查 1–8 个全局候选并做一次有界前瞻，按 operation_id 键集推进。游标不公开其他主体的操作 ID。空页可以仍有后续结果。
- 先检查主体元数据，再验证最多三条小型不可变历史及包／能力／协议摘要，最后读取受保护 RequestRecord。SQL 文本和材料容器有大小上限，LZ4 解压也使用更小上限；没有在候选页读取大块暂存正文。
- 页只返回准确原计划，不附带阶段、效果或完整数据校验成功的承诺。用户选定计划后仍须走独立 reconciliation。新增测试故意损坏已暂存正文：发现原计划仍可成功，完整历史核对明确失败。
- 页内使用单个 SQLite 只读事务，页间不承诺固定快照；权限在 SQL 前、候选读取前、受保护材料读取前和最终交付前复核。读取或授权错误使游标失效，另一 Store 的游标在 SQL 前拒绝。
- runtime 新增独立 MutationDiscoverySession 及 open／next／close API，与文件目标会话类型分离。每 worker 一个游标，包身份从当前 ManagedInstance 派生，主体是可信宿主提供的筛选项。当前 Registry revision、能力、期限与取消在读取及交付边界继续检查。
- 在 Store 查询前按候选上限预留保守读取额度，页回复单独预留 192 KiB；仍受任务与包额度限制。上一页必须已成功交付，才能继续；回执丢弃或读取失败使游标失效，不允许悄悄跳过页面。显式重新打开从起点扫描，不自动恢复权限。
- Workbench 原生 `start_mutation_discovery`／NextPlans 接回原 owner 和停止／实际 join／ack 流程。发现任务只允许翻页与释放，不允许 Prepare／Execute／BuildPlan／CancelPlan。发现完成或失败终止游标；失败不等于文件效果不确定。
- 旧私有 MutationRead 在领取前拒绝原生发现页，防止尚未适配的客户端消费它；本轮不虚构已有 UI 入口或跨语言发现契约。

## 验证证据

| 范围 | 结果 | 日志 |
|---|---|---|
| Core 完整 fault-injection 回归 | 744 通过，0 失败，15 ignored | `build/mutation-discovery-core-full.log` |
| 最后补充正文／历史边界后的 Core 专项 | 6 通过，0 失败；与全量有重叠 | `build/mutation-discovery-core-final.log` |
| Windows runtime mutation_owner／opt_in／reconciliation | 19＋2＋4＝25 通过，0 失败 | `build/mutation-discovery-runtime.log` |
| Workbench 全量，三份现有真实 Wasm | 167 通过，0 失败 | `build/mutation-discovery-host-full.log` |
| 最后发现失败状态修正后的宿主专项 | 2 通过，0 失败；与全量有重叠 | `build/mutation-discovery-host-final.log` |
| Core Clippy | 既有 collapsible_if 例外下通过 | `build/mutation-discovery-core-clippy-scoped.log` |
| runtime Clippy all-targets | 既有 collapsible_if／let_and_return 例外下通过 | `build/mutation-discovery-runtime-agent-validation.log` |
| Workbench Clippy | 通过，12 条既有其他模块警告 | `build/mutation-discovery-host-clippy.log` |

Core 原始 `-D warnings` 在既有 `store/blobs.rs` 和 `transaction.rs` 的两条 collapsible_if 上失败，未顺带修改无关文件；失败日志保留为 `build/mutation-discovery-core-clippy.log`。测试汇总按各次执行报告，不能简单累加；全量忽略项包含由父用例启动的子进程入口。

覆盖：实际重开库与快照、稀疏分页、主体／包／能力筛选、外库游标、每个授权点拒绝、损坏容器与异常解压长度、无持久写入、无 OS 效果、原目标删除后发现、命令去重、旧协议不能消费页、丢回执失效／重新打开、期限过期、真实 owner 归还及 ack。

## 剩余工作

1. 为发现启动、翻页与一次领取制定私有协议及类型化 Dart 入口；继续与选中文件执行任务隔离。
2. 补可恢复变更会话及恢复列表 UI；从原计划进入完整核对，不从发现结果生成执行许可。仅重开 SQLite 的测试不等于跨进程 UI 恢复验收。
3. 对大库补预算／租约耗尽后的受控续扫：当前游标不可持久化，重新打开从头扫描，页间也不是稳定 census。当前固定上限消除了单次无界扫描，但未完成任意规模扫描的续期／检查点能力。
4. 补真实进程崩溃、审批撤销及资源压力组合验收，继续推进公共 C／C++／Rust SDK。Windows 条件替换仍明确 Unsupported，SDK 未冻结。

本轮使用原生多代理，未调用 SubagentBridge；没有提交、推送、安装包或 Release。
