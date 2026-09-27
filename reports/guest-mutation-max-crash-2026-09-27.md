# 16 MiB Guest 文件效果故障与恢复界面资格

日期：2026-09-27。分支 `codex/io-safety-refactor`。接续 [非空 Guest 恢复界面](guest-mutation-crash-ui-2026-09-27.md)，将真实三语言及 Dart 会话的文件效果故障验证扩展至协议最大正文。SDK 尚未冻结。

## 本轮范围

- Create 正文为 **16,777,216 字节**，通过真实 Guest 会话分为 **274 个最多 60 KiB 的块**。仍使用显式预算批准、原计划审阅、Prepare、持久内容提交和独立 Execute，不扩大现有 30 秒授权期限。
- 创建文件的八个故障点：after-claim、after-temp、after-write-chunk、after-write、after-flush、after-publish、after-effect、after-observe。
- Delete 无需传输正文，保留原三个故障点及普通宿主对照，避免将最大正文扩展误当作只验证 Create 的理由。
- 新普通宿主重开原临时库并只读核对；Rust 恢复 Widget 通过实际按钮读取历史。文件效果前的目标保持不存在，不能用提前创建的标记文件掩盖错误重放。

## 文件现场与历史应分别断言

| 注入点 | 最终目标 | 暂存文件 | 持久阶段 |
| --- | --- | --- | --- |
| after-claim | 不存在 | 不存在 | Unknown |
| after-temp | 不存在 | 空文件 | Unknown |
| after-write-chunk | 不存在 | 正文首个 64 KiB | Unknown |
| after-write／after-flush | 不存在 | 完整正文 | Unknown |
| after-publish／after-effect | 完整正文 | 不存在 | Unknown |
| after-observe | 完整正文 | 不存在 | Observed |

文件内容与长度仅用于测试验证物理现场，不供生产恢复逻辑推导结果。恢复不能清理、续写或发布遗留暂存文件，也不能从目标文件存在推导成功。普通宿主携带同名故障环境变量仍须正常完成。

## 可重复验证与证据

```powershell
./tool/verify_guest_mutation_crash_recovery.ps1 -MaxContent -Flutter C:/flutter/bin/flutter.bat
```

最大正文模式使用独立的 `build/guest-mutation-max-crash-recovery`，不覆盖上一轮默认矩阵。最终同一源码版本 **52/52 通过、无跳过**：

| 场景 | 通过 |
| --- | ---: |
| Rust／C／C++：Create 八个故障点、Delete 三个故障点 | 33/33 |
| 三语言普通宿主 Create／Delete 对照 | 6/6 |
| Rust 实际恢复 Widget：Create 八点、Delete 三点 | 11/11 |
| Rust Widget 普通宿主 Create／Delete 对照 | 2/2 |

统一摘要为 `build/guest-mutation-max-crash-recovery/summary.json`；完整日志为 `verifier-final-rerun.log`，逐项日志位于 `run-20260927T101122346-1a507ee7/logs/`。主代理独立核对了 52 个不同用例的成功标记、真实通过计数及相关源码摘要，未拼接此前不同版本的结果。

最终普通宿主 SHA-256 为 `0570564fd56eee54bc8911fc4daa759323aab10ca1d80cacae1fa2153a38f66d`，隔离故障宿主为 `3fdf889df76aafa91bdb05386f74cd1e26fb7d5a5777b40aa4aad90195e81abd`。内置包保持 `c71dc27866378894d5288053238fef9e0ef15f6b578449279bc681e7fe8b560e`。三语言模块／包及相关 Rust／Dart 源码摘要见统一摘要，运行前后未变化。

最终三个相关 Dart 测试／夹具文件通过 `flutter analyze --no-pub --fatal-infos`，无诊断，见 `build/guest-mutation-max-crash-recovery/flutter-analysis-final.log`。最终 29 份固定源码摘要与工作树匹配；文档更新后的 `git diff --check` 通过。本轮构建的是实际资格使用的普通／故障 Release 宿主，未构建或发布完整 Windows 应用安装包。

新 Release 宿主的三个冒烟样例已通过：普通 Create、after-write-chunk 故障、新宿主 Widget 的 after-flush 故障，日志在 `smoke/*-fixed.log`。随后首轮完整矩阵在 `run-20260927T100416989-9de8b5a0` 的 Rust Widget Delete／after-claim 处被严格脚本拒绝：该用例本身通过，但遗漏 `GUEST_CRASH_WIDGET_CHUNKS=0` 日志标记。只补上这个删除零块标记，不放宽脚本校验；完整矩阵以新源码重跑，旧失败日志保留，不拼接两轮计数。

## 冒烟暴露的实际缺口

首个 Rust 普通宿主 Create 样例完成了 16 MiB、274 块准备与执行，随后在新宿主的恢复交付中失败。初始日志为 `build/guest-mutation-max-crash-recovery/smoke/native-create-after-claim-0.log`，review／prepare／execute 分别约 193／4078／592 ms；该样例未通过，不能将执行成功替代恢复验收。

