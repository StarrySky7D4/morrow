## 原生接线静态检查与启动失败测试候选（2026-10-13）

当前 r002 的 workbench_native.dart 静态检查已实际通过：原分析器退出 0、双流 EOF，输出 No issues found!，源码／配置守卫及独立回读通过。这只覆盖 120,251 字节的 r002 文件；第三方包仅核对目录元数据，不能据此宣称完整 SDK 已冻结。此前当前 Rust 编译、14 项合成回归和 16 项 Dart 内存测试的限定结果保持不变。

上一阶段源码与原始编译／测试证据已生成本地增量归档，220 个选定输入、223 个 ZIP 条目通过完整 SHA／CRC 复核。归档依赖已保留基线及工具环境，不是自包含 SDK，本次不上传该 ZIP。

另归档默认禁用的启动失败夹具 r001：四项纯数据测试与一项真实 native 测试源码尚未编译或运行。新候选的 workbench_native.dart 为 120,739 字节，不继承旧 r002 静态 PASS；原正常合同与 checker 不变。

Rust／Dart 联合运行、真实 Windows 沙箱与资源回收仍 NOT_RUN，SDK26／G04、namespace 与祖先目录持续 reparse 防护仍 OPEN，release_eligible=false。Stage341 仍 CLOSED_NOT_ACCEPTED，旧 sealed Unknown 保留且不重放；优先继续联合验收，扩展执行层后置。

见[机器可读进度](startup-integration-progress-v4.json)、[未激活候选](source-candidates/startup-failure-fixture-r001/README.md)与[上一轮实际结果](startup-integration-progress-v3.md)。本次仅同步开发分支源码候选与文档，生产入口、应用版本、依赖锁保持不变，不创建 Release。
