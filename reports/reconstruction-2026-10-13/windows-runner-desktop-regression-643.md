# Windows 安全执行链修补复验与暂停检查点（2026-10-13）

按用户要求暂停后续开发与验收，只整理当前结果并同步现有开发分支 `codex/windows-sdk-convergence-20261005`。完整安全执行链仍未通过，SDK26／G04 OPEN，`release_eligible=false`；应用版本保持 `0.1.9-test.58+62`。

## 本轮源码修补

- supervisor 增加异常资源闭合协议版本字段。565 的 RPC 故障资源隔离已有原限定 PASS，业务仍为 UnknownRetained、normal=false、accepted=false；这不代表正常启动／退出路径通过。
- runner client 在进程创建前绑定已选私有桌面，并在原启动流程中持有桌面及 UTF16 缓冲；保留 603 的原进程身份与清理前退出观察。
- desktop helper 读取当前窗口站名称，替代 Winsta0 硬编码；按长度、终止符、UTF16 和名称规则拒绝异常值。借用的窗口站句柄不关闭；现有 ACL、批准策略、v7 协议和重试规则未放宽。

这两处桌面接线修补已应用并编译，但下述真实复验仍失败，不能称为已修复 DLL 初始化根因。现有启动失败路径中的 TerminateProcess 未移除；清理前观察到退出不等于自然退出或正常回收。

## 已通过的限定验证

634 harness／runner／setup 三个选定程序均实际重新生成：Cargo 退出 0，外层退出 0，完整双流 EOF。源图为 2394 叶，其中原基线 2392 叶保持原字节，另两叶为当前 desktop 与 runner client 修补，后者包含 603 诊断；20 份原锁保持。使用独立 SOURCE／HOME／TARGET 并复制复用既有依赖缓存，不称为全部依赖从零构建。

636 独立实际读回核对原 API 链、进程／四时钟、完整双流、源图／锁、当前与旧 PE、缓存守卫及最终严格 HOME，结论为 PASS_COMPILE_ONLY。648 又对 91 份固定交付文件与实际产物作有界复核，无漂移；该交付复核没有重新运行全部缓存／源图检查或虚拟机验收。

| 角色 | 字节数 | SHA256 |
| --- | ---: | --- |
| harness | 95437824 | `6c765c4fa5d51db460f63eed91f76d6a4913e0c6e9bcde9d0d13a078e66f8913` |
| runner | 24352768 | `736ec2046bf2e4ed5188588eb16df9f851e8605f4465cbabe80b91f068fbe8b3` |
| setup | 37687808 | `b9ace394ac536394703aa4037ccfb371595cebe5fa1983ab473e13b541560103` |

637 的独立 helper 测试实际 7 项通过：6 项纯解析／边界测试，加 1 项当前测试进程窗口站的只读 API 查询。641 已复核原编译／测试退出 0、双 EOF 与完整输出。它不验证 SandboxOffline 账户能访问该窗口站，也不是完整库回归。

| 证据 | SHA256 |
| --- | --- |
| 636 实际三角色读回 | `75b84908fd3d10e1f93aa89f0f10a4586b9cdf116b966d51253c109278733351` |
| 641 helper 实际测试复核 | `a6287007bc917ad768292cf74a9a35f403ae733c9e61a3fbb39022562d876332` |
| 648 实际交付复核 | `d5731b8056ae9368b2379d397a7b6276467860417e4287ba18557e3892e3c6fc` |

## 643 新 campaign：真实执行未通过

旧 619 Unknown 已保留检查点后恢复指定基线；这不是原 owner 的正常 cleanup，也不改变旧失败。643 使用上述新三角色产物，传输连接已明确闭合，随后完成 39 个命令，覆盖 setup、原 owner／factory、Wasm 会话、proposal／review 和一次 Claim。

原 journal 保留一次 native Start 尝试，未得到启动 ACK。failure.json 的 start_attempts=0 是未完成启动返回的计数，不能据此称没有尝试 Start；journal 明确记录尝试与随后 Unknown。649 的限定只读观察确认，同一 runner 在库内既有清理之前已以 `0xc0000142` 退出。独立观察连接已关闭，但原执行连接和 owner 没有闭合证明，保留 Unknown，禁止重放。

因此首跳桌面绑定和实际窗口站名修补没有解决当前真实初始化故障。`0xc0000142` 对应 DLL 初始化失败；具体 DLL 及窗口站／桌面访问权限是否构成原因尚未证实。[微软状态码定义](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-erref/596a1078-e883-4972-9bbc-49e60bebca55)及 [CreateProcessWithLogonW 的访问与初始化要求](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createprocesswithlogonw)是后续诊断依据，不能代替当前权限实证。

## 暂停现场与恢复顺序

暂停时不发起新 Claim／Start，不结束原测试控制进程，不恢复虚拟机检查点，也不继续安装或修改权限。643 最后观察到的日志、原 API 回执、已完成构建与本机恢复索引均保留。643 尚无正常退出／释放证明；619 的 44 份归档不能冒充 643 完整归档。

恢复开发后，先核对原失败现场和当前源码身份，再获取实际启动上下文及目标账户窗口站／桌面访问证据。若证实需要独立窗口站，先验证受控 keeper 的新对象 ACL、所有权、runner／子进程／双流 join 和可确认释放，再使用全新 campaign 单次复验；不扩大既有交互或服务窗口站 ACL，不以延长超时替代修复。keeper 目前仅为设计，没有实现或测试。

完整 47 步、basic 文件／SID／网络见证、join／ACK、owner finish、factory release 与正常 owned cleanup 仍待验。555 正常 supervisor 路径未重跑；历史 Rust 合成／Dart 子集、SDK26／G04、通用 provisioning-home、namespace／持续 reparse、全 OS 后代树和生产／跨平台资格均保持原范围。Agent 会话层与安全执行层完成后的预览版准备仍待这些验收，不等待扩展执行层完工。

本次同步不创建标签或 Release，提交使用 [skip ci]；提交与远端成功状态以实际 Git 回执和远端 ref 核验为准。早期 [590 进度](harness-runner-setup-progress-590.md)、[603 诊断](windows-runner-startup-diagnostic-603.md)、[613 构建](windows-runner-diagnostic-build-613.md)、[619 失败](windows-runner-initialization-failure-619.md)及 [565 资源隔离](windows-startup-quarantine-acceptance-565.md)保留其历史时点与边界。
