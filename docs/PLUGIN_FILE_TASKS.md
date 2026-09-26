# 原 owner 后台文件任务

2026-09-26：原生 `IoWorker` 的类型化文件命令与 Workbench Rust 文件任务已接通。Linux 的真实 Store／三语言既有 Wasm 模块和测试用 Workbench 所有权路径已验证。后续已接私有进程协议及独立 Dart 客户端；系统选择器、Flutter 页面与 Windows 实机仍未验收。

## 一个 owner、一条现有队列

平台适配器提供已经获得合法选择授权的 `std::fs::File`，不传 guest 路径。`capture_file` 接收句柄并返回 `FileSession` 和 `FileCommandHandle`；准入线程不读取文件元数据、不 seek/read，拒绝时会丢弃传入句柄。原 owner 已移动到现有 `IoWorker` 线程；prepare、实际捕获、guest 执行、断开与维护均在该线程完成。

命令复用原 owner lane：最多八个排队／运行／未领取回执，持有完整回复上限的队列预留。新的原生接口不提供任意闭包或 guest 授权入口：

| 接口 | 结果与约束 |
| --- | --- |
| `capture_file(file, handler, max_bytes, secret)` | 显式已选句柄、声明内处理器、可信宿主新随机 secret；成功返回固定字节元数据 |
| `read_file_chunk(session, offset, limit)` | 精确文件会话，单块最多 64 KiB；零 limit 使用上限，offset 不自动推进 |
| `finish_file(session)` | 原 guest 执行 Finish；不论 guest 正常完成或失败，此文件资源随后退休 |
| handle `poll/read/cancel` | 非阻塞、一次领取；停止、到期或失去授权后抑制未领取字节 |

`FileSession` 只在原 executor 上有效，没有反序列化恢复入口。文件字节由执行线程局部资源表持有，退出／panic 会丢弃；调用端只持会话身份与有界回执，不携带 FileBroker 或 Store。捕获回执被取消且没有成功领取时，执行线程回收对应资源；资源回收不能恢复累计额度。已领取的文件须显式 Finish 或停止 worker，丢弃一个已消费回执不等于关闭文件。

捕获复用 [单文件合同](PLUGIN_SELECTED_FILE.md) 的有界读取、类型检查、实际字节摘要及逐块撤权检查。额外取消检查在捕获边界拒绝继续；单次 OS 系统调用和单次 guest 执行仍不能被命令队列强制打断。后台线程化不等于异步 guest 续接。

## 两层费用与交付

宿主 `JobLimits` 与原实例 IO 预算同时有效：

- 宿主账本在命令准入时保守预留：捕获为调用者的 `max_bytes + 1`；Read／Finish 为两份 IO 最大帧，即 256 KiB。受单作业与累计上限约束，与既有 guest 作业共享 `worker.bytes()` 累计账本。因而宿主 ceiling 必须留出读取和结束余量。
- 原实例账本仍由 FileBroker 按实际固定长度加 EOF 探测字节及原请求／响应帧计费。读取作业与持有文件资源沿原 Manager、绑定和共享额度验证；宿主预留不替代实例批准。
- 取消、失败和丢弃已经准入的命令均不退累计费用；未准入的 Limit／Busy／外来会话不占队列槽。

工作台每个文件任务只允许一个待处理／未领取回执，低于底层八项队列上限。块回执使用 `Zeroizing<Vec<u8>>`，队列取消或丢弃时擦除其拥有的块缓冲；这不承诺整个文件 spool、guest 内存或调用者额外副本都已擦除。

共享时钟的采样和对应授权／计费检查在同一个短临界区内执行。文件 read/seek/metadata、摘要计算和 guest 运行不占用这把时钟锁；状态查询不会因为锁跨外部 IO 而一直等待。底层同步阻塞仍需等待实际返回，不能用一个“已取消”状态冒充线程退出。

## Workbench 原生接入

`start_file(StartOptions, File, handler, max_bytes)` 只接受 FileRead 能力集合、当前选中包摘要／Registry 修订和声明的处理器。它复用普通 IO 的连接、原 owner 移交和准入失败清理，但文件会话不会在启动后立即 drain。

正常调用顺序：

1. `start_file` 获得 `TaskKey`；`poll_io` 观察，`read_file_result` 领取 Captured。
2. `request_file_chunk(key, offset, limit)`，随后 `read_file_result` 领取一个块。待领取时拒绝重复准入；无隐式重读或 offset 递增。
3. `finish_file(key)`，读取 Finished 后请求停止。命令终态错误也请求停止；不自动重捕获文件。
4. `poll_io` 等实际 join，再按原规则 `repair_io`／`acknowledge_io`。`cancel_io`、关闭及 EOF 沿现有停止路径回收。

