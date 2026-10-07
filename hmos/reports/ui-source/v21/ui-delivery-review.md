# dev21 UI 交付前有限独立复核

结论：在下列准确源码快照与范围内，没有发现新增的实际草稿丢失、旧 view owner 绕过或未捕获焦点异常的推送 blocker。这不是完整代码审查、系统输入队列排空或设备验收。生产源码、测试、Git 和设备均未由本审查代理修改或操作。

## 观察身份

2026-10-07T10:23:23.142Z fresh 文件读取；Index 接线代理随后通知的冻结 hash 与读取一致。报告只记录此快照，不把并行后续修改预先冻结。

| 文件 | bytes | SHA256 |
| --- | ---: | --- |
| `entry/src/main/ets/pages/Index.ets` | 285726 | `b0a53c29beb0f7a54a2d6dfb3db0f6c74cabf316996bf46198f906ce97ac37e9` |
| `entry/src/main/ets/pages/EditorViewLease.ets` | 749 | `4f3172c6c8dadec3019a36f0fa4ef82cfa1d1b8a9232f16f5a0cae4fad870730` |
| `entry/src/main/ets/pages/EditorTodos.ets` | 22309 | `6abaa19a360487aeab67f34e5f53ef966b1d3566a05b42377a9af2901e2aad18` |
| `tool/editor-todos-component-methods.test.cjs` | 26513 | `fcf7b8a213c257bc078f4774305c48a7e95ea7a4635a1b1b0a76093733cad63d` |

## 已复核的关闭边界

- `EditorViewLease` 保存 immutable mounted owner；Index 用 owner 作为 `ForEach` key，新挂载取得新实例。旧 mounted/revoked 通知核对当前 view owner，SDK text/selection/focus 入口先做 `ownsEditorView`；task rename 另外核 task ID 与 rename incarnation。category/stage Select 也已补同 lease 和 transition 门禁，先前中途观察的遗漏已修。
- lifecycle revoke 同步撤销接纳资格并停止旧 worker，然后才 resolve detach waiter。page disappear 会使 waiter结束但 `pageAlive=false` 拒绝 cleanup；没有把此取消结果当成功关闭。
- 手动放弃在视图还接纳回调时保留完整采集；先 pause writer、等待实际 lease revoke，之后 resume/flush 最新完整 raw，再发固定 ordinary discard。capture incomplete 阻止删除；Unknown 保留原请求待显式核对。它不会通过另建并保留 child 来冒充整份放弃成功。
- 保存后关闭的 owner、完整 raw 和 input epoch 检查仍保留；边界期间已接纳的迟到更新会 remount 原 raw，等待真正新 lease `onMounted` 后才开始 fork。remount 使用完整 TextValue 与原 scope，不通过 `attachDraft` 从字符串或业务列表重新构造。

## 已复核的 fork 与焦点接口

fork 保持原 source kind/revision/source bytes；父写入暂停但视图继续采集。child 首 ACK 后在同一同步 turn 复制父当前完整 raw 并切换 writer，child flush 明确确认后才发原 wire 的条件父退休。条件退休结果核对父/子 proof，Unknown 使用原请求，不自动采纳业务当前 revision 或把 legacy raw 行替换为 V2 TaskId。

前次 [focus-review](focus-review.md) 的两条失败探针对应源码修法已经存在：

- `focusIntent` watch 与本地 intent epoch 撤销 timer；Add 在 await 前冻结两种 intent，返回后核对，row focus 取消原 intent。Index 外部字段及正确 task incarnation 的 focus 增加 `editorFocusIntent`，旧 owner focus 不影响新 view。
- `areaFor` 对旧 owner/row/incarnation 直接 return，不删除当前有效 measurement；只有当前合法票据的无效 geometry 能删除其当前记录。
- wait-area、reveal 后下一 timer、有界 12×32ms 重试、requestFocus 异常捕获，以及 disable/disappear/owner switch 撤销均保留。失败或撤销焦点不删除已经创建的行。

本次读取的 [todos-focus-final-tests.log](todos-focus-final-tests.log) 是 82/82 PASS，组件规范化 hash 与上表一致，新增检查包含 row/外部 focus 优先、Add await 期间转焦与旧 area 票据不删新记录。[index-lease-fork-final-tests.log](index-lease-fork-final-tests.log) 是 66/66 PASS。本审查读取这些主代理/接线代理的 fresh 日志，没有重复执行整个矩阵；host 方法和 stub 结果不等于设备挂载/IME证明。

## 保留的资格范围

这条 lease 是应用 callback admission 边界。它证明撤销后的旧回调不能写替换视图，不证明尚未交付的 SDK 输入已被完整观察。`aboutToDisappear` 在节点销毁之前，不能写成 SDK queue drained；此限制继续保留于 [editor-lifecycle-audit](editor-lifecycle-audit.md)。没有把旧包 150003 崩溃证据改写成新包通过，也没有将 source/model review 扩大为新 HAP、安装、实际 fork/弃稿或设备焦点验收。

后续产品 build identity、完整模型矩阵和分支推送/readback 由主代理独立记录。本报告不宣称已经推送或完整 Flutter 对齐。
