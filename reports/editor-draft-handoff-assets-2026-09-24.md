# 父子草稿附件目录与恢复预览

日期：2026-09-24。基于开发工作树 `codex/io-safety-refactor`，应用仍为 `0.1.9-test.54+58`。本轮修改 Dart 目录与测试，不提交、推送或发布；未重新构建完整 Windows 应用，真实宿主测试使用既有 Release 宿主和 `.54.5` 插件，精确摘要见同名 JSON。

## 已完成

- `EditorDraftAssetCatalog.handoff` 核对父槽身份、当前代次、原保存操作/摘要、已提交 S1 证据、子来源修订和 ParentLink。子首保存必须继承全部父附件，顺序、别名和原始文本/选区/IME 值保持一致；不能将普通 source 或其他草稿附件冒充 parentDraft。
- 子首保存确认后，附件来源转为该子槽的 previousDraft。后续保存可以删除附件，目录不继续承诺已释放 pin 可用；未解析的重新选择使 binding 标记未完整捕获，不能静默漏存。
- 用独立 `EditorDraftCommitEvidence` 表示提交身份证据。V2 可以从精确 committed recovery 转换；sourceRevision=0 的伪造 EditorRecovery 被拒绝。新卡首 Create 的宿主证明公开接口尚未接通，newCard 用例仅验证目录结构规则。
- 修复共同 `_checkView`：显式 mediaType 不能绕过已知 MIME 对 TextureKind 的限制。未知 MIME 仍要求调用者明确提供与宿主一致的类型描述。
- 测试夹具可回传实际 V2 保存取得的提交证据，不根据当前卡片内容猜测 operation/digest。

## 验证

最终组合 **29 项通过，无跳过**：6 项新交接目录、7 项既有附件目录、8 项字段绑定、5 项代次 pin 采纳、3 项真实 Windows 宿主流程。日志 `build/handoff-assets-final-tests.log`。五个本轮修改的 Dart 目标静态分析无问题，日志 `build/handoff-assets-final-analyze.log`。这些数字包含重跑与原回归，不与早先报告叠加宣称额外覆盖。

新增真实宿主流程覆盖：原文件删除 → 根据父 pin 导出并测量 SHA → 构造完整持久交接提案 → 创建子 → 退休父 → 关闭并重启宿主 → 重新导出子 pin、绑定新的可读预览路径 → 继续保存 → 删除 pin 后拒绝旧选择。原预览文件在重启前删除，旧视图对象不能通过新目录登记。

同一流程使用真实 TextEditingController/Binding/Session：一次保存发出后立即继续输入，旧回执未覆盖后来的原文、选区方向/affinity 和输入法组合范围；随后新代保存保留这些值。该流程还核对 120,000 UTF-8 字节原始标题、附件顺序/别名和导出字节，并确认正式卡片修订未因草稿保存而变化。

这是控制器与真实宿主级验证，不是正式 NewIdeaDialog 自动保存或 Windows 窗口人工验收；未运行 Android/Web、强制掉电、磁盘满、性能或长期资源测量。

## 尚待接入与下一步

正式编辑器仍未使用本目录创建持久交接提案。接线继续要求工作台持有会话、父槽写入冻结、在途更新归属、富捕获证据、明确关闭/切库与 Unknown 核对。不能因为目录和测试已通过就开启不完整的自动保存。

只读审查确认：V2 adapter 目前在 NewIdeaDialog 检查 S2 前便执行恢复记录清理。下一步拆开“核对提交证据”和“确认清理”，仅在当前完整后继已可恢复或不存在更新编辑时清理。V1 新卡虽有宿主提交/证据历史，但会话只返回 Idea，未公开原 operation/digest；须提供按 card + 原 operation 的只读宿主证明，不能伪造 V2 recovery 或再次提交 Create。

细化约束已写入 `docs/EDITOR_UI_PERSISTENCE_PLAN.md`。SDK、历史容量维护、迁移 UI、远端 Unknown 与全平台资格保持开放。
