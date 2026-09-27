# 扩展预算插件的只读恢复绑定

日期：2026-09-27。分支 `codex/io-safety-refactor`。本轮继续上一轮 guest 审批与实际 SDK 接入，修复 mutation-budget-v1 包不能使用恢复发现／核对流程的问题。SDK 尚未冻结。

## 修复与边界

原恢复流程走普通 `bind_io`，它正确拒绝声明扩展预算却未获实际预算批准的包。直接放宽此校验会让旧执行入口绕过批准。本轮新增独立 `Manager::bind_mutation_history` 和工作台 `MutationHistory` 绑定：

- 仅扩展 mutation 包、当前包摘要／目录修订／启用状态及已批准的单个 Create 或 Delete 能力可准入。没有授予扩展 MutationBudget；计费仍使用包普通 IO 声明与普通硬上限。
- 同实例绑定历史后不能再绑定执行预算来重置账本。普通 `bind_io` 仍拒绝预算扩展包。
- worker 不接受任何 guest 作业；owner 通道仅允许 Discover、Next、Close、Reconcile。通用命令、原生目标选择、Prepare、Execute 等均被拒绝，派发层再检查一次。
- TargetBroker 的直接选择入口与资源签发也拒绝历史绑定，避免绕过 worker 重建目标权限。历史只核对原受保护记录，可写必要审计，不读取或修改目标文件来补造执行结果。
- 通用 `IoBinding::admit` 对历史绑定拒绝，避免公共 IO Broker 绕过 worker；发现／核对使用不对 SDK 导出的内部只读计费入口。
- Workbench 仅在发现／核对入口为扩展包选择历史绑定；旧包继续普通路径。既有协议、请求字段与 guest ABI 不变，不恢复旧句柄或执行许可。
- Guest 面板在真实退出、必要清理和 ACK 后可转到既有恢复页；只预填原包／subject／操作并滚动定位，用户仍须显式启动扫描。页面跳转本身不执行恢复或文件操作。

## 本轮验证

| 验证 | 结果 | 证据 |
| --- | --- | --- |
| Runtime 历史绑定、直接 broker、owner/guest 拒绝、普通额度、当前授权反例 | 4/4，含最后的公共 admit 加固 | `build/mutation-history-runtime-test-final.log` |
| 原 runtime 预算／owner／opt-in／核对回归 | 44 项通过；默认跳过的 6 项资格见下一行 | `build/mutation-history-runtime-regression-final.log` |
| Release 三语言真实 SDK 普通／16 MiB 资格 | 6/6，无跳过，8.22 秒 | `build/mutation-history-sdk-release-six.log` |
| 宿主完整 lib 回归 | 185/185，执行于最后公共 admit 加固前 | `build/guest-history-host-regression-final.log` |
| 最终加固后的宿主 mutation 与共享 owner 回归 | 43 项通过；42 项首跑通过，另 1 项补夹具后通过 | `build/guest-history-host-mutation-final.log`、`build/guest-history-host-owner-final.log` |
| Guest 恢复入口与插件设置 UI | 28/28 | `build/guest-mutation-recovery-handoff-widget-final.log` |
| 五个相关 Dart 文件经 Flutter 严格分析 | 0 诊断 | `build/guest-history-flutter-analysis.log` |
| Windows Release Rust/C/C++ 非空创建后正常重启与只读核对 | 3/3，未跳过 | `build/guest-mutation-recovery-ui/summary.json` |

真实验证使用临时受保护库及真实三语言 SDK Wasm，正文为高熵四块 184357 字节。Create/Query/Release/ACK 后显式只读核对并保存原计划、记录和结果字节；关闭原宿主，从外部覆盖目标为另一段内容，然后新进程打开同一库。错误摘要和撤销当前 file-create 批准均拒绝；重新批准后两次独立扫描／核对获得逐字节相同的计划、记录、结果及原操作 ID，阶段 Observed、结果 OsSucceeded。目标始终保留外部覆盖后的摘要，因此恢复没有重放原 Create。

最终生产代码成功 run：`build/guest-mutation-recovery-ui/run-20260927T091402384-1485a5ea`。早期成功 run 保留为中间版本证据。复验命令：`tool/verify_guest_mutation_recovery_ui.ps1 -Flutter C:\flutter\bin\flutter.bat`；默认 sysroot 为相邻 `../tools/wasi-34/wasi-sysroot-34.0`，可显式指定。脚本名保留 UI 集成路径命名，但本轮实际运行的是 Flutter 原生会话测试，并非跨重启 Widget 点击测试。

| 产物 | 本轮使用的 SHA-256 |
| --- | --- |
| Release 宿主 | `0f7c600b8f0bdc014e45bc2653d6d8e04939a477e55c311ac906815ae3e9f4f2` |
| 测试打包器 | `b30d56150b68281bf3e10c71c4c0a8f53335455582a35f85cd6b878f4a15756b` |
| 内置包，前后不变 | `c71dc27866378894d5288053238fef9e0ef15f6b578449279bc681e7fe8b560e` |

三语言 module/package 摘要见 summary.json。宿主及打包器因本轮代码重新构建而改变，验收期间保持固定；不宣称构建前后摘要相同。

首次测试中的错误假设已修正并保留日志：HostQuery 的 History 只有历史阶段／记录，没有完整 effect/outcome；完整结果必须由显式只读 Reconciliation 取得。宿主新增测试最初使用超过上限的扫描页大小，改为共享常量；可变 manager 测试访问改为 local_state_mut。全量宿主回归首次遗漏 MORROW_WORKBENCH_WASM 配置，补齐真实夹具后重跑。以上失败不作为通过证据。

普通 Dart 分析命令曾在关闭分析服务时发生 perf 临时文件删除错误（errno 1920）；使用 Flutter 严格分析入口成功完成检查，保留两类日志，不把失败退出当通过。SDK 契约同步检查通过。

Runtime Clippy lib 通过，仅保留此前 `collapsible_if` 与 `let_and_return` 两类例外，日志 `build/mutation-history-runtime-clippy-final.log`；不宣称无例外全仓 Clippy 通过。Rust edition 2024、Dart 改动文件格式及 git diff whitespace 检查通过。

## 仍未完成

本轮证明正常关闭后新进程恢复，不代表 guest 非空执行在进程崩溃、断电后的产品恢复界面资格。后续依次补真实崩溃到恢复界面的链路、16 MiB 全故障组合、完整 Windows 应用和系统选择器人工验收，以及其他平台。30 秒审批时限的人类可用性仍待验证，不能静默延长授权。条件 Replace 仍不提供不安全回退。

本轮未提交、推送或发布，没有操作用户真实内容库与目标文件。