在 owner 尚未归还时，依赖内容库的普通调用仍 Busy。已保存卡片、原 Manager、池和存储保护守卫随完整 `WorkbenchState` 移动，没有第二份 Store。收到 Finished 只表示结束命令完成，不代表维护和断开均成功。

Linux Workbench 的受保护存储打开仍明确拒绝。本轮仅在 `cfg(test)` 下构造真实 Store 供接口验证，维护仍按原实现失败，并验证 `RecoveryRequired`、修复失败和禁止确认；没有增加明文生产后备或把该结果当作 Windows 审计／DPAPI 资格。

## 复验

```sh
cargo test --locked --offline --manifest-path plugin_runtime/Cargo.toml \
  --features packages --test file_owner --test managed_file_io
cargo test --locked --offline --manifest-path workbench_host/Cargo.toml \
  --lib io_tasks::file::tests
python tool/plugin_transport_baseline.py verify
```

文件专项读取仓库保留的三语言 Wasm，测试内建立 FileRead 声明包；旧 HTTP 原包没有被增权或改写。原生接线详见 [owner 报告](../reports/plugin-file-owner-2026-09-26.md)，最新私有协议、Dart 验证与限制见 [协议报告](../reports/plugin-file-wire-2026-09-26.md)。


## 私有文件任务协议与 Dart（2026-09-26 后续）

`host.capnp` 追加 `fileStart / fileChunk / fileFinish / fileRead`，沿现有 ioStatus / ioPoll / ioCancel / ioRepair / ioAcknowledge 控制所有权；属于最外层调度消息，不能放进原 owner 的嵌套业务命令。宿主与 Dart 绑定及私有 digest 同步生成；guest IO 协议和冻结 Wasm 不变。请求／响应沿用 128 KiB 帧上限，单块最多 64 KiB。

`FileStart` 显式绑定随机非零 32 字节 submission、包 ID／摘要、注册表修订、声明 handler、可信宿主选中文件路径、捕获 ceiling 和最多 30 秒期限。一个宿主会话最多保留 512 个文件尝试身份，准入过程中消耗的身份不重用；任务忙时不能替换原身份。丢失启动回执只允许用 ioStatus 核对，不得自动再启动、重新打开路径或恢复旧引用。HTTP／文件状态使用当前共同的提交身份，避免文件任务带出陈旧 HTTP 身份。

`start_file` 仍接受已打开的句柄。新增 `start_selected_file`／`capture_selected_path` 仅用于可信私有 UI 适配：接受绝对、无 NUL、UTF-8 最多 4096 字节的路径，原 worker 在实时 FileRead／原实例／取消检查后才执行 open，再按普通文件捕获规则读取。错误只携带 OS 错误类别。路径授权来自可信 UI；本接口不能证明路径一定由系统选择器产生，也不提供选中时刻对象、symlink/junction 根约束或原子快照保证。对象身份从实际 open 起建立。同步 open 也可能阻塞；停止回执不能代替实际 join。

`fileRead` 每次返回一种结果：Pending、Captured(length + 原始字节 SHA-256)、Chunk(offset + bytes + eof)、Finished。不暴露内部文件 grant 引用。读取仍一次消费，错误响应不带部分结果；Rust 中间结果帧的文件字节在序列化后擦除，Dart 在复制为不可变自有模型后擦除其私有响应帧。这不是所有内存副本的全局擦除承诺。

`NativeFileTaskClient` 提供类型化入口并接入 RustWorkbench，复用原传输和调度隔离。请求写入前验证范围，保留 UInt64 为 BigInt；解码拒绝混合结果字段、畸形摘要、超大块和偏移溢出。调用端仍需消费 Captured 后再请求块，对照期望 offset／length／hash 整理最终内容；此客户端不自动循环或重试。

跨层验证发现旧文件 SHA-256 曾误用面向 schema 的文本规范化函数。现在文件内容摘要与文件引用的二进制熵 seed 均使用原始字节 SHA-256；回归覆盖非法 UTF-8、CRLF 和会被有损文本转换合并的两组 secret。引用只存活于原进程，不涉及持久引用迁移。

下一步接真实平台选择器与文件任务控制／结果页面，并在 Windows 验证选中、取消、丢回执、修复和实际退出。Linux 测试 Store 仍不能代表生产受保护存储。本轮 Flutter 启动自动审批因其尝试云元数据地址而拒绝；独立 Dart 协议测试通过，不记为 Flutter 窗口或完整适配器静态验证。
