# Windows 文件变更真实故障退出与恢复验收

日期：2026-09-27。开发分支 `codex/io-safety-refactor`，基线 `b9225f64f6c62584ad7243e30249d8a088bcb155`；本轮变更未提交、未推送、未发布。

## 实现与发现

- Workbench 的显式 `fault-injection` feature 现在转发 runtime 对应 feature，让独立测试宿主可触发已有 Create／Delete 故障点。默认 feature 不引入这些故障点。
- 新增 `test/mutation_recovery_crash_native_test.dart`：真实宿主创建独立临时受保护库、导入并批准本地插件夹具、准备原计划，在指定阶段直接退出；等待实际 exit code 86 后，由新宿主重开原库并发现、核对历史。
- 新增 `tool/test_mutation_crash_recovery.py`：逐例隔离故障变量，记录宿主及夹具 SHA-256、退出码和完整日志。超时或跳过不计通过；支持单例诊断但标明不是完整矩阵。
- 发现并修复 `RustWorkbench._exchangeDecoded` 竞态：请求 send／flush 尚未结束时，EOF 可先完成 reply Future 的错误，此时原代码尚未附着等待者，触发未处理异步错误。创建 reply Future 后立即标记错误已观察，随后仍等待同一个 Future 并向调用方抛出原始错误。不重试、不吞掉业务调用错误、不放宽关闭屏障。
- 新增确定性 `host_exit_during_send_test.dart`：人为延后 send 完成，让宿主先退出；检查无 zone 未处理错误、调用方收到原始 exit 86、关闭等待真实退出、后续调用拒绝且请求没有重发。

## 七项真实进程矩阵

所有例子使用 Windows、本地临时库及独立原生进程；故障点调用 `std::process::exit(86)`，跳过析构。恢复时原计划字节一致，发现结果不冒充执行结果。每例执行两次独立的发现与核对，比较阶段、效果、操作 ID、记录和结果原件，并检查目标存在状态不因核对改变。

| 用例 | 重开后阶段／效果 | 文件状态 | 结果 |
| --- | --- | --- | --- |
| Create after-claim | OutcomeUnknown／未保存效果 | 尚未创建 | PASS |
| Create after-effect | OutcomeUnknown／未保存效果 | 已创建 | PASS |
| Create after-observe | Observed／OS 成功 | 已创建 | PASS |
| Delete after-claim | OutcomeUnknown／未保存效果 | 原文件存在、字节保留 | PASS |
| Delete after-effect | OutcomeUnknown／未保存效果 | 已删除 | PASS |
| Delete after-observe | Observed／OS 成功 | 已删除 | PASS |
| 普通构建＋Create after-claim 环境变量 | 正常执行、重开为 Observed／OS 成功 | 已创建 | PASS |

Unknown 不由当前文件是否存在推断成成功；恢复路径没有选择目标、Prepare 或 Execute 调用。仅观察最终文件状态不足以证明任意实现都不会重放，本测试同时使用只读恢复 API 路径，覆盖该具体实现。

## 最终验证

- `build/mutation-crash-matrix-verified/summary.json`：完整矩阵 **7/7 通过，0 跳过**，进程退出码全部 0。
- `build/mutation-crash-transport-regression.log`：host_exit_during_send、host_request、service_run_transport_native、workbench_channel、workbench_close_native、application_shutdown 六组 **38/38 通过**。真实受控子进程测试设置 `MORROW_CLOSE_TEST_PYTHON`，没有因缺少 Python 跳过。
- `build/host-exit-during-send-red.log`：修复前 1 项失败，捕获未处理 StateError；`build/host-exit-during-send-green.log`：修复后 1 项通过。
- `build/mutation-crash-verified-analyze.log`：生产传输文件及两个新增 Dart 测试严格 JSON 分析，`diagnostics: []`，退出码 0。
- `git diff --check`：通过；保留仓库已有 CRLF 提示。
- feature 图分别保存在 `build/mutation-crash-host-default-features.log` 与 `build/mutation-crash-host-fault-features.log`。显式故障构建及默认构建均成功，默认目标由 Cargo 正常重建恢复；没有以复制文件覆盖 Cargo 产物。

### 失败记录与修正

首轮 `build/mutation-crash-matrix/` 保留失败日志：临时目录前缀不符合已有清理守卫，同时暴露上述真实传输竞态。第二轮 `build/mutation-crash-matrix-final/` 中 Create 三例和普通构建对照通过，Delete 三例在 Execute 之前失败：测试读取已被原生选择器独占的目标文件（Windows errno 32）。修正为选择前检查初始字节、确认宿主退出后检查保留字节；未放宽原生独占锁。第三轮完整重跑见 verified 目录，全部通过。不得引用前两轮为通过证据。

## 可复现命令

先准备当前本地插件包与分别构建的故障／普通宿主，不使用用户内容库：

```powershell
& 'C:\Users\Administrator\AppData\Local\Python\pythoncore-3.14-64\python.exe' -X utf8 tool/test_mutation_crash_recovery.py `
  --dart C:/flutter/bin/cache/dart-sdk/bin/dart.exe `
  --flutter-tool C:/flutter/bin/cache/flutter_tools.snapshot `
  --host build/mutation-crash-host/fault-host.exe `
  --normal-host build/mutation-crash-host/current-normal-host.exe `
  --package build/workbench-host/bundle/workbench.morrowplugin `
  --fixture build/mutation-recovery-test.mplugin `
  --output build/mutation-crash-matrix-verified
```

本轮产物 SHA-256：

| 产物 | SHA-256 |
| --- | --- |
| fault-host.exe | `722f5afeb0a63db40ef50f54fb99c152802424d770cdee5262967db6523a4578` |
| current-normal-host.exe | `8730a598a6c206109ec7d8dbe081d775bd24d1a5f6ce0b2b92483974d6592ad4` |
| workbench.morrowplugin | `c71dc27866378894d5288053238fef9e0ef15f6b578449279bc681e7fe8b560e` |
| mutation-recovery-test.mplugin | `4c7198f458591442933e03f530062325e10d8dd8ceee444b7fc4fb6b05ba0d01` |

## 仍开放的工作

1. 真实故障退出后，通过恢复界面按钮完成发现、选择、关闭／确认和核对的联调；当前验证到独立会话与展示模型，既有 widget 测试与真实崩溃测试仍分开。
2. 正式文件变更选择／编辑／审批流程与 guest SDK；Windows 条件 Replace 仍不具备产品可用资格，SDK 未冻结。
3. 外部强杀、任意断电与文件系统持久性验证；本轮显式 exit(86) 不代表这些场景。
4. Create 的写入／刷新／发布中间故障点及非空大内容、其他平台资格。本轮 Create 是零长度文件，不能推断非空流式创建全过程已验收。

只读复核未发现阻断问题。矩阵脚本的外层超时仅终止 Flutter runner，不保证所有原生后代进程同时结束；超时始终计失败，排查时须核实该次测试拥有的残留进程后再清理临时目录，不能按名称批量终止用户进程。本轮最终矩阵无超时。

本轮没有完整应用打包、发布性能结论、真实用户库改写或远端更新。
