# HMOS 待确认任务的显式决策设计

状态：**READONLY_DESIGN_ONLY / NOT_IMPLEMENTED / NOT_TESTED**。2026-10-09 只读检查，当前 HEAD `62a8b964bb27840f0348bb6b8ef520142880b9b1`。Root 正在对已冻结的 v32 样式跑完整 SDK；本轮只新增本设计文件，不修改产品、测试、SDK、Rust、设备或 Git。

## 实际缺陷与 Flutter 参照

当前 `Index.ets` 没有名为 `taskAction` 的方法。实际两个写入入口是：

- 编辑器已有任务：`Index.ets:2611` 的 `mutate`，在 `2621` 为所有有 task 的请求赋 `cmd.flag = task.completion !== 1`；编辑器行 `4236-4238` 对 `?` 仍绑定普通 `mutate('task_toggle', task)`。
- 详情任务：`Index.ets:2844-2847` 的 `detailTask` 同样使用 `task.completion !== 1`，详情行 `3866-3868` 对 `?` 仍绑定这个入口。

因此 `completion=2` 的一次普通点击会隐式发出 `flag=true`。概览 `3387-3400` 的三类进度统计本身按 0/1/2 区分，其任务预览目前只有显示，没有直接写入按钮。

权威 Flutter 行为来自实际 `build/win-cloud-20261005/lib/versioned_task_panel.dart`，而非根据通用 checkbox 推断：

- `plugins/versioned_content.dart:54` 枚举为 `incomplete / complete / legacyAmbiguous`；`plugins/versioned_idea_view.dart:36-37` 的 `needsExplicitDecision` 只对 `legacyAmbiguous` 为真。待确认是旧任务迁移留下的歧义，不能根据同名任务次数猜完成状态。
- `versioned_task_panel.dart:403-423`：待确认行显示 `help_outline`，**没有 checkbox**；普通 0/1 行才显示 `NeumorphicCheckbox`，其用户 bool 值进入 `TaskEditCommand.setCompletion(task.taskId, value)`。
- `465-482`：待确认行保留解释文字，并列出“标记完成”和“标记未完成”两个按钮，分别传 `true / false`，都绑定原 `TaskId`。
- `323-330`：待确认 context menu 同时列出两个显式方向；普通任务只列出相反方向。菜单打开时捕获实际 view，`canSelect` 要求仍是同一个 view，不能在换卡/更新视图后把旧菜单写到新目标。
- `118-178`：Unknown 保存原 `TaskEditCommand`，新操作禁用；重试只使用原 command。成功回调后仍等待父组件交付已接受的 view，不用本地 bool 或任务计数冒充完成。
- `versioned_task_panel_test.dart:264-323` 测试两个同名待确认任务分别以实际 `task-a / task-b` 选择 true/false；`482` 之后覆盖跨修订的原 command 重试；`762-806` 覆盖 legacy、不可写和 busy 不能派发。这里只阅读测试源码，未运行 Flutter 测试。

`styled_checkbox.dart` 负责原生 Checkbox 语义、连续轮廓和内凹材质，不定义待确认自动转换规则。待确认决策须按照 `VersionedTaskPanel` 复现。

## 实施范围与 UI

交接后只修已有任务的两处真实入口，复用现有 task row、材质、原始输入日志和 Native TaskId mutation；不新建任务存储、不替换原任务集合、不按文本生成身份、不把 pending 重新定义为 incomplete。

| 实际完成状态 | 行内展示 | 用户写入动作 |
| --- | --- | --- |
| 0 未完成 | 正常未勾选 checkbox | 普通点击明确设 true |
| 1 已完成 | 正常已勾选 checkbox | 普通点击明确设 false |
| 2 待确认 | help 图标、待确认解释，完整原 task 文本 | 两个明确按钮分别设 true / false；图标和普通 toggle 不派发 |
| 未知整数/不完整 task DTO | 无可写 checkbox，显示待核对状态 | 不派发、不猜测 |

编辑器和详情使用同一套状态分支。待确认解释及两个按钮采用可换行布局，小宽度不裁掉一个方向；保留任务全文、TaskId、已有顺序和概览三类计数。Context menu 也提供两个明确方向，沿用现有重命名/上下移/移除入口。无需额外确认对话框：用户点击已经写明方向的按钮就是该次明确决策。

新增可稳定定位的控件 id，例如 `task-ambiguous-complete:<TaskId>`、`task-ambiguous-incomplete:<TaskId>` 和详情对应 id；可访问文字含原任务文本与方向。普通行不暴露待确认双按钮；待确认行不保留会默认完成的旧点击动作。

## 精确身份、CAS 与 Unknown 不变量

