## 启动失败夹具实测与联合验收包进度（2026-10-13）

此前归档的启动失败夹具 r001 已完成四项纯数据测试：每项成功一次、未跳过，外层退出 0、双流 EOF，独立回读零漂移。原 Flutter 编译器／测试器清理时的实际退出 -1 保留，不改写成 0，也不据此宣称全部后代进程静止。四个选定文件的静态检查均退出 0、双流 EOF、No issues found!；其中包括 120,739 字节的新 workbench_native.dart。真实 native 失败夹具仍未运行。

正常路径联合验收包已完成本地数据准备：9,046 个负载文件、9,048 个 ZIP 条目、666 个 Dart 源码文件，两轮完整文件 SHA、ZIP SHA、逐项 SHA／CRC 回读通过且零漂移。包使用此前编译的当前 host／supervisor 和三个正常路径 Dart 文件，不包含新失败夹具；正常 19 字段与原资格合同保持。此结果仅证明本地包完整性，未执行产品程序或虚拟机验收。

后继宿主载体源码已通过独立审核，补入旧测试进程碰撞检查及不可读字段拒绝；它只是固定进程名和路径前缀的时点检查，不是连续命名空间或静止证明。传输、执行入口尚未选择，VM 传输与 Rust／Dart 联合运行均为 NOT_RUN。

此前 14 项 Rust 合成回归和 16 项 Dart 内存回归的限定结果保持。SDK26／G04、namespace 与祖先目录持续 reparse 防护仍 OPEN，release_eligible=false。Stage341 仍 CLOSED_NOT_ACCEPTED，旧 sealed Unknown 保留且不重放。下一步完成正常路径联合验收，再单独验证异常隔离；Agent 会话层与安全执行层完成后暂停，准备测试预览，扩展执行层后置。

见[机器可读结果](startup-integration-progress-v5.json)与[上一轮历史快照](startup-integration-progress-v4.md)。本次同步开发分支进度文档，生产源码、应用版本、依赖锁和历史候选保持原字节，不创建 Release。
