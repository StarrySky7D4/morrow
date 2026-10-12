# 启动集成本地时序验证边界 v9

## 失败启动 kind4 时序修正与本地正反回归（2026-10-13）

新增 r003 仅归档四个 Dart 源文件，不激活生产入口。kind4 准入同步锁存 pending；原 flush 成功或失败后才重新评估完成，失败仍保留 Unknown／无回执／不重试。相同测试源的 fixed-B 21 项逐项成功且未跳过；negative-A 使用旧 channel，只运行一个焦点，真实在原第 435 行提前 settled 断言失败（Expected false / Actual true），原非零退出保留。B 外层与内部 runner 均返回 0；A 均返回 1，双流 EOF 分别记录。

两个选定静态目标均退出 0；channel 保留 1 项 info，测试目标 No issues found。原 Flutter 编译器／测试器工具清理的实际 -1 保留，不改写为 0。外层捕获进程和内部 launcher 的 PID 分别记录，不据此声称操作系统同进程／祖先关系或全部后代静止。

本轮 B 源图 667 项中有 663 项与公开树完全同字节，仅三条生产 Dart 源不同及一个新增测试缺失。该结果覆盖实际 channel 的内存流／IOSink 时序，不覆盖 RustWorkbench.open 的原 catch 接线或真实 Windows job／owner 事务；既有 11 项启动观测测试未在此轮重跑。公开 Rust 生产入口仍缺少配套扩展，五叶候选保留在独立 r002 档案；因此不是全 runtime 配套合入。

原 native Unknown 继续保留、未确认闭合，不重放或终止。本轮没有新的 VM／native／DPAPI／真实库执行；旧正常验收与历史失败各自保留。SDK26／G04、namespace、ancestor 持续 reparse、生产安全及网络约束仍 OPEN，release_eligible=false；不宣称 SDK 冻结。Agent 会话层与安全执行层完成后暂停，扩展执行层延期。

见[机器可读摘要](startup-integration-progress-v9.json)与[四叶候选](source-candidates/startup-quarantine-dart-r003/README.md)。旧 v1–v8 与候选原字节保持。版本、锁、工作流、tag 与 Release 不变。
