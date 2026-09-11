# 三语言动态任务契约验证

日期：2026-09-11。Windows x64；应用基线 0.1.9-test.10。新增任务契约 v1、guest ABI v2；内容消息仍为 v6，资料库格式未改变。

## 真实执行

C／C++／Rust 新示例均没有写死卡片、标题、修订或操作 ID。每种语言从宿主接收 8 个任务并在原生队列中执行：Unicode 重命名、未授权卡片拒绝、重复操作返回原回执、第二次动态修订、摘要查询、历史操作查询、缺失操作查询、读取真实附件片段。

24 次实际任务均获得通过输入／结果关联和来源验证的 Response。队列排空、连接退休后重新打开 SQLite：目标卡片修订 3，两次创建加两次重命名共四条原子事件，未授权卡片仍为原内容；附件读取字节与源文件片段一致。

## 回归与边界

- 核心格式／Clippy、107 项默认测试、7 项故障恢复、自检、CLI 和 wasm32 编译通过；新增包用例验证 ABI v2 必需摘要与旧 ABI 隔离。
- 插件运行库 29 项测试通过；其中新增 5 项任务测试覆盖三语言共享消息的独立宿主／Rust SDK 向量、四类命令、UInt64／Unicode、错误版本、尾随／截断／超限、任务关联及伪造回复。
- 执行边界拒绝错误容量、越界、重复读取／完成、未完成、错误运行入口；复制完成消息后修改 guest 内存不能更改宿主副本，后续 trap 不交付完成结果。
- 恶意模块修改命令、伪造／跳过实际核心调用、重复调用或完成后 trap，均不能提供权威 Response；已经发生的提交仍保留。
- SDK 14 项回归、原生 C／C++ 编译与 Rust DLL／SQLite 实际授权链路通过。新任务 C／C++ 接口在实际 Wasm 模块中覆盖，未据此声称完成所有原生 ABI 平台验收。
- 原有 ABI v1 包、后台队列、取消／停止／撤权及 C++ trap 路径继续通过。实际 Web 存储 feature 的 core-web 编译通过，未运行 Web 插件执行器。

日志：`build/core-test.10/task-verification.log`、`build/plugin-runtime/task-verification.log`、`build/plugin-sdk/task-verification.log`。

## 本次产物

包目录：`build/plugin-packages/6c2f5099cc2146178bfdf9acc45ad9fb`。以下 SHA-256 均为实际字节摘要，不是作者签名。

| 包 | 字节 | SHA-256 |
| --- | ---: | --- |
| rust-task.mplugin | 50060 | `fd8250dc98be125b00f1d86c75032ee6773c75e1aff56ddd0f94dcb4085e2264` |
| c-task.mplugin | 47643 | `ee40cee94f1a7c7d7480da13d6c432fdc71489fa5cc0c38518e149bcea853c45` |
| cpp-task.mplugin | 48360 | `e9f7caf869279fc7e5298ef6d480a0fce50cb9d0e7e36947eda023e2064e110e` |

| 模块 | 字节 | SHA-256 |
| --- | ---: | --- |
| morrow_example_task.wasm | 93643 | `c4b892864cf3c5786feb8efaa91584cf89bd8eae8257b84bf8425f00eae01969` |
| c_task.wasm | 87039 | `47bc28ea7509affd04b8397c22fbe387d179f8c5515a9bfe2ddce61fe1bbcd0b` |
| cpp_task.wasm | 88009 | `5587f6c177b9d303a8f5e794d3c8285d8a3a034371507a157c974e550c9a4d94` |

SDK 编解码增量也改变了部分 ABI v1 模块字节；旧报告的摘要仅对应早期产物。本次旧、新例子都已重建并重新验证，完整摘要见运行日志。

任务 profile 当前只覆盖固定内容命令，不代表通用插件计算、多步骤事务、转换提案、任务持久恢复或 Flutter UI 已完成。下一步扩展任务类型与声明式 UI，不把该 profile 限制当作最终插件能力边界。未推送或发布 Release。
