# 文件变更计划与原子准备验收

日期：2026-09-27。开发分支 `codex/io-safety-refactor`，基于 `b9225f6`，应用版本保持 `0.1.9-test.56+60`。本轮是文件 IO 后续底座增量，未提交、推送或发布。

## 实现范围

- 新增独立 `file_mutation.proto`、有界 Protobuf＋LZ4 原件及严格规范编码校验。计划绑定 operation、subject、插件包、批准摘要、宿主目标引用、相对路径、create/replace/delete、预期对象身份、内容长度及摘要。create 要求目标不存在语义；replace/delete 必须带预期身份；delete 不带写入内容。
- 新增不可变 `RelativeFilePath`：保留原 UTF-8，拒绝绝对路径、穿越、空段、反斜杠、ADS、控制字符、Windows 设备名及非法尾部；限制总字节、UTF-16 段长和段数。仅为词法检查，不授予目录权限，不解决符号链接、重解析点或 TOCTOU。
- `prepare_file_mutation_local_authorized` 在同一个 Immediate 事务中保存 Prepared、审计后续容量、材料容量和计划原件；每段与最终提交前重新检查授权。拒绝、panic 或配额不足全部回滚。准确重试保留原事件和原件。
- 开库、历史查询、材料读取、审计事件核验与快照复用文件计划闭包校验；缺失/损坏原件、计划与 command 不匹配、专用协议出现不支持的执行阶段均拒绝。取消可持久保存并释放预留，原件继续保留。
- 通用追加与 dispatch claim 按三种文件变更 capability 阻断，不可借旧通用 IO schema 绕过专用准备入口。

## 版本和兼容性

复用数据库 v21 已有 `io_intents`、`io_evidence` 和预留表，无 SQL 结构迁移。文件计划独立 schema/version/digest 为特性门槛，要求数据库至少 v21；未修改冻结的 `io.capnp` 或旧插件原包。旧 reader 遇到新协议摘要会拒绝解码；这不是含新计划数据库的向后可读保证。既有 HTTP 记录和旧文件历史保持解码能力，但本版不允许新建文件派发历史。

## 验证

最终组合回归在 Windows 本机通过：14 个测试目标，**119 项通过、0 失败**。4 个标记 ignored 的入口是由父测试显式启动的崩溃子进程，不是未执行的验收场景。其中本轮新增路径/协议/Store/崩溃覆盖共 23 项。日志：`build/file-mutation-20260927/core-regression.log`。

- 真实进程在提交前、提交后分别以故障码 86 退出：提交前重开七表无残留；提交后保留完整 Prepared、计划原件和预留，准确重试不重计且不能派发。
- 准备与取消记录均通过 pending/封签/重开/快照；篡改、缺失原件拒绝开库；每个授权点拒绝、授权 panic、事件或材料容量不足均验证事务回滚。
- 旧通用 IO 摘要和新文件摘要都不能绕过三种文件变更能力的派发限制。
- Clippy 针对四个新测试目标及核心代码通过 `-D warnings -A clippy::collapsible_if`；仅豁免此前 `store/blobs.rs:224`、`transaction.rs:267` 已有的同类告警，不能称为无豁免严格全库检查。日志：`build/file-mutation-20260927/clippy.log`。
- 变动 Rust 文件格式检查及 `git diff --check` 通过。审查代理发现并复核关闭了旧摘要派发绕过；没有使用 SubagentBridge。

复现组合门禁：

```powershell
cargo test --locked --offline --manifest-path core/Cargo.toml --features fault-injection --test file_mutation --test file_path --test file_mutation_store --test file_mutation_crash --test io_intent --test io_intent_store --test io_intent_reservation --test io_dispatch_claim --test io_evidence --test io_intent_crash --test io_evidence_crash --test service_request_atomic --test card_snapshot --test sealing
```

## 明确未完成

本轮不开放文件系统效果，也未把这些准备接口接入公共 C/C++/Rust guest SDK 或 Flutter 操作入口。内容摘要不是已暂存字节的证明；下一步需有界持久内容暂存及配额、可信目录/文件句柄与撤权、平台可兑现的条件创建/替换/删除，以及崩溃后 Unknown 核对。目标适配器还须明确目录 grant 与单文件 grant 的动作组合；新计划 schema 发布后变更需保留旧摘要读取兼容。只有这些边界闭合后才能开放派发。目录枚举、完整文件 IO、异步 guest 续接、跨平台资格和 SDK 冻结继续开放。

上一轮 Windows 成品验收见 [文件任务生命周期记录](file-task-lifecycle-2026-09-27.md)。该成品早于本轮 Core 增量，不能作为本轮增量已进入安装包的证据。
