# 退稿与迟到输入接续：源码依据及 development fork 设计

2026-10-07。本文区分真实 Flutter 正式编辑器、已实现但尚未接入该编辑器的 raw 草稿库，以及本轮实现的 HMOS development fork。**本次 GitHub 交付收束为 native/ETS 基础能力源码检查点，Index 尚未接入 fork，没有设备 fork 验收；本文后续接线步骤属于设计合同。** 本文不提供受保护存储、签名或完整 Flutter 等价证明；完整目标仍 OPEN。

## 真实 Flutter 行为

参考 worktree 的 HEAD 从各自 `.git` 元数据只读核对：

| 工作区 | HEAD |
| --- | --- |
| `build/io-safety-refactor` | `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` |
| `build/win-cloud-20261005` | `e83cdf1d3506b001d07ffceaaa9f98b688bcc150` |

两份 `main.dart`、`editor_draft_session.dart`、`editor_draft_workspace.dart`、`editor_draft_binding.dart`、`editor_draft_handoff_native.dart`、`editor_draft_handoff_proposal_native.dart`、`versioned_editor_adapter.dart` 在归一 CRLF/LF 后一致；`editor_attachment_rebase.dart` 原字节也一致。`workbench_native.dart` 整文件有其他功能差异，但本次审查的完整 `_NativeEditorSession` 类归一后 SHA256 均为 `a13acc6312a93eaa866e44ff6d27f6abe67b7d7578da7e77beb4d2660c7943f8`，位于 io 的 2875–3157 行和 win 的 3019–3301 行。

| 关键原件 | io SHA256 | win SHA256 |
| --- | --- | --- |
| `lib/main.dart` | `6203b7465f320dc8463dfacb5387ea8b437ec3ec1458cca7e215e44b8f65cb1e` | `2cb2a519e31ac982d3a8638eb7de95fe63d5421ed3d1b6acda507cd142169f06` |
| `lib/plugins/editor_draft_session.dart` | `422a4ac7c7cace0115cc50290e027fe63348c6b8273e89a659c24c5302015e7e` | `107cee0dbfe8e8fd232b66eea92002077120abab0ba6fb67033fa0f3a7693af8` |
| `lib/plugins/editor_draft_workspace.dart` | `d68e2529d4d7e32a20b3537cb9e0908f2225ce8ea3a1dfe06624883907dde52b` | `5e0e18cc42cb488e0733a06c1802874f125640e49391b5b593e489a089154d7b` |
| `lib/plugins/editor_draft_binding.dart` | `c782ad86229efdd33debc1a5ce77f3f2c5abf5f387025ae6b70d7ad319293e20` | `d0cc43f59c10156b4303799117548d04dd264924c1cff9858067bdc540797ee3` |

以下行号使用 io worktree，两个 main 文件行号一致：

- `lib/main.dart:5210` 的 `_observeDraft` 比较完整 `TextEditingValue`，包括 UTF16 selection、affinity、direction 和 composing；每次不同的完整值推进 `_editGeneration`。`_save:5325` 冻结原 `_frozenFields/_frozenDraft/_submittedGeneration`，业务回执后若 generation 变化，转入 `_continueDraft:5224`，保留 live controllers；不会用原业务回执关闭较新的 S2。
- `_continueDraft` 重试仅开启原确认编辑的后继会话，不再次提交原业务保存。它保留 `_preparedSuccessor`，原会话已经继续而附件 alias 重映射尚未完成时也不会丢掉有效新会话。`editor_attachment_rebase.dart` 只映射原 S1 提交、确认且当前仍选中的本地附件，并保留当前 selected 子集、顺序、别名及正文完整 TextEditingValue。
- `_NativeEditorSession:3092` 仅接受其自身原回执的精确 revision 和完整 confirmed snapshot；`_openLegacyEditor:2100` 再核 live revision，没有读取较新 card 后自动 rebase。关闭 capture scope 是旧会话生命周期结束，不是复活已 discard 的 raw journal。
- 正式视图 `main.dart:5756` 仅因 importing 吸收操作，正文/title 等也仅因 importing 只读；saving 期间仍能观察完整后续字段值。`CardTipsEditor:5897` 仅用于 legacy Idea，V2 隐藏。`versioned_editor_adapter.dart:144` 明确拒绝非空 legacy todos/completed/editor.todos。

