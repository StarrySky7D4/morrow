# Workbench 原生文件变更任务（2026-09-27）

在 `codex/io-safety-refactor`、基线 `b9225f64f6c62584ad7243e30249d8a088bcb155` 继续接线。新增 Windows `io_tasks::mutation`，复用原 WorkbenchState／Manager／HostRuntime 的移交、维护、断开和真实 join。未新增 Store、后台任务系统或 guest 路径授权。

## 任务接口

- `start_mutation(StartOptions, Selection, SelectionScope)`：可信宿主显式传入已批准 scope；能力集合必须精确匹配 Create／Delete／Replace，选择种类必须匹配。随机选择 secret 由宿主生成；实际打开在原 worker，准入不进行文件 IO。不把任意 PathBuf 冒称为原生选择器授权。
- `request_mutation(TaskKey, Action)`：Prepare、Chunk、CommitContent、Execute、Query、CancelPlan、Release；一次最多一条待完成／未领取命令。Mutation 与既有 File 任务保持开放，不进入普通一次性 IO 的 drain。
- `mutation_status`／`read_mutation_result`／`cancel_mutation_command`：命令 ID 从 1 递增并绑定 TaskKey，领取与取消必须匹配原命令，拒绝迟到请求消费下一条回复。读取得到结构化 Delivery 或 Target 错误，不丢失外层交付不确定性。
- 失败后限制可能产生效果的动作，只允许明确查询、计划取消或释放；Query 确认 Prepared 后才解除不确定状态。OutcomeUnknown 保持待核对；Observed／Cancelled 是明确终态，仍可查询或释放，不冒充“尚未解决”。
- 成功领取 Released 才自动请求停止；`cancel_io` 可显式停止任何当前任务。只有原 worker 真实退出、维护／断开成功后才能 acknowledge。失败不自动重新选择、提交、执行或打开第二份内容库。

Chunk 用 `Zeroizing<Vec<u8>>` 传入，pending／过期任务／状态拒绝等提前返回也擦除所持缓冲；交给 runtime 时转移原分配，避免额外正文副本。Prepare 使用 Box，避免所有 Action 都占用大请求体的栈空间。

## 丢回执的两种选择边界

原生选择回执被取消或丢失时，runtime 会回收未交付的选择，Query／Release 可能返回 Missing；仍可 cancel_io→实际 join→ack 后重新选择，不能声称可恢复旧句柄授权。

宿主已经成功领取 Selected，但上层回包丢失时，Task 保存 reference／expected_identity，可从状态快照恢复。它们只表示当前选择元数据，不是跨重启身份或新的执行权限。其余持久命令的结果由原计划历史核对，查询不重放文件效果。

## 验证

| 范围 | 结果 | 日志 |
|---|---|---|
| 三份实际 Wasm：workbench、http-forward、service-outbound | locked/offline release 构建通过 | `build/mutation-task-guests.log` |
| Workbench lib 全量，设置上述真实 Wasm 路径 | 147 通过、0 失败、0 ignored | `build/mutation-task-host-full-final.log` |
| 最终状态字段调整后的变更专项 | 4 通过、0 失败 | `build/mutation-task-tests-final.log` |
| 最终 Workbench lib Clippy | 命令通过，其他既有模块仍有 12 条警告；新增模块无诊断 | `build/mutation-task-host-clippy-final.log` |

147 项包含原文件、HTTP、服务、TLS、owner 移交、查询和草稿等 lib 测试；最终专项与全量重叠，不累加。全量运行早于最后一次 terminal／reconcile_required 区分，之后对该变更执行专项和 Clippy。定向 rustfmt 与 git diff --check 通过。

初次全量执行缺少三份 Wasm 环境变量，60 项失败；补构建和明确路径后完整复验通过。严格 Clippy 首次发现本轮 Action 大枚举及 12 条既有告警；大枚举已修复，未通过禁用该 lint 掩盖新问题。最终普通 Clippy 保留既有 dead_code／collapsible_if 例外，不能称全仓严格 Clippy 零警告。

四项新增测试使用真实 Windows 受保护存储和临时目录，覆盖 70,001 字节分块创建，效果后取消交付得到 Unknown，显式查询恢复 Observed、禁止重复执行，原 owner 恢复，计划取消不删除源文件，旧命令 ID 拒绝领取／取消新回复，准入错误不移走 owner，原生选择丢回执后的明确退出。

原生子代理只读审查指出并修正选择回执两阶段丢失、提前拒绝缓冲擦除、待核对与已确认终态混淆等边界。未调用 SubagentBridge。

## 后续

1. 追加私有调度协议、提交去重身份、命令 ID 和类型化结果编码，并维护协议摘要及绑定。当前接口是可信原生 Rust API，不是可直接远程调用的协议。
2. Dart 客户端与工作台会话／选择审批／进度／结果 UI 接线；任务隐藏不自动重发，取消和实际退出分别呈现。
3. C／C++／Rust guest SDK 的能力、契约与模板，以及分块重置、跨重启 Unknown 业务核对继续开放。Windows 条件替换仍明确不支持。

本轮没有 Flutter 页面操作验收、安装包、远端推送或 Release；SDK 未冻结。
