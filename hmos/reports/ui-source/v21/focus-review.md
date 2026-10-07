# dev21 新增待办行焦点修复独立复核

范围：只读审查 `pages/EditorTodos.ets`、其真实组件方法测试，以及并行接线中的 Index lease/fork 相关方法。仅新增本报告；没有修改生产源码或测试，没有操作 Git、设备或产品构建。报告记录修复前的两个探针及主代理正在实施的最小修法，不给后续改版预先授予 PASS。

## 已有失败与首轮修复的证据范围

[add-row-jscrash.log](device-todos/add-row-jscrash.log) 是此前实际运行包的失败证据：版本字段 `0.1.0-hmos-dev.19/1000019`，错误 `150003`，原因是 component 不存在、不可见或 disabled；栈定位 `requestFocus` → `EditorTodos.ets:122:75`。它不能当作新焦点修复包已运行的结果。

首轮源码修复将 Add 成功与 ArkUI field 已挂载区分：等同 owner/row incarnation 的有效 area，先 reveal，再在之后的 32ms timer 尝试 requestFocus；最多 12 次尝试，捕获暂不可见错误；disable、disappear、owner switch 撤销 timer，焦点失败不回滚已创建行。

本次读取的 [todos-focus-model-tests.log](todos-focus-model-tests.log) 记录 78/78 PASS、0 FAIL，组件规范化源码 SHA256 为 `7f73e7b9666fb6f7bd2fb0fe9b6808774952c8406659c40912fae97d187b4be3`。已有检查覆盖 wait-area、reveal-before-focus、150003 后可恢复、永久不可见有界、disable/disappear/owner switch 撤销和 raw 不丢行。本审查没有重跑这份完整矩阵；日志是该首轮目标的证据，不是本报告后续建议已实现的证明。

## 两个真实组件方法探针

通过现有 `editor-todos-component-methods.test.cjs` 的相同源提取与 ArkUI decorator 去除方式加载真实 ETS 字段/方法，仅将 Node test 注册改为内存空注册来调用 fixture。没有保存临时测试文件，也没有替换组件的焦点/area/生命周期方法。SDK FocusController 在这个 host harness 中仍是可观察 stub，不能证明真实挂载或设备焦点行为。

| 探针 | 实际结果 | 可行动缺陷 |
| --- | --- | --- |
| 原 raw `a`，Add 创建 `todo_2`；新行 area 到达后，用户对应的 `focusFor(todo_1)`；再执行 pending focus 步骤 | `focused_id=todo_1`，但 `requestFocus` 仍请求 `editor-todo-owner-A-todo_2`；raw 保持 `a\n` | 首轮 `focusFor` 没取消 Add focus intent；`owns` 只核 owner/行/值归属，不核用户后来选择的焦点，重试会抢焦点。 |
| 同 row ID 建立旧 incarnation 1 与当前 incarnation 2 票据；记录当前有效 area 后交付旧票据 area | area map 由存在变不存在 | 首轮 `areaFor` 的 `!owns(ticket)` 分支按 row ID 删除 map；旧票据能够删除当前 incarnation 的测量结果。 |

第一个探针覆盖 Add 已排队后的转焦。源码另有同一意图生命周期缺口：`addRow` 在 `await model.add()` 前没有冻结 focus intent，用户在 count await 期间转焦后，Add 返回仍会建立一个新 pending focus。外部 title/body 等控件的焦点不调用 row `focusFor`，所以仅加“另一行 focus 时取消 timer”也不足以覆盖全部编辑器。

第二个探针为同一行构造新旧 incarnation 状态，检验的是 area map 对旧回调的权威检查；不将这个 host 状态构造称为设备回调次序证明。

## 最小修法及状态

主代理已收到两条探针并决定修复；本报告冻结时不声称最终改版已构建或设备通过。

1. 为 Add 自动焦点建立独立 focus intent 世代。开始 Add 时捕获世代；任何有效的用户转焦/输入意图撤销 pending timer 并增加世代；Add await 返回后只有原世代仍有效才可排队。timer 也检查原 intent，不得读取新 intent 借用权威。
2. 同 owner 的 row focus 撤销原 Add intent，包括用户手动点中新行；普通用户编辑应优先于延迟自动焦点。Index 外部字段焦点通过明确 `focusIntent` epoch 与组件连接，避免 timer 抢回用户已经选择的 title/body/hypothesis/conclusion 等控件。旧 view owner 的焦点不能撤销新 view 的 intent。
3. `areaFor` 先拒旧 owner/row/incarnation 票据且不修改 map。只有当前合法票据携带无效 geometry 时才删除其当前测量；避免旧 incarnation 的 area 事件撤销新 incarnation。
4. 保留 existing wait-area、reveal、12×32ms 上界与 requestFocus 错误捕获；撤销焦点意图不得删除已创建行或改变已采集 raw。后续 fresh tests 应分别覆盖 queued focus 转焦、Add await 期间转焦、外部 field focus、旧 ticket area 不删新 measurement，以及 disable/disappear 撤销。

## Index 中途接线复核

本次读到的中途 Index 已把 title/body/hypothesis/conclusion/单条 todo 的 text、selection、focus 入口接到冻结 owner 的 lease wrappers；task rename 额外核 task ID 和 rename incarnation。`EditorViewLease` 冻结 mounted owner，其注释明确 revoke 是应用接纳边界，`aboutToDisappear` 不声称 SDK queue drained。与先前 [editor-lifecycle-audit](editor-lifecycle-audit.md) 的建议一致。

fork 中途源码先暂停父写入、保留采集；child first ACK 后复制父当前完整 raw 并在同一同步 turn 切换 writer；child flush 明确确认后才发原 wire 的条件父退休。它保留原 source kind/revision/source bytes，未自动用业务卡片当前 revision 替代；Unknown 保留原操作。此为有限范围源码观察，不是完整最终 Index review、native dispatch 或运行时资格。

本次已读到的 category/stage Select 回调仍直接访问当前 Index，未加 text/focus 相同的 `ownsEditorView(owner)` 门禁，已告知接线代理；其后续修复和 fresh checks 由主代理独立记录。该建议限于同一 lease 的旧事件归属，不扩展业务功能。

业务保存自动关闭/keep-close 仍须限定于已交付且完整采集的应用 raw：组件 lease 撤销能阻旧 callback 写替换视图，不能证明尚未交付 SDK 文字全已持久保存。本报告没有以 78 tests、probe stub 或旧设备日志扩大该资格。

最终产品源码 identity、更新后的矩阵/SDK/HAP 及 dev21 真机结果，以主代理后续 fresh 证据为准。本报告的两个失败探针保留为修复前观察，不重写成新包设备失败或新包通过。
