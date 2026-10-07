# v23 restored-todo capture review and narrow component repair

本次修复限于 `pages/EditorTodos.ets` 的行组件和直接执行其实际方法的测试。未修改纯行模型、Index、Rust、版本或既有设备证据，未构建、安装或操作设备。范围是恢复期间的选区事件分类；不是 SDK 回调队列排空证明，也不是实机问题已修复的验收。

## 已观察到的旧候选行为

审阅的旧设备候选为 [final-artifact.json](../v22/lease/final-artifact.json) 对应的无签名 HAP，28,799,011 bytes，SHA256 `96C0AB68BCB513251C56AC9E61543594762693BEFE280E31E24552C50679CA28`。报告中的 `installed:false` 是构建时元数据；后续 [progress-clipboard.json](../v22/device-final/progress-clipboard.json) 分阶段记录安装和操作。版本名仍为 `0.1.0-hmos-dev.19` / `1000019`，不能据版本名替代该候选身份。此修复尚未进入该 HAP。

| 原始阶段（UTC，2026-10-07） | 原证据与限定结论 |
| --- | --- |
| `final-input-first-D-v22`，10:47:48 开始 | 保留原 `FAILED_OR_UNKNOWN`。输入脚本后编辑器已关闭，helper 无法找到目标行；未重放输入。 |
| `final-inspect-D-v22`，10:48:11 | 只读检查见编辑器关闭及已保留草稿提示。 |
| `final-recover-after-input-D-v22`，10:51:21–10:51:32 | 恢复后的首个 [restored tree](../v22/device-final/final-recover-after-input-D-v22-restored.json) 已出现“当前事件尚未完整捕获，原事件已保留；草稿清理已停止。”；[seek tree](../v22/device-final/final-recover-after-input-D-v22-seek-13.json) 同时显示完整 `first 汉字 🧪 é.`、`13/1000` 及已启用的行。该阶段为限定恢复检查 `PASS`，不是首次输入阶段改判。 |
| `final-preserve-before-final-D-v22`，10:51:32 开始 | 点击保留有命令回执，但 [after tree](../v22/device-final/final-preserve-before-final-D-v22-after.json) 仍有 `draft-title` 和编辑器，提示变成“当前输入尚未完整捕获，请完成输入后重试，编辑器保持打开。”。原 `FAILED_OR_UNKNOWN` 保留。 |

UI tree / progress 没有保存 `onUncaptured` 的原始事件 JSON，也没有 SDK 事件类型、选区或 preview 的完整时序。独有提示可确认 `Index.leaseUncaptured` 路径已执行；不能仅据它证明具体是某一次 `selection` 而非 `input`。下面的代码闭环是已证实可触发的缺陷，设备实际 kind trace 仍缺。

## 可触发阻断的源码闭环

设备候选的源码清单见 [final-source-copy-manifest.json](../v22/lease/final-source-copy-manifest.json)。修复前本地对应的行组件 SHA256 为 `6ABAA19A360487AEAB67F34E5F53EF966B1D3566A05B42377A9AF2901E2AAD18`；纯模型 `E7A0748F5066939AD42FA4A9B6BDE3D9D9761E15DF2E42403356C1277A48D710`；Index `34DDECA5A375328ADB1A7EC53DE7F8E5D1A90804CD8634FAA85672D7A588160C`。后两者本次未改。

1. `Index.restoreDraft`（1290–1302 行）恢复完整五字段 raw，并将 `draftRestoreInput=true`；16ms 定时器才取消该状态。`EditorTodos.editingEnabled`（3519 行）在此期间为 false。定时器并不证明原生控制器事件已结束。
2. 原行组件 `selectionFor` 先检查 disabled 并通知 `onUncaptured`，之后才检查行身份、聚焦及控制器选区恢复回声。因此，同 owner / incarnation 的未聚焦初建选区也能被当作未捕获输入。普通字段在 Index 已有聚焦选区门禁；待办行的 disabled 分支绕过了对应分类。
3. `Index.leaseUncaptured`（1097–1101 行）把事件存入 `retirementInput['todos:text']`，递增 epoch，并置 `draftCaptureIncomplete=true`。当前 hook（3530–3531 行）没有另按 row 或事件 kind 分类。
4. `todoRowsStatus`（638–644 行）在存在上述未处理事件时拒绝 status 清除阻断。完整恢复的模型 raw / `13/1000` 无法证明这个额外事件已捕获；取消 restore flag 也不能消解该事实。
5. `keepDraftAndClose`（1684–1694 行）首先调用 `flushDraft`；其 1311 行对 incomplete 立即返回 false。因此本次保留点击在首次 flush 即停止，未进入 view detach，也没有执行新 discard。

