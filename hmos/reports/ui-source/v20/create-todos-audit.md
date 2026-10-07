# dev.20 新卡多行待办原子持久化审计

2026-10-07。本 worker 实现隔离 HMOS development adapter 的新卡多行待办创建：只修改新 `rust/src/create_todos.rs` 与其测试、`lib.rs` 的模块/Request/创建与发布准入，以及 `Workbench.Command.todos`。未修改冻结 shared、Index、其他ETS models、formatter/FFI/header/DTS，未操作Git、设备、签名或HAP。**本报告为后端源码与实际Windows Store测试证据；独立native worker已完成combined源码检查，Index接入、最终产品HAP和设备资格仍待root验证，完整Flutter目标仍OPEN。** 本次分支交付为源码检查点，AppScope仍为dev19；参见 [checkpoint-status.md](checkpoint-status.md)。

## 实际Flutter参考及适用范围

本轮直接读取 `build/io-safety-refactor/lib` 的现有源码：

- `main.dart:5325–5381` 保存完整冻结 `EditorFields.todos`；业务投影对 `todos.text.split('\n')` 逐行 `trim`，忽略空项，`toSet().toList()`保留首次出现顺序；legacy完成状态按旧completed文本集合与新normalized行集合求交。
- `CardTipsEditor` 将同一newline草稿字段映射为各行，row IDs仅供view使用；selection偏移仍按UTF-16的此前行长度+LF累加。`TipListEditor` 的1000 Characters限制是全部行与换行总量，`TipPreferences.maxLines=100`；空/重复行在view仍占行数，保存才整理。
- `plugins/versioned_editor_adapter.dart::_prepare` 明确拒绝既有V2的非空 `draft.todos`、`draft.completed` 和 `editor.todos`；其TaskId任务由独立详情路径管理，不把legacy文本成员关系猜成V2任务。

本轮HMOS仅为**新卡**将raw newline字段整理后创建真实V2 Task，全部默认Incomplete，`legacy_completed=false/legacy_duplicates=0/origin=None`。没有旧卡完成集合可以继承，不捏造legacy provenance或迁移。已有V2 `edit` 只要请求todos非空（包括空白/LF）就拒绝 `V2EditorTodosUnsupported`；任务仍通过原TaskId commands修改。新卡投影采用参考legacy编辑器的文本整理规则，**不声称FlutterV1/V2 schema、迁移、保护会话或完整行UI已经等价**。

## 单次创建与JSON接口

`Workbench.Command` 新增 `todos:string=''`，Rust `Request` 新增默认空String。root的新卡创建应发送完整冻结raw todos，已有V2普通edit发送空字段；原raw journal仍按既有协议保留完整原文/选区/composition。

现有共享API已经支持 `tasks_v2::Properties.tasks:Vec<Task>`，不需要增加事务或修改共享schema。本轮在Engine原有create分支准备全部Task，编码到同一Properties，经 `tasks_v2::decode` 和 `CardRecord::new_with_attachments` 验证后，沿用原连接、CreateContent授权、**一次 `host.create_content(original operation, original CardRecord)`**。Store将卡片、全部任务、完整create command/receipt及outbox放在同一事务；不循环调用task_add，不以读回列表代替原提交结果。

新卡从已确认raw draft发布时，`published_assets` 在原四个文本/类别/阶段检查之外，严格核对历史准确代次的 `values.todos.text == Request.todos`。发布仍使用原draft operation/generation及pin/附件核验；不同raw即使整理后相同也不能借用该代次。历史已提交请求采用原published history，较新或已弃的raw journal不改写原create。

## 完整原文字数、行数与共享预算

`create_todos::prepare` 先复用固定Unicode16 `editor_field::inspect("todos",raw)`，原始**完整**字段≤1000 graphemes且既有text512KiB预算生效；然后检查原始LF行数≤100（空String为0行）。空白/重复行不能先被剔除后绕过原view容量；CR或其他Unicode段落符不自动猜成LF行。

整理严格采用当前实际Dart VM的String.trim集合，包括U+FEFF及U+0085；Rust默认Unicode trim不处理BOM，故未直接代用。不会normalize组合序列、折叠大小写或替换标签；`e+combining acute`与`é`仍可成为两个不同任务。原raw字串不被这个纯投影修改。

全部Task形成最终body后，原共享 `tasks_v2::decode` 继续验证TaskId、非空单项≤2048 UTF-8 bytes、任务数≤128、完整Properties≤64KiB与其他common-field预算。新增view行数100不放宽原128限制；1000 grapheme也不放宽单项/aggregate字节限制。例如一个含1024个combining mark的grapheme可通过字数，却被2048B单任务限制拒绝。包含metadata的最终body仍按64KiB核验，失败在create写入前拒绝，不留下任务子集。

## TaskId与原raw身份扩展注册

