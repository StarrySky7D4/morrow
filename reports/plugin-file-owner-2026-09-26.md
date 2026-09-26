# 插件文件后台 owner 队列与工作台原生接线

2026-09-26，基于本地 `e63d49f`。本轮保持应用版本与主线基准，不运行 Actions/CI、不推送或发布。

## 完成的工作

上轮真实文件入口是同步 FileBroker 方法，不能直接放入工作台的启动准备回调。本轮在既有 `IoWorker` owner lane 上增加类型化捕获、分块读取和结束，执行线程持有资源表，复用已有有限队列、停止、panic 清理、断开和实际 join。

工作台新增 `start_file`、`request_file_chunk`、`finish_file`、`read_file_result`。普通 IO 与文件任务共享原准入／所有权路径；文件任务使用同一 TaskKey 和取消／修复／确认机制，最多一个待领取回执。成功结束或终态错误都沿原 worker 停止，只有实际回收后才恢复本地内容库访问。

同时补齐两处接线约束：

- FileBroker 的取时和对应检查在后台共享时钟下串行完成，避免状态轮询推进时钟后把正常调用误判为回拨；不把时钟锁持有到文件系统调用、摘要计算或 guest 执行。
- 文件命令在宿主 JobLimits 账本预留完整 ceiling，并继续受实例实际字节／资源账本约束。宿主捕获预留 max_bytes+1，Read/Finish 保守预留两份最大 IO 帧；取消不退累计费用。

捕获取消不会交付半成品。未领取且取消的 Captured 会回收资源；外来 executor 的 FileSession 在入队前拒绝。读取回执的块缓冲使用 Zeroizing，停止和失效后拒绝迟到交付。没有引入新依赖、unsafe、guest 协议或文件写入。

源码重点：`plugin_runtime/src/io_jobs/file_commands.rs`、`file_io.rs`／`file_io/selected.rs`、`workbench_host/src/file_tasks.rs`。完整接口、配额和剩余接线见 [文件任务合同](../docs/PLUGIN_FILE_TASKS.md)。

## 实际验证

| 验证 | 结果 |
| --- | --- |
| runtime 全部常规回归 | 412 项通过，6 项 ignored |
| 新 owner 文件专项 | 8 项，已包含在 412 中；三语言原模块、队列上限、取消、迟到拒绝、跨 worker、panic／维护失败原 owner 归还、宿主累计额度不退款 |
| 原文件专项 | 23 项，已包含在 412 中；保留源文件改变、时钟／期限及原有交付边界 |
| 独立三语言 IO SDK | 5 项 ignored 已显式执行并通过；使用保留 Wasm，测试内声明 FileRead |
| Workbench Linux lib | 60 项通过，包含新增 3 项文件任务测试；最终增强准入额度拒绝后又跑该 3 项通过；编译保留 8 项既有平台/死代码等警告 |
| network 常规 | 134 项通过，12 项 ignored |
| 三语言服务 HTTP 原包 | 6 项 ignored 单独执行通过，包括持久配置资源撤权、缓存隔离、等待中本地操作、停止与重开不重发；三包摘要仍匹配 |
| runtime lib Clippy | `-D warnings` 通过；首轮 type_complexity 提示通过提取命名函数类型修复 |
| transport 固定文件 | 17 个原件摘要不变，无 guest 重编或重封 |

不相加分项计数。runtime 剩余一个 C/C++ 原生 service codec ignored 本轮没有重验，Windows 专属目标在 Linux 为零项，不算通过。证据位于 [本轮日志](evidence/plugin-file-owner-2026-09-26/)，附源码 SHA-256。

工作台新增测试直接调用当前 Rust API：三语言 90,007 字节捕获／删源／两块交付／Finish、错误 handler／修订／能力／ceiling 的读取前拒绝、错误 TaskKey、Ready 后取消，以及未 join 时禁止取回 owner。Linux 仅用测试构造提供真实 Store，受保护维护仍失败，断开成功但进入 RecoveryRequired，不能确认或伪装恢复。

## 当前界限

完成的是后台执行与工作台原生接口，不是系统文件选择器、私有进程协议、Dart/Flutter 页面或 Windows 成品验证。没有把宿主路径加入 guest API，也没有新增生产明文存储后备。

单个同步系统调用仍不可强制打断；owner 命令在其完成前不能执行其他存储业务。文件固定字节不是源文件的原子快照，没有新增持久文件证据、跨重启引用、目录或写入能力。既有 Windows/Flutter、DPAPI/TLS 生命周期资格继续开放，整体仍为 Linux 本地稳定候选。

下一步从原生接口接有界私有文件任务协议与平台选择句柄交接，再接 Dart/Flutter 结果消费和实际窗口验证。源码、Git 增量历史、保留原包和本轮证据保存 Google Drive 检查点，恢复后无需重建旧 guest。