raw 库是另外一层已实现能力：

- `editor_draft_binding.dart:97` 原子读取全部五个控制器及附件 metadata；失败标记捕获不完整，不把旧字段子集冒充最新完整快照，也不修改控制器。只有完整新 observation 才能清除捕获失败。
- `editor_draft_session.dart:490` 的 `flushLatestVisible` 暂停 debounce，最多等待一项已发保存和保存一项较新 generation；保存期间再次输入即拒绝关闭资格，不自动循环发更多代次，Unknown 不自动重放。
- `editor_draft_workspace.dart:139/269` 两次核对 local generations；释放 view lease 不 discard、不取消已发请求、不销毁唯一 live S2。prepare 或 finish 失败均保留会话。
- 两份 `lib/` 源码搜索中，`EditorDraftSession.newSession/restore`、`EditorDraftWorkspace`、`EditorDraftBinding` 只有定义，没有正式 NewIdeaDialog 的实例化接线；ParentLink 客户端/提案客户端已实现，main 已接恢复查看，但没有在正式 `_save/_continueDraft` 中生成并自动完成提案。`versioned_editor_adapter.dart:53` 也明确 durable successor 为 opt-in，正式编辑器仍默认立即确认恢复清理。不能把这些库能力称为该对话框已经实现完整 durable parent handoff。

## HMOS S0 与 guard 检查点的准确边界

原 `rust/src/editor_draft.rs:473` 明确禁止复用已经 inactive 的 draft ID。普通首写按当前 card 校验 source revision，普通 origin3 仅认同 scope 以前已确认的 pins；fresh 子草稿不能靠旧 asset ID 或旧 URI取得文件权限。discard 会清除当前主 pins 并核对/退休独立 staging；历史事件可能保留 bytes，不代表当前 export authority。

接入前的 Index guard 检查点在成功 discard ACK 后检测 epoch 或完整 raw 变化，会保留界面并标记已退休身份，避免 dispose/close。但它没有新的 durable writer。`retirementCapture` 仅把迟到 SDK 事件存入内存；新卡 todos 的 row 事件没有被猜成完整 aggregate。`attachDraft` 会清事件、更换 view owner、仅从字符串重建字段并重新选入 card 的全部附件，因此不能直接拿来作无损接续。

原受保护 ParentLink 要求成功业务 capture evidence、精确 committed operation/digest 和 sourceKind existingCard；未发布新卡的手动 discard 没有这些证据。HMOS 回执内容 hash、raw SHA、普通 Store receipt 都不能冒充该受保护 committed SHA。

## 本轮 development fork 合同

这是隔离 development 数据库的新增协议，独立于原 ParentLink/ParentRetirement。原原件、字段预算、全局 raw 预算、附件预算和身份容量继续适用；空间或身份不足必须拒绝整项，不退休父草稿、不截断内容。

冻结的 JSON 接口方向：

```text
draft_fork:
  fork.child = 完整原 Write JSON，fresh child draft_id/operation_id，expected_generation="0"
  fork.parent_draft_id
  fork.parent_generation
  fork.parent_save_operation
  fork.parent_request_sha256 = 原确认 canonical protobuf request 的 64hex SHA256

draft_fork_retire:
  fork_retirement.card_id / child_draft_id / child_operation
  fork_retirement.parent_draft_id / parent_generation / parent_save_operation
  fork_retirement.parent_request_sha256 / operation_id
```

