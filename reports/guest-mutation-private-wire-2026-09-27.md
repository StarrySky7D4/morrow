# Guest 文件变更私有协议与 Dart 客户端

本轮继续在 `codex/io-safety-refactor` 接通工作台 guest 任务的私有协议和类型化客户端。现有原生直接执行路径继续独立；本报告不将客户端接线等同于完成产品审批界面，也不作为 SDK 冻结结论。

## 实施

- `host.capnp` 追加五个 guest 调度动作：Start、Submit、Status、Read、CancelCommand。启动包含可信目标选择与可选的显式批准预算；目录声明不自动转换为批准。guest 路由拒绝混入原生变更字段及当前 guest 动作不适用的计划／正文等字段；业务分派器拒绝嵌套调度动作。
- 外层调度器调用原工作台任务 API。Prepare 的内部租约签发、Execute 的内部一次性许可签发都仍经原 owner 队列；结果区分可信 Owner 辅助回执、原始 Core guest 回帧和运行失败。
- 运行时仅在完成帧等于实际宿主响应、并通过原请求关联校验后保留原始字节。宿主直接传递这些字节，不重新编码或构造虚拟请求。保留字节计入领取上限，抑制结果时同步清除。
- 队列拥有的输入帧使用 `Zeroizing`；运行时解码的 Chunk 正文通过作用域清理覆盖早退与取消。宿主编码临时 Chunk 后清理正文，Dart 发送器清理 guest 启动／提交的 arena 与序列化副本。此范围不宣称清除 Wasm 内存、操作系统副本或所有依赖内部副本。
- Dart 增加独立 `GuestMutationBackend`、批准预算、命令、回执、状态和原始帧模型。读取必须携带原命令回执，UInt64 使用 BigInt；选择目标 reference 与 Prepare 后取得的 guest 租约 reference 分开保存。后续帧逐项核对任务、命令、提交、操作、类型及租约。
- `RustWorkbench` 接入独立客户端；服务运行期间五个动作仍走外层调度器。每个方法仅进行一次显式交换，不自动重放任务或恢复旧许可。
- 交叉审查修复三处客户端边界：直接构造读取回执也必须提供后续帧的原租约引用；Prepare／Execute 内部 Owner 审批失败保持类型化失败；成功 Release 已清空目标而保留历史批准时，仅允许已终态 Release／HostRelease 的这类状态，不误报为矛盾。
- 标准 Dart 生成工具纳入 Core mutation schema，生成 native／web 绑定和摘要；这不表示 Web 已有原生文件变更适配器。

## 验证记录

以下记录为 Windows 本地验证；各专项存在重叠，不加总为独立测试总数。

- Runtime 原 owner／opt-in／只读核对：40 项通过。常规运行中 6 项 SDK 实际插件案例标记 ignored，随后由资格脚本显式执行，不能把 ignored 计为通过。
- Runtime Rust／C／C++ 普通及 16 MiB 实际插件：6/6 通过，完整内容本轮约 1.99–2.39 秒。日志 `build/guest-mutation-wire-runtime-sdk.log`。
- Release 三语言私有协议 → 原 owner：3/3 通过，严格核对所载 Wasm 摘要及精确测试名。高熵正文为 3×60 KiB＋37 B，检查准备前后无文件效果、第二次明确 Execute、实际创建／删除、结果关联、Release 和退出确认。日志 `build/workbench-guest-private-wire-qualification.log`，摘要 `build/workbench-guest-private-wire/summary.json`。
- Flutter 敏感请求清理与受控进程路由：31/31 通过；该管道测试不冒充真实 Rust 服务执行。日志 `build/guest-mutation-wire-flutter-routing.log`。

- Release 宿主全量 182 项及目录／IO 集成 10＋5＋2 项，共 199 项通过，0 失败／ignored。日志 `build/workbench-guest-private-wire-host-regression.log`；保留既有 unused/dead-code 警告。随后补入独立 Owner 预检失败用例，最终 Release 私有协议专项 4/4 通过（与全量重叠），日志 `build/guest-mutation-wire-final-focused.log`。该失败用例首次短名配合 `--exact` 的零匹配运行不计入通过。
- 原生直接执行／恢复客户端、会话与新宿主导出的原生协议夹具回归 56/56 通过，日志 `build/guest-mutation-wire-native-client-regression.log`。

- Guest Dart 受控客户端及真实响应完整 `read(receipt)` 路径 11/11 通过。覆盖 Create／Delete 全部回帧、Owner 选择／计划／IssueGuest 失败，以及真实 Release 终态。日志 `build/guest-mutation-dart-final-review.log`。此前只调用结果解码的测试未覆盖完整状态解码，复核先用真实 Release 帧复现失败，再修正并补完整读取路径。
- 对三语言真实 SDK 私有协议资格导出的原始帧分别复验 Dart 完整读取：Rust／C／C++ 各 2/2，通过日志 `build/guest-mutation-dart-{rust,c,cpp}-fixtures.log`。这些是已执行插件的回帧复验，不是 Flutter 端直接驱动三种插件。
- 新客户端与测试严格分析 0 诊断；共享接线 8 个文件分析无问题。标准 codegen `--check`、本轮 Rust 格式、PowerShell 脚本语法及变更空白检查通过。

## 可重复入口

`tool/verify_workbench_mutation_guest.ps1 -PrivateWire` 会编译三语言 guest 与 Release 宿主，检查精确测试注册、实际载入摘要和构建前后产物身份，并按语言导出真实私有响应夹具。默认模式仍验证上一阶段 Rust 任务 API。运行时最大内容资格仍使用 `tool/verify_plugin_mutation_wasm.ps1`。

## 尚未完成

1. 产品 guest 独立会话及审批界面：实际批准预算、计划审阅、明确 Prepare 与第二次 Execute，保留原回执而不复用原生执行状态。
2. Flutter 到真实原 owner 的 guest 按钮联调，包括丢回执、隐藏页面、资源退出和 Unknown 恢复。
3. `mutation-budget-v1` 包在当前重新授权下的跨重启只读恢复；不得重建旧租约或自动重放效果。
4. 完整 16 MiB 故障矩阵、其他平台资格与具有条件保证的 Replace。

本轮未提交、推送、打包或发布。SDK 尚未冻结。