建议新增统一的 `taskAction`/`taskDecision` helper，参数至少包含**渲染时的 CardView 原 snapshot、原 TaskView、显式目标 bool（普通 toggle 可只对 0/1 推导）、相应页面/编辑器 owner**。实际名称由交接实施时决定。

1. 详情判断需同一当前详情 card/selected，编辑器判断需原 editor view lease；两者都需仍 pageAlive/foreground、实际可写、无 dirty 或重命名输入、无 RawFork/业务待核对/附件/粘贴/字段验证/原 `pending` 写入。按钮 enabled 与 helper 内检查必须一致，不能只靠 UI disabled。
2. 渲染时原 card 的 id/source 必须仍与实际 `cards` 中当前对象一致；task 必须仍在这个实际 card 中，按 TaskId 查找，原 completion/text 与渲染值一致。不按数组 index、同名计数、当前 selected 猜原 task。
3. 编辑器需要把 render-time card snapshot 传入专用任务 Builder 或闭包；不能在按钮点击后才用 `this.current()` 重新取得新 source 并把旧 task 套到新 CAS。旧菜单回调也受相同 snapshot/owner 检查。
4. completion=2 时只有 `typeof complete === 'boolean'` 的显式调用可以继续；缺目标 bool 的普通 toggle 必须退出。0/1 普通 toggle 才允许推导相反方向；任何其他 completion 均拒绝。legacy/format-1 任务保持只读，迁移属于已有明确迁移流程。
5. 删除 `mutate` 中 `task.completion !== 1` 对 task toggle 的隐式语义。两处任务 UI 全部改用统一 helper；保留旧 generic mutate 时，对 task_toggle 加封闭的无显式目标拒绝，确保私有方法直调、残留菜单或旧控件不能绕过待确认门。
6. 通过身份检查后只生成一次 operation，命令沿用 `action='task_toggle'`、原 card id/source、原 task_id、显式 flag 和固定 now_ms。source 是现有 Core 的真实 source-card CAS；不要添加另一个 revision 协议。
7. 复用实际 `submit` 和 `retry` 及实际 `Workbench` admission queue，保持先完整保存编辑器原始输入、原 immutable `pending` 序列化字符串及现有 business-source continuation。不得直接 `sendRaw` 绕过输入保留，或为了 UI 简化重 attach/rebase 原草稿。
8. 请求抛错、Unknown、失去合格回执或提交后列表读取失败时，仍保留同一原 wire、source、task_id、operation、flag；两种决策按钮和所有新任务写入都禁用。显式 retry 只重发相同 wire；不因当前 task 还是 2 或用户重复点击生成第二个 operation。
9. 返回真实 `not_committed` 只解除原未提交请求，并保留原待确认展示和完整 raw 输入；后续要由用户重新明确选择。不是成功、不是自动完成、不是自动反向尝试。
10. 合格成功后安装 Native 返回的实际完整 card view，再由实际 completion 0/1/2 计算 UI。不得先改 `task.completion`、按成功次数减 pending、按 checkbox 本地状态算完成，或把两个同名 task 一起决定。

现有 `retry` 在 `result.ok` 时直接更新 cards/清除 pending，对 task completion 还应补一个有边界的实际回执检查：需 committed effect、合法正 receipt_revision、完整实际 card/task DTO，且 target card 可以按实际 id 找到。若接受的是同一 receipt 修订，应核对原 TaskId 的显式目标状态；若真实历史重试的 receipt 修订小于当前 card 修订，必须保留 Native 最新视图，不能强行把现在的 task 写回旧目标，或因最新状态不同就判定原 operation 没提交。不完整/伪成功/无合法回执的响应继续作为 Unknown 保留原 wire。具体检查不得超出当前 Native Reply 能证明的范围；本协议没有新增历史 task 读取 API。

现有 Native 已满足基本命令语义：`hmos/rust/src/lib.rs:697` 将 task_toggle 映射到 `tasks_v2::Command::SetCompletion { id: r.task_id, complete: r.flag }`；随后 `VersionedContentChange` 带原 source_card 交 Core CAS。`tasks_v2.rs:399` 只改查找到的那一个 TaskId 的 completion 字段。`lib.rs:600-615` 在 mutation 前查原 operation 历史，`775-790` 的提交后列表读取失败仍是错误；保持同一原 wire 的重试是必要条件。无需改 Rust 或复制其转换逻辑到 UI。

## 后续源执行验证计划

以下都为 **NOT_RUN**，交接后执行实际 Index 方法和实际 Workbench queue，平台/Native 回复按已有 harness 明确受控；不把 regex 找到按钮、纯手写 helper 或受控回复冒称设备/Store 资格。

