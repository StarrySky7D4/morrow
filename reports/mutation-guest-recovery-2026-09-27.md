# 多块 guest 文件变更的回执与崩溃恢复（2026-09-27）

接续 [预算与最大内容资格](mutation-budget-2026-09-27.md)，本轮补足真实 Wasm import 与原受管 owner 之间的非空故障路径。现有原生接口崩溃测试不能代替这条链路的证据；本报告分别记录同进程丢回执、独立子进程退出和普通构建控制。

## 范围与边界

- 所有文件和内容库位于测试独有 TempDir；不接触用户内容库。
- Rust／C／C++ guest 使用现有 SDK 示例的实际编译产物，经 `morrow_mutation_v1.call` 到原 managed owner；不使用能伪造效果的 callback。
- 正文为确定性 SHA-256(counter) 高熵数据，分为三个完整 60 KiB 块及一个尾块。各块不同，避免等长重复内容掩盖错序或偏移错误。
- 目标选择、精确计划批准、持久内容提交和独立一次性 Execute 许可仍分别执行；测试不自动签发丢失的许可。
- 故障是专用 `fault-injection` 构建中的 `exit(86)`，不是断电、任意外部强杀或磁盘损坏。恢复由原进程之外的父进程重新打开原库进行验证；没有宣称已完成产品恢复界面联调。

## 丢失回执

测试等待 guest job 的 Ready 状态，再丢弃 handle 而不读取响应，不依靠固定 sleep 推断执行完成。

逐块丢失 Stage 回执后，Query 应报告精确累计长度；原 submission 换 call ID 只领取原结果。第二块以同 submission 篡改内容必须拒绝，随后累计长度保持不变。Commit 丢回执后 Query 必须确认原持久内容已存在，而目标文件仍不存在。

Execute 丢回执后，先验证原文件的完整字节，再将目标替换为测试用的无关文件。Query 和原 submission 的结果重取只能返回历史 Observed；新 submission 再 Execute 必须保留 Unknown 语义，不能覆盖无关文件。最后显式 Release 并等待原 owner 回收。

## 真实退出矩阵

| 边界 | 原库预期 | 文件预期 |
| --- | --- | --- |
| 内容写入后、receipt 写入后、事务提交前 | Prepared；内容与 receipt 一起回滚 | 无目标文件 |
| 内容事务提交后 | Prepared；内容与 receipt 可完整核验 | 无目标文件 |
| 执行 claim 后 | OutcomeUnknown，仅可核对 | 无目标文件 |
| 临时文件创建后 | OutcomeUnknown | 空临时文件 |
| 首个原生写入块后 | OutcomeUnknown | 仅有 64 KiB 正文前缀 |
| 全部写入后、刷新后 | OutcomeUnknown | 完整临时文件 |
| 发布后、效果返回后 | OutcomeUnknown | 完整最终文件；不能根据存在性伪造 Observed |
| 原结果持久观察后 | Observed | 完整最终文件，原结果摘要可核验 |

本轮唯一新增的执行边界是写循环内的 `after-write-chunk` 测试钩子，沿用现有 feature 开关；普通构建中的调用为空操作。其余退出点复用已存在的事务与原生效果钩子。没有修改文件发布、权限、计费或恢复业务语义。

## 可重复验证

```powershell
./tool/verify_plugin_mutation_recovery.ps1 -Sysroot <WASI-sysroot> -Python <python>
```

脚本构建三语言 guest 以及不同摘要的普通／故障 Release 测试宿主。先带故障环境变量运行普通宿主的三项 SDK 创建／删除控制，再逐语言运行丢回执测试和十二退出点矩阵。严格枚举测试名并验证十二个不同成功边界，不允许零匹配或少跑边界报告成功。旧 SDK／transport 原件摘要在前后核验。

输出在 `build/mutation-guest-recovery`，包含各语言日志、普通构建控制和宿主／Wasm SHA-256 的 `summary.json`。重新执行先写入 running 状态；失败不能沿用旧 passed 总结。

## 当前证据

Windows Release 专项脚本已完成，退出码 0。每种实际 SDK guest 均通过十二个独立进程退出边界：**36/36 案例**；多块丢回执 **3/3**，普通构建故障开关无效控制 **3/3**。顶层日志为 `build/mutation-guest-recovery-qualification.log`，逐语言详细日志与摘要在 `build/mutation-guest-recovery`。

| 验证项 | Rust | C | C++ |
| --- | --- | --- | --- |
| 已 Ready 后丢失 Stage／Commit／Execute 回执 | 通过 | 通过 | 通过 |
| 四个内容事务退出边界 | 4/4 | 4/4 | 4/4 |
| 八个 native Create 退出边界 | 8/8 | 8/8 | 8/8 |
| 普通宿主携带两个故障开关仍正常完成 | 通过 | 通过 | 通过 |

每个崩溃子进程必须退出 86，父进程确认正文已经经四个 guest Chunk 送达；只有 native 执行案例要求已有独立执行许可回执，内容事务退出发生在许可签发前。重开原库后显式核对正文与 `LiveStaging` 审计回执的存在性、计划摘要、内容长度／摘要及容器摘要，并验证临时或最终文件。执行开始后的八点均将目标路径替换为无关文件，再核对原历史与拒绝重复 Core claim，文件保持不变。该结果证明独立 Store 恢复和 Core claim 防重放，不等同于覆盖重启后的全部 guest API／产品交互。

| 测试宿主 | SHA-256 |
| --- | --- |
| 普通 Release | `7018339a2bbb28b99e8bb5cd39ac45dfb6cc66e8a336193b2802fa81fcd49918` |
| fault-injection Release | `276df788f40bcacfa088550ecc032f6716df48ed97185415ff5a0828ec0d4410` |

三种 guest 的 SHA-256 与前一轮最大内容资格一致，见 `summary.json`；17 个 transport 原件及 36 个 SDK 固定文件／13 对原件在运行前后保持一致。

最终普通 Release 回归：`mutation_owner` 34/34（六项需单独准备产物的 SDK 测试仍显式 ignored）、`mutation_owner_opt_in` 2/2、`mutation_reconciliation` 4/4，共 **40/40**，见 `build/mutation-guest-recovery-runtime-regression.log`。这包括真实时钟的最大内容 WAT 正常流程；三语言 SDK 的最大内容资格仍引用上一轮独立结果，不把本轮三个普通控制冒充六项全部重跑。Rust 格式检查与 `git diff --check` 通过。

测试期间修正了两处夹具问题：部分文件摘要的 Rust 类型推断，以及已有历史但无 Response 时应断言 `EvidenceUnavailable` 而非 `Ok(None)`。没有为获得通过修改生产恢复语义。工具复核还补上实际执行数量检查，以及子进程异常时有界回收、无法确认退出时保留路径和 PID；退出后父断言失败的普通 TempDir 仍会按默认方式清理，日志中的断言失败不会变成通过。

## 仍开放

这是非空四块内容的故障验收，不替代完整 16 MiB 的所有故障组合或任意文件系统的持久性证明。产品审批预算与恢复 UI 接入、完整应用构建、跨平台资格及条件 Replace 仍待完成。SDK 未冻结，本轮没有提交、推送或发布。
