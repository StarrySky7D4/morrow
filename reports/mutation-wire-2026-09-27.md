# 文件变更私有协议与跨语言绑定（2026-09-27）

在 `codex/io-safety-refactor`、基线 `b9225f64f6c62584ad7243e30249d8a088bcb155` 继续实现 Windows 文件变更入口。新增五个私有外层调度动作与标准生成的 Dart native/web 绑定，保留旧编号和 guest IO 契约。行为规范见 [私有协议](../docs/PLUGIN_MUTATION_WIRE.md)。

## 实现与审查修正

- Start 绑定精确包／Registry revision、选择 scope 和有界路径／期限。启动身份最多 512 个；相同身份只恢复原 key，变参拒绝，ack 后旧身份不能重启。无其他活动任务且格式有效的申请在目录／预算检查前记录，失败后不能因安装或权限变化重新执行。
- Submit 对计划、命令种类、正文、offset 做长度分隔摘要，每任务最多 512 个已入队身份。重复只返回原 commandId；迟到 Read／CancelCommand 不能领取或取消新命令，已消费命令不重入队。
- 扩充状态及结构化错误，提供 Core 历史阶段、operationId 和 OS 效果投影。Created／Deleted 响应种类和 Observed 阶段都不冒充 OS 成功；Query 当前没有效果原件，effect=0。
- 新动作在原 owner 嵌套业务调用中全部拒绝，非 Windows 外层明确平台未适配。混入其他命令载荷、过大块或帧在执行前拒绝；原命令内容缓冲使用 Zeroizing。
- 原生子代理分别生成绑定、编写协议测试及只读审查；未使用 SubagentBridge。审查修复了“失败清理 Task 被重复启动误认成成功 mutation”及“catalog／budget 拒绝前未保留身份”的问题。

## 验证

| 范围 | 最终结果 | 证据 |
|---|---|---|
| Windows Workbench lib 全量（真实三份 Wasm） | 152 通过、0 失败、0 ignored | `build/mutation-wire-host-full.log` |
| 变更任务与协议专项（包含在全量） | 9 通过、0 失败 | `build/mutation-wire-tests-final.log` |
| Dart 新绑定＋原文件客户端／会话 | 31 通过、0 失败、1 skip | `build/mutation-wire-dart-final.log` |
| Workbench lib Clippy | 命令通过，仍有 12 条其他模块既有警告 | `build/mutation-wire-clippy-final.log` |
| 标准生成器及 --check | 通过 | `tool/generate_workbench_client.py` |
| 定向 rustfmt、Dart format、git diff --check | 通过 | 本轮工具结果 |

分项重叠，不相加。Dart skip 是既有 FileRead 三语言旧帧夹具未提供；**新增变更帧测试实际执行通过**。四份原始 Rust 回复位于 `build/mutation-wire-fixtures/rust-mutation-{selected,prepared,created,query}.bin`，Dart 核验协议摘要、任务 key、选择引用、历史状态、operationId、成功效果及 UInt64 精度。此证据仅是生成绑定解码，不是完整类型化 Mutation 客户端或 UI 验收。

复验命令需要当前构建的 MORROW_WORKBENCH_WASM、MORROW_HTTP_FORWARD_WASM、MORROW_SERVICE_OUTBOUND_WASM；三份插件在上一轮从当前源码 locked/offline 构建，本轮没有修改其源文件。Rust 定向测试设置 MORROW_MUTATION_WIRE_FIXTURES 导出原件，Dart 同变量读取这些原件。

过程中的错误已区分记录：状态投影漏写 InvalidPhase 分支导致一次编译失败，已改为明确拒绝；新增测试的两处失败来自真实创建测试竞争全局效果门及释放独占句柄前读取原文件，已修正；Dart 新测试的可空生成字段访问导致一次加载失败，已修正。上述最终结果均来自修正后的运行。

## 仍有阻断的产品范围

**原 worker 退出后，现有 Mutation Query 没有执行载体，不能对未知结果进行完整核对。** 当前可以保留不确定状态并拒绝自动重放，但不能把停止／退出／ack 当作对账完成。接下来优先提供原 owner 归还后的重新授权只读查询，绑定原计划并补效果详情，再覆盖跨进程重启。查询不得恢复原现场引用的执行许可。

之后继续实现类型化 Dart 客户端、UI 会话及审批／进度／结果页、C／C++／Rust guest SDK 和分块恢复／重置。Windows 条件替换仍明确 Unsupported，SDK 未冻结。新增 web 绑定生成不等于 Web 文件变更资格。

本轮无安装包、提交、推送或 Release。
