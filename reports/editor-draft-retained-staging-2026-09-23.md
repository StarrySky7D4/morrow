# 草稿附件可释放持久归属：Core 原子暂存

日期：2026-09-23。当前仍是 `ddd9cc8eec224af51f4e58654c9b297332147d96` 上的本地工作树；应用和第一方插件版本不变。没有提交、推送或发布。

## 查明的问题

当前宿主草稿附件导入先调用 stage_blob 保存字节，再写入进程内 draft_staged。字节与草稿归属不是一个持久事实；进程退出后，未选入草稿的附件失去归属及恢复入口。

直接把它改成私有 CardRecord 的附件也不正确：Core 的附件绑定同时写当前 card_blobs 和历史 event_blobs。后者必须保护审计历史，移除当前附件也不能释放它。将每个未选附件都写成卡片附件，会把临时导入变成永久保留。

## 本轮实际实现

新增 Store::stage_blob_retained(reader, length, expectedSha256, owner, now)：首次写入在同一个 SQLite 事务中完成字节、分块校验数据和可释放 Snapshot retention；复用已有物理 blob 时也在同一事务建立归属。没有新增卡片/业务事件/永久附件事件引用。

同 owner 已存在时，先核验唯一归属、持久元数据、实际分块字节和总摘要，再比较固定长度与摘要；精确重试只读返回原 blob，不再读新 reader、不更新时钟，也不因后来容量用满或系统时钟回退而重做导入。来源不匹配则拒绝。

新增 retained_blob_local(owner) 只读恢复入口。同 owner 出现多个 Snapshot 不是任取一个，损坏的归属/元数据/字节也不会被当作成功恢复。合法 Protobuf 未知字段继续保留；修复了压缩容器长度与解压后正文上限的区别，避免拒绝带未知字段的合法大元数据。

原 stage_blob 复用共同内部实现，保持原有语义。释放一个 Snapshot owner 不影响其他 owner 或已存在的卡片/历史引用；最后一个可释放归属移除后，仍遵循已有退役时钟和至少 60 秒宽限，不能立即删除原件。

## 验证与限制

- Core 五组实际存储/附件测试合计 **30 项通过**：既有 attachment_crash 4、attachment_reads 6、attachments 11；新增 retained_staging 8（含一个子进程夹具入口）、retained_staging_compat 1。启用了 fault-injection，真实子进程分别在 stage-before-commit、stage-retained-before-commit 和 stage-after-commit 退出；重开验证字节与归属同时有或同时无。日志 `build/review-core-retained-staging-tests.log`。
- 新测试还覆盖原文件已删除后的原归属重试、多 owner 共享/释放/宽限回收、普通卡片及历史 pin 继续保护、导入读失败/长度/摘要/插入归属失败回滚、模糊或损坏归属拒绝，以及实际达到 2048 blob 条目上限后仍能核对已确认导入。该容量测试不冒充 2 GiB 磁盘耗尽压力测试。
- 初次定向 lib 命令只完成编译，筛选后为 0 tests；日志 `build/review-core-retained-staging-lib.log` 不计单测通过。新增子进程测试首次有 String/Path 编译错误，修正后运行上面的完整通过组合。
- Windows x64 Release 重建成功；随后用该产物执行六个实际客户端/宿主测试文件，**15 项通过、无跳过**，覆盖既有草稿附件、重启读取、S1/S2 编辑、前驱附件与捕获保存。日志 `build/review-core-retained-staging-windows.log`、`build/review-core-retained-staging-client.log`。
- 没有 Android/Web 整包或实体设备验收，没有重跑尚有既有告警的严格 Clippy。

新原语只证明字节与可释放归属原子持久化，**尚未接入宿主草稿导入协议**；当前 UI 仍不能恢复未选入草稿的导入附件。owner 释放后 Core 可再次接受该 owner，原业务操作的永久防重放须由宿主持久操作记录负责。

同 owner 的确认重试可绕过新的容量/时钟写入检查；不同 owner 的首次导入仍受现有保守物理容量准入，即使稍后可能去重，也不保证满库继续导入。已选入草稿的正式证据仍按已有历史保留规则保护，不能把本轮可释放暂存当作历史压缩已完成。

## 下一步

宿主需要先记录固定导入提案，再使用本原语保存字节与归属，最后确认已就绪；按原操作核对时不得先重开路径。持久提案、消耗与放弃、列表/导出以及 Dart Unknown 接线见 [暂存接入方案](../docs/EDITOR_DRAFT_STAGING_PLAN.md)。草稿来源交接、富捕获、恢复界面、语言/字体在途记录、迁移入口、完整错误协议及 SDK/平台门槛继续开放。
