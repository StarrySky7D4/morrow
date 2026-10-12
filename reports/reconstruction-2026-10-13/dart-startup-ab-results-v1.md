# Dart 启动隔离 A/B 内存回归实际结果

日期：2026-10-13。状态：**限定范围的 A/B 回归通过**。

- **A：旧实现负对照准确复现缺陷。** 只运行新增的第16项反例，1项可见测试失败、隐藏加载成功。原始 launcher 退出码为 **1**，捕获状态仍为 `CLOSED_CAPTURE_FAILED`，机器事件 `done.success=false`。失败原因是原本应抛出 `WorkbenchSupervisionUnconfirmed`，旧 channel 却在没有请求正常关闭时接受 Released 并将 exit Future 完成为0。预期失败没有被改写成退出0或产品通过。
- **B：修正版全部16项可见测试成功，均未跳过。** 隐藏加载成功，原始 launcher 退出0，机器事件 `done.success=true`。
- 两路原始 runner Future、launcher 捕获和双流 EOF 均已闭合，捕获及后置守卫错误为空。各自编译器和测试器的实际退出码 **-1** 保留不变；对应一次已送达的原工具清理 `SIGTERM`／`SIGKILL`，双流 EOF 已观察。没有据此宣称整棵后代进程树静止。

独立回读及本次公开整理均按36个固定输入核对两遍完整 SHA-256，无输入漂移。公开 JSON 仅列内容摘要和证据 SHA；原始日志、本机路径、账户、内部进程身份及凭据不上传。

这16项测试只导入 channel 和 supervision，使用内存 FakeProcess 流，**不导入 `workbench_native.dart`**。它们证明的是上述逻辑回归，不能替代该文件本轮静态检查、完整 Rust／Dart 联合验证、真实 Windows 产品、DPAPI 或 VM 验收；这些范围仍为 `NOT_RUN`。生产接线未激活，旧 sealed Unknown 保留且不重放，历史源码与失败记录不被覆盖。

SDK26／G04、namespace 与祖先目录持续 reparse 防护仍 **OPEN**，`release_eligible=false`。
