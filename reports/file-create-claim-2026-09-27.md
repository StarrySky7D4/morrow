# 文件创建的持久执行边界与证据闭包

日期：2026-09-27。基线 `b9225f6`，开发线 `codex/io-safety-refactor`，应用版本仍为 `0.1.9-test.56+60`。本轮未提交、推送、发布或构建安装包。

## 实现

1. 新增专用 `Store::claim_file_create_local_authorized`，核对 Create 动作、文件变更协议、相对目标、原始计划、已暂存内容及审计回执。以 Immediate 事务完成 Prepared→OutcomeUnknown 严格 CAS；重复 claim、取消后 claim、缺原件或错绑定不能取得新执行记录。
2. `file_content::require_for_dispatch` 直接读取并核对有界内容和回执，不回调 plan/history 验证，避免 Unknown 历史检查产生递归。内容摘要、长度、请求摘要及 exact container 回执均须一致；零字节也必须存在完整的空内容原件和回执。
3. Unknown 历史读取、全量完整性核验、重开内容库均要求原内容与回执闭包。即使两者及其回执事件同时删除，也不能退回“尚未暂存”的解释。LiveStaging 与 LegacyImport 的回执序号都必须早于派发事件。
4. 创建、删除在 claim 前必须保留确切响应容量；Unknown 期间响应材料必须尚未写入且预留仍完整。补堵公开 `store_io_material(Response)` 和 `release_io_material_reconciliation` 对文件变更的旁路，保留专用观察事务内的材料写入。HTTP 等其他能力继续沿原通用路径。
5. 原删除观察仍在同一事务内消耗预留、写响应、追加 Observed，不暴露中间状态；本轮没有新增 Create 的 Observed/outcome 入口。更改未改变 v23 表结构；旧文件计划 reader 会拒绝它不支持的 CreateUnknown 阶段，不宣称旧 reader 兼容。
6. 授权在读取原件前以及提交前核验，拒绝和异常在提交前回滚；提交结果未知仍须核对历史，不自动重放。Unknown 是尝试前的持久边界，不是操作成功，也不是操作系统授权。

## 验证

- Core 完整故障注入回归：**715 passed / 0 failed / 13 ignored**，83 个汇总（含 doc-tests），最终退出码 0。日志 `build/file-create-claim-core-full.log`。ignored 为父测试驱动的独立子进程入口，不等于遗漏对应崩溃场景。
- Create 专项：**9 passed / 0 failed / 1 ignored**。全量运行期间补强了损坏后直接 lookup/content read 的断言，并以最终专项独立复验通过；日志 `build/file-create-claim-targeted-final.log`。专项计数已包含在全量测试目标中，不重复累计。
- 专项覆盖确切一次 claim、取消/错绑定/缺相对目标/缺内容/缺回执拒绝、逐处授权失败回滚、零字节原件、提交后内容和回执双失、两种来源的回执顺序、通用 Response/Observed/释放预留拒绝、预留提前释放后的 claim 拒绝，以及完整备份/签名封存/重开可读。
- `file-create-claim-before-commit` 与 `file-create-claim-after-commit` 通过真实子进程退出码 86 验证。重开只见 Prepared 或完整 Unknown＋内容＋回执；后者重复 claim 被拒绝。没有创建用户目标文件。
- Windows runtime 相关组合：**145 passed / 0 failed / 3 ignored**，内部 53、file_owner 10、io_binding 9、io_execution 19、managed_file_io 54。包含既有真实删除效果/观察和崩溃测试；日志 `build/file-create-claim-runtime.log`。
- Core 与 runtime Clippy 均通过 `-D warnings -A clippy::collapsible_if`，沿用既有记录的例外，没有本轮新增 lint 抑制；日志 `build/file-create-claim-core-clippy.log`、`build/file-create-claim-runtime-clippy.log`。受改文件 rustfmt 和 diff --check 通过。
- 独立只读复审检查了递归、证据双失、派发与响应容量旁路，并复核修复后的 Delete 原子观察不会被中间暂态误拒。HTTP 实际回归由完整 Core 套件与 runtime 相关组合提供；不扩称其他平台或完整产品验收。

主要命令：

```powershell
cargo test --locked --offline --manifest-path core/Cargo.toml --features fault-injection --no-fail-fast
cargo test --locked --offline --manifest-path plugin_runtime/Cargo.toml --features fault-injection --lib --test managed_file_io --test io_binding --test io_execution --test file_owner --no-fail-fast
cargo test --locked --offline --manifest-path core/Cargo.toml --features fault-injection --test file_create_store
```

## 范围与下一步

这一步只落地可信宿主的持久执行边界。实际临时文件创建、分块写入、flush、不覆盖发布、原句柄关闭及 Create 观察保存仍未接通；运行时没有因为新增 Store 方法而获得自动创建或重放能力。下一步应把已保留的目录链、实时租约和专用 claim 与这些操作连成同一次执行，并验证每个外部效果和持久观察之间的崩溃。

本轮的 snapshot 验证指完整备份 `snapshot_to` 后重开和审计核验。`open_card_snapshot` 是局部卡片读取快照，只验证其读取范围和最新读点，不宣称对所有较早文件操作做全量内容核验；为每次卡片查询重读所有暂存原件会违背惰性读取目标。文件历史/内容接口继续执行自身的闭包检查。

后续原生实现不能依赖清除普通删除 disposition 来取消 `FILE_FLAG_DELETE_ON_CLOSE`：微软 [FILE_DISPOSITION_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_disposition_info) 明确该成员对此标志无效。不覆盖发布应使用保留父目录句柄并令 ReplaceIfExists 为 false，再验证实际平台行为；见 [FILE_RENAME_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info)。临时文件也属于外部效果，必须在 Unknown 提交并确认后创建；孤立临时文件、写入/flush/发布失败、失权、关闭不确定与跨重启核对均需单独实现。

条件替换、完整 Unknown 业务恢复、原 owner 队列、公共 C/C++/Rust SDK、Flutter UI 与其他平台资格仍开放，SDK 尚不能宣布冻结。