业务TaskId为 `task-hmos-create-<SHA256>`，hash domain固定 `morrow.hmos.create-task-id.v1\0`，绑定长度框定的card ID、原create operation、原raw身份digest及整理后首次顺序index。相同原请求重试生成完全相同ID；不同卡/operation/raw不能借用相同view-row ID。无随机fallback，不从当前已有任务推导原创建身份。

整理会将不同空白/重复输入投影成同一Task列表，blank-only甚至没有Task；若只存整理后的任务，核心proposal比较就无法区分改过的raw。本轮在**非空raw的新创建body**登记一个小型adapter-owned未知Properties字段：

| 注册项 | 值 |
| --- | --- |
| legal field tag | **50001**，length-delimited；常量 `create_todos::RAW_IDENTITY_FIELD` |
| payload | `morrow.hmos.create-todos.raw.v1\0` +32字节SHA256 |
| digest输入 | 同一domain、长度框定的literal `todos` 字段身份、长度框定的**完整原raw UTF-8 bytes** |
| 空/absent旧请求 | 不添加字段，旧task-free create proposal字节保持兼容 |
| 含空白/重复/LF的非空raw | 即使整理后0项或相同Task列表仍保留不同原身份 |

当前真实shared schema使用字段1–7、10–14、20–22，reserved8/9；50001未冲突，且避开protobuf全局reserved19000–19999。此字段只注册在HMOS-owned模块，不修改共享proto或赋予共享字段新语义。它是**开发adapter原请求身份元数据，不是Flutter业务schema等价、加密raw日志、签名或protected capture证据**。

原 `cards_v2`/`tasks_v2` scanner在确认完整有界message后保留未知字段；真实Engine测试另外加入无关未知50002，验证普通edit/favorite/task新增/完成/重命名/排序均准确保留两份unknown payload。不会通过重新编码typed Properties删去已有未知内容。扩展同样计入最终body64KiB和核心完整proposal/hash预算。

核心历史lookup的Committed仅保留原提交事实；仍重建并提交完整原proposal以触发原Store command比较。修改过的raw不会因历史Committed而跳过payload核验；拒绝改过重试时保留 `effect=committed` 这一原操作事实，不能反写not_committed或消费新的输入。第一次未提交的V2非空todos edit在权威absence检查后保持not_committed。

## Fresh检查

| 范围 | 结果 | 证据 |
| --- | --- | --- |
| 本模块实际Rust/Engine/Store focused suite | **9 PASS，0 FAIL，2条件默认ignored**；1.40s，ignored不计为通过 | [log](create-todos-rust-tests.log) |
| 实际安装Dart VM/Characters捕获 | **PASS /50完整样例**；Dart3.12.0 windows_x64，实际package config指向Characters1.4.1/Unicode16 | [capture log](create-todos-dart-capture.log)、[完整原字串/输出/身份](create-todos-reference.json)、[测试源](../../../rust/src/create_todos/reference.dart) |
| final Rust对真实Dart完整输出条件比较 | **1 PASS，0 FAIL**；50个完整原raw的UTF16/UTF8/count/SHA、完整normalized顺序、原行数及接受边界一致 | [log](create-todos-dart-compare-tests.log) |
| 实际Store子进程突然退出与原create核对 | **1 PASS，0 FAIL**；7个真实事务边界，单独启用host测试fault-injection | [log](create-todos-store-crash-tests.log) |
| 冻结后独立combined Rust suite | **123 PASS，0 FAIL，6默认ignored**；102.53s。另有5项真实Flutter/Dart条件对照全PASS，13.74s；第6项Store crash已按上一行单独PASS | [combined log](input-native-final-rust-tests.log)、[实际条件对照](input-native-final-all-flutter-compare.log)；由native worker运行，不累计重复focused检查 |
| 冻结后独立双ABI原生候选与来源 | ARM64/x64 release staticlib均PASS；完整261项repository native inputs及候选/保留库/reference inventory核验PASS；未复制生产库 | [native审计](editor-input-native-audit.md)、[input check](input-native-input-check.log) |
| Index/最终产品HAP/设备 | 未接入；本worker未运行；dev20业务设备 **NOT_RUN** | 不以host源码或staticlib成功代替最终产品包；已安装dev19的证据独立保留 |

9项focused覆盖：LF/Dart trim/首次dedup及组合序列不normalize、raw1000/+1与原100/+1行、TaskId完整原身份、单次真实创建及跨重启同receipt1、blank/重复变动即使normalized相同仍原operation拒绝、无关unknown与原raw扩展穿过各修改、旧空create真实原core proposal兼容、单项/aggregate预算拒绝不留card/operation/outbox、准确raw journal发布及后续推进/弃稿/重启后的原create历史核对。原create后更多任务/卡片修改重试只返回原receipt1，不覆盖current卡；原事务event仍可还原初次全部Task及ID。

