# 非空 Guest 故障后的只读恢复界面验收

日期：2026-09-27。分支 `codex/io-safety-refactor`，基于 `b9225f64f6c62584ad7243e30249d8a088bcb155` 上的本地未提交工作树。本轮衔接扩展预算包的独立历史绑定，验证真实 Rust／C／C++ Wasm 文件操作在进程故障后，由新普通宿主恢复原记录。SDK 尚未冻结。

## 产品改动

恢复页新增原生文件操作与 Guest 文件操作两个工作流快捷入口，复用已有本地化标题。按钮只填写稳定的历史 subject，不启动发现、不批准预算、不执行文件变更；有活动恢复任务时不可切换。仍允许编辑其他工作流的 subject。

两种工作流标识集中到 `lib/plugins/mutation_workflow.dart`，原执行页面继续导出原常量，避免既有调用方变更。标识用于选择历史，不构成授权；当前包摘要、目录修订、启用状态与能力批准仍由原历史绑定检查。

## 验收方法

`tool/verify_guest_mutation_crash_recovery.ps1` 重新构建三语言实际 SDK Wasm 及包；普通宿主与开启 `fault-injection` 的宿主输出到不同目录。脚本固定包、模块、宿主、打包器与内置插件摘要，逐项拒绝失败退出、缺失标记、零测试与 Skip，结束后恢复原环境变量。

在本工作树使用 PowerShell 7 重跑：

```powershell
./tool/verify_guest_mutation_crash_recovery.ps1 -Flutter C:/flutter/bin/flutter.bat
```

需要已有锁定的离线 Cargo 依赖、WASI sysroot 与内置工作台包；脚本不下载或替换正式发布包。真实 Flutter 用例串行执行，避免共享测试缓存冲突。

- Create 使用确定性高熵正文 **184,393 字节**，分为四块提交；Delete 的初始目标为三个字节。
- 实际 Guest 会话完成审阅、Prepare、分块持久提交及显式 Execute。故障分别注入在取得执行权后、操作系统效果后和持久观察后，要求原宿主确实以 **86** 退出。
- 新普通宿主重开同一临时内容库，以当前目录身份显式发现原计划并独立核对。Native 用例核对两轮记录／结果逐字节一致；恢复不会补做 Execute。
- Create 在 after-claim 时保持目标不存在，避免预先放置目标掩盖意外重放；已有操作效果的用例写入外部标记内容，核对后要求它保持不变。
- Rust Widget 用例不注入恢复页种子或手填内部 subject，通过实际按钮选择工作流、发现、读取、选中原计划、离页返回、释放／ACK，再独立核对／ACK。
- after-claim／after-effect 保持 Unknown，不显示成功；仅 after-observe 与正常宿主对照显示持久 Observed 及成功结果。文件存在本身不被用作成功证据。

这里的故障是宿主测试开关调用 `std::process::exit(86)`，不是断电或内核故障。执行准备由真实会话 API 驱动；本套 Widget 资格覆盖恢复交互，不重复宣称已测试整个 Execute 人工审批界面。

## 验证记录

完整矩阵 **32/32 通过，无跳过**。汇总为 `build/guest-mutation-crash-recovery/summary.json`；逐项日志位于 `build/guest-mutation-crash-recovery/run-20260927T092918408-660bcc79/logs/`，总日志为 `build/guest-mutation-crash-recovery/full-verify.log`。主代理另行核对全部 32 份日志的预期标记和真实通过计数。

| 场景 | 实际通过 |
| --- | ---: |
| Rust／C／C++ × Create／Delete × 三个故障点 | 18/18 |
| 三语言普通宿主 × Create／Delete，故障环境变量无效对照 | 6/6 |
| Rust 实际恢复 Widget × Create／Delete × 三个故障点 | 6/6 |
| Rust 恢复 Widget 普通宿主 × Create／Delete 对照 | 2/2 |

普通宿主 SHA-256 在本轮构建前后均为 `0f7c600b8f0bdc014e45bc2653d6d8e04939a477e55c311ac906815ae3e9f4f2`；隔离故障宿主为 `0b01fab8ffe70fb8e24e191666f79b859112c0d71ed371185c29816eed6fba51`。内置包前后均为 `c71dc27866378894d5288053238fef9e0ef15f6b578449279bc681e7fe8b560e`。三语言模块与包摘要见汇总，和上轮历史绑定资格一致。

已完成的独立检查：恢复页回归 **13/13**，见 `build/guest-crash-recovery-scope-widget.log`；产品改动严格分析无诊断，见 `build/guest-crash-scope-analysis.log`。

首轮 Rust 恢复 Widget 已到达业务成功标记，但因临时目录前缀不符合清理保护而失败，不能计作通过。修正为 `morrow-external-guest-crash-widget-` 后独立冒烟通过，保护条件未放宽；失败与通过日志分别保留为 `rust-widget-smoke.log` 与 `rust-widget-smoke-final.log`。

完整矩阵之后还加固了两份测试的失败清理：删除临时库前独立观察当前宿主退出，未知时保留并记录路径；正常宿主关闭失败仍传播原错误。Widget 对 `runAsync` 异常返回的 null 不当作退出证明。此改动不修改生产语义或业务断言；其最终源码代表路径复验另行记录，不把前面的完整矩阵描述为在清理补丁之后运行。

最终补丁后 **4/4** 代表路径通过：Native Create／after-effect／故障、Native Delete／after-claim／普通、Widget Delete／after-observe／故障、Widget Create／after-claim／普通。摘要 `build/guest-mutation-crash-recovery/representative-final/summary.json` 固定最终源码和逐项日志摘要；同目录保留原始日志。最终 8 个相关文件的严格 Flutter 分析无诊断，见 `build/guest-mutation-crash-recovery/strict-analyze-final-after-cleanup.log`；`git diff --check` 通过。

## 后续门槛

1. 16 MiB 最大正文的完整故障组合，包括暂存文件、分块写入、写完、flush 与 publish 边界。
2. 完整 Windows 应用构建、系统文件选择器和最长 30 秒审批时限的人工可用性验收。
3. 其他平台的实际存储、执行与恢复资格；本轮仅验证 Windows。
4. 条件 Replace 的后端保证仍缺失，继续明确拒绝，不提供非原子降级。

本轮未提交、推送或发布。已有历史绑定与 Guest 执行协议保持原语义，不据此冻结 SDK。
