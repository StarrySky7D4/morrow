# Index raw fork 与 view lease 集成检查

2026-10-07。此次交付收敛为 GitHub 分支检查点；本 worker 不操作 Git、设备、产品 HAP、Rust、NDK 或 native 采用。生产改动仅 `pages/Index.ets` 与新 `pages/EditorViewLease.ets`；新增一个实际 Index 方法 suite，并窄同步原三个 suite 的依赖，未缩减或改变它们的原断言。既有 EditorDraft/EditorDraftFork model 未改。

**冻结 Index 下 fresh 66/66 PASS，0 failed/cancelled/skipped/todo，3881.5057ms。** 新 suite 15 项，原 todos/field/clipboard 分别16/20/15项。证据为 [index-lease-fork-final-tests.log](index-lease-fork-final-tests.log)。这是实际源码方法与受控 framework/native replies 的集成检查，不是 SDK declarative 编译、真实 Store、IME、设备运行或发行验收。

## 已接入的路径

Index 使用既有 `EditorDraftForkCoordinator`；`beginRawFork`、`finishRawFork`、`retireRawForkParent`、`retryRawFork` 为实际 UI 接线，接续状态与显式核对按钮可见。原业务 create/edit reply 属于当前 submitted draft/view owner，且较晚完整 raw 仍在编辑器时进入此路径。普通成功业务变更需要接续时也不再调用 `attachDraft(currentCard)` 从字符串重新构造 raw。业务原请求 Unknown 仍由原 `pending` wire 显式核对，不自动改 operation 或借较晚输入重放。

接续顺序：

1. 确认父普通 journal 已完成已 issued 写入，具有准确 active/current generation、save operation、native request SHA。Unknown 不在 begin 中自动核对。
2. 同 live lease owner 冻结 child scope、完整 first TextValues/asset selection、first operation 与父 proof。原 source_kind/revision/source bytes 全部继承；构造模型暂停父自动/显式写，但接受的字段、选区和多行待办 capture 仍更新父完整 current。
3. 首 child ACK 必须通过模型对 full raw、父 proof、scope、pin metadata/aliases/order、first/current generation 的强验证。Unknown 原 serialized wire 保留，仅显式 retry。历史首 ACK 的 current 已推进或 inactive 保持模型冲突，不复活。
4. 同步取得 restored child，把父较晚完整 current 交给 child，并在同一个同步步骤切 Index writer。随后接受的 SDK capture 写 child。不得等父清理回执才切换 writer。
5. flush child，核完整 confirmed/current/editorValues、capture completeness、owner 与 input epoch 后，才发送冻结条件 `draft_fork_retire`。后继 child pin 来源为 origin3；首 child 的 origin4 来自准确父 pin 子集。
6. 父清理 ACK 核完整原 source、原 generation/save operation/request SHA、原值及准确 retirement marker/link；成功只清理父，live child 保持编辑。清理期间更晚的 child 输入仍需自己的正常 journal 确认，状态文字不宣称它们全部已持久。

first fork、child save、父 conditional retirement 的 Unknown 分别按各自固定原 wire 显式核对。核对父清理前也先确认最新完整 child；错误 proof/metadata 不作为成功。明确 first not_committed 后的显式重试可以释放暂停父并创建新操作；曾 Unknown 的请求不能这样降级。

未加入草稿的 Ready/Unknown import、pending spool 或 attachmentPending 阻止 fork 准入，仍归原 import owner；不在父退休时静默遗漏。所选未发布父 pin 由既有模型/native 协议承接；asset ID、metadata、aliases、顺序均不从当前卡片列表猜测。

## 手动放弃与成功关闭

新 `EditorViewLease` 在 aboutToAppear 冻结 mountedOwner，在 aboutToDisappear 回传该固定 owner。Index 先撤销应用 admission，再解决 detach waiter。每次 remount 都换 fresh owner 并用 ForEach key 创建新实例；不能只修改旧组件 @Prop 后复用 mountedOwner。

普通文本、selection、focus、多行待办 capture/status/uncaptured、category/stage Select 的 raw/业务回调有 owner 守卫；旧 lease 的输入不能借当前 owner 写新编辑器。重命名另绑定 task ID 与 fresh incarnation，防同 view 中取消后重开或另一任务的旧回调。selection restoration timer 同样冻结 lease owner。

admitted title/body/hypothesis/conclusion/todo focus 与重命名 focus 递增 `editorFocusIntent`；row focused 也递增。Index 向 EditorTodos 传 `focusIntent`，组件的异步 Add/focus 取消由 root 独立实现与检查。本报告不借其设备或82项模型结果替代本 Index 的证明。

**手动“放弃草稿”保持真正删除语义。** 用户明确整份放弃后，先完成普通已确认 raw，暂停父写；移除可交互 view 并等真实 lease lifecycle 回传撤销。撤销前已接受的合法完整输入仍捕获，撤销后旧 owner 无 admission。边界 capture 不完整则停止删除，保留原事件/原 scope/TextValues，用 fresh lease 恢复编辑；不以 last-complete 字符串冒充原完整输入。完整边界通过后才 flush 并发送固定 `draft_discard`。Unknown 保持 view detached，只有显式原请求核对。此路径不创建或保留 child 冒充“已放弃全部草稿”；已接续的 live child 再次放弃也执行同一边界。

