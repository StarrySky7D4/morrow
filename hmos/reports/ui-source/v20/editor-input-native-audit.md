# dev.20 Windows Flutter 输入 formatter 原生审计

记录时间：2026-10-07。本 worker 范围为纯 Rust `editor_input`、C++ NAPI、native DTS、真实 Flutter 全值对照、主机测试和独立候选库。没有编辑 Index／其他 ETS／版本／冻结 shared，没有运行设备、Git 或 HAP。当前交付是**源代码检查点**，本轮 formatter／多行待办 UI 尚未接入；不宣称dev20完整发布。完整应用等价仍 **OPEN**，dev20 HMOS 控件／IME 设备验收 **NOT_RUN**。

## 实际 primary sources

实际 Flutter SDK `C:/flutter` HEAD `559ffa3f75e7402d65a8def9c28389a9b2e6fe42`；fresh 核对下述 tracked formatter／controller 路径 clean，installed cache 的 sky_engine source另以完整hash记录：

- `packages/flutter/lib/src/services/text_formatter.dart:518–600`：Windows 默认 `MaxLengthEnforcement.enforced`，完整 `truncate`／`formatEditUpdate`。
- `packages/flutter/lib/src/services/text_editing.dart`：TextSelection 的 start/end 排序、copyWith、完整 affinity／directional。
- `packages/flutter/lib/src/services/text_input.dart`：TextEditingValue、selection／composing 结构。
- `packages/flutter/lib/src/widgets/editable_text.dart:4604–4626`：formatter 只在 textChanged 或 composition commit 运行；链中每个 formatter 的 oldValue 都是初始原值。
- `packages/flutter/lib/src/material/text_field.dart:1553–1560`：自定义 inputFormatters 后才加 maxLength formatter。
- `bin/cache/pkg/sky_engine/lib/ui/text.dart`：TextAffinity.index 为0 upstream、1 downstream；TextRange collapsed 仅 start==end。

Morrow 实际参考 `build/io-safety-refactor` HEAD `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420`，lib／pubspec scope clean；installed `characters 1.4.1` 是 Unicode16.0.0。复用 dev19 已严格固定且真实 corpus 对照通过的 `unicode-segmentation =1.12.0`，本轮没有改变 Cargo graph 或冻结 shared。

## 完整协议与只读边界

新增 `native.editorInput(input:string):Promise<string>`，C 导出 `morrow_hmos_editor_input`。严格 JSON 请求：

```json
{
  "field":"title",
  "mode":"field",
  "old_value":{"text":"","selection_base":0,"selection_extent":0,"affinity":1,"directional":false,"composing_start":-1,"composing_end":-1},
  "new_value":{"text":"示例","selection_base":2,"selection_extent":2,"affinity":1,"directional":false,"composing_start":-1,"composing_end":-1}
}
```

`mode` 省略时为 `field`，使用五字段固定 limit60／1000／5000／10000／20000，禁止 limit override。`mode:"todo_row"` 只允许 `field:"todos"` 且必须给 `limit` 整数0..1000，该值来自完整 row 集合的剩余额度。原件、raw draft、字段计数和业务保存继续使用独立规则。

成功回执包含 `ok,error,field,mode,limit,action,format_applied,old_value,new_value,value`，完整 echo 原 old/new；三套计数为 `old_grapheme_count/old_utf16_length/old_utf8_length`、`new_*` 和结果 `grapheme_count/utf16_length/utf8_length`。固定 `unicode_version:"16.0.0"`，`request_sha256` 是完整 incoming JSON UTF-8 bytes 的 SHA256。root 可对完整旧／新回执和 field／mode／limit 做 exact comparison，再检查自身 editor／row／epoch／IME owner；SHA 不是授权 ticket。

action 为 accepted／retained／truncated；row accepted 可包含真实 CR/LF filter，不能假设 value 总等于 new_value。错误 action=error，三个 TextValue 全null，所有 count/length 为−1，明确 error。原生不获取 Store／FD／capture lineage，不修改选择或原 caller string，不保存业务，也不代 root 判断异步结果仍属于哪一个编辑者。

request JSON、每个 text 和完整 serialized reply 各保持512KiB UTF-8上限。回执包括旧／新／结果原文，重复原文可能使 reply 比 request 更早超预算；此时完整失败，没有部分值、少字段或隐含成功。old/new 严格 scalar Unicode、selection/composing UTF-16 offsets −1或0..length、affinity0/1；既有 raw journal 允许的 mixed−1 sentinel 和合法 transient offset（含paired surrogate内部）继续保留。孤立 surrogate／非法形状明确拒绝，不用U+FFFD替换。NAPI在转UTF-8前验证原requestUTF-16，拒绝时 worker 未启动。无效UTF-16只能由caller暂存原字串；当前protobuf string不保证其无损durable恢复，不能把严格拒绝说成已完成此编码范围的持久化等价。

## Windows 完整 formatter 行为

先匹配真实 EditableText gate：仅 `old.text != new.text` 或 old composing 非collapsed而 new composing collapsed 才调用链。纯选区变化、未commit的composition-only更新原样返回，即使恢复的raw正文已超limit；`format_applied=false`。

