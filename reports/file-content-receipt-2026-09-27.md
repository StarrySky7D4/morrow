# 文件暂存回执与 v23 审计迁移

日期：2026-09-27。开发基线 `b9225f6`，应用仍为 `0.1.9-test.56+60`。未提交、推送或发布。

## 本轮变化

- 新增独立 Protobuf＋LZ4 `Receipt`，绑定计划请求、实际内容长度/摘要及原始内容容器摘要。来源区分现场 `LiveStaging` 与迁移观察 `LegacyImport`；独立事件 ID 绑定操作 ID 和请求摘要，不与计划 ID 混用。
- 数据库 v23 新增 `file_content_receipts`。首次暂存同事务写内容、回执索引、kind 6 operations、outbox 与 operation_events。授权失败、容量不足或提交前崩溃整体回滚；准确重试不重复写入或记账。
- 正反闭包连接计划、内容、回执、操作和事件。单边删除内容/回执、绑定错误、孤儿和事件缺链拒绝；读取还验证 pending 原文/序号或封签队列关系。完整签名链仍由受信开库/封签/完整性检查负责。
- LiveStaging 序号必须晚于 Prepared，早于后续 Cancelled；LegacyImport 可晚于旧取消事件，只证明升级时观察到字节，不补造原始提交或授权。
- 审计解析、pending、开库、封签、快照及卡片 readpoint 支持 kind 6。回执进入既有签名审计队列，不另建日志或内容权威来源。

## 迁移

v22→v23 先验证完整旧库，再同事务建表、为已有暂存内容生成 LegacyImport 事件、验证新闭包并提交。原字节和旧事件不改写。旧库没有暂存内容时无需生成导入事件。导入事件遵守已有容量限制；空间不足返回 EventCapacity 并完整保留 v22，可由受信维护入口提供足额 EventBudget 后重开，不能暗中绕过限额或部分导入。

受支持的 v22 是上一轮最终格式（含 operation_id/subject 长度 CHECK）。未发布的中间实验表结构不属于已承诺迁移来源。v23 仍不保证旧版 reader 可读。

## 验证

- Windows 本地完整 Core 故障注入回归：**698 passed / 0 failed / 11 ignored**，80 个测试汇总（含 doc-tests）；退出码 0。11 个 ignored 均为由父测试带隔离数据库/故障边界调用的子进程入口，并非跳过业务验收。
- 命令：`cargo test --locked --offline --manifest-path core/Cargo.toml --features fault-injection --no-fail-fast`。完整日志：`build/file-receipt-20260927/core-full.log`。
- 本轮新增回执 codec 5 项、Store 7 项、迁移真实崩溃父测试 1 项均包含在上述 698 项中，不重复累计。覆盖重复暂存、篡改与单边丢失、pending/封签/快照、取消事件顺序、v22 导入、迁移容量不足回滚及提交前后真实进程退出。
- Clippy 通过（`-D warnings -A clippy::collapsible_if`，保留仓库既有类别例外），日志 `build/file-receipt-20260927/clippy.log`；新增 Rust 文件格式检查与 `git diff --check` 通过。
- 本轮是 Windows Core 源码级验证，未重新构建 Flutter/Windows 安装包，未补充其他平台资格。此前 Windows 成品不能代表已包含本轮 Core 改动。

## 下一步与限制

回执证明持久字节及其历史关联，不是操作系统权限，也不证明文件写入成功。实际文件派发仍关闭。下一步需绑定可信目录/文件句柄、动作类型及预期对象身份，落实实时撤权与原子创建/替换/删除，再接 Unknown 核对和公共 SDK/工作台入口。目录枚举、跨平台资格及完整 SDK 冻结继续开放。

此前等待批准的五个文件纯格式清理仍未执行；其中 card_snapshot.rs 本轮新增了必要的 kind 6 功能分派，未来清理只能撤格式部分，必须保留功能改动。本轮未重复尝试被拒绝的清理。
