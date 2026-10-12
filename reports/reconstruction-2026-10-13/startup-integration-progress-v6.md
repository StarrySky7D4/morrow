# Windows 正常原生会话联合实测（v6，2026-10-13）

本轮 **1 项可见 native qualification test 实际通过**。独立回读对原 16 份公开证据文件进行了两遍完整 SHA 核验，输入零漂移；“16”是证据文件数，不是 native 测试数。

同一原始身份的 31 帧 trace、11 项 observations 记录了 `Running → healthy Stopping/Reclaimed → ACK → Released`。PS30 generation 为 1，effects 为 10，consumed completions 为 7；原生 host 实际退出 0。宿主捕获退出 0、双流 EOF，独立会话关闭确认。仅 Process 范围临时使用 RemoteSigned，随后恢复 Undefined／有效 Restricted，持久策略未改变。

原 Flutter 编译器和测试器实际退出均为 -1，按 `ACTUAL_ORIGINAL_TOOL_CLEANUP_MINUS_ONE` 保留原工具清理分类，不改写为 0，也不据此宣称整个操作系统后代进程静止。原收据的静态 VM `NOT_RUN` 字段保持原文；当前实际运行由独立闭合的宿主／来宾证据建立，静态字段没有被改造成运行证明。

此前 **14 项 Rust 合成回归、16 项 Dart 内存回归**的限定结果保留。启动失败夹具的 **4 项纯数据测试与 4 个静态目标检查**保持历史有效，真实失败启动 native 夹具仍为 `NOT_RUN`。这些范围不能合并为本次 native 通过数量，也不覆盖全部 GUI／FFI 或完整产品资格。

**NORMAL ONLY。** SDK26／G04、namespace、祖先目录持续 reparse 防护仍 OPEN；完整生产安全、网络约束与完整 SDK 均未验收，`release_eligible=false`。历史 Stage341 仍 `CLOSED_NOT_ACCEPTED`，旧 Stage324 sealed Unknown 保留且不重放。当前通过不升级历史失败或 Unknown，也不授予重放权限。

下一步完成 Agent 会话层与安全执行层剩余验收，单独验证失败启动与拒绝范围；完成这两层后暂停准备测试预览，扩展执行层继续延期。

参见[结构化结果](startup-integration-progress-v6.json)与[上一阶段历史快照](startup-integration-progress-v5.md)。本次只同步开发分支文档，不修改生产入口、应用版本、依赖锁或工作流，不创建 Release；原始日志与运行包留在本地。
