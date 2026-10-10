# Windows Agent SDK C28 阶段 25：本地验证与公开范围

2026-10-10。原始 11 项库测试已实际 `PASS`（exact 8+3）；Workbench 23 项测试已实际 `PASS`（exact 6+5+12）。两组对应编译、测试捕获和外层均闭合于退出 0，源码及严格 HOME 守卫通过。结果限定于本报告列出的本地范围。应用版本仍为 `0.1.9-test.58+62`；本阶段不发布 Release。

## 本轮宿主改动

四处改动涉及宿主命令定义、命令观测接线、有限诊断类型与可选 Agent SDK discovery metadata。既有执行权限、owner 身份、生命周期控制和独立 admission 继续由原合同决定。

`AgentCommandSnapshot` 是同一条原命令的一次一致、只读本地观察，字段为：

| 字段 | 含义 |
|---|---|
| `worker_stage` | worker 的有限阶段观察 |
| `worker_error_class` | worker 的有限错误类别 |
| `delivery_stage` | 交付端的有限阶段观察 |
| `delivery_error_class` | 交付端的有限错误类别 |
| `command_started_observed` | 是否观察到命令开始 |
| `provider_observed` | 是否观察到 provider |

阶段类型包含 18 个已观察值：`Queued`、`Received`、`CommandStarted`、`Checkpoint`、`OwnerMismatch`、`PrepareIo`、`PortStart`、`PortRejected`、`ProviderObserved`、`RegisterProcess`、`RegisterRejected`、`ControlVeto`、`ReceiverError`、`ReceiverDisconnected`、`ReplySendFailed`、`ReceivedOk`、`Cancelled`、`PortAbsent`，另有 `NotObserved`。

错误类型包含 19 个类别：`Invalid`、`Limit`、`Busy`、`Cancelled`、`Unknown`、`Unavailable`、`Maintenance`、`Disconnect`、`Unsupported`、`R2Invalid`、`R2Contract`、`R2Limit`、`R2Correlation`、`R2Denied`、`R2Conflict`、`R2NotFound`、`R2CommitUnknown`、`R2Storage`、`RegistrationError`，另有 `NotObserved`。未知编码映射到 `NotObserved`；`NotObserved` 或 false 不证明没有发生副作用，也不许可自动重试。

快照不携带原始系统错误、账户、命令载荷、回调或执行句柄；它不是持久化记录或 wire 合同，不能批准、取消、恢复或重放操作。可选 Agent discovery metadata 仅描述 session/process 原合同的身份、宿主前提和字节上限。其 `authority=none`，不创建宿主、不打开 owner、不启用 dispatch route；Workbench routes 为空，自动运行及生产公开绑定均未开放。包 preflight、原 owner、完整 wrapper 审核与当前独立 admission 仍是各自必要条件。

## 已闭合的本地证据

| 范围 | 已观察结果 | 边界 |
|---|---|---|
| 四处改动的普通 SDK library 整合构建 | Cargo 退出 0，外层退出 0，输入和 HOME 守卫通过 | 普通 library 编译；不执行产物，不证明单元测试、生产 GUI 或 SDK 冻结 |
| 两成员 Windows 验证副本的派生锁 | 由同版本 Cargo 离线生成 635-package 锁；后续 locked/offline metadata 退出 0，锁稳定 | 本地验证锁；不替换生产或原 900-package 锁 |
| 当前 metadata 与源码图 | 544 packages / 544 resolve nodes；图与来源审核实际退出 0 | 当前 Windows 两成员 profile；不是 Cargo unit graph、测试执行或所有 feature/platform 等价证明 |
| 原始 11 项库测试 | `PASS`，exact 8+3；清单 284 项，仅运行选定 11 项 | 其余 273 项未执行；PTY 测试未执行，生产原生资格仍未闭合 |
| Workbench 23 项测试 | `PASS`，exact 6+5+12，每项 1 passed / 0 failed / 0 ignored；完整清单 285 项 | 其余 262 项仅列出、未运行；不是完整生产资格或 SDK 冻结 |

图审核覆盖 registry/vendor checksum 来源、21 个路径包的 manifest 与原源码连续性、两个实际 Git 包身份，以及选定 11 项测试的闭包和当前 feature 边界。选定闭包是当前 metadata 的 feature/dependency-kind 投影，不是编译器实际执行单元图。没有把 metadata shape 或子进程退出 0 单独当成图通过依据。

原 900-package 验证锁向 635-package 派生锁的变化经过独立语义审查：删除 265 个 package，10 个保留 package 共删除 12 条依赖边，未新增 package。变化不全是不可达行裁剪；原 all-kind 闭包中的 `zeroize_derive` 在当前未选 derive feature 下被移除。此解释限定于实际 metadata 的当前 feature，不证明旧 feature 集或其他平台等价。原生产锁、原 900-package 锁和冻结插件保持原字节；原 11 项测试方法源码保持原字节。

## 原始 11 项的闭合范围

在原方法源码保持原字节的前提下，8 项 start diagnostic 和 3 项 runner client 测试分别通过 `--exact` 选择并各运行一次。测试程序列出 284 项，实际执行这 11 项；其余 273 项未执行。清单捕获及 11 次运行均有真实退出 0，测试外层退出 0，所有必要捕获闭合；输入前后和严格 HOME 守卫通过。测试 HOME 的前后状态等于已成功编译的合法 HOMEafter。