回执增加 canonical `request_sha256` 与独立 `fork_link/fork_retirement`。fork link 保存 schema_version=1、父准确 generation/save operation/request SHA 及 child 首 operation；fork retirement 保存准确 child 身份、原清理 operation 与同一 link。development 字段 13/14 不声明 protected capture、迁移或生产权限。

父原 `source_kind/source_revision/source_card` 必须完整继承，包括 newCard 的 kind1/revision0。业务 card 已经变化时，fork 用原确认父权限保存完整 raw，随后保持显式 source 冲突，不能自动采用当前列表里的新 card。特殊首写允许已确认父的选中 pins 用 origin4 验证继承；child 首回执后，后续普通保存使用 child 自己的 origin3。首 fork 的资产必须是父确认 selected inventory 的有序子集，aliases 完全相等，bytes/name/mime/length/SHA 不变；晚到的 aliases/reorder 变化不能直接借原 proof 提交，需先在仍 active 的父上完整确认或明确拒绝。未选 staging 必须明确拒绝退休或按其原授权核对处理。

接入必须按顺序执行：

1. 暂停父 debounce/后续写入，仍接受完整 raw capture。view/controller owner 保持；writer revision/epoch 独立撤销旧异步 worker。business readiness 与 raw capture completeness 分开，formatter pending 不应阻止完整 IME raw 保留。
2. 读取准确 active 父回执及原 request SHA，冻结首 fork 请求。将最新全部五字段 TextValue、category/stage、原 selected 子集/顺序/aliases 作为 child 完整原请求，不使用 card 全附件集合替代。
3. child 首回执严格核对 frozen request、原 source、fork link 与 pin metadata；Unknown 保留同一请求，不产生新 child 或自动重放。父保持 active，直到 child 首结果被确认。
4. 确认 child 后立即切换 writer，保留视图和当前更晚的完整 raw capture。首 ACK 只确认被冻结的 S2，不证明 await 期间的 S3；S3 需要 child 自己的新 generation。
5. child durable 后条件退休父，使用准确原退休 operation。父推进、child/link 不匹配、active/staging authority 改变时拒绝。退休 ACK 后仍由 child 接受迟到 capture，不能再次更新旧 writer。
6. 完整捕获失败时保留最后完整 raw 与原 SDK 事件，并阻止猜测性业务提交/关闭。原 JSON/Values 无法完整表达的新事件不得被宣传成已经写入 raw journal；若需跨进程保留这种事件，必须另外设计有界 durable event 容器。

两阶段不是跨草稿原子事务：child 写入成功而父退休未确认时允许两个 durable 所有者并存，恢复时按同一 link 和原 operation 只读核对后处理。它防止先 discard 再尝试重新取得附件权限，但不能从早已 inactive 且已释放授权的父草稿事后恢复 authority。

手动“放弃全部 raw”还需单独定义最终 child 生命周期。若确认父退休后立刻普通 discard child，而 view/callback 仍存活，就会在第二次 terminal ACK 中重现同一迟到输入问题。仅退休父并保留 child 可提供可恢复接续，但不是删除全部草稿的同义词。只有已建立真实完整捕获和 view/callback 生命周期边界后，才可授予最终清理资格；现有 Flutter view lease 的 release 是保留 session 的 detach，也没有替 HMOS 提供 SDK 回调排空证明。

## create 后的 legacy todos 与 V2 任务

Flutter `main.dart:5373` 的 legacy 业务投影为 LF split → Dart trim → 非空 → 首次去重，完成集合按文本成员关系求交；raw 字段继续完整保留。`_continueDraft` 不清空 todos controller，后续仍是 legacy V1 编辑。V2 正式编辑器和独立 TaskId 面板不接受这个 legacy 字段。

HMOS `rust/src/create_todos.rs` 在一次 create 中把原 raw 投影成真实 V2 Task；TaskId 绑定 card、原 create operation、完整 raw digest 和 normalized first index。本文不把这种 V2 创建适配声明成 Flutter V1 schema 等价。

