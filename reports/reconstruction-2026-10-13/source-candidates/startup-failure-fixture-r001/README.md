# 启动失败隔离测试候选 r001

状态：SOURCE_NOT_RUN。这里只归档四份 Dart 覆盖源码、原生接线差量及源码清单；不会替换生产目录或激活入口。

基于 [r002 原生接线](../startup-quarantine-dart-r002/README.md)，新增默认禁用的编译期夹具。只有显式 qualification 编译、qualification 选择与指定模式同时满足，才会在原启动交换成功并验证 private-control 后、openCompleted 前抛出固定哨兵。未知模式或缺少 gate 在 IO／Process 前拒绝。原 catch、private marker、close、channel、正常 19 字段合同及 checker 保持不变。

包含四项纯数据测试和一项真实 native 测试源码，均未编译或执行。该候选的 workbench_native.dart 为 120,739 字节，不能套用旧 r002／120,251 字节的静态检查 PASS。失败隔离的预期只证明资源闭合，业务仍 Unknown、owner Retained、normal_shutdown=false、accepted=false。

后续需要独立版本的编译／launcher／admission／证据契约及真实资源验收，旧正常 launcher 和 checker 不适用于这个异常场景。本归档未包含可执行载体。见[源码清单](manifest.json)、[原生差量](workbench_native.dart.diff)与[当前进度](../../startup-integration-progress-v4.md)。

SDK26／G04、namespace 等仍 OPEN；release_eligible=false。