对应编译仅选择两个原 workspace 成员，执行 library `--no-run`；Cargo 与外层均退出 0，输入和 HOME 守卫通过。只选定 sandbox library test artifact 用于这 11 项运行，PTY test artifact 未执行。这些测试不覆盖 Windows 生产沙箱、原生 Start、真实账户/会话或全部生命周期验收。

此前单成员 test-build 在原 PTY 源码产生三处 `c_void` 类型不匹配。实际编译的 winapi 未选 `std`，与 metadata 中已包含 `std` 的 feature union 不同；原 PTY Windows dev 依赖本来就声明 winapi/std。后续只增加第二个原成员作为 `--lib --no-run` 编译根，沿原声明满足该 feature，编译完成；没有修改源码、manifest、依赖身份或锁，也没有增加 PTY 测试执行权限。

成功尝试从失败尝试经过严格守卫接受的 HOMEafter 继续，并据其真实演化进行前后核验；旧失败的收据、捕获和 HOME 历史均保留。没有删除失败现场、伪装成 fresh metadata HOME 或以成功覆盖失败。

## Workbench 23 项的闭合范围

Workbench 23 项测试已实际 `PASS`：exact 6+5+12，每项一次、1 passed / 0 failed / 0 ignored；完整 library 清单 285 项，其余 262 项仅列出、未运行。library `--no-run` 编译的 Cargo 与外层均退出 0；清单及 23 次运行的捕获和测试外层均退出 0，源码、物理与严格 HOME 守卫通过，测试 HOME 前后等于成功编译的 HOMEafter。此结果不替代完整生产资格或 SDK 冻结。

后继独立测试源码副本保留原 1655 个文件字节，仅补入固定开发基线原有的 1606-byte 测试公共 helper，形成 1656 个文件；产品源码、原锁和 23 个方法保持原字节。成功编译产物随后由 tests-only 后继复用，未重编译；后继只让经过编译记录绑定的原产物和保存副本两个精确路径按原有 128 MiB PE 上限读取，其他文件仍使用原上限。旧失败与其守卫接受的历史现场均保留。

## 保留的失败与未知

Workbench 的三次入口失败继续保留：首次 preflight 将原 workspace 错误要求为空，实际原声明为 resolver 2，拒绝发生在 Cargo 启动前；后一次编译因验证载体未包含原测试公共 helper 而退出 101；再一次测试入口在清单或方法启动前，因将 64 MiB 通用读取上限误用于 71,202,816-byte 测试程序而拒绝。它们分别是入口检查、验证副本闭包和守卫读取上限失败，不能写成选定测试方法失败。

早前离线 metadata 与受限上下文中的库测试编译曾报告固定 crossterm patch 无法离线 checkout；该 patch 不在当前已解析 package 集中。已完成的前后守卫没有发现锁、源码或 HOME 变更。进程上下文与 Git 配置隔离变量存在差异，但未建立单一因果结论。后续验证不覆盖原失败。

图读取器的早前尝试也保留失败：实际 HOME 报告的 exact-seed 字段协议、显式 comparator 的末尾 wildcard 显示规范化及读取器局部变量问题，经独立 SOURCE 审核后修正，随后实际图审核完成。它们不能被改写为 Git 源码损坏，也不能凭 SOURCE 静态审核冒充已运行通过。阶段 18 的 Cargo 退出 0、外层退出 1，以及原生 Start 的 Unknown 历史仍按原记录保留。

`SDK26_G04=OPEN`，`release_eligible=false`，native 状态为 `SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY`。本阶段不重放原生 Unknown，不运行虚拟机或启用生产原生执行。owner finish、factory release、cleanup/join、真实断连、Windows 生产沙箱、账户/TLS/公开服务、GUI 与跨平台资格仍分别待验收。

## 公开内容与范围

公开候选范围仅为四处宿主源码改动、五份当前状态文档标记及本报告。文档和代码版本不因这些本地证据自动提升；SDK 未冻结。本报告不将本地验证或文档生成当作提交、推送或发布证明；不创建 Release。

原始 11 项和 Workbench 23 项的结果来自各自真实闭合记录与独立读回，保留失败、未运行及 Unknown 的区别。源码准备、批准、metadata 或普通构建均不能代替实际测试退出证据。开发分支提交与推送仍须按各自真实记录核定，本报告不提升原生产或原生资格。

公开材料不包含私有 Drive 内容、账户或凭据值、本机绝对路径、原始本地日志、验证脚本、HOME/vendor/target 大树、编译产物或私有收据。独立验证仅在本地证据链中保留；此报告给出范围和结果，不构造公开重放入口。

历史参考：[阶段 21 合成会话](windows-agent-sdk-c28-stage21-session.md)、[阶段 21 构建历史](windows-agent-sdk-c28-stage21.md)、[阶段 18 编译历史](../reconstruction-2026-10-09/windows-agent-sdk-c28-stage18.md)。当前状态入口见[项目状态](../../docs/PROJECT_STATUS.md)与[文档导航](../../docs/DOCUMENTATION_INDEX.md)。
