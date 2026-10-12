# 失败启动隔离：Rust 完整运行链组合候选 r002

状态：**SOURCE_NOT_COMPILED_NOT_RUN**。本目录为惰性源码归档，不激活生产模块，不提供运行、恢复、清理或 VM 许可。

本候选以已固定的 349 源图（2392 个文件）为基线，合入 354 的五个 overlay。五叶中四叶与 354 原字节相同；账本 `startup_quarantine.rs` 也与 349 原字节相同。`workbench_supervision.rs` 保留完整 349/347 文件作为前缀，仅追加 354 的 276 字节私有 runtime 模块与零参数导出。

本次唯一新增 Rust 修正是从 supervisor binary 删除三行遗留 `cfg/path/mod` 声明。runtime 已迁入 library 并使用 library 的 `crate::Result` 与账本模块，旧声明会让 binary 再次编译该实现。binary 继续调用 library 的零参数入口。其余 354 字节保持不变；349 的 345 停止状态、346 同句柄读取、347 异常账本改动及全部 20 个锁均保留。Linux process/controller 文件未改。

`overlay/` 保存五个最终原始文件；`baselines/source349/` 保存其五个原始基线，`baselines/frozen354/` 保存唯一被修正的 binary 原件。两个 raw diff 与 `composition.json` 提供逐文件 SHA-256 和组合证明。此包仅是增量候选，需要与对应完整基线整合，不能作为独立可构建工作区或直接覆盖任意 checkout。

`exact-tests.json` 列出未来选定的 14 项测试：四项受控 AgentWorker 停止、五项内存账本、五项 parser/heartbeat 逻辑。**本完整版本尚未编译，14 项当前源图测试均 NOT_RUN**。此前 353/354 的有限逻辑结果和 355 的旧版本产品编译，均不能冒充本版本通过。

尚需完整 Rust 类型检查、library 和两个产品 binary 构建、当前源图精确回归及 Rust/Dart 联合边界验证。实际原 child/双 EOF/Job 空/reader join/stdin reap、异常持久化读回及 Windows 安全验收仍需真实证据；合成账本或局部 heartbeat 测试不提供 kernel proof。无法确认闭合时必须保留 Unknown/noRetry，不以正常 Released/healthy 替代。

祖先目录持续 reparse 防护、namespace、SDK26/G04 仍 **OPEN**，`release_eligible=false`。本包不含工具、运行脚本、日志、二进制、口令、账户或本机绝对路径。原始文件版权/许可证声明保持不变；许可归属沿用仓库对应源码，未新增第三方实现。
