# dev.19 编辑字段计数与完整粘贴审计

Fresh 来源核对时间：`2026-10-07T07:14:41.467Z`。本报告范围为字段规则、实际 ETS 模型、异步粘贴 preflight 和转换回执验证；完整 Flutter 功能等价仍 **OPEN**。本 worker 未运行 HAP 构建、安装或设备验收，未修改 Index、Rust、原生桥接或 Git。整包、最终原生版本及分支交付由独立 validation 记录。

## 实际 Flutter 来源

三份参考工作树下述范围均 clean，Git blobs 相同：

| 参考目录 | Fresh HEAD |
| --- | --- |
| `build/io-safety-refactor` | `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` |
| `build/win-cloud-20261005` | `e83cdf1d3506b001d07ffceaaa9f98b688bcc150` |
| `build/windows-sdk-reconstruction` | `20669f671152972470340a65eac3458dd2f61b4d` |

| 路径 | 共同 Git blob |
| --- | --- |
| `lib/main.dart` | `2e00f41251c3215f0b6d886712c86adfdf2bb6e6` |
| `lib/tip_list_editor.dart` | `1a674f64827cec137aef1361529fb834a1582aff` |
| `lib/card_tips_editor.dart` | `16620678b6cfd0d9a6c6615fbb62e78c7be7f5a3` |
| `pubspec.lock` | `c1af24a5a695ef2f4fbb3023ec1a5113f71d2539` |

`lib/card_tips_editor.dart` 亦逐工作树核对为 clean、共同源码；其 `CardTipsEditor → TipListEditor(maxTextLength:1000)` 保持多行待办在同一草稿字段。`pubspec.lock:52–59` 固定 `characters 1.4.1`，原生 agent 独立核对 Unicode 16.0.0 对应性。

`main.dart:5596–5630` 的真实 `insertPaste` 先以 UTF-16 selection 的 start/end 替换选区，再对**完整未来字串** `value.characters.length` 检查：

| 字段 | 用户可见字符／grapheme 上限 |
| --- | ---: |
| title | 60 |
| todos | 1000 |
| hypothesis | 5000 |
| conclusion | 10000 |
| description | 20000 |

原件、Markdown 图片引用和编辑选择使用不同预算／坐标，不应把 `String.length` 或单独 inserted 片段长度当上述完整字段计数。`main.dart:5649` 的图片 Markdown 追加也检查追加后完整 description 的 Characters 长度。todo 行控件会把行的 UTF-16 selection 映射回完整 newline 字段；汇总限制见 `tip_list_editor.dart:148–154,222–232`。

当前 HMOS `Index.taskText/editorValues.todos` 是待添加单条待办的输入，已有任务另存于 `current().tasks`。本轮把完整传入的 todos 字串上限由 500 调为 1000，不会把已有行与待添加行合并计数；Flutter 的多行编辑、行间选区映射和全部待办的总量规则仍未复现。纯字段 worker 的 1000 规则通过，不等于这些 UI/任务集合语义已等价。

## 控件超限行为的明确差异

实际本地 Flutter SDK `C:/flutter` HEAD=`559ffa3f75e7402d65a8def9c28389a9b2e6fe42`，`text_formatter.dart` Git blob=`caf82b4888ba6a7fff7987b35d8cd941e75b2faa`；所读 formatter/TextField 路径 clean。`TextField` 把 `maxLength` 交给 `LengthLimitingTextInputFormatter`，本参考未覆盖 `maxLengthEnforcement`。

SDK `text_formatter.dart:518–535,541–600` 的默认行为：Windows/Android 使用 `enforced`，已达上限且无选择时保留旧值，否则按 Characters 截短；iOS/macOS/Linux/Fuchsia 和 Web 使用 `truncateAfterCompositionEnds`，有效 composing 可暂时超限，完成后截短。该行为与 Flutter 自定义 paste 的完整超限拒绝是两条路径。

HMOS 本轮要求移除编辑字段控件的 UTF-16 提前截断，由完整未来字串的异步 grapheme 检查决定是否粘贴／业务保存；超限内容不截成“成功”。因此**直接输入的自动截短行为尚不等于 Flutter 控件**。原始草稿／IME 恢复与业务保存分别处理，不把 composing 草稿当已验证业务内容。本报告不宣称控件输入、系统 IME 或设备 UI 全部等价。

## ETS 与原生接口

新增 [EditorFieldPolicy.ets](../../../entry/src/main/ets/model/EditorFieldPolicy.ets) 接受注入 worker `send(request:string):Promise<string>`，root 绑定 `native.editorField`。请求严格为 `{field,text}`，text 是完整未来原字串。原生回执为 `{ok,error,field,grapheme_count,utf16_length,utf8_length,limit,unicode_version}`，本轮要求 `unicode_version:'16.0.0'`。

- `checkField(field,text,stillOwned)` 校验字段、固定限额、完整 UTF-16/UTF-8 长度和计数回执；超限 `EditorFieldGraphemeLimit` 返回完整 count、`ok:false`，可用于计数 UI。
- `requireField` 和 `requireTextValue` 校验后拒绝超限／错误，后者拒绝活跃 composing。`checkValues(values,guard)` 在 await 前复制全部五字段并逐项检查，可用于原始状态 inspection，不因 composing 单独拒绝。
- `requireValues(values,guard,allowComposing=false)` 用于完整业务值检查；raw 流程可明确传 `true`，或只 inspection 并保留原 journal 的结构／字节边界。允许 composing 不等于放松字数、编码或业务条件。
- guard 必须捕获完整编辑身份、card/draft、编辑 epoch、原 TextValue／选择快照及生命周期。每次 await 后检查 guard 和 policy stop epoch；关闭／更新后的晚结果不能赋予新输入保存资格。计数调用本身是只读，不自动重试业务或 raw journal。
- 孤立 surrogate 在编码前明确拒绝，保留原字串供 IME／用户处理；有效 emoji 配对、有意 U+FFFD、空白和换行不 normalize、trim 或 replace。