本次没有取消 Index 的这些 fail-closed 检查，也没有把未知事件或正确显示的正文当作 full-TextValue 捕获证明。

## 本次窄修复

实际 [行组件源码](../../../entry/src/main/ets/pages/EditorTodos.ets) 的 `selectionFor`（323–332 行）现在依次核对：活跃原始 owner / row / incarnation、行聚焦、确实发出的控制器恢复选区，然后才分类 disabled 的真实事件。

- 未聚焦的初建或刷新选区不产生 raw selection，也不产生未捕获事件；这与原 enabled 分支的聚焦准入一致。
- 匹配恢复回声要求 `expectedBase/Extent` 均非负，且只消费一次。`rowChanged`（301–313 行）在新 incarnation / pending / composition 路径之前清空旧 marker，旧票据不能消费新行的回声。没有发出的 `-1/-1` 不再被哨兵误认作恢复回声。
- 已聚焦且 disabled 的非回声选区，包括无效或无选区位置，仍逐字保存原事件 JSON。所有 disabled 文本 input / preview 保持原路径；没有截断、补全 preview 或猜测 aggregate。
- `focusFor`（334–338 行）允许同身份 disabled blur 撤销当前聚焦及旧 marker；旧身份 blur 无权撤销新行。测试覆盖已交付真实选区先被保留、再 blur 的时序。它不声称 blur 之后没有尚在 SDK 队列中的旧事件。

## 首次窄修复的 host 验证与冻结身份

[todos-capture-model-tests.log](todos-capture-model-tests.log)：实际 ETS 模型与组件方法共同 **88/88 PASS，0 failed / 0 skipped**，1,206.218ms。原 82 项仍通过；新增六项直接调用实际行 / 父组件方法，覆盖：恢复失能初建、已发控制器回声跨 disabled、真实选区先交付后 blur、无效 disabled 选区、旧 incarnation 不能借新票据、pending 刷新取消旧恢复 marker。旧 disabled 输入 / 选区断言改为先实际聚焦，保留它原本要验证的真实聚焦事件语义。

| 本次冻结文件 | Bytes | SHA256 |
| --- | ---: | --- |
| `pages/EditorTodos.ets` | 22,682 | `61C7E8CF04E6293EB0217CA9F417CF9CF71713227D942FE47C2BBEF1CB7ECB91` |
| `tool/editor-todos-component-methods.test.cjs` | 32,081 | `EE5AC765BBBE9DA12CD6F7D467750056ACB6F70EF65FF063D8FDDF917B6D1318` |
| `todos-capture-model-tests.log` | 9,360 | `4DC2F0AB160AB0DA57C7883AF2E5F2B949B3E7C5D15979F41D1073EF2D1DF3C3` |

这些 host 方法测试证明该分类路径及 owner / incarnation 约束，没有重现实际 SDK 事件调度。上述身份是首次 `61C7` 修复冻结，原 88 项日志保留；后续构建、设备和最终源码范围分别记录如下。既有 96C0 阶段失败不改判，不自动清除任何已记录 Unknown input。

## 后续只读 Watch 审查及完整快照门禁

