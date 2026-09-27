# 文件计划显式持久取消（2026-09-27）

在 `codex/io-safety-refactor`（HEAD `b9225f64f6c62584ad7243e30249d8a088bcb155`）继续补原 owner 文件变更任务。源码、测试和记录保留在本地；本轮未提交、推送或打包。

## 行为

新增可信宿主 `IoWorker::cancel_mutation_plan(MutationSession)` 与 `MutationResponse::PlanCancelled(Record)`。它通过原队列、原 Manager／Runtime、当前实例权限执行，调用已有 Core 历史事务将精确匹配的 Prepared 转为 CancelledBeforeDispatch；不新增数据库格式，不触碰选中文件。重复显式请求返回同一取消记录，不追加事件。

这与 `MutationHandle::cancel`（取消一次命令／交付）、`release_mutation`（释放当前选择与内存）不同。未准备的选择返回 Missing；已派发或已观察的历史返回 AlreadyDispatched，不重写结果，不自动重试文件效果。

确认持久取消后立即擦除未提交 spool，保留已入库正文与审计回执，阻止此会话继续分块、提交和执行；目标句柄仍遵循显式 release 或 worker 实际退出的原有规则。查询识别到持久取消也清除未提交 spool。取消回执丢失只报告外层 Unknown；再次查询可以核对真实取消结果。数据库提交不确定仍须核对，不能将错误视为回滚。

历史读取前核验原绑定和配额。查询与追加事务分别完整校验历史，因此分别预留正文成本，顺序释放并发 job 租约，既不免除第二次读取费用，也不把单作业限额减半。取消仍受当前权限与累计预算约束；配额耗尽时不能绕过核验。跨重启的历史读取不恢复现场授权。

## 验证

| 范围 | 结果 | 日志 |
|---|---|---|
| Runtime lib＋九个相关集成目标 | 232 通过，0 失败，6 ignored | `build/file-plan-cancel-runtime.log` |
| 最后一次预算修正后 mutation_owner／opt-in 专项 | 16 通过，0 失败 | `build/file-plan-cancel-final.log` |
| Runtime lib＋上述两个专项 Clippy | 通过，保留既有 collapsible_if 例外 | `build/file-plan-cancel-clippy.log` |
| workbench_host lib 编译检查 | 通过，5 条既有 dead_code 警告 | `build/file-plan-cancel-host.log` |

分项有重叠，不累加。232 项组合运行早于最终预算修正；最终专项与 Clippy 针对修正后代码。定向 rustfmt、git diff --check 通过。本轮未修改 Core，未重新执行 Core 全量；上一轮 739 项结果属于上一轮证据。

新增四项真实 Windows 集成测试涵盖：未持久／已持久正文取消，丢取消回执后的只读核对，重复取消不增事件，取消释放持久预留，关闭重开仍保持同一取消记录，取消后拒绝继续写入，已观察删除不能改写，Delete／不支持的 Replace 取消不改变原文件，以及 foreign worker 在入队和计费前拒绝。所有文件与数据库均使用临时目录。

## 剩余

目前仍为可信原生宿主接口；尚未接 Workbench 用户任务协议、Dart／Flutter 控制与 C／C++／Rust guest SDK。分块显式恢复／重置、跨 worker／跨重启 Unknown 业务核对、取消事务的专项进程崩溃测试仍待完成。既有 Core 通用取消事务有持久／快照测试，不将其视为本轮端到端崩溃资格。SDK 未冻结，Windows 条件替换仍明确不支持。
