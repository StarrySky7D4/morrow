# dev.9 平台与文字草稿接口核对（2026-10-05）

状态：实现计划阶段的源码/SDK/设备只读核对。此记录没有运行 dev.9 HAP，没有测试真实输入法预上屏，也不作为 dev.9 草稿恢复或 IME 运行验收。

## 实际环境

- SDK 根目录：`C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony`。
- `ets/oh-uni-package.json`：`apiVersion=26`、`platformVersion=26.0.0`、`version=26.0.0.105`、`releaseType=Release`。这是本机编译 SDK 版本，不是设备完整软件版本。
- 工具：`C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe`。
- 本次 `hdc list targets` 可见 `127.0.0.1:5555` 和 `127.0.0.1:5557`。
- 对 `5557` 执行只读 `bm dump -n dev.morrow.hmos`，应用为 `0.1.0-hmos-dev.8` / `1000008`，`reqPermissions=[]`。这只证明原 dev.8 仍安装，不证明 dev.9 已安装或运行。
- 本次未启动、停止、重置、安装、卸载设备或修改其数据库。原实例损坏和第二实例建立的历史过程参见 [dev.8 环境记录](../v8/emulator-environment.md)；本记录不把历史安装/运行状态代替当前查询。

## 原草稿模型与 schema

当前 `hmos/rust/editor-draft-model/schemas/editor_draft.proto` 保留原 `morrow.workbench.editor_draft.v1` schema：`TextValue` 含原始 `text`、`selection_base`、`selection_extent`、`affinity`、`directional`、`composing_start` 和 `composing_end`。`Values` 保留标题、正文、假设、结论、待办原始文字以及类别/阶段。

只读 SHA256 比对结果：

| 文件 | HMOS 与两份实际参照的相同 SHA256 |
|---|---|
| `editor_draft.proto` | `8F80DB114BCC480D9553AE60916170B1982C9CDE0D0452753A35B491279FDDDC` |
| 原 `editor_draft/model.rs`（HMOS 为 `src/lib.rs`） | `D11632512506F357A73112568A6E8856AA74E87500B91709C87083A50DAC6C8E` |

比对参照为 `C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/io-safety-refactor/workbench_host/` 和 `C:/Users/Administrator/Desktop/CodeXProjext/morrow/build/win-cloud-20261005/workbench_host/` 内的 `schemas/editor_draft.proto`、`src/editor_draft/model.rs`。核对时两工作树 HEAD 分别为 `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420`、`772466177fe589cee53bc633e69f411c34610104`；文件相同结论来自实际文件哈希，不从 HEAD 推定。

原模型使用 UTF-16 code unit 偏移，允许空/中间文字、反向选区、单端 `-1` 哨兵和 surrogate pair 内的暂态位置；禁止真实越界和两个有效 composing 端点反向。不得在保存草稿时 trim 原始文字、按 Unicode scalar 重新计算偏移或擅自规范化原模型字段。HMOS 的独立 wrapper 只提供原模型编译所需的 `crate::Result`，模型复用不证明原正式宿主存储、附件 pins、S1/S2 捕获和恢复链全部接通。

## 本地 SDK 的输入控件声明

以下文件均位于上述 SDK 的 `ets/component/`，实际读取了声明和相邻说明：

- `text_common.d.ts:481`：`EditableTextOnChangeCallback = (value: string, previewText?: PreviewText, options?: TextChangeOptions) => void`。
- `text_common.d.ts:625`：`PreviewText` 只有 `offset: number` 和 `value: string`。
- `text_common.d.ts:1543`：`TextChangeOptions` 含 `rangeBefore`、`rangeAfter`、`oldContent`、`oldPreviewText`。其中修改范围不能作为当前选区的替代。
- `text_input.d.ts:1107`、`text_area.d.ts:540`：两控件的 `onChange` 接受上述回调。
- `text_input.d.ts:1121`、`text_area.d.ts:552`：`onTextSelectionChange` 回调给出选择起点/终点或光标位置。
- `text_input.d.ts:697,713` 和 `text_area.d.ts:63,81`：控制器有 `caretPosition`、`setTextSelection`。`TextArea.setTextSelection` 仅在起点小于终点时有效，组件获得焦点后显示高亮；折叠选区应使用 `caretPosition`。
- `common.d.ts:25050,25065`：`TextContentControllerBase` 有 `getSelection()`、`clearPreviewText()`。控件未绑定或已释放时不能依赖这些方法的效果，尽管返回声明没有完整表达运行期 `undefined` 情况。
- `common.d.ts:16514`：`MenuPolicy.HIDE=1`；恢复范围可用它避免自动弹出选择菜单。

普通 `TextInputController` / `TextAreaController` 没有设置 IME 预上屏会话的公开方法。`addText`/`deleteText` 的 SDK 说明明确警告它们只改变应用内显示、不改变输入法内部逻辑，不适用于预上屏。`clearPreviewText` 也不能作为重建会话的接口。NativeNode 声明虽有 `NODE_TEXT_INPUT_ENABLE_PREVIEW_TEXT` / `NODE_TEXT_AREA_ENABLE_PREVIEW_TEXT` 和包含 preview 的 change 事件，仍不能据此推定可设置或恢复原输入法组合会话。

