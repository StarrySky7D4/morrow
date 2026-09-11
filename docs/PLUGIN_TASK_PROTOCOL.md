# 内容命令任务契约与三语言 SDK

基于 0.1.9-test.10，新增任务契约 v1 与 Wasm guest ABI v2。它们独立于原有内容消息 Cap’n Proto v6、包 schema v1 和资料库格式。旧 ABI v1 固定输入样例继续用于回归；新例子从宿主读取动态输入。

## 输入、执行和结果

本阶段实现的是**内容命令任务 profile**：宿主提供一条固定命令，插件解码输入、调用既有内容接口，再提交关联结果。支持重命名、摘要查询、操作结果查询和附件片段读取。通用计算、转换提案、多步骤任务、UI 事件与恢复还需扩展任务类型；此 profile 不被当作所有插件业务的最终模型。

| 消息 | 字段与规则 |
| --- | --- |
| Invocation | 任务契约版本、schema 摘要、task_id、完整原始内容命令字节；内容命令自身继续校验 v6 与两个 schema 摘要 |
| Completion | 契约版本、schema 摘要、task_id、完整 Invocation 原字节 SHA-256、核心响应原字节 |
| TaskReport | 宿主执行状态，以及可选的已验证核心 Response；取消、trap 或校验失败时不交付权威 Response |

输入和完成消息至多 128 KiB，内层内容消息仍至多 64 KiB；Cap’n Proto 解析限定访问预算、深度，并拒绝尾随消息。输入由可信调度方创建并以所有权移入队列，guest 的内存改写不会修改宿主固定输入。task_id 用于任务结果关联，operation_id 用于内容事务去重，两者都不是权限。生产任务身份生成与持久恢复仍需由后续调度器统一管理。

在这个 profile 中，宿主只允许一次、且与固定输入逐字节一致的内容调用。非法尝试会使任务进入协议失败状态，不能接着发送另一条合法命令挽救任务。请求仍经过真实连接、包能力上限和对象授权。不同卡片是否可读写由核心授权决定，不能仅靠输入中的卡片 ID 推断。

完成消息只有在版本、任务 ID、输入摘要和**本次实际核心回复字节**全部一致时才能产生 `TaskReport.response`。即使 guest 自行编码出语法正确的成功回执，也不能获得权威结果；它不是宿主的提交证明。`execution.host_calls` 计数进入受控交换回调的尝试，不是成功事务数量。

完成后 trap、取消或多次发布都会使结果不可用；这不能回滚先前的真实提交。操作结果仍须使用稳定 operation_id 核对，不能自动重跑整个插件任务。

## Guest ABI v2

入口继续是 `morrow_run() -> i32`；任务完成时返回 0。只允许以下导入：

| 导入 | 参数／限制 |
| --- | --- |
| `morrow_v1.exchange` | 原有四参数有界内容消息接口；输出容量 65536 |
| `morrow_task_v1.read_input` | output offset、capacity；容量必须 131072，预先校验整个输出区；每次任务只能读取一次 |
| `morrow_task_v1.complete` | input offset、length；有界复制完成消息；必须已读输入且只提交一次 |

发布完成后不能继续交换内容。禁止 start、WASI、自定义身份、外部内存及重复导入，沿用既有 fuel／内存／调用限额。ABI v1 加载器拒绝这些新导入；ABI v2 不能通过无输入的旧执行入口运行。

`.mplugin` 中 `guest_abi_version=2` 必须携带正确的 task_schema_sha256；v1 必须为空。旧宿主会拒绝不支持的 ABI，而不会忽略任务契约继续执行。

## SDK 用法

| 语言 | 接口 | 示例 |
| --- | --- | --- |
| Rust | `wasm::read_task()`、`Invocation::request()`／`command_bytes()`、`wasm::complete_task()` | sdk/examples/rust-task |
| C | `mp_wasm_task_read`、`mp_task_decode/get/complete/free`、`mp_wasm_task_complete` | sdk/examples/c-task |
| C++17 | 拥有任务的 `morrow::task`、只读 view、关联完成消息构造；RAII 释放 | sdk/examples/cpp-task |

C 的 task/view 遵守原生 codec 同样的对齐、长度和非重叠缓冲区约定；view 指向任务所拥有的字节，在 free 后失效。C++ task 不可复制，可移动。C／C++ 使用 Rust 实现的独立任务编解码，Rust guest 使用同一 SDK；宿主独立生成固定契约与验证消息。SDK 没有授予权限或打开核心资料库的入口。

可信宿主使用 `Worker::submit_task(Invocation, timeout)` 取得 `TaskHandle<TaskReport>`，调用方非阻塞轮询结果。队列仍最多 64 个未完成任务，取消／停止／撤权规则见 [后台任务](PLUGIN_TASKS.md)。这还不是 Dart／Flutter UI 事件桥接。

```powershell
# 在构建三语言模块后，以 ABI v2 打包：
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.10 --example plugin_package -- pack-task build/plugin-c-guest/c_task.wasm build/task-example.mplugin org.morrow.example.c-task 0.1.9-test.10 rename,summary,operation,attachment
# 实际任务资格；命令使用隔离的合成资料库：
cargo run --locked --manifest-path plugin_runtime/Cargo.toml --target-dir build/plugin-runtime --features packages --example qualify_tasks -- build/task-example.mplugin
# 完整重建、边界检查与三语言实际执行：
pwsh -File tool/verify_plugin_runtime.ps1
```

目标包必须不存在，工具不会覆盖旧文件。完整验证创建独立输出目录。结果与摘要见 [动态任务验证记录](../reports/plugin-task-contract-validation.md)。
