# v23 待办回调保护与分支交付检查点

2026-10-07，基线 `8c3059615d9adc1296363cd365267b24929b3cc7`。只交付 `hmos/`，目标分支 `codex/ArkTsUI`，不并入主线。源码仍 **0.1.0-hmos-dev.19 /1000019**；v23 是证据目录，不是新发布版本。完整 Flutter/Windows UI 与功能目标仍 **OPEN**。

## 本轮实际实现

`EditorTodos.ets` 在更换行、取消格式处理或清理 owner 状态前，核对 parent 的完整 owner/revision/TextValue。分项更新 props 时的混合快照不再打断自己的 formatter；真实完整外部替换仍取消旧请求。disabled 的完整恢复快照继续准入。

行输入组件忽略未聚焦控件的初始化选区通知，只消费实际发出过的 controller 恢复回声。真实已聚焦行在 disabled 后发出的不同选区以及文字/preview 仍按原事件报告；失焦、更换行和旧 incarnation 不借用新的恢复标记。这是窄化事件分类，不清除既有 Unknown，不证明 SDK 输入队列已经排空。

Index、Todo 模型、Rust、协议、C++、依赖与业务接线未改。最终源码身份：

| 文件 | 字节 | SHA256 |
| --- | ---: | --- |
| `entry/src/main/ets/pages/EditorTodos.ets` | 22,951 | `E0A3DA36C9AEED16B37FCD3BFD46B83EF35A32373933D38E5413692C15C835DE` |
| `tool/editor-todos-component-methods.test.cjs` | 42,667 | `B03017FC9089AD2393ADB24576CAAFCC5A11FE296BF9F3F22330A7DF097EDC56` |

[Todo 审查](todo-capture-review.md) 在构建前冻结，其“最终产品构建 NOT_RUN”描述报告冻结时刻；以下主代理后续构建结果补充该资格，设备状态仍 NOT_RUN。[独立交付审查](delivery-review.md) 未发现阻止本次源码分支推送的确定缺陷。

## 新跑验证和最终包

| 检查 | 实际结果与边界 |
| --- | --- |
| 全部实际 ETS/tool 模型 | **755/755 PASS，0 fail、0 skip，7,992.1465 ms**；[完整日志](models-final-tests.log)，SHA `4475DF7AD691523ACA6DDCD0F1F38F868DFFAF8E3489931BA7249D4B0F554389` |
| 组件实际方法专项 | **95/95 PASS，0 fail、0 skip，1,361.9155 ms**，包含于完整模型，不累加。[专项日志](todos-watch-final-tests.log)，SHA `14B8F504026C8BBE5F18D8CB0E50346A588BD3C06F5A880B49D6E9129EF5121B`；首次 88 项日志原样保留 |
| 最终隔离 API26 构建 | **SUCCESS /13.482 s**，34 项任务全部执行；[日志](hap-build.log)。312 份复制输入 build 前后逐字节核对；[复制清单](ui-final/source-copy-manifest.json)、[准备/核对工具](ui-final/prepare.cjs) |
| 仓库构建输入 | **337 项 before/after/disk PASS**，包括最终修复、方法测试与验证工具；[输入清单](build-inputs.json)、[disk 日志](build-manifest-disk.log)。隔离产品源码与仓库源码身份一致；暂存身份另由 staged 日志记录 |
| Rust/native | **复用未变的 v22 双 ABI archives**。267 项 native 输入重新核对 Git 基线与当前文件，两个 immutable archive 与当前采用字节一致。未重跑 Rust 测试或重新编译 native，不把 v22 的 158 项结果记为本轮新跑 |
| 最终包内 native | ARM64/x64 的 `libmorrow.so` 与 `libc++_shared.so` 共 **4 项 PASS**，与本次隔离 SDK 的 stripped 输出逐字节相符；[包内核对](native-package-check.json) |
| 最终设备 | **NOT_RUN**，未安装下述最终 `935E` 包，未证明恢复/完整关闭/业务保存流程 |

最终 unsigned debug HAP **29,102,519 字节**，SHA256 **`935E1B2E784D216F552890C67785179C2EC22470E3993067AB1D3A7EC69106AF`**。本机 immutable archive 为 `hmos/.build/artifacts/dev23-todo-watch-final/entry-default-unsigned.hap`；[产物元数据](ui-final/artifact.json)。`reports/build-manifest.json` 的 artifact 指向该 archive，未把旧 root 构建输出冒充新包。HAP 不进入 Git。

## 实际设备观察：失败结果保留

原已安装 UI 候选 `96C0AB68…` 的公开 D 输入恢复后能读到 `first 汉字 🧪 é.`，但保留关闭因未完整捕获事件而停止。v23 前置观察、焦点/选区操作及一次保留尝试见 [preflight](device-preflight/progress-clipboard.json)，不据可见原文与计数清除 Unknown。

本轮安装首个独立候选 **`3116FBB89FB09A0B4747BCF207F87C18FE7AC58BFD24B23233BE7619DB602EBC` /29,102,190 字节**。它含首次 `61C7` 行回调修复与 v22 native，**不含最终 `E0A3` 完整快照门禁**。安装前仅记录本轮自有公开 D 草稿可见文字；完整 IME/选区保全资格为 false，不声称 lossless capture。安装/启动有 ACK，host archive SHA、准确安装路径及 saved/fresh bundle 元数据见 [安装记录](device-current/installation.json)；bundle 版本不能证明已安装字节摘要。

[实际恢复阶段](device-current/progress-clipboard.json) `v23-fixed-restore-D` 为 **FAILED_OR_UNKNOWN**，错误 `restoration must not raise missing capture`。可见准确 title/body/Todo 原文，但仍出现“当前事件尚未完整捕获”提示。**后续 keep 未执行，未重放失败动作**。原 trace 没有 field/kind/raw payload，不能确定来自 Todo selection、Todo text 还是普通字段初始化；不能声称 Watch 门禁解决此设备失败。原失败及 raw/截图保留。

最终 `935E` 包与两个旧候选范围分开。下一轮须记录真实 field/kind/原始选区或 preview，验证准确新包的恢复及完整关闭，再验证业务保存，不用 host PASS 替代设备资格。

## 候选业务接续设计未实现

新增 [native 业务交接合同](business-handoff-design.md) 与 [Index 接入设计](index-business-integration-design.md)。它们列出原完整请求持久日志、固定实际 outer wire、已确认附件快照、准确 source0 child、条件父退休、Unknown 恢复及关闭后重新编辑的需求。接口、预算和 lifecycle 尚待工程核定，生产 API/Proto/ETS 接线均未实现。

v22 严格业务基础仍未接 Index；业务原 wire 跨进程恢复、保存成功后的准确子草稿接续、真实 TaskId 全篇编辑及完整设备资格仍 OPEN。本轮是有界源码修复、验证与候选设计交付，不是全功能验收。
