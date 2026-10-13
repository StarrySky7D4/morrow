# Windows harness／runner／setup 当前限定进展（阶段 590，2026-10-13）

当前三个原始角色已经实际编译并通过独立读回；真实安全执行链推进到一次性 Claim，但原生启动／握手未闭合。结论是 **编译通过、真实安全链未通过**，SDK26／G04 仍 OPEN，release_eligible=false。

## 已通过

- 570 的原 harness 构建、576 的原 runner 与 setup 构建：原 Cargo 与外层捕获均退出 0，双流 EOF、命令、进程身份、产物与原始源码／20 个原锁绑定得到独立 577 复核。2394 个选定源文件保持原字节。
- runner／setup 从原 harness 构建根选取原 dependency-package 的原 bin；没有删成员、改锁、重打包旧 H011 程序或伪造 provider。复用了本轮成功的 harness 依赖缓存，并明确记录缓存复用。
- 原始 HOME 内容验证与独立调用／原始双流绑定是分别核对的证据；577 补齐 576 源审发现的绑定缺口，保留原 P2 报告，不反改旧报告为通过。
- 新的固定三角色副本与七文件验收包完成两遍完整 SHA／CRC 检查；各次真实 transfer 完成七文件 guest 读回与自身会话闭合。
- 只读前提检查每次为 17 项，查询无错误。它只覆盖固定账户、服务、规则名、已知进程名与选定路径，不证明全局 WFP 或全系统进程为空。

| 角色 | 字节数 | SHA256 |
| --- | ---: | --- |
| harness | 95,393,280 | 3b7ff6ed6844034162804f3b8419e26fe57a2fc95058d63de3311cb749a09f2e |
| runner | 24,366,080 | d63114a9612a7dc6bd665c1acd2eefb9d50d5847982b1e3b4d496aac340e58e3 |
| setup | 37,714,944 | eaaa899a4b7743f681d541c7d34d472623d78c7b39e8880faf0a72040d44a7dc |

## 本地验收载体修正

1. 在原 30 秒 review TTL 之前增加独立的 25 秒 review→Start 预算门，从 review 发送前计时，context 不重置。原 60 秒 grant、35 秒 Start 等待与一次性授权不变。
2. 修正远程双参数调用中多余的一元逗号。旧写法把 byte[] 又包入 Object[]，在远端正文开始前绑定失败；系统 PS5 的普通三字节合成回归复现旧错误并验证修正。
3. 将独立 harness 子进程的 CODEX_HOME 固定到本次合成 home。原物化使用合成 home，而后端按进程环境解析 home；本地子进程回归确认父环境不变，真实 590 日志确认 launcher 复用原物化路径并通过 RunnerPinVerify。

以上为本地资格验收载体修正，三个原始 Rust 角色与 20 个原锁没有因此改写。通用后端显式携带 provisioning-home 的接口仍是独立设计缺口，不能因单个 harness 的环境修正宣称已补齐。

## 真实执行结果与未闭合处

- 579：transfer 通过；execute 在远程参数绑定处失败。原连接保留。后继只读检查确认固定旧 attempt、evidence、controller 不存在、已知 actors 为 0；不将原连接 Unknown 写成正常释放。
- 583：新 campaign 实际完成 39 个原始命令，包含固定 setup、物化、原受保护 owner／factory、Wasm 会话与 guest proposal、trusted review、一次性 Claim。native Start 尝试在 RunnerPinVerify／PinRejected 返回 Unknown；没有 Start 确认，不重试。
- 590：在保存旧失败日志与本地 Standard 内存检查点、显式恢复测试基线后，以新 campaign 执行。原物化路径校验通过；同样完成 39 个命令后，在 PipeConnectIn 出现 StartWaitTimeout，真实启动确认、provider-ready 与实际句柄仍未观测到。已读回的 89 条 journal 为有界现场前缀，不是最终验收收据。
- 限定应用事件查询没有找到匹配的崩溃记录；这不能证明 runner 没有崩溃。原 runner 的静态 PE／PDB 检查显示 main 与 read_frame 非巨型栈帧，也不能排除其他早退／加载器／握手问题。
- 每个 campaign 的 Claim 只发送一次；Start 只尝试一次。失败记录、原连接及 owner 状态保持 Unknown，未通过后继改写或自动重放。恢复测试基线不等于原 owner 正常 cleanup。

588 的失败现场复制曾因仍持有的写入流而失败，原失败保留；589 只读后继按固定名单以共享读写模式取回 44 份原公开日志／步骤记录，逐份核对 guest／传输／host／guest 后验 SHA 并关闭自己的诊断会话。没有复制内容库、DPAPI 载荷或密钥。

## 后续验收门槛

优先定位 runner 早期管道握手，补足实际进程终态与错误观察；修正必须保留原路径／SHA／持有句柄、原 review／Claim 与 no-replay 门。只在新 campaign 复验，不向旧 Unknown 发送新的业务命令。

47 个命令的完整正常链、basic 子进程的工作区写入／外部写拒绝／实际 SID／网络 10013 见证、join／ACK、factory release、owner finish 与原 owned setup cleanup 均未在当前组合通过。原 555 supervisor 正常路径也未在本轮重跑；[565 真实 RPC 故障隔离](windows-startup-quarantine-acceptance-565.md)仍只保留其原限定结论。

独立 derived workspace 的开发依赖闭包、全 OS 后代树静默、namespace／持续 reparse、生产 GUI 与跨平台资格仍未完成。此前 [v12 合成回归](startup-rpc-fault-validation-v12.md)与 23 项 Dart 回归保留历史范围，本轮没有重跑。

应用版本保持 0.1.9-test.58+62；本轮没有 commit、push、CI、标签或 Release。会话层与安全执行层完成验收后再准备测试预览，扩展执行层延期。