选择 base/extent、affinity、directional 和 composing 一律仍为 UTF-16。原生计数不改变选区，不用 grapheme offset 替代现有编辑协议。

## 完整未来粘贴与已确认附件

[EditorPaste.ets](../../../entry/src/main/ets/model/EditorPaste.ets) 的 `insertPaste` 仅生成完整候选 TextValue，不授予保存权限；`insertPasteChecked(value,inserted,field,policy,guard)` 异步检查完整 prefix + inserted + suffix，随后再检查原 TextValue 和 owner。

[ClipboardPaste.ets](../../../entry/src/main/ets/model/ClipboardPaste.ets) 的 root 集成接口为 `run(plan)`，hooks 增加必需 `checkText(futureValue,boundary):Promise<void>`。实际 Index 对目标字段的完整未来 TextValue 调用 `requireTextValue`，沿用自己的编辑 epoch／owner 快照；一次提交多个字段时可用 `requireValues` 检查完整冻结值。

协调器在任何 confirm/import 之前，把图片 placeholder 替为固定长度、有效格式的未来 asset ID，拼接完整未来内容并 await preflight；晚返回后重读 editor/card/draft、generation、blocked、原选区和 selected/import 槽位。每个 import 继续使用独立 operation、准确 selected receipt 和既有 journal；Unknown 不重放。所有 pin 确认后用实际 asset IDs 重建正文，再 await 检查并重读边界才 commitText。最后一次检查失败／目标变化时保留已确认 pin，正文未加入；stop 不启动后续请求。实际 Index 的 owner 另捕获 `editorInputEpoch`，即使文字／选区绕回同值，旧粘贴也不继续。

## 富转换回执与独立预算

[AttachmentFiles.ets](../../../entry/src/main/ets/model/AttachmentFiles.ets) 不再用 `paste_text.length>20000` 的 UTF-16 误限。可信 native converter 新增 `paste_grapheme_count`、`paste_utf16_length`、`paste_utf8_length` 和 `unicode_version`；ETS 验证 Unicode16、完整非负整数 count≤20000、UTF-16/严格 UTF-8 原字串长度与空值一致性。失败回执保持空正文／图片／SHA和三个 count=0。缺失元数据、错误版本、错误长度或孤立 surrogate 均拒绝，不走旧回执 fallback。

完整 converter UTF-8 输出和 serialized JSON 仍分别受 512KiB 预算，字段 worker 请求与 text 也有独立 512KiB 上限；一个含极多 combining marks 的 grapheme 不获得无限字节资格。Markdown renderer 的既有 Strings256KiB、后台 V2 字段／body64KiB，以及原件 source plain/HTML/XML2Mi UTF-16、RTF8Mi 分档和编码差异保持独立；任一单项计数通过不证明业务持久化通过。原件 64MiB／20 spool 槽位及 sidecar、图片／文件配额、source SHA、FD 关闭、conversion binding 与 Unknown spool 保留规则均不变。

## 实际主机 ETS 证据

六组当前实际生产 ETS 及一组逐字提取的 Index 方法，经 DevEco TypeScript 转译后执行，共 **277/277 PASS**；测试未复制生产计数／协调逻辑。policy 测试的 native worker 为明确的 synthetic contract，分段算法本身由 native agent 的 Rust／真实 Flutter 对照报告另行证明，不能将 host Intl fixture 冒充运行设备或 Unicode16 原生验收。

| 实际模型 | Fresh 结果 | 日志 |
| --- | ---: | --- |
| EditorFieldPolicy | 16/16 | [editor-field-policy-model-tests.log](editor-field-policy-model-tests.log) |
| EditorPaste | 12/12 | [editor-paste-model-tests.log](editor-paste-model-tests.log) |
| ClipboardPaste | 14/14 | [clipboard-paste-model-tests.log](clipboard-paste-model-tests.log) |
| ClipboardFiles | 70/70 | [clipboard-files-model-tests.log](clipboard-files-model-tests.log) |
| AttachmentFiles | 97/97 | [attachment-files-model-tests.log](attachment-files-model-tests.log) |
| ClipboardInput（未改生产源码） | 55/55 | [clipboard-input-model-tests.log](clipboard-input-model-tests.log) |
| 实际 Index 剪贴板集成方法 | 13/13 | [index-clipboard-integration-tests.log](index-clipboard-integration-tests.log) |

覆盖各字段正好上限／+1、emoji／combining／ZWJ family／flag／CRLF、完整选区替换及跨插入边界的 grapheme、保留完整 UTF-16 caret、失效 owner／generation／selection／IME／stop、原生异常与预算失败、不可信回执、pin 后检查失败保留资产、不重放 Unknown，以及旧原件／全局配额回归。Index 新增实际输入文字绕回同值／选区往返在 count 延迟期间使旧 paste 失效，以及完整未来 title 超限前不发生 import／不保留未发 spool。**dev.19 实际设备字段输入／IME／粘贴 NOT_RUN**；本报告不增加新设备、签名发行或完整目标资格。
