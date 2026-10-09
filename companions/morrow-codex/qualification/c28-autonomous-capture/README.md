# C28 自主捕获资格工具

这些源码用于 Windows Agent 会话层和安全执行层的内部资格验证，不是产品启用入口或公开插件 ABI。

`CapturePump.cs` 是 C#5/.NET Framework4 读取库，准备固定线程后附着同一 Process，读取不依赖主机 Poll。它不 Start、kill、重连或重新执行任务。真实 EOF、已输出帧、主机保存 ACK 与业务 join 分开记录；超限/保存故障保留 Unknown，不能伪造完整保存。

`PumpTests.cs` 供使用者调用的合成验证入口为 `--run-synthetic <全新绝对目录>`；内部另有测试子进程模式，无匹配参数时返回 78。已保存的本机七项读取验证包括无 Poll 排空、正常 ACK、非法 ACK、单路保存失败、截断、超限及准备后取消；不构成真实沙箱或远程断连证明。编译与测试不自动执行，需固定 .NET Framework 工具并在独立目录重新验收。

`pending-stage19/` 保留已经静态审核的后继源码，四个入口以无条件 throw 禁用。产物、环境和 once 权限字段保持 PENDING；不得删除拒绝语句来直接启用。只接受已审核字节的显式一次程序集安装，保留同一程序集、类型和会话；安装或 Start 结果不确定时不能重试。ACK 辅助文件只有定义，不启动进程。

源码文件按原字节保存；真实环境清单、凭据、原始私人操作记录、缓存及编译产物不随本目录公开。保存目录中的模板不能直接作为可部署包。当前 `SDK26_G04=OPEN`、`release_eligible=false`。

构建和验收边界见 [阶段 18 记录](../../../../reports/reconstruction-2026-10-09/windows-agent-sdk-c28-stage18.md)。
