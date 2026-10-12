## 当前 Rust 候选编译与 14 项回归实测（2026-10-13）

修正版数据准备已实际成功，2,392 个源码文件、5,589 个 Git seed 文件完成字节核对，20 份依赖锁保持原字节。实际源码映射及五个 overlay 与已归档的 startup-quarantine-runtime-r002 完全对应；旧候选文件中的 NOT_RUN 保留为当时快照，本节记录后续实际结果。

当前候选的完整 Rust 库测试目标与 host／supervisor 两个程序均完成离线、锁定依赖的 Windows AMD64 编译；两次原 Cargo 均退出 0、双流 EOF。当前两个编译后的 HOME 核验通过。复用 TARGET 已明确记录，编译警告原样保留；这不是 Flutter GUI 构建或完整 SDK 验收。

14 项精确合成回归全部实际通过、未跳过：4 项 AgentWorker 停止、5 项内存 owner 账本、5 项 supervisor 解析与心跳。每项原测试进程均退出 0、双流 EOF，守卫无错误。库测试程序已用于这 14 项测试；host／supervisor 产品程序仅编译和核对，未运行。此前 Dart 修正版 16 项内存测试通过的限定范围保持不变。

当前 workbench_native.dart 静态检查、Rust／Dart 联合运行、真实 Windows 沙箱及资源回收仍为 NOT_RUN。SDK26／G04、namespace 与祖先目录持续 reparse 防护仍 OPEN，release_eligible=false；Stage341 仍 CLOSED_NOT_ACCEPTED，旧 sealed Unknown 保留且不重放。下一步优先完成原生接线检查与 Windows 联合验收，扩展执行层继续后置。

本次更新开发分支的实测报告与进度文档，保留历史失败和范围边界；生产入口、应用版本与依赖锁保持不变，不创建 Release。

见[机器可读实测](startup-integration-progress-v3.json)、[原 Rust 候选及组成](source-candidates/startup-quarantine-runtime-r002/README.md)、[此前 Dart A/B 实测](dart-startup-ab-results-v1.md)。公开报告仅含选定结果与哈希；原始本机日志、独立核验记录和产物留存在本地，不包含凭据、数据库、私有云端信息或本机操作脚本。
