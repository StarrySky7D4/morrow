# 首次 RPC 错误的原生验收测试候选

这里只归档一份 12,649 字节 Dart 测试源码，未放入生产 test 入口，未编译、未运行。测试需三个显式编译开关、固定计划与全新证据目录；缺少选择时会跳过，跳过不构成验收。

测试调用原 `RustWorkbench.open`，要求固定首次 RPC 错误进入原 catch，再核对两份有序启动快照、实际异常退出及双 EOF、原资源隔离回执。即使测试通过，业务结果也仍为 Unknown、owner 为 Retained、accepted=false，不能算正常关闭或业务成功。

完整验收载体、环境绑定和独立 checker 尚未完成；本目录不是可直接执行的自包含包，也不授权重放原 Unknown。见 [v12 限定状态](../../startup-rpc-fault-validation-v12.md) 和 [源码哈希](manifest.json)。
