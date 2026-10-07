# dev21 编辑器视图生命周期与迟到回调审计

本报告是只读源码审计和接入建议，不是 SDK 回调队列排空、设备 IME、草稿 fork 或完整 Flutter 对齐的验收。审计代理没有修改生产源码、测试或历史报告，没有运行 Git、设备或 HAP 构建。Index 由另一代理并行接线，下列旧闭包位置是本次读到的接入前快照，不能当作最终接线版本仍存在缺陷的断言。

## 可成立的边界

冻结每次挂载的视图 lease，所有 SDK 回调携带该 lease，随后在真实组件生命周期回调中撤销 lease，可以证明**旧视图事件不会获得替换视图的写权**。这是一条应用接纳边界。它不能证明系统中尚未交付的输入事件已被采集，也不能把 `stopEditing()`、一次销毁通知、一次计时器或若干帧等待称为输入队列已排空。

明确确认“放弃这份草稿”允许删除这份视图的完整草稿；业务保存回执或“保留草稿并关闭”没有删除未提交输入的同等语义。三种入口必须分开处理。保存后保留子草稿继续编辑是一项可审阅的保留策略，不能写成手动放弃全部草稿成功，也不是当前 Flutter 自动关闭分支的完全相同行为。

## 实际 Flutter 参考

本次读取 `build/io-safety-refactor` 和 `build/win-cloud-20261005` 中的实际文件。下列三文件两份内容仅行尾不同，LF 规范化 SHA256 完全相同；没有使用 Git HEAD 代替源码核对。

| 文件 | io-safety-refactor 原始 SHA256 | win-cloud 原始 SHA256 / 两份 LF SHA256 |
| --- | --- | --- |
| `lib/card_tips_editor.dart` | `cae29cfd83020ff9f8a9e17b128b5f9c83d3260d2e117cb41630b83befcfde64` | `5dffdc4dfd6f45c17e7145721815e6070976e1e687cbeedba572dbf17c8c4c0d` |
| `lib/tip_list_editor.dart` | `d4af62531cbc28807c2ce0356e6b7ea26a12e81c82e979d9d470deeb11dd9191` | `fefdce765a39cd0ac78adf786a91f9267211ed7a844d298d5051874fe75104ea` |
| `lib/main.dart` | `6203b7465f320dc8463dfacb5387ea8b437ec3ec1458cca7e215e44b8f65cb1e` | `2cb2a519e31ac982d3a8638eb7de95fe63d5421ed3d1b6acda507cd142169f06` |

`CardTipsEditor` 的第 33–40 行将父控制器完整 LF 字串拆成行；第 52–64 行在更换父控制器时移除旧 listener，并在 dispose 时移除 listener。第 67–89 行把行内容重新聚合给父控制器，并按各行 UTF16 长度及 LF 映射选区。这里的行身份是临时 `TipItem.id`，不是已提交 V2 `TaskId`。行 composing、affinity 和 direction 不会因为父聚合有一个 TextEditingValue 就自动获得完整映射资格。

`TipListEditor` 第 190–237 行通过 `ValueKey(item.id)` 和按行 ID 留存的 `GlobalKey<_TipRowState>` 保持行状态。第 335–341 行的选择通知由 microtask 触发，实际检查 `mounted && focusNode.hasFocus` 后再读取当前行 controller；第 345–354 行外来行值更新控制器，并在 dispose 时释放 controller/focus node。这是 Flutter 已实现的局部生命周期保护，没有声明平台输入消息队列已排空。第 385–422 行是真正 1..3 行 TextField；总 1000 grapheme 的剩余额度是其他行 grapheme 数加 LF 数后的余额，不是给每行另设 1000。

`main.dart` 第 5210–5221 行观察完整 TextEditingValue：文本、选区、affinity、direction 的变化增加 `_editGeneration`。第 5349–5392 行冻结 S1 字段及业务请求；第 5400–5405 行在业务 ACK 后发现世代变化则 `_continueDraft()`，否则 `Navigator.pop`。第 5224–5277 行实际延续 editor session、保留控制器并处理附件重映射。它没有等待一个系统输入队列排空回执。第 5713–5736 行页面 dispose 会 close editor session 并释放控制器。不能把代码中已观察世代保护扩大为尚未交付平台消息的持久化证明。

另行读取的 Flutter raw library 提供更强的可见草稿保留合同，但这份实际 `main.dart` 没有引用 `EditorDraftWorkspace`、`EditorDraftViewLease` 或 `EditorDraftSession`，不能把 library 的合同算成主 UI 已接入：

- `lib/plugins/editor_draft_workspace.dart` 第 4–15、109–112 行明确 lease release 仅 detach UI，不 discard、不取消操作，也不销毁 workspace 的唯一 live S2。
- 第 139–181 行 `prepareClose` 要求 visible binding 先完成采集，然后 flush 每个 session 并冻结 local generation；第 197–209 行核对 dirty/saving/unknown/conflict；第 265–279 行 `finishClose` 再核对世代，并要求所有 view lease 已 release 才 dispose。
- `lib/plugins/editor_draft_session.dart` 第 271–301 行 capture failure 保留上次快照和原操作，detach 后仍阻止 close/eviction；只有新的完整 observation 可以清除失败。`detachUI()` 第 571 行本身为空方法，不是系统输入 drain API。

