# 混合格式工作台读取与展示准备

本轮为主工作台 V1/V2 接线增加完整扫描和纯展示模型，不开放迁移入口。应用版本 `0.1.9-test.54+58`、第一方包 `0.1.9-test.54.3` 不变。

## 实现

- `RustWorkbench.loadVersioned` 调用新只读扫描器，按页读取真实格式，保留 V1、V2 与删除记录。它不导出附件、不迁移、不把 V2 转成旧 Idea JSON。
- 页面交付前核对本会话已知修订，重读并发新增或更新的记录，最多三轮收敛。末次比较到交付间无异步等待；失败不返回部分快照。修订始终为 BigInt；不是数据库级原子快照，也不声称能观察当前会话以外的任意写入。
- 拒绝重复 ID、游标循环、错 ID 回复、修订倒退、非法 u64 和不支持的格式；没有新增总卡片数上限。
- `VersionedIdeaView` 保留格式、修订、来源、退休 TaskId、完整附件长度及宿主投影计数。V1 展示行保持文字匹配；V2 行保留独立 TaskId 与三态完成状态，不把待确认当作未完成或已完成。

## 验证

`build/review-versioned-workspace-tests.log` 共 15 项通过：扫描器 5、展示模型 3、真实版本化客户端 5、真实捕获编辑 2。其中新增真实扫描测试建立混合临时库，每页只读一条，随后关库并在缺少插件的条件下重开；格式、TaskId、歧义、来源原件、删除标志与修订均保留，返回列表不可修改。此测试不经过 RustStudioStorage 或正式主窗口。

`build/review-versioned-workspace-analyze.log` 六个目标无问题。首次合并检查指出两处测试使用相对 lib 导入，修正后通过。子代理直接调用 Dart 分析器曾遭遇 Windows 临时性能文件清理失败，未将该错误计作分析通过。

Windows Release 构建记录：`build/review-versioned-workspace-windows.log`。完整应用迁移及实窗交互仍未验收。

## 明确未完成

`RustStudioStorage.open`、旧 `load()`、主查询和 `List<Idea>` 展示仍走原路径。本轮不能宣称“V2 库已能在正常主界面重开”。必须按[完整接线计划](versioned-workspace-integration-plan-2026-09-23.md)同时处理初始加载、刷新、查询、TaskId 操作、编辑、删除恢复和偏好暂存，完成后再启用迁移入口。

跨重启编辑恢复、完整保存/错误协议和 SDK 冻结继续待办。未修改用户资料，未提交、推送或发布。

后续实现与当前边界：[主工作台混合格式与 TaskId 接线](mixed-workspace-ui-2026-09-23.md)。本文件上文保留当轮核查状态。