## raw、selection、composing 的实现约束

1. 同时监听 `onChange` 与 `onTextSelectionChange`，每个字段绑定独立控制器。将 raw、selection、composing 存入原 `TextValue` 模型，序号/修订继续使用十进制字符串跨 JSON 传递。
2. 官方 ArkUI 引擎当前 master 的 `AddTextFireOnChange()` 使用 `GetBodyTextValue()` 构造 `value`；预上屏时该正文排除 preview 串，`previewText` 另给起点/内容。`SetPreviewTextOperation()` 首次预上屏会从原正文中剔除被替换选区。因此在此实现语义下，合法 preview 的 raw 为 `value.slice(0, offset) + preview.value + value.slice(offset)`，composing 为 `[offset, offset + preview.value.length]`，全部按 UTF-16 units。必须验证 `offset` 合法；不可遇到异常偏移时静默丢弃 preview 并声称已保存完整 raw。
3. 活跃预上屏期间 UI 的受控正文继续使用回调的正文值，单独构建草稿 raw，避免把 preview 重复拼回控件而扰动 IME。`rangeAfter` 不应直接覆盖选区；使用真实 selection 回调/已绑定控制器的选区。
4. 重启恢复时可以完整显示保存的 raw；待控件绑定、文字布局和用户聚焦后应用光标/范围，程序恢复期间应抑制反向写回草稿。条件隐藏的正文/实验字段尚未挂载时不能恢复控制器选区。
5. SDK selection 回调不提供 Flutter 的 base/extent 方向和 affinity；模型可保留原字段，但 ArkUI UI 捕获/恢复不能声称具备这些全部语义。
6. 将 composing 元数据存入 protobuf 不等于重建 IME 会话。重启后候选串作为普通文字继续编辑；原输入法候选、组合状态和会话尚未恢复。真实预上屏、中文/emoji、替换选区、取消/完成候选、进程终止重启必须另做设备验证。
7. 草稿确认只证明那一代冻结快照已持久化；最新尚在去抖/在途的输入不能称为已落盘。后台回调或退出清理不是进程被直接终止时最后一个字符必然落盘的证明。

## 已存在 UI 模型的只读接口观察

首次核对时 `hmos/entry/src/main/ets/model/EditorDraft.ets` 已存在，定义原字段、深拷贝和独立冻结请求，草稿代数以字符串传输，未知结果只允许显式重试原请求。随后只读复核观察到 `Index.ets` 正在接线，新增 `draftTextChanged`、`draftSelectionChanged` 和独立控制器；raw 合成逻辑按正文加 preview 构建。此时实现仍在修改中，不能用协调器存在或纯模型测试替代 UI 与输入法验收。

已提交父任务的复核风险：`restoreSelection` 当时的延迟回调只检查 `editorOpen`，应绑定实际 draft/session 身份，并避免每次重新聚焦都应用旧范围；同步赋值期间的 `draftRestoreInput` 标志不能单独证明后续 UI 帧的初始化/change/selection 回调被正确抑制。这些是当时接口接线观察，最终稿是否解决及真实设备效果需后续验证。另需核对候选串恢复后的 composing 状态和界面文案是否如实表达。

## 官方来源与证据强度

- [OpenHarmony 文字共用接口文档](https://github.com/openharmony/docs/blob/master/en/application-dev/reference/apis-arkui/arkui-ts/ts-text-common.md)：核对 PreviewText、change 参数和 TextChangeOptions。
- [OpenHarmony TextInput 文档](https://github.com/openharmony/docs/blob/master/zh-cn/application-dev/reference/apis-arkui/arkui-ts/ts-basic-components-textinput.md) 与 [TextArea 文档](https://github.com/openharmony/docs/blob/master/en/application-dev/reference/apis-arkui/arkui-ts/ts-basic-components-textarea.md)：核对控件/控制器公开能力。
- [官方 ArkUI text_field_pattern.cpp](https://github.com/openharmony/arkui_ace_engine/blob/master/frameworks/core/components_ng/pattern/text_field/text_field_pattern.cpp)：本次读取 `AddTextFireOnChange`、`SetPreviewTextOperation` 和 `FinishTextPreviewOperation`。
- [官方 ArkUI text_field_pattern.h](https://github.com/openharmony/arkui_ace_engine/blob/master/frameworks/core/components_ng/pattern/text_field/text_field_pattern.h)：本次读取 `GetBodyTextValue`、`GetPreviewTextValue`。

以上在线 master 源码用于说明当前官方实现和设计风险，未与本机 SDK 二进制 build pin 建立相同版本证明。公开接口存在、模型哈希一致和 dev.8 仍安装，均不证明 dev.9 的真实输入法和持久草稿行为已经通过。
