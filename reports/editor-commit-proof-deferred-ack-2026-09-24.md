# 历史提交证明与 V2 延迟确认

日期：2026-09-24。开发工作树 `codex/io-safety-refactor`；应用 `0.1.9-test.54+58`、内置插件 `.54.5` 不变。只更新本地源码与 Windows 预览，不提交/推送/发布。

## 实现

新增私有 `inspectEditorCommit @123`，Response 新增 `editorCommitProof @46`。请求只含卡片和原操作身份；宿主核验历史 operation_commit/operation_evidence 与真实捕获投影，支持 V1 Create、LegacyEdit 和 V2Edit，拒绝缺失、异卡、非捕获操作及无关请求字段。只返回 id、operation、32 字节证据摘要、sourceRevision、committedRevision；不读取当前卡片猜原结果、不执行 guest、不恢复 capture、不清理 recovery。正常宿主及原 owner 命令通道共用业务处理路径；旧动作编号保留，私有 schema 摘要改变，不保证旧二进制混用。

Dart 严格核对身份、摘要长度、外层修订和连续 UInt64 修订；新卡允许来源 0，超过有符号 int 正范围仍保持精确。V1 原编辑会话提供 `inspectCommittedSource`，仅查询其冻结的原操作并缓存核验结果。查询失败清除在途 read future，不能清除原保存意图或触发重新 Create。

提交证据模型移到 `editor_commit_proof.dart`；原附件模块继续导出，保持现有导入兼容。没有改变持久化 schema 或公开插件 SDK。

V2 新增 `observePresented`（只读核对）与 defer adapter 的 `acknowledgeAccepted`（显式原号清理）。延迟模式在展示成功后固定证据与 accepted 身份，继续编辑/关闭时不清理；即使旧 capture 已关闭，同一工作台仍可明确确认原提交。在途确认合并，成功幂等，失败仍保留原身份。默认非 defer 行为保留；历史确认允许当前卡片后续推进，首次后继观察仍要求精确基线。

## 验证

Dart 最终相关组合 **52 项通过，无跳过**；十个修改目标 analyze 无问题；生成绑定一致性检查通过。对应 `build/editor-commit-proof-final-tests.log`、`editor-commit-proof-final-analyze.log`、`editor-commit-proof-bindings-check.log`。

新增四项真实客户端测试覆盖：

- V1 新卡第一次捕获 Create → 原会话返回真实证明 → 父附件原文件删除后完整提案/建立子/退休父 → 当前卡推进 → 缺失插件下重开仍可只读核对旧证明、恢复子草稿并导出原附件。
- V2 defer 保存后 recovery 保留，关闭旧 capture/打开后继不会清理；完整提案 Prepare 确认后显式原号清理，再建立子与退休父。
- 宿主在线时破坏一次证明回复。trace 严格只有失败与显式重试的两次 Inspect，没有任何 Create/save 重放；宿主保存的首提交回执确实存在。
- 当前卡片推进后严格观察拒绝新基线，但历史确认仍可清理原 committed recovery；此前是否观察过 S1 两种路径均验证。

Rust 组合执行 **16 项，均通过**，其中交接协议的 2 项被两个测试 target 重复引用，故为 14 项不同测试；不将重复算作新增覆盖。包含实际 V1 Create/LegacyEdit/V2Edit 的只读历史证明、恢复、原提案与交接协议。日志 `build/editor-commit-proof-rust-tests.log`。Release 宿主构建通过，保留原有三条 dead_code warning。

Windows 完整预览已在后续输入风格修复轮完成最终构建，正式 `lib/main_rust.dart` 入口；最终构建日志 `build/style-input-windows.log`，其 `INSTALL` 命令退出 0。使用最终包内宿主与插件再次执行 `editor_commit_proof_native_test.dart`，4 项全部通过，无跳过；日志 `build/editor-commit-proof-packaged-tests.log`。这四项是上述 52 项的重复成品验收子集，不额外计算为新测试。产物同时包含随后完成的输入风格修复，不将其写成上一轮已经完成的构建。源码、既有日志与当前产物摘要见配套 JSON。

## 边界与下一步

正式 UI 尚未启用 defer，也未调用持久交接目录建立后继。`acknowledgeAccepted` 自身不证明当前完整 S2 已持久，调用方必须先持有精确提案及回执/Inspect，再核对当前输入代次。不能只翻开关就宣称自动保存完成。

下一步接工作台 owner 与正式编辑器：父槽冻结、最新原始输入/IME/附件与富捕获证据、Prepare/Unknown、子会话替换、显式关闭和切库。客户端进程完全退出后的原操作发现不能依赖 V1 会话内存缓存。SDK 冻结、历史容量维护、迁移 UI、远端 Unknown 与全平台资格保持开放。

本轮没有 Android/Web 资格、物理 UI 人工验收、强制掉电或性能结论。协议证明测试、控制器测试与正式用户流程分别记录，不能互相替代。