fork 应完整保留原 raw todos，即使其中一部分已经由 S1 create 生成任务，也不能自动清空或当作 `task_add` 再发一次。原来源继承后业务冲突是安全而明确的状态。未来如需从这个完整 legacy raw 继续修改业务，应独立绑定原 create 提交证明、S1 原 raw digest/投影/TaskId 映射及 S2 完整 raw，准确处理重复/空白与原 completion；不能从 view row ID 或当前任务文本猜身份，也不能用多个顺序 task commands 冒充原子 legacy 保存。未实施此业务 reconciliation 前，只能声明后续待办原文可恢复、业务来源待明确处理。

## 验证范围

本文的 Flutter 结论来自上述 fresh 源码和 source hashes，当前不是新的 Flutter/SDK/设备测试结果。本次没有新增未来 Index 集成测试，也没有把 fork 接入实际用户界面。

独立只读复核的最终生产范围为 `rust/src/editor_draft_fork.rs`、`editor_draft.rs`、`editor_draft_staging.rs`、`draft_bridge.rs`、`lib.rs`，独立 model schema 的 Slot13/14，以及 `EditorDraft.ets/EditorDraftFork.ets`。原 Slot1–12、WriteRequest、原受保护 ParentLink/Retirement 字段未改；新字段缺省为空，普通历史 journal 的 canonical 编码保持原形状。当前 decoder 仍检查 decode→encode 的完整 canonical bytes，并拒绝原受保护 lineage/captured recovery。新 fork 的降级读取/旧包执行资格不在本次范围。

审查覆盖新 dispatch/严格 JSON shape、原 canonical request SHA/完整父 Slot CAS、首子请求及后继 lineage 不变、原 grant/Store 原子 journal 写与 pin 权限、准确历史重试/current 状态分离、未选 staging 拒绝及父冻结后的 import 门禁、条件退休 marker 与普通 discard 区分、child 后继 origin3/source bytes、原 normal coordinator 的 pause/issued request/Unknown/composition 保留。上述有限范围未发现剩余阻塞缺陷。`native-fork-inputs.json` 的10项原件在审查时全部 SHA/bytes 匹配；ETS生产原件为 `A3AE1E99D198FCBA3CDEF741A6F138612AD16979BE42C7A8809003F00D04F965` 和 `2F5CC8586D2400F99EE7598DD816970E63D9E3D4B53C5DFFBC54CB4E4062263C`。

本 worker 读取并核对了其他 implementation worker 的 fresh 原始测试日志，未把只读审查说成自己重跑的测试：

| 实际范围 | 结果 | 证据 |
| --- | --- | --- |
| 实际 Rust host/Store 默认完整 suite | library141 + binary3 = **144 PASS**；0 FAIL；7条件默认 ignored 不计通过 | [native-default-tests.log](native-default-tests.log)、[native审计](native-fork-audit.md)、[10项输入](native-fork-inputs.json) |
| 单独启用 Store 子进程故障边界 | **1 PASS /14 vectors**；0 FAIL；fork和retire各7个事务边界，完整原操作核对及export bytes/SHA | [native-fork-store-crash-tests.log](native-fork-store-crash-tests.log) |
| 实际 ETS fork 模型、controlled transport/clock | **17/17 PASS**；0 skipped/cancelled/todo | [fork-model-tests.log](fork-model-tests.log)、[ETS审计](editor-draft-fork-audit.md) |
| 既有实际 ETS EditorDraft 回归 | **25/25 PASS**；原测试断言范围未改 | [normal-draft-regression.log](normal-draft-regression.log) |

这些结果分别是 host Store 与独立模型资格；SDK declarative 编译、最终 native archive/HAP、Index接线、实际迟到SDK事件、跨重启UI恢复、设备和发布资格需各自证据，不能互相替代。当前主界面的退稿/保存后接续缺口仍未因此宣称关闭。