实际Dart捕获独立执行真实 `splitLF → trim → 去空 → toSet → toList`，并用实际Characters计数；50项包括全部trim whitespace与明确non-whitespace、BOM/CRLF/NUL、emoji/family/flags/Indic、组合序列、重复/空项、100/101行、1000/1001边界。每项保留全文及UTF8 SHA，Rust逐项核对；不是只比较预设计数常量。实际pub cache的Characters README声明Unicode16，pubspec与当前package config路径已直接核对。

## 真实进程退出/Unknown结果范围

测试仅对自己的临时开发Store创建子进程，显式启用现有 `morrow-core/fault-injection`，在 `after-begin/after-card/after-operation/after-event/after-task-evidence/before-commit/after-commit` 退出86，无stack unwind/connection drop。父进程确认child已退出后fresh打开Store并只读核对原operation：前六边界card/operation/outbox全部没有，after-commit则三Task与原operation/outbox全部存在。没有部分任务或第二次业务operation。

测试随后明确以**同一个**原create请求核对/执行，结果仍为一张卡、三个准确TaskId、receipt revision1和一个event；Store integrity check通过。它验证真实进程丢失结果的持久原子性及同原请求核对，不提供设备进程/IME生命周期、硬件空间耗尽或生产保护资格。业务没有加入自动Unknown重放；测试用的fault feature及child环境变量不加入release/HAP构建。

## Frozen来源身份

| 文件 | SHA256 |
| --- | --- |
| `rust/src/create_todos.rs` | `ED0CF6965C7C5AB2A98E2A92B582A650DD4E756564D5D7563E87BB51C1BD3900` |
| `rust/src/create_todos/tests.rs` | `DD3E16D9BF8DEFDF976862179E60AA1C729C395298CF034AC17371F94FAB2BB6` |
| `rust/src/lib.rs`（本worker冻结时，含独立formatter模块） | `00D11B29CA60F309C8FB59451D3317E79720E83BEC1E70FF4EF8CDC3171FF3D9` |
| `entry/src/main/ets/model/Workbench.ets` | `1A5BCBEE38CE7267B2E178FCFEFFB445AAA029AB73C9074B7593B147569269BF` |
| `rust/src/create_todos/reference.dart` | `0689BF0B9A327760F4143C6D1DDA9473CC66A8527E829A8B5CDEFFBCD3D07062` |
| `create-todos-reference.json` | `2495C566CF47645407946861195574C97D0CB9181055110EADAF0D87583E478C` |
| reference `lib/main.dart` | `6203B7465F320DC8463DFACB5387EA8B437EC3EC1458CCA7E215E44B8F65CB1E` |
| reference `lib/card_tips_editor.dart` | `CAE29CFD83020FF9F8A9E17B128B5F9C83D3260D2E117CB41630B83BEFCFDE64` |
| reference `lib/tip_list_editor.dart` | `D4AF62531CBC28807C2CE0356E6B7EA26A12E81C82E979D9D470DEEB11DD9191` |
| reference `lib/tip_preferences.dart` | `C09D7D56758437B0554C5340B3278B1CAF3F7B102D5E6CBE290806C88638F9B6` |
| reference `lib/plugins/versioned_editor_adapter.dart` | `E1AD98C236EC877F4DB512567920F0DA72ABB8300673F5194F5DBFB90360E6C6` |
| shared `plugins/workbench/src/tasks_v2.rs` | `43849B828B2C6F4605EA8FE677CBB4E95A64FCFB68EEC4639346556E9947C70D` |
| shared `plugins/workbench/src/cards_v2.rs` | `ED92F85AF871A1AF28B6BD680217DDE9F39E1A8CB5701F5DE15A0EA04AC86CED` |
| shared `plugins/workbench/schemas/tasks_v2.proto` | `2534CA1314EA7901F418A4CD0AF80A1559F736BEB32F38888EC1516153DA2FB8` |
| shared `core/src/store.rs` | `280A5868F73750DD7DA1A0897CF9FCA55804B8339BD17C741F30BCE3B4896266` |
| actual Dart SDK `lib/core/string.dart` | `D6676A9CC1B761B1B322713EC72A3276AED7B84C20111D8C262BFE096B4EC2B7` |

本轮未用Git操作判断HEAD/clean；上表是实际读取文件的物理字节身份，不将历史blob或源码等价扩大为整个参考工作树资格。combined native最终source/input freeze已由 [独立native审计](editor-input-native-audit.md) 核验，旧formatter-only库不包含本业务实现。UI row/controller/selection/composition由root与ETS worker继续集成，不属于本后端完成范围；现有HUKS/Store保护、捕获票据/迁移、签名/ARM64真机和完整Flutter目标继续开放。