这一 library 与 actual main 的边界，以及开发用两阶段 fork 协议的范围，延续 [dev20 continuation-design](../v20/retirement/continuation-design.md)。

## HMOS 已有保护与接入前漏洞

2026-10-07T10:09:20.219Z 读到的源码快照：

| 文件 | SHA256 |
| --- | --- |
| `entry/src/main/ets/pages/Index.ets` | `1b87235120a267271b5df39889e96190714562a1f399d7c694b311031d80de6f` |
| `entry/src/main/ets/pages/EditorTodos.ets` | `2e00afde5759be4494f659174a5695b2f6a8fe47030697f33de554fcee649d62` |
| `entry/src/main/ets/model/EditorTodos.ets` | `e7a0748f5066939ad42fa4a9b6bde3d9d9761e15df2e42403356c1277a48d710` |

`EditorTodos.ets` 第 87–108 行冻结 `owner + row ID + incarnation`，实际输入和选区方法核对同一行实例。第 266–275 行 restore 定时器另外核对 serial、focused 及旧票据。第 298–306 行 TextArea 闭包携带冻结 ticket。因 disabled 而不能写入的同 owner 事件由 `onUncaptured` 保留完整事件 JSON；旧 owner/incarnation 不通知新 owner。这些保护应保留。

但第 66、259 行在生命周期结束时置 `alive=false`，子组件在 `stopEditing()` 前已撤销资格；此后 `ownsIdentity` 失败，`onUncaptured` 也不再通知。因此现实现适合撤销旧回调权限，不是 detached session 的完整迟到事件恢复器。`model/EditorTodos.ets` 第 129–148 行清楚分开 `raw_capture_complete` 与 `business_ready`：formatter pending/失败可以只撤销业务资格，不能据此拒绝完整 raw journal 保留；capture failure 则必须继续阻止声称完整保留。

接入前 `Index.ets` 标题第 3180–3181 行、正文第 3042–3043 行、hypothesis/conclusion 第 3245–3251 行、现有卡片单条 todo 第 3320–3321 行，SDK 闭包只调用当前 `this.editorInputChanged` / `draftSelectionChanged`，没有冻结产生它们的 view owner。`attachDraft` 第 1041 行以后更换当前 owner/writer，旧闭包若随后交付，就可能通过“当前页面仍活着、当前 editorOpen、当前 writer 存在”的检查，把旧值写进替换 editor。这是可行动的回调归属缺陷，不能只靠 epoch 检查异步 formatter 解决，因为错误值已在 formatter 请求之前进入了新 owner。

task rename 第 3303 行也需要冻结 task 身份；即使 view lease 不变，从任务 A 切到任务 B 时旧 SDK 回调也不能取得 B 的写权。focus、blur、selection restore、拖动和 reveal 延迟任务均应遵守相同的 lease/行/任务归属。

## API26 允许证明什么

本地 API26 声明位于 `C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/component/`：

| 声明 | 实际合同及限制 |
| --- | --- |
| `text_area.d.ts:83–91`、`text_input.d.ts:723` | `stopEditing(): void` 退出编辑状态，没有异步完成或消息排空回执。 |
| `common.d.ts:24545–24556` | `aboutToDisappear` 在 custom component destroyed **之前**运行；可作为应用 lease 同步撤销点，不能称“所有子节点已经销毁”。 |
| `common.d.ts:25039–25050` | `getSelection()` 只有当前 start/end，unbound/released 时可能 undefined；不能读出完整 TextEditingValue。 |
| `common.d.ts:25052–25065` | `clearPreviewText()` 通知输入法清空 preview；会影响候选态，不是无损 snapshot/drain。 |
| `text_common.d.ts:481,1543–1583` | onChange 第三参 `TextChangeOptions` 可含 rangeBefore/rangeAfter、oldContent、oldPreviewText；本次读到的 HMOS 回调只接前两参。它仍没有 affinity、direction 或平台队列世代 token。 |

公开 `TextContentControllerBase` 没有本次需要的“完整文本 + 选区 + composing + 异步排空确认”声明。TextArea/Input 文档对 `getText` 的提及位于 `<!--Del-->` system API 段，不能假称应用已能用公共 `getText()` 读到这一完整状态。此结论限定于实际读取的公共 controller 声明，不是对所有 SDK 内部能力的猜测。

