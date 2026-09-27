# 文件内容持久暂存与 v22 迁移

日期：2026-09-27。开发基线 `b9225f6`，应用版本仍为 `0.1.9-test.56+60`，仅本地源码增量。

## 已实现

`FileContent` 采用独立有界 Protobuf＋LZ4 原件，最多保存 16 MiB 内容；绑定 operation、subject 和文件计划请求摘要，核验实际内容 SHA-256。规范编码拒绝未知、重复、乱序及非最短编码，解压前检查容器/原文上限。空内容保留独立行，区别于从未暂存。

`stage_file_mutation_content_local_authorized` 只接受已有 Prepared create/replace。事务内重读计划和原件，匹配内容长度与摘要；以 INSERT-only 方式保存。初始访问与提交前检查当前授权；精确重试不改原件、不重复计费，仍必须授权。Delete、已取消计划和身份不匹配拒绝暂存。`CommitUnknown` 应读取原操作确认，不能视为文件已执行。

暂存表升级为数据库 v22，v21→v22 迁移前后执行完整性核验，不改写旧业务原件。诚实旧库自动迁移；留着新表却伪装旧版本的库拒绝打开。旧版 reader 会拒绝 v22，不能作为向后读写兼容承诺。

`file_mutation_content_local_authorized` 在访问和交付前再次检查授权，以 subject/operation 隔离数据。取消后仍可由授权宿主读取留存原件，但不可再 stage。快照、开库校验原件与计划关联，拒绝孤儿、损坏原件、超长 SQL 元数据和过大容器。

逻辑费用按 `max(原文长度, 容器长度)` 收取，避免高压缩数据绕过预算。费用接入共享字节额度，终态 IO 预留也改用同一入口，消除手工求和漏掉暂存/服务材料的路径。取消不删除已留存证据，计费继续存在；本轮未增加自动清理策略。

## 验证

最终完整 Core 回归：**685 项通过、0 失败**，进程退出码 0。10 个 ignored 为父测试显式启动的故障注入子进程入口，不是跳过对应崩溃验收。日志 `build/file-content-20260927/core-full-final.log`。本轮新增内容 codec 5、Store 9、真实进程崩溃 1、SQLite 页容量不足 1，共 16 项，已包括在全套计数中。

- 四个真实进程退出点：暂存提交前/后、v21→v22 迁移提交前/后；前者不留半成品，后者可恢复原字节与准确重试，派发仍被拒绝。
- Create/Replace、空内容、16 MiB 上限、授权拒绝、取消留存、请求绑定、孤儿/篡改/超限 SQL 数据、低预算重开及 HTTP 后续预留共同计费均通过。
- SQLite `max_page_count` 限制实际触发 `StorageFull`，确认原计划保留、内容无残留且可重开；不代表真实介质故障验收。
- 第一轮全套因 TLS 的两处旧库模拟夹具保留 v22 新表而失败；修正夹具后专项和最终完整全套均通过。共适配 17 个旧版本 fixture 文件，伪降级拒绝断言未放宽。
- 最终新增代码专项 Clippy 通过 `-D warnings -A clippy::collapsible_if`，仅豁免已有 lint 类别；日志 `final-clippy.log`。新代码 rustfmt 和 `git diff --check` 通过。五个其他文件的误触格式化尚保留，原因见下方。

复现：

```powershell
cargo test --locked --offline --manifest-path core/Cargo.toml --features fault-injection --no-fail-fast
```

子代理误对父模块运行 rustfmt，额外格式化了 `audit/sealing.rs`、`plugin_package.rs`、`plugin_package/registry.rs`、`store/blobs.rs`、`store/card_snapshot.rs`。逐文件验证当前文本等于 HEAD 经 rustfmt 后文本。两次自动审批仍拒绝限定撤回，已向用户请求授权；未绕过拒绝或丢弃这些改动。

## 尚未开放

本轮未打开文件变更派发，没有写入用户选择的目标文件。后续需要可信目录/文件句柄、动作与目标类型约束、撤权与替换条件、原子执行及 Unknown 核对，以及 SDK/工作台接线。Prepared 当前允许内容尚未暂存；整行内容丢失会表现为未暂存，不能据此检测曾经暂存过的事实。执行就绪阶段应增加可核验的暂存承诺后才开放 dispatch。

stage 不新增 operation event，因此 CardReadSnapshot 的读点不能证明暂存内容的新鲜度。宿主必须从本接口重新取得并校验原件。取消内容的留存清理、跨平台资格与完整 SDK 冻结仍开放。

本轮没有构建应用安装包、提交、推送或发布；此前 Windows 成品不包含本轮 Core 变更。
