# 内容命令与纯转换任务契约、三语言 SDK

基于 0.1.9-test.10，任务契约已扩展为 v3，Wasm guest ABI 仍为 v2。它们独立于原有内容消息 Cap’n Proto v6、包 schema v1 和资料库格式。旧 ABI v1 固定输入样例继续用于回归；新例子从宿主读取动态输入。

## 输入、执行和结果

本节描述**内容命令任务 profile**：宿主提供一条固定命令，插件解码输入、调用既有内容接口，再提交关联结果。支持重命名、摘要查询、操作结果查询和附件片段读取。纯转换任务已按下节实现；修改提案、多步骤任务、UI 事件与恢复还需扩展任务类型；此 profile 不被当作所有插件业务的最终模型。

| 消息 | 字段与规则 |
| --- | --- |
| Invocation | 任务契约版本、schema 摘要、task_id、完整原始内容命令字节；内容命令自身继续校验 v6 与两个 schema 摘要 |
| Completion | 契约版本、schema 摘要、task_id、完整 Invocation 原字节 SHA-256、核心响应原字节 |
| TaskReport | 执行状态，以及按任务类型互斥的可选 Response／TransformOutput／PluginFailure；执行或协议失败时三者均不交付 |

输入和完成消息至多 128 KiB，内层内容消息仍至多 64 KiB；Cap’n Proto 解析限定访问预算、深度，并拒绝尾随消息。输入由可信调度方创建并以所有权移入队列，guest 的内存改写不会修改宿主固定输入。task_id 用于任务结果关联，operation_id 用于内容事务去重，两者都不是权限。生产任务身份生成与持久恢复仍需由后续调度器统一管理。

在这个 profile 中，宿主只允许一次、且与固定输入逐字节一致的内容调用。非法尝试会使任务进入协议失败状态，不能接着发送另一条合法命令挽救任务。请求仍经过真实连接、包能力上限和对象授权。不同卡片是否可读写由核心授权决定，不能仅靠输入中的卡片 ID 推断。

完成消息只有在版本、任务 ID、输入摘要和**本次实际核心回复字节**全部一致时才能产生 `TaskReport.response`。即使 guest 自行编码出语法正确的成功回执，也不能获得权威结果；它不是宿主的提交证明。`execution.host_calls` 计数进入受控交换回调的尝试，不是成功事务数量。

完成后 trap、取消或多次发布都会使结果不可用；这不能回滚先前的真实提交。操作结果仍须使用稳定 operation_id 核对，不能自动重跑整个插件任务。

## 纯转换任务（任务契约 v2）

`Invocation::new_transform` 固定 handler、input_type、output_type 和输入字节，不包含内容命令。标识符非空、至多 256 字节，拒绝控制字符和路径分隔字符；输入及输出分别至多 64 KiB，外层消息仍至多 128 KiB。内容与转换字段不得混用。当前示例支持 `bytes.reverse` 和 `bytes.ascii-uppercase`，两者的输入／输出类型都是 `bytes`；前者反转原始字节，后者仅转换 ASCII 小写字母，均不承诺 Unicode 文本变换语义。

转换任务的交换请求在进入核心之前拒绝，即使该实例另有内容授权也不能调用内容接口。宿主核验完成消息的版本、schema 摘要、任务 ID、固定输入 SHA-256、输出类型和长度后，交付 `TaskReport.output`；`response` 为空。输出是**插件产出的数据**，不是核心回执，也不是算法正确性证明。结果展示或保存前仍需相应类型解析和业务验证；将结果写入卡片须另走授权、修订和事务提交，不能将输出字节直接当作命令执行。

内容任务继续只交付匹配实际核心回复的 `response`，`output` 为空。取消、trap、重复完成或协议失败均不交付转换结果。单包 handler 声明与输入／输出约束已建立，见 [处理器声明](PLUGIN_PACKAGE.md#纯转换处理器声明)。宿主在执行前检查名称、类型和输入上限，完成后复核输出上限；插件内部仍自行分派。跨包选择、类型版本协商、修改提案、持久恢复与 UI 任务仍待实现；结构化插件错误按下节实现。

| 语言 | 纯转换 API | 可编译示例 |
| --- | --- | --- |
| Rust | `Invocation::transform()`、`wasm::complete_output()` | sdk/examples/rust-transform |
| C | `mp_task_get_transform()`、`mp_task_output()`、`mp_wasm_task_complete()` | sdk/examples/c-transform |
| C++17 | `morrow::task::transform()`、`task::output()` | sdk/examples/cpp-transform |

C 的空输出允许 data 为 NULL 且 length 为 0；非空数据遵守既有缓冲区约定。转换 view 借用 task 所有的数据，在释放 task 后失效；C++ view 也不延长 task 生命周期。

任务契约 v1／v2 的实验包携带旧 schema 摘要，会被当前加载器拒绝；需要同步 SDK 并重新构建、打包。guest ABI v2 的承载接口没有改变，包 schema 仍为 v1，内容消息仍为 v6。此变化不迁移主 Flutter 资料，也不自动升级历史包。

实际三语言执行及产物摘要见 [纯转换验证记录](../reports/plugin-transform-validation.md)。

## 插件业务错误（任务契约 v3）

纯转换完成时恰好选择 Output 或 Failure。两者同时存在、均缺失或夹带内容 Response 均拒绝。Failure 绑定版本、schema 摘要、task_id 和完整输入 SHA-256，含固定错误码和消息：

| 错误码 | C 常量 | 含义 |
| --- | --- | --- |
| invalidInput | MP_TASK_INVALID_INPUT | 输入格式或参数不合法 |
| unsupportedInput | MP_TASK_UNSUPPORTED_INPUT | 处理器不支持该输入 |
| resourceLimit | MP_TASK_RESOURCE_LIMIT | 插件报告业务处理资源不足 |
| failed | MP_TASK_FAILED | 其他业务失败 |

消息是非空 UTF-8 纯文本，最多 1024 字节，拒绝控制字符；未知错误码拒绝。resourceLimit 是插件声明，不等于宿主确认 fuel／内存耗尽。消息仍属于插件数据，未来 UI 应显示来源并按普通文本呈现，不作为 Markdown、命令或宿主权威诊断使用。

| 情况 | execution | response | output | failure |
| --- | --- | --- | --- | --- |
| 内容任务完成 | Ok(0) | 实际核心回复 | 空 | 空 |
| 纯转换成功 | Ok(0) | 空 | 插件输出 | 空 |
| 插件报告业务失败 | Ok(0) | 空 | 空 | 关联后的插件错误 |
| 取消、trap、非零返回或协议失败 | Err(...) | 空 | 空 | 空 |

Ok(0) 表示 guest 完整执行完成协议，不代表业务成功。内容任务不能用 Failure 替代实际核心回复；先前提交不能因错误或取消回滚。纯转换没有内容提交，不自动重试。错误消息使用独立的 1024 字节上限，因此成功输出上限为 0 的处理器仍可报告错误。

Rust 使用 `wasm::complete_failure(task, FailureCode, message)`；C 使用 `mp_task_fail` 编码后调用 `mp_wasm_task_complete`；C++ 使用 `task::fail`。三者在完成导入成功后返回 0，编码或传输失败仍按执行错误处理。

示例新增 `bytes.require-ascii`：保留 ASCII 原字节，非 ASCII 返回 unsupportedInput。宿主同时检查执行状态和业务结果。此次任务契约升为 v3，guest ABI v2 的导入不变；旧任务包因 schema 摘要不匹配而拒绝，需同步 SDK、重建并打包。实际证据见 [结构化错误验证](../reports/plugin-failure-validation.md)。

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
