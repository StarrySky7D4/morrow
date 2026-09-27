# 真实宿主故障后的恢复界面联调

2026-09-27。工作树 `codex/io-safety-refactor`，基线 `b9225f64f6c62584ad7243e30249d8a088bcb155`。本轮补齐上一轮会话级崩溃矩阵与 Flutter 恢复面板之间的验证；没有修改生产恢复逻辑，也没有提交、推送或发布。

## 本轮结果

新增 `test/mutation_recovery_crash_widget_native_test.dart`。使用 `LiveTestWidgetsFlutterBinding` 的真实时钟，运行真实 Flutter 控件与 Windows 宿主；默认测试视口、真实滚动及点击，没有替换为假后端，没有直接调用恢复会话业务方法绕过按钮。

事故前通过原生 API 在独立临时受保护库中导入、审批插件并准备计划，注入 Create／Delete 故障，等待实际退出码 86，再以新宿主重开同一库。事故准备并非产品变更编辑界面，不能把本轮宣称为完整文件变更 UI 已完成。

恢复阶段实际点击：选择操作类型与插件、填写精确主体 → 发现 → 领取历史页 → 选择计划 → 离页并重新挂载 → 关闭发现任务 → 领取关闭结果 → 等待实际退出并确认 → 独立核对 → 领取核对结果 → 确认返回 Local。恢复阶段只通过按钮派发，session／view 仅供断言。

验证内容包括：

- 历史页返回唯一原计划，字节与事故前一致，不带可冒充执行结果的 phase／effect。
- 所选计划绑定原主体及 disposition；旧任务未退出确认前，核对按钮不可用。
- 离页返回保留相同发现请求和所选计划，文件状态不变。
- after-claim／after-effect 显示 OutcomeUnknown 和未指定效果，不能显示 OS 成功；after-observe 显示 Observed 及保存的 OS 成功。
- 领取核对结果后等待 ACK 返回 Local；卸载面板后 await 新宿主实际 close，随后清理临时库。新宿主的关闭错误不吞掉。

## 完整矩阵与证据

`build/mutation-crash-widget-matrix-verified/summary.json` 标明 `suite=widget`、`complete_matrix=true`，**7/7 通过、无跳过**：

| 操作／故障点 | UI 核对阶段 | 结果 |
| --- | --- | --- |
| Create after-claim | OutcomeUnknown，无 OS 成功文案 | PASS |
| Create after-effect | OutcomeUnknown，无 OS 成功文案 | PASS |
| Create after-observe | Observed，OS 成功文案 | PASS |
| Delete after-claim | OutcomeUnknown，无 OS 成功文案 | PASS |
| Delete after-effect | OutcomeUnknown，无 OS 成功文案 | PASS |
| Delete after-observe | Observed，OS 成功文案 | PASS |
| 普通宿主＋Create after-claim 环境变量 | 不崩溃，Observed／OS 成功 | PASS |

宿主与插件夹具 SHA-256 与 [上一轮会话级矩阵](mutation-crash-recovery-2026-09-27.md) 相同，完整值保存在本轮 summary.json 中。

- `build/mutation-crash-widget-regression.log`：既有恢复面板 11 项及 EOF／send 竞态 1 项，**12/12 通过**。
- `build/mutation-crash-widget-final-analyze.log`：最终新增测试严格 JSON 分析，`diagnostics: []`，退出码 0。
- `build/mutation-crash-runner-suite-control/summary.json`：runner 增加 suite 选择后，原 native 普通构建对照 **1/1 通过**；这是单例而非重新跑完整 native 矩阵。
- 独立只读复核确认真实时钟、恢复按钮调用链、最终 Local 断言及真实 close 等待，没有发现阻断问题。

脚本 `tool/test_mutation_crash_recovery.py` 新增 `--suite native|widget`，summary 记录 suite 和 test_file，默认输出目录分开。原有 native 用法不变。复现命令沿用上一轮的所有宿主／夹具参数，增加 `--suite widget --output build/mutation-crash-widget-matrix-verified`；诊断可增加 `--case create-after-effect`，此时证据明确标为非完整矩阵。

## 调试过程与测试边界

最初的 Automated 假时钟测试存在选择包前等待 Discover 可用的错误条件；随后暴露出弹出路由返回触发实际状态刷新，与假时钟 pumpAndSettle／清理等待交错的问题。早期 smoke 出现 3 分钟超时或读按钮等待超时，均计失败，没有据此修改生产授权或恢复行为。

诊断改用同步阶段输出、有界控件状态等待、点击前再次确认按钮可用，并使用 LiveTestWidgetsFlutterBinding 真实时钟。smoke5 单例通过后，又补了按钮已派发请求的断言，最终以完整七项矩阵验证最终源码。失败 smoke／通过 smoke5 与最终矩阵分别保留；最初两次单例复用了 smoke 目录，其第一份日志已被覆盖，不作为可独立追溯的验收证据。

三个失败测试专属残留库经绝对路径、父目录、重解析点及活跃进程核验后清理；`build/mutation-crash-widget-cleanup.json` 记录清理前相关测试进程数为 0、清理后残留为 0。全部失败日志保留。

本轮是测试进程中的真实控件＋真实宿主联调，不是安装包手工验收。没有计数证明所有可能的内部 OS 尝试；不重放结论限定于当前只读恢复调用链、原计划与文件状态断言，并依赖已有 Rust claim／终态禁重放门槛。未覆盖外部强杀、断电、Create 非空大内容的全部中间故障点、Replace 或其他平台。

## 下一阶段

[文件变更 guest SDK 接入计划](../docs/PLUGIN_MUTATION_SDK_PLAN.md) 已基于代码核验并更新看板。优先补可信目标选择与正式变更编辑／审批流程；三语言 SDK 原型可在可信测试宿主下并行验证，但产品交付不能绕过 UI 授权门槛。保留现有 IO v1／transport 冻结候选原件，新增扩展需显式版本化、原 owner 调度及 Rust／C／C++ 真 Wasm 验证。SDK 仍未冻结。
