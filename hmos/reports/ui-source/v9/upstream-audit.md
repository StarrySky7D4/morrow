# 2026-10-05 dev.9 参照版本与草稿来源审计

本次只读核对当前工作树、Flutter `pubspec.yaml`、Git blob 和 SHA-256；没有运行 Windows 构建或测试，没有操作设备。以下版本为本次观察值，参照分支仍在推进。

| 活跃工作树 | HEAD | Flutter 源码版本 |
|---|---|---|
| `build/io-safety-refactor` | `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` | `0.1.9-test.57+61` |
| `build/win-cloud-20261005` | `772466177fe589cee53bc633e69f411c34610104` | `0.1.9-test.58+62` |
| `build/windows-sdk-reconstruction` | `20669f671152972470340a65eac3458dd2f61b4d` | `0.1.9-test.58+62` |

三个工作树的 `lib/main.dart`、`lib/plugins/editor_draft_binding.dart`、`editor_draft_session.dart`、`editor_draft_workspace.dart` Git blob 分别相同，本次检查这些路径及原草稿 schema/model 没有已跟踪修改。test.58 版本号不表示普通编辑器 UI 或草稿集成发生变化。

下表的原文件在三个工作树及 HMOS 对应副本均逐字节相同，也与 `hmos/rust/editor-draft-reference.json` 记录一致：

| 原来源路径 → HMOS 副本 | SHA-256 |
|---|---|
| `workbench_host/schemas/editor_draft.proto` → `hmos/rust/editor-draft-model/schemas/editor_draft.proto` | `8F80DB114BCC480D9553AE60916170B1982C9CDE0D0452753A35B491279FDDDC` |
| `workbench_host/src/editor_draft/model.rs` → `hmos/rust/editor-draft-model/src/lib.rs` | `D11632512506F357A73112568A6E8856AA74E87500B91709C87083A50DAC6C8E` |

来源清单仍以 `io-safety-refactor` 的 `925fb8ca` 为基准；其中原 `editor_draft.rs` 的 SHA-256 为 `B4ADAFD9EDCA7ECBA9E4DD3A23963F226E3009233914D62C16630A1C7AC0DA28`，原 `versioned_record.rs` 为 `6A0131915BAC966FF745DA5580A91256A918D0C10B246B9E7C4120E972385E8A`，本次均与记录匹配。这两项属于事务、历史回执和 source 验证的适配来源，不能宣称 HMOS 整个宿主模块逐字节复用。

生产 Flutter `lib/main.dart` 没有直接实例化专用 `EditorDraftBinding`、`EditorDraftSession` 或 `EditorDraftWorkspace`。这些模块及对应测试存在；普通编辑器采用自身的输入观察、冻结业务保存请求和保存后继续编辑逻辑（`main.dart:5210`、`5350` 起）。因此专用模块的测试资格不能推导为生产 Flutter 普通编辑器已经自动接入该 durable journal，也不能继承为 HMOS 测试通过或功能验收证据。

当前 HMOS provenance 明确为 `development-unsealed` / `raw-text-durable-journal`：复用原 schema/model 的原始 `TextValue`、UTF-16 范围、固定 source 基线、generation CAS 和精确历史请求语义。它保存 raw journal；尚未实现 captured 正式 S1 提交证明、S1/S2 父子交接或 `editor_recovery`。带 assets、consumed imports、predecessor/evidence、parent links、retirement markers 的请求由阶段门拒绝。业务保存成功后若仍有新输入，保留原 journal 并要求显式解决 source 冲突，不将其称为 captured S2。

后续对齐应分别补齐正式捕获与交接语义，以及生产存储身份、单 owner、平台密钥和封存/恢复资格；本报告及本轮 raw journal 接入均不扩大这些边界。UI 竞态修复和设备上的显示、输入、恢复结果需由 HMOS 自身代码检查与实际产物证据确认。