读取的 SDK 原始 SHA256：`text_area.d.ts` 为 `a0f6ef26a0fd23933df202535ae392af305cff5cadfc36285101e0525abebbb4`，`text_input.d.ts` 为 `a293ba14430404d37191f8edc84f9d87da163df933cb7d11d3e1aa93d92cbec9`，`text_common.d.ts` 为 `d06fb8570b900b39adeb2159c161f3087907b58dfffaf189a558d51bc644a216`，`common.d.ts` 为 `2662a40432b2fa6efcfe387c3bc4bdc79ac9a993de1e415042b37a78d6315614`。

## 可执行接入建议

1. 每次挂载生成新的、不可借给下一视图的 lease ID。wrapper 在 `aboutToAppear` 冻结 mounted owner；不能只改变一个复用组件的 `@Prop owner` 就称建立新 lease。builder 接收该冻结值，各 SDK closure 再携带它；task rename 额外冻结 task ID，行输入继续核对 row incarnation。
2. Parent 校验 lease 为当前 admitted lease，而不是在回调执行时读取新的 `attachmentEditorIdentity` 来生成权威。旧方法可以留作内部调用，但来自 SDK 的入口必须先做 lease guard；按钮、restore 和异步 worker 也不能绕过它。
3. 视图 lease 与 raw writer scope 分开。父 writer 暂停写入时保持完整采集；child 首 ACK 后切换 writer，复制同一视图最新完整 raw，再条件 retire 父。scope 切换本身不能把已挂载节点的 owner 偷换成下一编辑器。
4. 删除 view 时等待真实生命周期回调，由该回调同步撤销应用接纳资格，然后才进入冻结 raw 的持久化/删除阶段。`onRevoked` 的名称和日志应指应用资格撤销，不写 SDK-drained。重复迟到 mounted/revoked 通知必须核对同一 lease，不可撤销后来挂载的实例。
5. 同 lease 在 detaching、尚未撤销期间的合法输入仍完整采集并增加 epoch；已有 raw write 不能取消，等待其已发操作明确结束，再 flush 最新值。任何 capture incomplete 都是 sticky：不能用 last complete snapshot 或 opaque JSON 事件集合假称当前 raw 已完整。失败后保留 session/原请求，重新挂载新 lease 不自动清除失败。

### 手动确认放弃整份草稿

确认文案必须涵盖这份编辑器的完整 raw，包括 detaching 期间仍被接纳的输入。先结束这一视图的接纳，再 flush 准确完整 raw，并以当前确认 generation 发出固定 `draft_discard`。因为用户明确请求结束并删除该视图，这一应用 cutoff 后的旧 lease 不获得任何新视图写权；它无需伪造 SDK queue drain。

如当前是已确认 fork child，先确认准确父条件退休，再 ordinary discard child，才可宣称这份逻辑草稿已放弃。不得为了迟到事件继续 fork 并保留 child，然后把 UI 关闭当作整份放弃完成。Unknown 保留原 wire/operation，只能显式核对；不自动重发新 operation、新建 scope 或打开可接纳旧事件的 replacement editor。

### 业务保存与保留关闭

业务 ACK 只确认冻结 S1，不确认 S2 原始输入，更不授权删除迟到输入。原 source kind/revision 与完整 legacy todos 应保留，create 生成的 V2 TaskId 不能拿来自动替换 raw 行身份；自身业务 revision 已变化的冲突仍显式存在，不自动 rebase。

最小保守交付策略是保存确认后完成 raw child 接续并保持编辑器打开，提示卡片已保存、输入草稿仍保留，允许用户再明确选择继续、保留关闭或放弃。它避免把一次自动关闭的 lease cutoff 冒充完整输入持久化，但属于对 Flutter 自动关闭分支的明确差异。

“保留并关闭”可证明已交付、已完整采集且已 ACK 的 raw 被保留，以及旧 lease 无替换视图写权；本地 SDK 合同不足以证明尚未交付的系统文字也已保留。若后续要支持完整 detached late-input recovery，必须另定旧 session observer/container 合同：携带冻结 lease/owner/row/incarnation 和原事件，只送旧 session；留存当时行映射与事件次序、预算、Unknown，不能写新视图或从任意零散事件编造 aggregate。当前 `EditorTodos` 在 alive=false 后忽略回调，此恢复路径尚未实现。本报告不将新增一个 observer 回调本身等同于完整 raw recovery。

## 后续验证与资格

实际 Index 方法测试应分别覆盖：旧 direct callback 在更换 view 后不写新 draft；同 view 换 task 后旧 rename 不写新 task；旧 row incarnation 不写复用行；detaching 合法输入仍被采集；revoked 后通知不复活 writer；capture incomplete 阻止完整保留宣称；manual discard 最终无遗留 child；Unknown 固定原 wire；业务保存晚到输入继续保留原 source/raw，而不拿整份 card assets 或 V2 tasks 自动改写。

设备测试可以证明特定输入/关闭/恢复序列。它不能用固定延迟、一个 PNG、普通 typed input 或 model PASS 宣称全部 SDK/IME 回调已排空。最终接线源码、产品 SDK 构建和 dev21 设备结果由主代理独立记录，本报告没有授予这些项目 PASS。
