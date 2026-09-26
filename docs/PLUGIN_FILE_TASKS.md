# 原 owner 后台文件任务

2026-09-26：原生 `IoWorker` 的类型化文件命令与 Workbench Rust 文件任务已接通。Linux 的真实 Store／三语言既有 Wasm 模块和测试用 Workbench 所有权路径已验证。系统文件选择器、私有进程协议、Dart/Flutter 页面、Windows 实机仍未接入本轮资格。

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

文件专项读取仓库保留的三语言 Wasm，测试内建立 FileRead 声明包；旧 HTTP 原包没有被增权或改写。详情见 [本轮报告](../reports/plugin-file-owner-2026-09-26.md)。下一步是基于此原生入口设计有界私有文件任务协议、平台选择句柄交接和 Dart/Flutter 消费，不把系统路径变成 guest 授权。
