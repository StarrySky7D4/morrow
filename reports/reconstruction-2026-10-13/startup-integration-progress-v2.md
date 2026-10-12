## 启动隔离 A/B 实测与准备工具修正（2026-10-13）

旧 channel 的新增反例已实际复现：未请求正常关闭时仍接受 Released 并返回 0。负对照保留原退出 1 与 CLOSED_CAPTURE_FAILED；修正版完整 16 项 Dart 内存回归全部成功、未跳过，原 runner 退出 0。两路捕获、runner Future 与双流 EOF 已闭合；编译器／测试器的原工具清理退出 -1 保留，不据此宣称全部后代进程静止。

本轮测试只导入 channel／supervision，不导入 workbench_native.dart；该文件当前静态检查、完整 Rust 候选编译与联合运行、真实 Windows 资源回收及 VM 验收仍为 NOT_RUN。此前基线通过结果不覆盖当前候选，Stage341 仍 CLOSED_NOT_ACCEPTED，旧 sealed Unknown 保留且不重放。

另修正数据准备脚本中 overlay 局部变量覆盖控制快照的问题，17 个 helper 原字节及逆差量已核对。修正版入口仍 SOURCE_NOT_RUN；公开归档仅含语义补丁和核对清单。编译捕获与 HOME 守卫已形成禁用草稿，尚未绑定真实准备记录或运行编译。

SDK26／G04、namespace 与祖先目录持续 reparse 防护仍 OPEN，release_eligible=false。下一步执行修正版数据准备，再验证完整 Rust 编译、原生接线和 Windows 原资源闭合。本次同步开发分支的实际测试进度与准备工具修正，生产入口、应用版本和依赖锁不变。

见[A/B 实际结果](dart-startup-ab-results-v1.md)、[准备工具补丁](source-candidates/data-preparation-control-fix-r001/README.md)与[机器可读进度](startup-integration-progress-v2.json)。历史 v1 保持原样。
