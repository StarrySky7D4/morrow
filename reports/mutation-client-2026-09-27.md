# 文件变更核对协议与类型化客户端（2026-09-27）

基线 `b9225f64f6c62584ad7243e30249d8a088bcb155`，分支 `codex/io-safety-refactor`。本轮将上一轮只读核对接到私有启动协议，并完成 Dart 变更客户端和 RustWorkbench 原生通道接线。行为规范见 [私有变更协议](../docs/PLUGIN_MUTATION_WIRE.md)。

## 实现

- 追加 mutationReconcile@133／Request@55。提交携带完整原计划与当前包审批，不带路径；与现场启动共享最多 512 个身份，使用不同摘要域，失败准入保留身份，重复只返回原任务 key，已 ack 的身份不能重新启动。
- 重新使用已归还的原内容库，保留当前权限及实时交付检查。新增真实协议用例先创建一次文件，停止／join／ack 后移除目标，再通过协议核对原 Observed 与实际效果，验证不重新选择或创建目标。
- 新增 MutationTaskBackend、模型、codec 与 NativeMutationTaskClient，覆盖选择启动、原计划／分块／提交／执行／查询／持久取消／释放、状态／领取／命令取消和独立核对。BigInt 保留 UInt64；一次调用只发一帧，不自动重试；Core 容器有界复制，仍由 Rust 解析。
- RustWorkbench 暴露 mutationTasks，Windows 原生进程才声明支持。六类控制动作走外层调度，避开服务业务转发。发送帧和构造 arena 清理覆盖变更正文及核对计划；原调用者缓冲不会被改写。
- 审查修正了 Dart 对合法重复启动的当前命令限制、终态与待核对误互斥、核对阶段与效果不一致、OS 拒绝码为零以及选择身份对照等问题。预算上限改用 runtime 常量；Core 原已验证安装包声明，因此这不是已复现的超限准入故障。

## 验证

| 范围 | 结果 | 证据 |
|---|---|---|
| Workbench lib 全量，三份真实 Wasm | 163 通过，0 失败，0 ignored | `build/mutation-client-host-full.log` |
| 变更协议专项（含原用例） | 19 通过，0 失败 | `build/mutation-client-host-tests.log` |
| Dart 客户端＋生成绑定＋旧文件客户端／会话 | 39 通过，1 既有夹具跳过，0 失败 | `build/mutation-client-dart.log` |
| Flutter 发送缓冲＋原生受控传输 | 23 通过，0 失败 | `build/mutation-client-flutter-transport.log` |
| 最终 Dart analyze --fatal-infos | No issues found | `build/mutation-client-analyze.log` |
| Workbench Clippy | 通过，12 条其他模块既有警告 | `build/mutation-client-clippy.log` |
| 标准绑定生成与 --check | 通过 | `tool/generate_workbench_client.py` |

专项与全量有重叠，不累加。Dart 的跳过项是既有 FileRead 三语言夹具缺失；新增 mutation 原件实际被读取，包括 `build/mutation-wire-fixtures/rust-mutation-reconciled.bin`。Flutter 原生传输使用受控 Python 子进程，验证 Dart 管道／路由／缓冲，不冒充真实 Rust 外部服务或真实 OS 操作。Rust 协议测试承担真实内容库及文件效果核验。

第一次 Flutter 回归失败来自旧测试仍期待初始化 page，而当前生产初始化已为 pageVersioned；确认实际打开路径后更新断言，最终全部通过。子代理遇到的分析器临时 perf 文件异常在主代理本地分析未复现；实际给出的六处新代码花括号提示已修正，最终分析无问题。

## 下一步与边界

继续补 Rust 侧原计划构造、持久计划发现、可恢复 Dart 会话及 UI 审批／进度／结果流程，再覆盖跨进程重启、真实崩溃与 SDK 接入。当前客户端是传输层，不是完整产品工作流；不存在“UI 已能操作文件”的验收。Windows 条件替换仍明确 Unsupported；其他平台资格及 C／C++／Rust guest 文件变更 SDK 未完成，SDK 未冻结。

本轮没有提交、推送、安装包或 Release。
