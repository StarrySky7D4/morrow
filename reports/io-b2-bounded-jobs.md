# IO-B2：有界 IO 作业与契约路由

日期：2026-09-17。结果：**PASS_SCOPED**。范围：`morrow-plugin-runtime 0.1.9-test.50` 新增 `io_jobs` 作业执行器（submit／poll／read／cancel、队列／调用／字节／期限界限、契约路由、停止回收与迟到结果拒绝）。真实 HTTP／文件后端、异步 guest 挂起与远端效果核对不在本次范围；主应用版本、冻结 SDK、核心格式与其它平台未改动；已推送 `track-a/w1-io-contract`（`d086eaf`），未发布。

## 实际交付

- `plugin_runtime/src/io_jobs.rs`：`IoWorker`（独立线程、有界 sync channel、容量 1..=64）、`JobHandle`（非阻塞 `poll`、恰好一次 `read(max_bytes)`、`cancel`）、`Router`／`RouterFault` 契约路由、`JobLimits`（调用数、每作业字节、总字节）、`JobReport`（执行结论、调用数、字节、`cancelled`、`unknown`）。
- `plugin_runtime/src/package.rs`：抽出 `run_io_frame`，`run_file_frame` 复用它；IO 作业沿用同一 IO task ABI 与内容交换拒绝规则。
- `plugin_runtime/src/lib.rs`：导出 `io_jobs` 模块（沿用 `packages` 与平台门控）。
- 测试：`plugin_runtime/tests/io_jobs.rs` 6 项（多次调用顺序与恰好一次读取、poll 非消费与 read 字节上限、队列容量与字节上限、预算拒绝先于外部效果、期限压制迟到结果、停止回收且不交付迟到成功）。
- 设计说明：[有界 IO 作业](../docs/PLUGIN_IO_JOBS.md)。

## 退出证据对应

| 看板要求 | 证据 |
| --- | --- |
| 有界 submit／poll／read／cancel | 容量满返回 `Busy` 且不阻塞；`poll` 返回 `Pending／Ready／Consumed／Unavailable` 且不消费；`read` 恰好一次、超限返回 `ReadBound` 并保留结果；`cancel` 与句柄 Drop 都请求取消 |
| 完整任务输入 | `submit` 接收 1..=128 KiB 的完整输入帧，空帧或超限在准入前拒绝；输入字节立即计入总额 |
| 多次调用 | 单作业按 guest 顺序路由多次调用（测试 3 次），每次请求先计费、响应后计费，`calls` 与 `bytes` 如实记录；调用数上限在路由前检查 |
| 队列／并发／字节／期限贯通 | 容量、调用数、每作业与总字节、期限全部在提交或调用边界强制；累计字节只增不退 |
| 内容事务锁外工作 | 执行器独占核心于独立线程，调用方的 Store 借用与执行线程分离；归还后 `integrity_check` 通过 |
| 停止后回收且拒迟到结果 | `stop` 先撤权再取消，`try_finish` 归还核心、`pending == 0`、`phase == Stopped`；期限或撤权后到达的结果以 `Cancelled`／`Deadline` 交付且响应被丢弃，绝不作为成功 |

## 验证证据

| 检查 | 结果 |
| --- | --- |
| 运行时 IO 作业 `cargo test --offline --features packages --test io_jobs` | **6 通过，0 失败** |
| 运行时全量 `cargo test --offline --features packages` | **284 通过，0 失败**（上一项 278 → +6） |
| 运行时故障注入 `cargo test --offline --features fault-injection` | **285 通过，0 失败** |
| 运行时 clippy `cargo clippy --offline --all-targets --features packages -- -D warnings` | 无警告 |
| wasm32 库编译 `cargo check --offline --target wasm32-unknown-unknown --lib` | 通过 |

核心未改动，沿用上一项结果：默认 412 通过、故障注入 451 通过、clippy 无警告。日志：`build/io-b2-test2.log`、`build/io-b2-runtime-default.log`、`build/io-b2-runtime-fault.log`。

## 审查发现与修复

- 首版把 `Invocation` 当作任务输入，与既有 IO task ABI（FileBroker 以请求帧为输入）不符，guest 的 `read_input` 会读到错误字节；改为提交完整输入帧。
- 首版在路由之后才检查字节预算，可能先产生外部效果再拒绝；改为请求字节先计费、预算不足则在路由前拒绝，路由后溢出标记 `unknown`。
- 迟到结果原先只检查运行前令牌；改为运行结束交付前再次检查，迟到结果替换为 `Cancelled`／`Deadline` 并丢弃响应。

## 产物与边界

无数据库格式变化，无独立二进制产物变化。仍未完成：真实第三方 HTTP/HTTPS 与文件系统后端（IO-D1–D3）、guest 发布授权路由与远端身份凭据、异步 guest 挂起与执行器池、远端效果核对与录制隔离回放、按配额退休与 GC、主应用 UI 与三语言 SDK。
