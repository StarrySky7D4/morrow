# Workbench guest 文件变更接入

本轮将运行时已有的 `mutation-v1` 和显式预算接入工作台任务层，并让插件目录展示其声明。SDK 仍为实验状态；此报告不把声明展示、Rust 任务 API、Flutter 实际执行界面视为同一个验收门槛。

## 实施范围

- 私有目录 `PluginEntry` 追加 `mutationSupported @15` 和可空 `mutationBudget @16`，预算为两个 UInt64。包缺失时不保留可执行能力声明，旧包无扩展时仍为 false/null 或 mutation-v1 + null。
- Flutter 使用 BigInt 读取声明，校验正值、单次不超过 32 MiB、累计不超过 256 MiB、累计不小于单次，以及预算与 mutation 能力的关联。详情只读显示声明上限和每次操作仍需批准的提示，覆盖九种语言。
- `start_selected_guest_mutation` 是独立的可信宿主入口。批准预算与包、摘要、目录修订、目标、主体、期限和审批摘要一起进入启动提交指纹；扩展预算不能由包声明自动获得。
- 原受管 owner、Store 和 worker 保持单一。Select 和 BuildPlan 使用可信 owner；Prepare 经内部签发和领取 Stage 租约后运行真实 guest；Execute 需要另一条精确计划摘要的显式命令，内部领取一次性许可后才运行 guest Execute。
- guest Frame 与可信 Owner 回复分别呈现；不为模拟旧原生结果而自动追加 Query，也不在 Wasm 失败后降级为原生文件效果。
- 旧 native 命令入口拒绝 guest TaskKey。必要的 HostQuery、HostCancelPlan、HostRelease 是显式历史核对／清理动作，不允许重新执行文件效果。

路径、租约和执行许可留在宿主；插件只收到原计划对应的有界帧。审批队列的推进是非阻塞的，读取状态可以推进已经明确提交的内部步骤，不能创造新的批准或重发不确定的 Execute。

## 验证记录

宿主 guest 专项 **7/7** 通过，使用真实转发 import 的独立 WAT。覆盖非空四块 Create／Delete、第二次确认、Release 后真实 join／ACK、预算双维去重与缺失批准拒绝、普通 mutation-v1 的旧预算路径、未读命令与取消、不确定状态下禁止重执行。Commit 已 Ready 后通过公开 cancel 接口放弃回执，再 HostQuery 恢复真实 staged／durable；另以明确注入的 retained-attempt 状态验证 Absent／Prepared 不能清除不确定。后者是状态回归，不冒充真实 OS 崩溃验证。日志：`build/mutation-guest-host-tests.log`。

Rust／C／C++ 实际 SDK 经 Windows Release Workbench API 的非空 Create／Delete **3/3** 通过。每轮严格加载指定 Wasm，测试打印的实际模块 SHA-256 与文件摘要一致；结果见 `build/workbench-mutation-guest/summary.json` 及 `build/workbench-mutation-guest-qualification.log`。三模块摘要与此前 runtime 资格原件一致；本次宿主测试可执行文件摘要为 `b081e58d14be3bd4fe75412fdd03400bbcd3c13075ce08c5932d086d0c75b5bf`。

宿主完整回归首轮为 119 passed / 60 failed：51 项缺 `MORROW_WORKBENCH_WASM`、5 项缺 `MORROW_HTTP_FORWARD_WASM`、4 项缺 `MORROW_SERVICE_OUTBOUND_WASM`，全部在夹具加载处报告 NotPresent。保留原日志 `build/workbench-mutation-guest-host-regression.log`；从本地源码重新编译三个实际 Wasm 并配置路径后，Windows Release 宿主 **179/179**、目录 **10/10**、IO 批准 **5/5**、mutation 目录 **2/2** 全部通过，共 **196** 项，0 失败／忽略。日志为 `build/workbench-mutation-guest-host-regression-with-fixtures.log`。七项 guest 专项已包含在 179 中，不重复计入总数。

已完成的目录层验证：Flutter 目录与编解码测试 23 项通过；i18n 生成检查 22 份产物通过。限定文件 Flutter 分析没有 error/warning，另有 `_openMutationRecovery` 的既有花括号 info；独立 Dart analyzer 曾因 AppData 性能记录清理发生内部错误，未将其当作静态检查通过。

| 最终 Dart／Flutter 回归 | 通过 | 证据日志 |
| --- | ---: | --- |
| 目录模型、详情与新字段编解码 | 23 | `build/mutation-catalog-dart-tests.log` |
| 请求敏感字段与受控 Python 子进程调度／管道传输 | 26 | `build/mutation-guest-catalog-wire-regression.log` |
| 既有文件执行与恢复界面 | 22 | `build/workbench-mutation-guest-ui-regression.log` |
| 类型化客户端、会话及新导出的 Rust native 协议夹具 | 36 | `build/workbench-mutation-guest-dart-client-regression-with-fixtures.log` |

上述 **107** 项均实际执行，没有把首次缺少夹具时的 skipped 计为通过；它们仍是既有 native 路径及目录回归，不是新 guest Flutter 界面验收。Workbench 绑定生成检查、i18n 检查、新增 Rust 模块格式检查及 `git diff --check` 通过。旧 transport 17 个原件、SDK 36 个文件／13 对 Wasm 包摘要检查通过；这类摘要检查单独不构成运行验证。

可重复入口：`tool/verify_workbench_mutation_guest.ps1`。它构建 Rust／C／C++ 示例和 Windows Release Workbench 测试宿主，严格检查测试名称、实际通过数量和 ignored 数量，记录宿主与 guest SHA-256；开始即将摘要标为 running，失败改为 failed，避免旧的 PASS 被当作本次结果。该脚本的范围是 Rust Workbench API，不是 Flutter、系统选择器或跨平台验收。

审查中修复了内部 Issue／Authorize 的二次读取竞争、未识别 Owner 结果误投影、Release 未启动回收，以及失败／缺失历史错误清除核对要求的问题。HostQuery 会同步 Prepared 的暂存状态；已有的原效果证明不会被后续只有阶段的查询抹掉。仅 JobReport 外层读取容量使用 `2 * 128 KiB + 4096` 容纳完成帧和已解码元数据；guest 单帧 128 KiB、最大响应预留和运行时计费没有变更。

## 后续门槛

1. 为 guest 启动、命令、状态与 Owner/Frame 结果添加独立的私有协议和 Dart 类型；保持旧 native 路径语义清楚。
2. 接入实际批准预算、计划审阅与第二次 Execute 确认，确保切页、丢回执及重启恢复不自动重新执行。现有 native discovery/reconcile 仍使用普通 IO binding，不能用于声明 `mutation-budget-v1` 的包；需单独接入当前授权下的只读恢复，不能将新启动租约当成原执行许可。
3. 用真实 native host 验证 Flutter→私有协议→Workbench→Wasm→原 owner 全链路，包含非空正文与实际系统选择器验收。
4. 保留原有完整 16 MiB 故障组合、平台资格、条件 Replace 和 SDK 冻结门槛。此前 runtime 的 Windows 测试不能替代这些验收。

本轮不更新应用版本，不提交、推送或发布。
