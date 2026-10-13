# Runner 启动入口与诊断候选（599／603，2026-10-13）

本记录补充[当前三角色与真实链进展](harness-runner-setup-progress-590.md)，不覆盖其 Unknown 或未验收结论。

599 在同一受控虚拟机、管理员身份下，以原物化路径和完整 SHA 绑定的 runner 执行一次无参数探针。真实 runner 退出 1，双流 EOF 与完整 SHA 一致，stderr 为 `Error: runner: no pipe-in provided`；外层捕获退出 0，自己的连接已闭合。605 的实际只读核验通过，28 份固定输入无漂移。此结果证明该身份下的加载器／参数入口可达，不验证 SandboxOffline 登录、管道握手、原 Start 或业务执行；旧 590 Unknown 保留。

现有日志缺少刚创建 runner 的身份与库内终止前的真实状态，因此 603／604 经作者自核与独立源码审核后，607 将一处诊断叶应用到开发源码：

- CreateProcessWithLogonW 成功后，仅记录原 runner PID，不记录命令或口令。
- 进入现有启动失败清理时，使用仍持有的原进程句柄进行零等待；仅在句柄 signaled 且 GetExitCodeProcess 成功时记录完整 u32 十六进制退出码。
- 查询失败、未 signaled 与仍运行均保留 unknown。清理前已退出不等于自然退出，也不等于业务成功。

原路径／SHA 校验、权限、v7 协议、Claim、重试行为与既有 TerminateProcess 调用保持原状态。该补丁不是 noKill 政策修补，也不是已证实的握手根因修复。602 的有界等待、单次 go gate、原 owner 债务交接方案仍为 DESIGN ONLY，没有应用。

选定源文件为 `companions/morrow-codex/derived/c28-cleanup-com-002/codex-rs/windows-sandbox-rs/src/elevated/runner_client.rs`：原 36,863B／SHA256 `9e543c2fb4973cd3e0601d715662f66d16a6a8e572990184d26b7b438827b0a4`，候选 38,100B／SHA256 `2ab470b915119b8db63a5d29b82f3bbe125ff11af9f4a948b8433bfc2a31b12c`。只有这一源叶改变，原 20 锁保持；未做大范围格式化。

**新诊断叶尚未编译。** 577 的三个已验证程序仍只证明原始基线，不能扩大到新叶。610 正准备新 SOURCE／HOME／TARGET 的隔离三角色构建；计划复用已成功的依赖缓存，但选定程序必须重新生成，并重新记录原退出、EOF、SHA 与独立读回。当前不声称新构建或后续 VM 复验已经通过。

完整 47 步、basic 权限／SID／网络见证与正常 join／ACK／owner／factory／setup cleanup 仍未通过。SDK26／G04 OPEN，release_eligible=false；版本、提交／推送、标签与 Release 保持原状态。