主代理首个独立候选构建产物 `dev23-todo-capture/.../pages/EditorTodos.ts` 的 146–148 行分别为 owner、revision、source 注册 Watch；266–270 行的 `updateStateVars` 按 owner → revision → source 逐项 reset。[OpenHarmony 官方 Watch 说明](https://github.com/openharmony/docs/blob/master/zh-cn/application-dev/ui/state-management/arkts-watch.md)规定回调在属性变更后同步执行。这里的三个属性不能被当作原子快照。

模型先在 `input` 中置 pending，再由 `emit` 同步捕获完整 future raw 和父级返回的 revision；格式器仍在异步等待。旧组件无条件 bind 中途的“新 revision + 旧 source”会递增 model version / row incarnation 并清 pending。文字不同的两次 bind 还会清 formatError，从而可能把没有确认的格式器结果误标为 business-ready；同文字但 selection / composition 不同的路径会留下 interrupted formatError，阻断之后的业务确认。两者均不直接改写父级 durable raw。

原方法 harness 剥除了 decorators，capture 同时更新两个 props，echo 最后只执行一次 `sourceChanged`，因此原 88 PASS 不覆盖逐属性 Watch 调度。新增场景保留真实组件 / 模型方法，仅按每个 prop 赋值后调用其真实 Watch 方法来交付父级完整/中途快照；没有复制 bind 或门禁实现。

主代理授权后的最终组件在 `sourceChanged`（75–82 行）先复制完整 source，并直接调用父注入的 `isCurrent(owner, revision, value)`。不匹配的中途组合暂缓，发生于任何 owner 清理、bind 或 stop 之前；没有报告新的 capture failure，也不借旧组合更新 rows。完整 self echo 通过后命中模型的 exact fast-return，保留 pending / incarnation；真实外部完整替换仍撤销旧格式器。此检查不依赖 editingEnabled，disabled 的完整恢复快照仍准入；`availabilityChanged` 的 disable → stop 行为不变。

[todos-watch-final-tests.log](todos-watch-final-tests.log)：**95/95 PASS，0 failed / 0 skipped，1,361.9155ms**。原 88 项加七个实际方法场景：changed-text 两种 prop 顺序、同文字 reverse selection 与旧回执围栏、同文字 IME commit、真实外部完整替换、new-owner 分步交付与 area / owner 围栏、disabled 完整恢复。自己的回传在准确 ACK 前持续 business-ready=false，准确 ACK 后继续采纳；外部 replacement 不接纳旧 ACK。已有 disabled-preflight 测试在重启前恢复真实父级 current 检查，以提供可合法重新 bind 的完整快照，原失能期间事件通知断言保留。

| 最终冻结文件 | Bytes | SHA256 |
| --- | ---: | --- |
| `pages/EditorTodos.ets` | 22,951 | `E0A3DA36C9AEED16B37FCD3BFD46B83EF35A32373933D38E5413692C15C835DE` |
| `tool/editor-todos-component-methods.test.cjs` | 42,667 | `B03017FC9089AD2393ADB24576CAAFCC5A11FE296BF9F3F22330A7DF097EDC56` |
| `todos-watch-final-tests.log` | 10,220 | `14B8F504026C8BBE5F18D8CB0E50346A588BD3C06F5A880B49D6E9129EF5121B` |

## 首个 3116 设备候选失败与最终资格边界

主代理 [installation.json](device-current/installation.json) 记录首个候选 `3116FBB89FB09A0B4747BCF207F87C18FE7AC58BFD24B23233BE7619DB602EBC` / 29,102,190 bytes 已安装启动。它包含首次 `61C7` 组件，不包含后续 `E0A3` Watch 门禁；实际 SDK 构建和设备操作由主代理执行，本审查仅阅读证据。

[progress-clipboard.json](device-current/progress-clipboard.json) 中 `v23-fixed-restore-D` 于 11:12:42 UTC 开始，记录 **FAILED_OR_UNKNOWN**，错误为 `restoration must not raise missing capture`。[restored tree](device-current/v23-fixed-restore-D-restored.json) 和 [row tree](device-current/v23-fixed-restore-D-row-seek-0.json) 再次显示独有未捕获事件提示与完整 `first 汉字 🧪 é.`。keep 阶段未执行，失败未重放；不能把候选安装成功或 95 项 host PASS 写作恢复/关闭 PASS。

这些文件仍没有原始 `onUncaptured` 事件的 field / kind / selection / preview trace，不能断言此次触发者就是待办选区。具体待定位入口还包括：待办 disabled 文本事件保留路径；Index 的 `leaseSelectionChanged`（1078–1084 行）在 `draftSelectionChanged` 的聚焦/恢复检查之前验证 positions，故普通字段未聚焦的 invalid 初建选区也能进入 `leaseUncaptured`。后者本次仅只读指出，未修改。完整 Watch 门禁修的是格式器资格，不能自动清除这些尚未证明完整捕获的事件。

最终 `E0A3` 源码的产品 SDK 构建、精确包安装、原草稿恢复、保留关闭及真实 callback trace 截至本报告均 **NOT_RUN**。模型/Index/Rust 未改，源和日志已冻结；下一轮须按准确 field、kind 和原始值诊断，不得因 visible text / counter 正确而静默清除 Unknown。