正 limit 的 enforced 分支：

1. new完整 grapheme count≤limit，完整接受 new（保留其 backwards selection／composition）。
2. new超limit，且old恰好等于limit、old selection collapsed，完整保留old；即使old或new正在composition也遵循此分支。
3. 其他超limit情况取new完整Unicode16 grapheme前缀。selection用 `min(start,truncatedUTF16Length)`／`min(end,truncatedUTF16Length)`，真实Flutter会把反向选区变为排序后的base/extent，并保留affinity/directional。
4. new composing 非collapsed且prefixUTF16Length>composing.start，保留start并把end clamp至prefix；否则设(-1,-1)。没有先进行general Unicode normalization／trim。

`ok:true` 表示只读 formatting proposal 有效，**不是 grapheme／业务保存许可**。selection-only更新可完整保留超限raw；todo_row limit0的no-growth链也可能保留原已超限内容。业务保存仍需要独立完整五字段 `EditorFieldPolicy`、composing规则和冻结 backend bytes预算。

## 实际待办行规则

`CardTipsEditor → TipListEditor`（参考 `card_tips_editor.dart:106`、`tip_list_editor.dart:148–154,222–232,392–409`）使用未提交字串集合，总1000 graphemes包含行间换行；不是每行各1000，也不是详情 VersionedTaskPanel 的 business TaskId 集合。

row余额是 `1000 − sum(other rows Characters.length) − (rows.length−1)`，clamp到0..1000。Add按钮还检查行数<100、完整join('\n').characters.length<1000；删除／排序和view row ID由独立模型处理，formatter不发task_add或生成business ID。

真实row链依序：limit0自定义 no-growth guard → `FilteringTextInputFormatter.deny(RegExp(r'[\r\n]'),replacementString:' ')` → 仅正limit时的Windows enforced。零额度没有构造非法maxLength0；仍允许删除和选区变化。

filter对每个CR／LF分别替换一空格，CRLF因此变两个空格。它会在无匹配时也 finalize：invalid selection重置(-1,-1,affinity1,directionalfalse)，collapsed／invalid composing重置(-1,-1)。合法位置不移动，因为每个匹配与replacement都是一UTF-16 unit。native `todo_row` 完整实现此链，ETS不能再复制截短或仅replace字符串而假称全值相同。特别地，零额度guard在filter前比较原字串 Characters，不能换成filter后count比较。

## 真实 Flutter 对照与控制器差异

`input-flutter-reference_test.dart` 在真实Morrow Flutter工作树执行实际 installed SDK formatter，最终捕获297完整身份：五字段 × 七种Unicode样例 × 六种编辑分支，加上selection-only／composition-commit／mixed sentinel／CRLF／zero／positive row边界和3个zero-retained finalize回归。用lossless codepoint/repeat generators、完整old/new/text SHA和实际结果prefix及其SHA记录，避免提交多兆重复文字。

Rust逐项重建完整old/new，验证完整result文字、selection、affinity、directional、composing、action、gate和全部三套长度／count：285个在wire预算内逐字段一致；另外12个大family ZWJ值在Flutter可格式化，但明确触及native独立预算（3 request、9 reply），按错误验证，**没有计入完整值等价成功**。原raw仍属于caller，不因失败而删除／截短。3个zero-retained回归为old collapsed composing、old invalid selection、old CRLF：guard返回old后，filter仍会finalize，最终retained value未必等于完整old；仅正limit的retained分支严格保持old。

另一个真实 Flutter widget test用Windows平台和TextField(maxLength60)验证3个完整controller值：composition范围内selection-only保留raw、composition commit执行截短、selection跨出composition时controller清空composing。后者发生在 formatter之后的 `_handleSelectionChanged → TextEditingController.selection`（`editable_text.dart:4394–4405,343–352`），纯原生 formatter proposal 本身不会复制该controller副作用。结果单独记录 `input-flutter-reference.json.widget.json`；不把formatter级一致说成完整ArkUI controller/IME/runtime已等价。

初次fixture缺foundation import的编译失败保留 `input-flutter-initial-compile.log`；初次widget把跨出composition的controller结果误当formatter结果，失败保留 `input-widget-initial.log`。改为明确记录controller后处理边界后，两真实Flutter测试全部通过，没有忽略或删除该差异。

## formatter-only 候选 checkpoint（业务源变更前）

- `input-native-focused-tests.log`：8 PASS、1 conditional ignored。
- `input-native-rust-tests.log`：114 PASS、0 FAIL、4 conditional ignored；88.31s。这份binary编译于新增create_todos业务源之前。
- `input-native-final-flutter-compare.log`：业务变更前4 conditional comparisons全PASS；7.07s。对应最初294身份（282匹配／12预算差异）、dev19的1198计数身份、旧5完整转换和6完整容量身份。之后按root源审查增加3个zero-retained案例，最终297身份由 `input-native-final-focused-tests.log` 的9/0/0 PASS验证；最终combined对照另行记录，不能把最初日志说成297。
- 两个 `input-bridge-*-syntax.log`：installed SDK API26 C++17 ARM64／x64 PASS。
- 两个 `input-native-*-build.log`：locked/offline formatter-only release staticlib，ARM64 5.82s／x64 12.63s PASS。