1. 详情与编辑器的 completion=2 普通 toggle 都得到 **0 Native mutation**；help 图标只显示状态，不能默认完成。
2. 两处显式 true/false 各生成一次 task_toggle，原 rendered source、实际 task_id、显式 flag 不变；成功只更新实际返回 view。
3. 同名重复任务 a/b 均 completion=2：a 明确 complete 后只有 a 变 1，b 仍 2；随后在实际新 snapshot 上明确 b incomplete，只有 b 变 0。重排后仍按原 TaskId，不能按 index/text 匹配。
4. 正常 0/1 checkbox 仍能分别 true/false；completion=3、不完整 DTO、format-1、deleted、busy、dirty、原重命名、Unknown/RawFork/业务待核对等状态均 0 新请求。
5. 渲染后 card source 改变、task 被移除/替换/改变 completion、详情关闭/换卡、editor lease 撤销或前后台 epoch 改变时，旧按钮/菜单动作 0 请求，不生成新 operation。
6. Native 已提交但响应丢失、Native 尚未提交但响应丢失各自只保留一条原 wire；重复点击两个方向都不能替换它；显式 retry 的 bytes、CAS、TaskId、operation、flag 全部相同。
7. committed 但列表读取 Unknown、不完整 success DTO、缺 effect/receipt、未知 task completion 都不乐观清 pending；真实历史 receipt + 更新的实际 card 可被安全接受，最终 UI显示最新 view。
8. 明确 not_committed 不显示已完成，raw 输入/原待确认 task 保留。一次任务决定不能消耗另一条未提交 raw todo 文本、附件 pin 或正在编辑的正文。
9. 实际 `submit/flushDraft/retry` 的先后和 business-source continuation 仍成立，后续普通输入/重命名/排序/移除的既有检查无回归。
10. 专门把同一 TaskId/source 下的两项动作和状态分支接入源执行组件验证；完整 SDK 构建检查产品调用图。设备另验证两处控件、窄宽度可读性、禁用状态和实际点击方向；未取得真实 pending fixture 和执行结果时保持 NOT_RUN。

可扩展 `index-business-test-harness.cjs`，其 `actualMethod` 已用 TypeScript AST 从原 Index 逐方法抽取；加入新 helper 与实际 `detailTask/canChangeCard`，并保留实际 `submit/retry/current/command/canEditTasks`，不要用固定返回 true 的仿制 guard。报告需绑定最终 Index、Workbench、所有实际模型/测试与 fixture 的运行前后完整哈希；此设计不触发重新运行 v32 SDK。

## 此次实际读取身份

这些哈希只绑定只读设计来源，不代表实现或测试通过。未来交接先刷新来源，尤其 Index 正在由 Root 纳入 v32 SDK。

| 文件 | 字节 | SHA256 |
| --- | ---: | --- |
| hmos/entry/src/main/ets/pages/Index.ets | 338531 | 0930A469A4DD92E98DB914E40FB464763A7FF0A0C4A5678E325F72A62684D773 |
| hmos/entry/src/main/ets/model/Workbench.ets | 2502 | 71582E668921776554E48F0469E1F6B323728DB076082FD22ED5AE7E4BB0B423 |
| hmos/rust/src/lib.rs | 64423 | 1CC8E44723FC69965777C0C502EBFBB88C4561BF89CD163D51E684AB1E2CA938 |
| hmos/shared/plugins/workbench/src/tasks_v2.rs | 16389 | 43849B828B2C6F4605EA8FE677CBB4E95A64FCFB68EEC4639346556E9947C70D |
| build/win-cloud-20261005/lib/versioned_task_panel.dart | 21175 | F97EC18EC42B4F6F33963EB0EB5DA15A137F2588F6E4915C7DB99F7E6D40C9A3 |
| build/win-cloud-20261005/lib/styled_checkbox.dart | 14979 | B9B14DA6DF8476A6DCDEB08EDF424A1C918573BA1CADCDA9C6D0C5CAE10E2BBD |
| build/win-cloud-20261005/lib/plugins/versioned_idea_view.dart | 5282 | B9BF37C9672F7F3F9C1BE0D609B71DC5E918B4BB68C9AFD288EDD80FE5957B87 |
| build/win-cloud-20261005/lib/plugins/versioned_content.dart | 8771 | FB8174A7429DECC0E5CD640E0A2CC6FA946B6EF402038B90C61F7902381B502F |
| build/win-cloud-20261005/test/versioned_task_panel_test.dart | 26887 | EC8B1658ACBF4C1D6656CD7E72809D080B9B7FCFE4B24CB6C8CD2ED890706C9E |
| hmos/tool/index-business-test-harness.cjs | 35080 | 76122ACA4AAE4D2FFB13DDC321B9E5E61CFC183FBA060396F41E6833C8DA97E9 |