静态追踪发现：Reconcile 的单次读取计费为正文长度＋原计划容器＋有界结果空间，而原独立历史绑定和工作台采用普通 16 MiB 单次上限。因此最大正文即使成功保存，恢复时也会因元数据开销超过该上限。本轮修复保持全部读取计费，没有拆账后一次性读取超额内容，也不借用声明但未批准的扩展写预算；正文与累计额度及只读能力边界保留，具体见下节。

带分阶段诊断的独立复现已确认失败在 `read-reconciliation-23`，消费结果为 `failure=target/null/limit`，非超时。日志 `smoke/native-create-after-claim-0-diagnostic.log` 保留原始错误；该次测试仅保留自己的临时库供诊断，未重放原 Execute。

## 生产修复

独立只读历史配置增加精确有界的 **12,448 字节**元数据预留：原计划容器最大 8,352 字节，加结果预留 4,096 字节。`JobLimits::mutation_history` 不借用未批准的 32／256 MiB 执行预算，最大正文仍为原普通声明的单次上限（不超过 16 MiB），累计仍为原声明（不超过 64 MiB）。正文在读取 Store 前独立检查，不能挪用元数据余量；完整读成本和 Observed 的第二次读取照常收费。公共 admit、Guest 作业、文件选择及效果命令继续拒绝历史绑定。

工作台只为扩展预算包的显式 Reconcile 选用新配置。原累计额度不足以容纳完整元数据预留时保留旧的小记录读取路径，不因本修复让低预算插件完全失去原有恢复能力。普通包及普通 IO 构造不变。

运行时独立测试已 **6/6 通过**，见 `build/mutation-history-metadata-binding.log`：16 MiB 受保护原件可读；同绑定第三次读取后，第四次仍因原累计预算拒绝；元数据上限／累计上限多一个字节、较小声明正文越界、直接 Broker 和 Guest 入口均拒绝。

相关运行时回归 **40 项通过、6 项显式 SDK 资格跳过**，见 `build/mutation-history-metadata-regression.log`。这些跳过项不计作通过；实际三语言与最大正文另由本报告的独立矩阵验证。宿主历史专项 **4/4 通过**，见 `build/guest-mutation-max-crash-recovery/host-history-tests.log`，覆盖 16 MiB／274 个逻辑块实际创建后的 Observed 核对、错包／撤权拒绝及累计额度紧张时的小记录回退。块数由长度和块上限计算，实际链路另检查累计 stagedBytes、持久内容及最终文件摘要，不将它描述为宿主独立计数器。

宿主相关回归在 Release 模式、实际 Rust mutation Wasm 及 Workbench Wasm 配置下 **45/45 通过**，见 `build/guest-mutation-max-crash-recovery/host-mutation-release-regression.log`。此前 Debug 回归为 **43/45**：最大正文实际 Rust SDK 路径触及 30 秒期限，另一例遗漏 Workbench Wasm 环境配置。失败日志 `host-mutation-regression.log` 保留；没有延长权限期限，不能用 Release 成功宣称 Debug 最大正文也已合格。

编译检查另发现 `shared_objects::start_reader` 在仅启用 `package-management` 时引用未编入的 `remote_reader`。将该单方法的条件与目标模块对齐为 `packages + Windows`，未改实现或 unsafe。修复后 `package-management`、`packages` 检查通过；前者仍有既有 unused／dead_code 警告。最终 packages Clippy 仅沿用 `collapsible_if` 与 `let_and_return` 两项既有允许项，其余按 `-D warnings` 通过，日志分别为 `build/mutation-history-package-management-fixed.log`、`build/mutation-history-packages-after-cfg.log`、`build/mutation-history-clippy-after-cfg.log`。无 feature 与 `package-execution` 检查也通过，后者仍有既有 dead_code 警告；这些检查不等价于其他平台资格。

## 验收边界

本轮验证的是 Windows 文件效果阶段的受控 `exit(86)`，不是断电、任意磁盘损坏或其他平台验收。Core 的四个内容事务提交故障点在先前小正文运行时资格中已验证，但其 **16 MiB 与产品恢复界面组合仍需独立补齐**。完整应用构建、系统选择器及 30 秒人工审批可用性也继续开放，条件 Replace 仍不提供降级实现。

下一阶段四点分别为 `file-content-after-bytes`、`file-content-after-receipt`、`file-content-before-commit`、`file-content-after-commit`，应在 Prepare 的 Commit 期间退出且不调用 Execute。重开后均应核对到原 Prepared 记录、无结果且目标不存在；前三点正文与回执一起回滚，最后一点两者完整持久化。现有恢复 DTO 不暴露正文回执，不能仅凭 Prepared 文案声称持久化正确，需另用重新打开的 Store 对正文哈希和原 receipt 做独立验证。

本轮未提交、推送或发布。