| 保留 checkpoint archive | bytes | SHA256 |
| --- | ---: | --- |
| `hmos/.build/editor-input-native/dev20/arm64-v8a/libmorrow_hmos.a` | 55394254 | `D3B145C26E6B03B637980606D5C0D609D559EDFA078777758FFCE12EDD37BE3E` |
| `hmos/.build/editor-input-native/dev20/x86_64/libmorrow_hmos.a` | 53801506 | `512DC58F952EAC0252D1ED9FF30A66EF888EE9223C11610CB3C7AA1C73D39307` |

这些checkpoint不包含随后新增的create_todos业务实现，不用于证明新卡待办创建。root新增授权由独立agent实现该业务模块并冻结后，再在不同 `dev20-final` 目录构建完整候选、fresh完整suite和source inventory。不得覆盖上述checkpoint或dev19证据；本worker不复制cpp/rust生产库。

## 完整 dev20 最终原生集成

独立 create_todos agent 已明确冻结业务源；本 worker fresh 核对源 SHA 与其回执一致：`create_todos.rs` 为 `ED0CF6965C7C5AB2A98E2A92B582A650DD4E756564D5D7563E87BB51C1BD3900`，`create_todos/tests.rs` 为 `DD3E16D9BF8DEFDF976862179E60AA1C729C395298CF034AC17371F94FAB2BB6`，最终 `lib.rs` 为 `00D11B29CA60F309C8FB59451D3317E79720E83BEC1E70FF4EF8CDC3171FF3D9`。没有与本worker的module／FFI区块冲突。

业务模块在同一原create操作中把新卡legacy未提交todos投影为V2任务，原raw字串／view row ID不作为business TaskId；保留独立后台任务／properties预算，历史重试验证完整raw identity，既有V2 edit不能用非空legacy字串覆盖tasks。**这是HMOS适配的V2创建路径，不能声称它就是Flutter V1／protected capture保存**；详见独立 `create-todos-audit.md`。本worker只验证完整源码构建，不改该业务实现。

| 最终combined检查 | 结果／证据 |
| --- | --- |
| default Windows Rust suite | 123 PASS、0 FAIL、6 ignored，102.53s；`input-native-final-rust-tests.log` |
| 五个实际Flutter／Dart条件对照 | 5 PASS、0 FAIL，13.74s；`input-native-final-all-flutter-compare.log`，涵盖297完整编辑身份（285值一致／12明确预算差异）、1198字段、50完整normalization身份、5完整转换输出、6容量身份 |
| 第六个ignored实际Store crash | 独立业务agent以显式 `morrow-core/fault-injection` 运行，7 subprocess边界PASS；`create-todos-store-crash-tests.log`。此feature仅host测试，**不是release native features** |
| OHOS locked/offline release staticlib | final ARM64 16.33s、x64 8.92s PASS；两个 `input-native-final-*-build.log` |
| compiled ELF导出 | 双ABI `morrow_hmos_editor_input`存在；`input-native-final-archive-symbols.log`，SDK LLVM15以`--no-llvm-bc`跳过无法读取的Rust LLVM22 bitcode，仅核机器对象symbol |
| 最终hash与全部input inventory | 261 repository native inputs、2 final库、2保留dev19、2 formatter-only checkpoint、实际FlutterSDK／包／fixtures全部PASS；`input-native-input-check.log` |
| UI／整包／设备 | UI尚未接入；本worker未跑HAP或设备，不宣称dev20完整发布；root独立SDK构建／分支检查点不能代替运行资格 |

| 最终独立 archive（含create_todos） | bytes | SHA256 |
| --- | ---: | --- |
| `hmos/.build/editor-input-native/dev20-final/arm64-v8a/libmorrow_hmos.a` | 55415340 | `8295AFFE01A0C8C2F24B6080DC29CAA304F03687245BE240ACB0722E0E664AB5` |
| `hmos/.build/editor-input-native/dev20-final/x86_64/libmorrow_hmos.a` | 53821348 | `24635BF2B6A4A57CCC512227D87A25EE76B3E9D7BF49D7FC0EF96E866EB15A33` |

最终source／库已冻结；`input-native-inputs.json` SHA256为 `E04FE055A2059933FE02B9D9C7AA3E07060B58FA7127D5C92AB0245DF5D11879`。其fresh清单包含新Rust模块／测试、Cargo.lock、全部既有native source／schema、bridge／DTS／CMake和build/checkscripts；对照生成Dart和capture另列reference inventory。`input-native-input-check.ps1 -IncludeFlutterReferences -IncludeExternalDependencies`全通过；root采用库后可加 `-ProductionArchives` 核对cpp/rust库，没有ignored库的新checkout可 `-SourceOnly`，不会把该模式说成库已验证。

本worker没有复制生产库；所有dev19库和本轮formatter-only checkpoint均保持原hash，未被final build覆盖。root接入UI、系统IME安全采用、保存／恢复与设备验收仍需继续，完整目标保持OPEN。