exact create/edit 成功关闭由 `closeSavedEditor` 实施：先核 submitted input epoch 与完整 raw，撤销 live view 后再核同 epoch/raw/completeness。撤销前出现更晚 capture（包括值改回原值但 epoch 已变化），不得消费或关闭；fresh lease remount 同 TextValues，等实际新组件 mounted ACK 后再接续 fork。保持草稿关闭也等应用 lease 边界后再次确认已接受 raw，成功仅称“已确认的完整输入已保留”。

**aboutToDisappear 是销毁前生命周期通知；它不是节点已销毁或 SDK callback 队列排空的证明。** 此检查只证明应用对已交付/已接受 capture 的 owner 撤销与 journal 顺序。撤销后 SDK 尚未交付的文字、旧 row 聚合事件及完整 IME 队列 handoff/recovery 仍 OPEN；不能声称所有未交付输入已保留，不能将此限定闭合当成完整 Flutter 自动关闭语义验收。

## 测试范围和修复证据

新15项 actual Index/lease 检查覆盖：

- actual constructor/model/lifecycle 与 frozen source identity；first ACK 期间完整 composition/selection，先切 child writer、latest flush 后清理父。
- first/child/parent retirement 三处 Unknown 原 wire；较晚 raw 另行确认；wrong scope/fullraw/pin aliases/retirement proof；相同 raw 的不同 owner。
- 未选中导入/Unknown/spool 准入拒绝；first ACK 期间 incomplete 阻止清理；手动放弃等实际 lease revoke；第二次 child 放弃不复活。
- incomplete detach 保留事件与 full TextValues/fresh lease；exact saved close 和 same-value/new-epoch 回绕；discard Unknown 保持 detached；rename incarnation、focus intent 与旧 selection timer。

测试 fresh 抽取 Index 实际方法，并执行实际 EditorDraft/EditorDraftFork 和 EditorViewLease lifecycle 方法。ArkUI 的 mount/unmount 投递、clock、native reply/persistence 均受控。digest 是 fixture 值，不能资格化真 canonical protobuf SHA 或授权 Store。lease 的 SDK 装饰器只为此 Node 方法执行去除，真实 ArkTS 检查需 root 对相同冻结字节另行完成。

首轮14/15，发现真实源码 race：remount 后立即 begin fork 时，新组件尚未 aboutToAppear，owner admission 尚未建立，导致接续未开始。已改为等待实际 fresh lease mounted waiter；当前66项通过。保留 [index-lease-fork-initial-failure.log](index-lease-fork-initial-failure.log)，不覆盖或当作当前失败。

原三个 harness 保留原 method anchors/业务断言，仅加载实际新 fork/helpers、补 lease 状态与受控 framework lifecycle 投递。它们原有无 request SHA 的 normal-only synthetic records仍不满足 fork准入；真实新 fork proof/切 writer 资格来自本次独立15项。原直接调用 editorInputChanged 的“late SDK callback”测试只检查内部退休 guard；新15项另检查生产 leased callback 拒绝旧 owner，不能把前者当成新 SDK 生命周期证明。

在仓库根目录 PowerShell 可复跑：

```powershell
& 'C:\Program Files\Huawei\DevEco Studio\tools\node\node.exe' --test hmos/tool/index-draft-fork-integration.test.cjs hmos/tool/index-editor-todos-integration.test.cjs hmos/tool/index-editor-field-integration.test.cjs hmos/tool/index-clipboard-integration.test.cjs
```

## 冻结字节与开放范围

| 文件 | SHA256 |
| --- | --- |
| `entry/src/main/ets/pages/Index.ets` | `B0A53C29BEB0F7A54A2D6DFB3DB0F6C74CABF316996BF46198F906CE97AC37E9` |
| `entry/src/main/ets/pages/EditorViewLease.ets` | `4F3172C6C8DADEC3019A36F0FA4EF82CFA1D1B8A9232F16F5A0CAE4FAD870730` |
| `tool/index-draft-fork-integration.test.cjs` | `A9E53FCDE26E546A602451FAB655236090DC598137A667E6C50374E52F871500` |
| `tool/index-editor-todos-integration.test.cjs` | `DB9C44A42BA9B10BEB981EA89597F50A8E428D3180B3AE2028970537CAD7A797` |
| `tool/index-editor-field-integration.test.cjs` | `4833D28153CE5558D1EC348E48EA1104E83ABE6E1BDE348C17B85B7DC339FE8B` |
| `tool/index-clipboard-integration.test.cjs` | `D4D9AFFBF1E6AFEF1392B57554273AEF4A1B802AF0003611059185BD6206E4C5` |

既有 EditorDraft `A3AE1E99…04F965`、EditorDraftFork `2F5CC858…62263C` 保持不变。本接线没有把 source_kind1/revision0 的 create 后草稿偷偷变为已有 V2 业务编辑；source 冲突保持。原 newline raw 不通过 task_add 重放、不清空较晚 todos、不重写已存 TaskIds。

raw fork 为独立 HMOS development adapter lineage，不是 protected S1/S2 资格。首 child ACK 前的 Unknown 原 wire/更晚 capture 仍只有内存保留，没有因此新增跨重启 pending intent；此实际 UI suite 不证明原生持久事务或 crash/reopen。精确业务结果 source rebase、`editor_save`/`editor_commit_inspect`、continuation/V2整篇 projection 仍未实现或接线；本次未采用设计中的新 wire。

本 worker 的 SDK 编译、HAP、安装、设备、签名、发行、完整 Flutter/Windows 目标均未验收；由 root 单独保存后续相同冻结源的构建/设备证据。本报告不声明 commit/push 成功，不合入 main。
