# v23 分支交付独立审查

结论：本轮相对 `8c3059615d9adc1296363cd365267b24929b3cc7` 的待办组件修复未发现必须阻止源码分支推送的确定缺陷。可交付范围是源码、实际方法/模型检查及独立 SDK 构建证据；完整目标仍 OPEN。最终包的恢复、保留关闭及真实 SDK 回调顺序尚未通过设备验证。

本审查只读取最终源码、差异、测试源码、候选构建清单和既有设备记录；未编辑生产源码或测试，未执行 Git 修改、设备操作或构建。

| 最终审查输入 | Bytes | fresh SHA256 |
| --- | ---: | --- |
| `entry/src/main/ets/pages/EditorTodos.ets` | 22,951 | `E0A3DA36C9AEED16B37FCD3BFD46B83EF35A32373933D38E5413692C15C835DE` |
| `tool/editor-todos-component-methods.test.cjs` | 42,667 | `B03017FC9089AD2393ADB24576CAAFCC5A11FE296BF9F3F22330A7DF097EDC56` |

## 完整快照和生命周期

`sourceChanged` 先复制完整 `source`，再通过父级 `isCurrent(ownerKey, revision, value)` 核对；不匹配时在 owner 清理、`model.bind` 和 `model.stop` 之前返回。实际 Index 注入的 `todoRowsCurrent` 核 editor/draft/lifecycle、owner 和 revision，并以 `samePasteTarget` 比较 text、两个 UTF16 selection、affinity、directional 和两个 composing 字段，不只是文字相同。

因此分步属性交付产生的“新 revision + 旧 TextValue”不会取消尚在等待的格式器或换 row incarnation。完整 self echo 通过模型 exact fast-return，真实外部完整替换仍撤销旧回执。门禁不借 `editingEnabled` 判断快照资格，禁用中的完整恢复仍能初始化行，随后明确 stop；旧身份回调没有借用新 owner 的资格。

## 恢复选区与禁用事件

子组件先核 owner/row/incarnation 和 focused，再区分实际发出的 controller selection echo。`rowChanged` 会清旧 echo 标记；只有非负且准确匹配已发出的 base/extent 才消耗一次 echo。未发出恢复命令的 `-1/-1` 不再被错误当作 echo。禁用中的当前 focused selection（包括非法/缺省位置）仍以完整事件 JSON 交给 `onUncaptured`；禁用 text/preview 不依赖 focused，仍保留。disabled blur 可以撤销当前 focus，旧 incarnation 的 blur 不会撤销新行 focus。

未聚焦控件的初始化 selection 不会凭空制造新的 raw selection 或缺失捕获。这里采用 focused 状态分类，不能证明实际 SDK 在 blur 后没有尚未交付的真实 selection；也没有证明任意 IME composing 可由控制器恢复。它们仍是设备/lifecycle 资格缺口，不能从 host 方法调用推导为队列已排空。

## 测试没有替代运行时证明

实际方法 harness 读取原组件与模型，仅去掉 ArkUI decorators/struct spelling 和声明式 builders，控制器、格式器返回与 SDK 调度由 stub 驱动。旧 harness 的 capture 一次更新 revision/source，确实不能覆盖独立 Watch 调度。新增 `deferProps`/`deliver` 每次交付一个属性，并调用原 `sourceChanged`：覆盖 changed-text 两种顺序、同文字反向 selection、IME commit、外部替换、new owner 和 disabled restore；检查完整 raw、pending/business-ready、incarnation 及旧回执资格，而非只断言某个门禁存在。

旧 disabled selection 场景先调用实际 focus 方法再禁用，明确测试真实 focused selection；新增初始化无焦点场景另测不会产生缺失捕获。这项调整没有移除 disabled text/preview、非法 selection 或旧身份拒绝的断言。

只读核对 [todos-watch-final-tests.log](todos-watch-final-tests.log)：最终实际组件方法与模型合计 **95 PASS / 0 failed / 0 skipped，1,361.9155ms**，日志中的标准化源码 SHA 与最终 `E0A3` 相同。[models-final-tests.log](models-final-tests.log) 为 **755 PASS / 0 failed / 0 skipped**。这些是 host 证据，没有执行原生分词算法或模拟真实 ArkUI Watch/控制器调度。

[hap-build.log](hap-build.log) 记录最终独立候选 SDK 构建 **SUCCESS，13.482s，34 tasks executed**；[ui-final/source-copy-manifest.json](ui-final/source-copy-manifest.json) 的组件输入确为 `E0A3`。此审查核对日志与输入清单，不独立重建或安装包。

## 首个候选失败不改判

[ui/source-copy-manifest.json](ui/source-copy-manifest.json) 明确首个候选只采用 `61C7E8CF04E6293EB0217CA9F417CF9CF71713227D942FE47C2BBEF1CB7ECB91` / 22,682 bytes 组件。[installation.json](device-current/installation.json) 安装的是 **3116FBB89FB09A0B4747BCF207F87C18FE7AC58BFD24B23233BE7619DB602EBC / 29,102,190 bytes** 包；该包没有最终完整快照门禁。

[progress-clipboard.json](device-current/progress-clipboard.json) 的 `v23-fixed-restore-D` 在 2026-10-07 11:12:42 UTC 开始，结果 **FAILED_OR_UNKNOWN**，错误 `restoration must not raise missing capture`。恢复树记录“当前事件尚未完整捕获，原事件已保留；草稿清理已停止”，后续 row tree 显示 `first 汉字 🧪 é.`。可见文字/计数不是完整 raw capture 或关闭成功证明；该阶段未执行 keep。

记录没有原始 callback 的 field/kind/selection/preview trace，不能确认失败来自待办 selection，不能把最终 Watch 修复说成已消除该失败。现 Index 的普通字段 `leaseSelectionChanged` 在 focus/restore 策略之前检查 positions，也是需要准确 trace 区分的路径；本轮没有修改它。

最终 `E0A3` 包的设备恢复/保留关闭资格仍 **NOT_RUN**。后续应针对精确最终包记录事件来源与完整 raw 值，再验证恢复、编辑和保留关闭；本轮 push 不提升为 Flutter/Windows 全功能或真实设备验收通过。新 business intent/source0 handoff 文档仍为候选合同，不是本次已实现的能力。
