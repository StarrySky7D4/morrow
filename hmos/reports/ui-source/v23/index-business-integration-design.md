# v23 严格业务保存接入 Index 与原请求持久恢复设计

2026-10-07，只读生产审查。基线分支 `codex/ArkTsUI`，由本地 ref 文件读回 `8c3059615d9adc1296363cd365267b24929b3cc7`；未执行 Git、构建、设备操作或生产修改。本轮只有此新设计报告。下文拟议的 request journal / business handoff 接口尚未落盘或冻结，不把 v22 的有界通过当作这些新流程已经通过。

## 准入与状态一致性结论

严格保存必须同时接入 **原请求、准确 publication、准确历史业务结果、业务 source0 子草稿与恢复发现机制**。把 `create/edit` 的 action 换成 `editor_save`，再沿用旧 `retry()` 的成功分支，不能完成接线：新回复的 `cards` 为空；旧成功路径用当前全集加 `receipt_revision`，清 `pending` 后关闭/退休或建立原 source rawfork，无法建立自己的准确业务基线。

现有 `EditorBusinessCoordinator` 可复用完整原 Submission 字符串、两层 JSON UTF8 预算、canonical publication 摘要、历史 Card source 摘要、明确资格、同 wire Unknown 和 own root/latest baseline。它没有持久 request journal、没有 source0 handoff、也没有跨进程发现入口。50003 只存 mode / wire SHA / publication SHA；从摘要不能恢复原 JSON，不能用重新序列化相同语义的字段冒充原实际请求。

成功业务 S1 与当前 active raw parent 是两份独立证明：前者可能是父草稿旧 generation 的 immutable publication；后者可已经包含较新的 S2 或来自真实 rawfork 后代。恢复和 handoff 必须保留这一区分。自己准确 committed 历史不会因 current 卡片推进/删除、页面换 owner 或后续读失败而变为未提交；这些变化只阻止消费该结果或继续保存。

目前全目标仍 OPEN。既有 lease cutoff 只限定本应用已接受的完整 callback；SDK 尚未交付的队列文字和原平台全部 TextEditingValue 元数据仍未取得保全资格。

## 本轮源码与可复用范围

以下是 fresh 读取的完整 SHA256，后续实现若修改这些文件需重新冻结身份。

| 原件 | SHA256 |
| --- | --- |
| `entry/src/main/ets/pages/Index.ets` | `34DDECA5A375328ADB1A7EC53DE7F8E5D1A90804CD8634FAA85672D7A588160C` |
| `entry/src/main/ets/model/Workbench.ets` | `1A5BCBEE38CE7267B2E178FCFEFFB445AAA029AB73C9074B7593B147569269BF` |
| `entry/src/main/ets/model/EditorBusiness.ets` | `6469D1F2AF4E2DA6B9066B52917E658BC13ECECB4EF2A4E6B92FBB4AD0D996CF` |
| `entry/src/main/ets/model/EditorDraft.ets` | `A3AE1E99D198FCBA3CDEF741A6F138612AD16979BE42C7A8809003F00D04F965` |
| `entry/src/main/ets/model/EditorDraftFork.ets` | `2F5CC8586D2400F99EE7598DD816970E63D9E3D4B53C5DFFBC54CB4E4062263C` |
| `entry/src/main/ets/model/AttachmentFiles.ets` | `74AD85E0E6504EB97BC81D9C1983D6F3369E6C72FE0C14D81ED0F92B4B8849BB` |
| `rust/src/editor_business.rs` | `BE299723AF695C603CC8F1C41AFCC621FE0CF7EE94327903CD1EF5B1B86940C1` |
| `rust/src/editor_draft.rs` | `B1A547C40AFF9297A9ADD46518A4569B5C99E03D8FD25D44670118562DACC6C4` |
| `rust/src/draft_bridge.rs` | `F73A5ED46FF45A52B40D784C8D0F3CE3A1DFF6294DF122CCD0A34B878F623CCF` |
| `entry/src/main/cpp/bridge.cpp` | `21F623A9E79B2850820E993FD60217AD5DB5D60D01D1AA102516D5323EFB4CEF` |

| 实际通道/状态 | 可复用 | 接线时必须改变或补齐 |
| --- | --- | --- |
| `Index:185–203` 的 `submittedRaw/inputEpoch/editor/draft` 与原 retirement/fork strings | 同进程 owner、away/back epoch、完整 raw 比较和固定原操作原则 | 全是内存。不能作为重启恢复库，也不能把持久 operation 所属事实绑定为可替换 view owner |
| `Index:1855–1900 submit()` | 字段异步检查、完整 raw、确认 publication 后再发送 | 普通 cmd 不能作为 strict envelope；必须包含 request SHA tuple、stage/category/全文 todos，一次冻结；用户 save 对未建 journal 的 existing draft 要 `ensureConfirmed()` |
| `Index:1902–1965 retry()` | 普通 TaskId / favorite 等原请求通道继续使用 | strict 回执使用 `EditorBusiness` 独立分支。不能 `cards=result.cards` 清空列表；不能无资格沿旧 close/rawfork；Unknown 后 `not_committed` 不能无条件清掉原 wire |
| `Workbench:26–41` 单 tail + `native.request` | 已序列化 request 原样发送，进程内业务/草稿/附件的派发顺序；没有隐式 mutation retry | 仍是内存队列。首次发送 hook 要直接入此队列，不能内部先异步持久化而提前放开 parent writes；新 `Reply.editor_commit` DTO需显式类型接线 |
| `EditorBusiness:240–340` prepare / restore / mayConsume / save / inspect / continueTodos | prepare强制 current publication；restore允许当时 active 的历史 publication，current可已退休；同原字节与 own qualified baseline | restored 默认 Unknown、要显式核对；供当前 owner消费的 hooks 要重新绑定，不能恢复旧页 epoch为当前权威。continued context必须来自该模型准确历史对象 |
| `EditorDraft:412–478` pause / full capture / flush / ensureConfirmed | pause不取消已发请求，仍 `update` 完整 raw；Unknown显式 retry；准确最新 ACK | 现校验仅支持普通/rawfork link，不支持 business link/source promotion；不能强塞新 child DTO进旧构造器 |
| `Index:1140–1244` rawfork | 先子确认、切 writer、flush latest、条件退休父、Unknown原 wire | 只继承原 source，不解决 kind1 create 后的 source冲突；business-success路径要独立 source0 handoff，不能把 rawfork视为业务接续 |
| `Index:1683–1800` keep/closeSaved/discard + lease revoke/remount | detach/revoke 后才 cleanup；exact epoch/fullraw/incomplete守卫；manualdiscard独立同意 | 接入业务 handoff 后识别其 pending/Unknown、准确 child链和条件 cleanup。不得将未闭合 handoff改为假成功关闭或自动删除 |
| `AttachmentFiles:308–324,655–672` | filesDir受界目录、完整 UTF8 bytes写循环、fsync、发送前保存 exact import request；错误记录保留 | schema、namespace、20 spool/64MiB计费及清理合同是 import专用，不能直接存 business请求或用 spool cleanup删除业务历史 |
| `rust/editor_business:1039–1080` immutable operation history / strict inspector | 先确认原commit，再做publication/command/payload/hash核对；准确历史 result 与 live诊断分开 | 当前不保存全文请求，也无 journal list/read；历史 proof成功不能让client自行生成source0 authority |

本轮在 HMOS production、shared及两个真实 Flutter reference 的 `lib/` 搜索没有发现名为 `businessRequests` / `BusinessRequests` 的现有容器。若该名称指后续拟新增状态，则应明确为新接口。已有事实只有上述 `pending`/coordinator内存和 Core immutable command/receipt历史；后者不保存实际完整 Submission JSON。

Flutter参照使用 `build/io-safety-refactor/lib/plugins/workbench_native.dart:2875–3157` 的真实 `_NativeEditorSession`，不是仓库根旧 `lib/` 同名实现。它冻结 operation、fingerprint、raw字段、附件和 `_pendingBytes`；`_matchesConfirmed:3092` 要求 own对象、准确revision+1、非historical/deleted、完整snapshot；新scope要再次核 live。`main.dart:5224,5325` 的迟到generation分流和 `versioned_editor_adapter.dart:144` 对非空legacy todos的拒绝继续是行为参照。Flutter workspace/session库不是正式编辑器中已经自动完成 durable handoff的证明，见 [原只读审计](../v20/retirement/continuation-design.md)。

## create / edit / continued 的实际 UI 上下文

| 准入模式 | source与context来源 | Index要显示/提交的待办形态 |
| --- | --- | --- |
| 新卡 `create` | 当前准确 `scope.kind=1/revision=0/source=''`，无context；新 business operation 与 draft save operation不同 | 保留当前完整 multiline raw，不trim或拼当前 tasks；100行/1000 grapheme，全部五字段无composition |
| 普通既有 V2 `edit` | 准确 `scope.kind=0` 的完整冻结 source 与 revision；无owned root context | TaskId详情路径保持；strict publication todos必须真实为空。当前普通 `save()`把cmd.todos置空但保留非空待添加raw的行为不适用于strict保存，必须显式提示完成/取消该单条，不能悄悄清raw或构造另一份空publication |
| HMOS-owned `continued_todos` | 自己严格create/continued的完整原 request，最新 qualified immutable历史 baseline六字段，native source0 business link或同等级明确准入 | 继续同一完整raw multiline row编辑器；相同label按已实现legacy成员规则保留准确TaskId/completion/unknown bytes，非空todos不是普通existing V2批量编辑权限 |

模式不能取自 `selected.length`、当前card列表、TaskId前缀或复制50003 marker。现在 `newCardTodos():504` 仅看 `source_kind===1`；native child改为source0后若不改这组判断，builder会立刻切回single `task_add`并丢失全文UI映射。最短需要独立 `ownedTodosContext` / `usesRowTodos()`，同时覆盖 builder、bind/unbind direct slot、capture/isCurrent/status、paste/adopt、row focus/reveal、`inputReadyFor`、业务dirty/消耗比较与 Save mode。不得从当前 tasks重建raw，也不得因source0把全文todos清空。

ordinary TaskId/favorite/category/stage动作继续用原通道；它们可推进 business source/CAS，不能自动成为owned continuation baseline。若源码后续要求这些操作与持续编辑共存，需要准确各自历史结果/授权合同，不能从最新卡片偷偷rebase。

还必须闭合“exact成功关闭或用户明确discard child后，再打开已保存owned卡片”。旧exact-close会退休S1 parent，此后不能把inactive parent复活为handoff authority；当前native普通strict edit也拒绝source中的create/continued marker。需要独立保留own root/latest原wire与准确历史资格：再次用户编辑可按新scope建立fresh source0 publication，重新inspect准确own历史再continued，或先统一建立source0 child并keep-close。此项由native准入合同明确选择，不能因为删过草稿就删除业务root身份，也不能看current marker猜owned资格。

## 最短完整调用次序

### 1. 冻结 S1 与首请求

1. 同一个当前 live lease/session/draft下确认：前台、非retiring/rawfork/handoff/Unknown、无未完成paste/import、无incomplete、五字段确认、模式准入有效。existing unchanged草稿在用户Save动作调用 `ensureConfirmed()`；普通 `flush()`可能返回undefined，不够建立publication。
2. 等待已经派发的草稿写完；若Unknown只呈现显式原请求核对，不隐式retry。暂停parent debounce与显式writes；这个暂停不阻止完整SDK capture。冻结原owner+epoch+完整TextValues/assets/category/stage、准确confirmed publication、source、mode与唯一business operation。
3. 构造 `EditorBusinessCoordinator.prepare(create/edit)` 或由自己已qualified baseline调用 `continueTodos()`。异步字段/hash等待期间每次核 owner和inputepoch；期间有较新输入则本次尚未发送的proposal取消，原raw保留，恢复parent writes。不能把新输入改进已冻request。
4. 在首次发送前完成独立durable原请求journal（后文）。整份Submission字符串及outer `originalSave`均计512KiB完整UTF8，root字符串的转义也计入。持久化失败不发业务、不清输入。若采用前端journal，journal写完后再次核exact。
5. `model.save()` 的send hook同步进入已有Workbench tail，不在hook内部再做未排序的异步FS准备。队列入列与仍exact的冻结同一同步段完成。原业务入列后才resume parent raw writes，后续raw保存尝试排在该业务之后；这样避免hash/持久化等待期间publication被autosave提前推进。已有Workbench队列不能保证另一套绕过它的native写者顺序，此集成需继续唯一admission owner。

parent仍完整capture S2，且已确认prior pin元数据完全匹配时，origin0后继先 `verify_pin`，可以继续保存，并不因业务source推进一概失败。只有未找到匹配prior pin的source0资产才走 `rust/editor_draft.rs:583–589` 的 `live.encode()==source_card`门禁；未确认资产不能因此借用已知digest作为授权。最新完整S2仍可以dirty，与confirmed values不同。新business handoff应允许直接冻结这种latest S2到firstchild，不额外强迫它先经旧source成功flush；首选资产全部来自准确confirmed parent pins。若S2确实已由parent确认，照样使用其较新active proof。尚未ACK raw可能仅内存，不能声称每个输入即时掉电持久；firstchild full raw durable是新的确认边界。

### 2. 业务结果与 Unknown

由 `EditorBusiness` 验证完整 `editor_commit` 后才记录事实。`cards/receipt_revision`旧分支不用来拼baseline。`historical_card.source`准确hex/contentSHA、原card/op/sourceRevision/resultRevision/event/完整pub与wire/publicationSHA、字段与有序附件必须满足既有validator。必须是 `development_editor_wire_v1` 才给own continuation/consume；`legacy_semantic_only`最多显示旧语义历史，不能提升同等资格。

首次明确 prewrite `not_committed`可以退回rejected并让用户修正产生新proposal；一旦原save已有Unknown，再来的reject/timeout/畸形/超回复预算都不能洗掉它。保留唯一原request和originalSave。核对按钮显式 `inspect(original nextRevision)`；失去inspect回复后仅 `retryInspect`同原inspect wire；原业务retry只 `retrySave`。snapshot absent不等于原工作永远不会到达，不授权新operation、清wire或cleanup。

记录committed事实不依赖旧view仍存在。owner已变化时不得选中新card、改新editor dirty、清新todo或创建新child；只把历史结果记入相同durable request记录。列表刷新另行 `list/query`，失败不把准确commit变成Unknown或再次业务mutation。

### 3. own-success后建立 source0 child

有较新raw/epoch、或exactclose revoke之前发生改变时，使用独立business handoff，不调用 `attachDraft(currentCard)`：

1. 验准确 own strict commit，原publication tuple与原 request保持不变。settle原parent已发raw写，Unknown只显式核对；准确confirmed record必须current-active、generation/saveop/SHA、scope与pin metadata有效。另行保留最新complete raw S2，允许它dirty。所有首选资产仍须在该confirmed parent pins内，不能把未确认新asset借成origin4。未选ready/pending/Unknown import及未绑定spool必须解决或保留阻断，不能因物理blob尚在就退休它们。
2. pause准确active parent。首child完整request采用latest complete S2一次冻结，字段可与parent confirmed不同，唯一child draft/first operation、expected_generation0。source0/revision/source必须由native重新核原业务历史结果生成或核对，不能client以currentcard生成权威。原业务publication与active parent证明分别传入；parent ancestry必须真实连到原publication，不能仅同card当作同会话。
3. 首选asset origin4是准确parent pins有序子集，保alias、metadata及完整raw；root publication中未发布而由active parent确认的新pin也不能省略。后继写转origin3。首child确认后同步把live writer切为child，复制当前parent最新完整raw到child；保五TextValues的selection/composition，既有row view identities不是TaskIds。
4. 确认child latest raw durable且属于当前owner/epoch；后来的完整callback只写此child。再以固定原retire wire条件退休parent；该退休需要准确child first receipt/link、current-active/CAS、active parent proof和全部原请求归属。child newer可以保留，但历史first ACK不能覆盖其latest或复活inactive child。
5. child first或parent retire Unknown各自保唯一完整原wire，显式核对；不得另造child/firstop/retireop。parent knowncommitted但currentchanged是事实加冲突，不能catch成“没有创建”再换op。只有首request已明确拒绝且无历史commit时可释放parent暂停。

rawfork只保存原source的独立development能力仍可用于无business-success的raw接续。它与新业务link互斥/识别的校验必须由native/ETS一起支持，不能拿13/14当15/16的替身。外部业务已推进/删除时，保准确历史source和raw，显示真实CAS冲突；不自动吸收current tasks/assets，不能称S2再次保存已经可用。

### 4. exact-success关闭与手动关闭

成功时epoch+fullraw准确相等且无incomplete，可复用 lease revoke→前后exact复核→确认→条件cleanup。revoke前或过程中改变则remount完整相同values与fresh lease，进入上述source0 child；不能改字符串重建或伪造IME结束。原请求journal直到准确cleanup ACK且后续context已有独立持久归属才标terminal。

manual discard仍需单独明确整份删除同意、先revoke真实view owner后cleanup；pending业务/handoff/Unknown时不能把“放弃草稿”解释成取消已发业务。manual keep-close只保存已接受的完整raw，不授予删除迟到SDK未交付事件的资格。knowncommit、child durable、父退休、view closed是四个事实，不能合并为一个 `dirty=false`。

## 原完整 wire 的跨进程保存办法

### 已选方向：native自有独立request intent，具体wire待冻结

Root在本轮协作中已选择独立native durable intent journal，business link只引用准确intent proof，避免重复整份maxwire；native代理正在拟定 before-save prepare、pins持有及first/retire/close literal wire计划。这个方向尚不等于API已冻结或实现。原 `editor_save:{request_json}`本身原字节保持，业务Core mutation前先确保持久write-once intent及所选S1资产原件归属。它是另一份明确授权的持久事务，不声称与业务commit/父子handoff是all-or-none；顺序不变量为 **业务若可提交，则原request已经durable可发现**。业务事务仍只有既有一次create或一次VersionedContentChange，不做多次task_add，也不绕过HostRuntime/原grants直接修改Store。

private intent自身的操作身份必须用独立domain稳定生成，不能占用原business operation或其它草稿operation；全局/异对象operation冲突需要真实Store测试。intent写入与business effect分开：intent committed不表示business committed，不能提前改变 `Reply.effect`/ `editor_commit`；业务effect仍来自准确business历史/事务。新的private journal必须被各business发现/query路径正确识别并验证，而不是混进用户卡片或忽略未经验证的任意host前缀。

最短必要能力（名称为设计占位，待native冻结）：

| 能力 | 输入/返回与不变量 |
| --- | --- |
| 首次before-save intent prepare | card+business operation、schema、完整原 `request_json`、精确SHA/pub tuple与恢复用immutable DraftRecord/来源引用；独立持有所选S1 pins，完整metadata/order/aliases一致；同operation已有不同bytes必拒；存入确认失败/Unknown则业务不发，显式核原prepare |
| bounded list/read | 从当前库发现未完成请求及仍需root/baseline的上下文；返回完整原bytes或exact不可变引用，包含byte_length/SHA/模式/pub身份；分页有完整性/预算，未知namespace不混入businesscards |
| exact history读取或可信restore cache | 新的只读publication-history入口按card/draft/gen/saveop/SHA返回完整immutable record；替代方案是intent带完整record缓存，但缓存不构成authority，inspect/handoff仍在native读真实history |
| 条件complete/release | 以准确sameop/hash和terminal close/handoff/retirement证明标完成；Unknown不能release；own root或latest baseline仍被active child引用时不得删除/释放原wire |

intent记录至少包含 `schema/card/operation/request_json/request_sha256/publication/expected_revision`；恢复context需要原完整publication record或可准确读取其历史的native key；pending inspect/first-child/retire outer wire分别保存原字符串和独立phase身份。state仅恢复线索，不能 client写 `committed:true` 充当业务权限。能够从原wire确定的metadata再核一致，不以新metadata改写原wire。旧SDK/NAPI body/reply限额仍保持。

新intent还必须授予自己准确S1的有限、独立publication/pin写准入。现 `editor_save`在原业务尚absent时走current `publish_assets`：如果prepare已durable、业务实际未执行，而parent已有S2推进g2，则原S1 pubg1再次执行会被current-generation gate拒绝。新流程需核intent在prepare时的exact-current publication、immutable原request和独立active pins，准许显式同wire首次执行/重试，Core仍核原source CAS；不能只把没有原business commit的历史publication当无限write fallback。该行为及intent是否取消/完成/被retire必须有明确native状态门禁。若外部source已经冲突，继续保留原wire/raw，不把absent或reject自行当取消原operation的证明。

限定一份活动business intent对应同card/session，可参考现16活动草稿/256累计identity作新journal显式上限，但必须由native合同注册，不能自动算作原草稿已授权的额外名额。全部request/snapshot/metadata/body计费，不把50003塞满body或把full wire当免费pin。原草稿body4MiB、active64MiB、16槽/256identity与业务512KiB/Properties64KiB等原限制不放宽；intent/root保存的历史量也需要有界回收/引用策略。

新business child link采用native拟定的 **exact intent pointer+SHA**；其目标必须已经落盘、不可变、可重取且不会被GC误释放，link/ancestor证明仍由native重核。每份current Submission只内含恒定root与latest六字段，不递归嵌套历代请求；不会把全文wire重复写进每一代link。pending intent覆盖commit→childACK之间的crash，持久plan需同样保存first/retire/close各自literal wire，不得仅记录operation后重新拼JSON。新View/wire读取完整回复仍计512KiB；不能丢字段或截短后称valid，必要独立有界读取/分页。

### 若native暂不实现：独立前端filesDir journal

可实施process-crash基础方案为新 `EditorRequestJournal`，复用AttachmentFiles的受界读写方法但不复用import schema/目录。`context.filesDir`内独立namespace，唯一operation目录、write-once rawUTF8 Submission文件、完整pub snapshot/metadata，各自长度/hash/目录实际总量上限；创建/存储失败不进入Workbench。完整write循环+fsync+readback后最后exact复核，然后model.save直接入队。不能使用Preferences、cacheDir或内存Map代替完整原件。

每份immutable记录需明确完成边界，例如原wire文件与metadata均准确落盘后再提交不可变ready记录；process crash留下partial/corrupt/unknown children必须保留诊断，不能自动当作未发送而删除。可用独立temp→publish写法，但API26 rename、目录durability及掉电原子性的精确合同须先本地SDK验证和故障测试；现writeSmall只有文件fsync，不足以证明这一套新设计掉电全原子。无明示ack时恢复为Unknown，发送前保存完成不等于业务执行。

完成/hand-off checkpoint用append-only独立小记录，不能TRUNC覆盖原wire。外层encoded request、嵌套Submission、metadata与pub snapshot完整 bytes分别独立验证；分成sidecar不让传输512KiB或整体retention预算消失。若请求太大、quota满、symlink/namespace不安全、校验失败，保留原文件并停止发送/清理，提供用户可理解的核对入口。该本地文件不是受保护证据，native仍核真实原history。

此方案比native自有intent多一个存储边界，handoff View还需要精确journal关联；不能把前端sidecar当最终protected资格。本轮Root已选择native方案，前端filesDir只作未采用的替代评估，不新增实现范围。两种方案本轮均没有新SDK/filesystem/Store测试。

## 重启还原与状态机

启动 `open`后同时读取draft列表、request intent列表与真实imports。不要先依draft/card列表断定创建未提交；已成功并退休的parent可能不在active列表，但原request/historicalpublication仍必须可以查。

恢复原请求时 `EditorBusiness.restore(exact request_json, immutable publication)`，初始Unknown；绝不把durable `prepared`当未提交证明。旧view owner/epoch只用于诊断，新的view采用fresh lease，已知历史事实由immutable request归属保存。原S2 raw以真实active descendant完整values展示，绝不用S1/当前card字符串覆盖它。读出的child若alreadynewer或inactive，不以旧first ACK重建/复活；明确当前status并提供原operation核对。

| 持久观察点 | 恢复后的动作 |
| --- | --- |
| intent保存完整，业务ACK未存/未知 | 原wireUnknown入口；用户显式inspect/retry同op，不自动业务replay；有准确S2 draft则独立恢复 |
| inspector证明committed，原child尚无确认 | 保准确历史基线与原source冲突；恢复active parent/current proof，再发已冻结handoff或明确创建新未发proposal，不能换已经Unknown的childop |
| firstchild commit已证明、parent未退休 | 采用准确current-active child及其完整latest，校验business link/context；原parent暂留，显式核原固定retire请求 |
| firstACK历史存在但child newer/inactive | 历史已提交事实保留，当前冲突/退休事实保留；不覆盖latest、不复活，不静默再建同scope |
| parent退休已证、terminalcheckpoint丢失 | native proof核同link/retireop后补状态，不重放业务；raw/context已在child/准许archive后才回收intent |
| knowncommit但live已推进/删除 | 显示自己准确历史commit和真实source冲突；禁止currentcards补baseline/自动continued，保完整raw和原wire |
| inspect absent或读取损坏/超预算 | absent只是本次快照，损坏不是absence；原request不丢、不生成新op、不清parent/assets |

推荐把状态拆为 `request frozen/issued_unknown/rejected/qualified_committed`、`handoff none/first_unknown/child_confirmed`、`child latest_confirmed/unknown/conflicted`、`parent retirement none/unknown/confirmed`、`view mounted/revoked/closed`。生命周期移出可替换的Index字段容器，至少有稳定workspace/session owner；Index只持当前view租约及展示投影。旧generic pending还用于TaskId等命令，严禁两条通道同时消费同一editor保存。

最短ETS集成可增加稳定owned session容器，持 `business model / intent proof / exact root-latest context / parent-child phase / original lifecycle wires`，以及Index的 `prepareBusinessSave / reconcileBusiness / finishBusinessHandoff / restoreBusinessContext`四条调用路径；这些名称是设计占位。`isCurrent`绑定业务所属稳定session及允许的真实parent→child lineage，`isExact`再绑定原frozen lease、inputepoch与完整raw。否则把 `isCurrent`永久闭包为 `editorDraft===oldParent`，切child/dispose parent后 `continueTodos()`会错误拒绝；反过来只看当前card ID又会误把外来新session当原owner。新view重新挂载后仅同session的历史context可明示承接，旧SDK callback仍只认原冻结lease。

child切writer时同一同步段更新 `editorDraft/draftSource/editorValues`及准确scope，但不清raw或重建字符串。完整row值及revision要明确交接，保持native确认的ownedTodos context，使旧async formatter/row status不能盖掉新revision。newCard/restore/select与关闭按钮识别session pending/Unknown，保持一份card只有一条editor-save owner。新strict历史回执用于业务context；当前cards只用于另行刷新展示，不参与source或baseline资格计算。

## 缺失接口与必要验证

### 原wire与候选intent envelope映射

Native代理已产出 [完整候选合同](business-handoff-design.md)。本轮Root按用户即时推送要求收束为设计检查点，**没有生产intent/新dispatch/source0 child实现，也没有最终API冻结**；下列映射只说明如何接当前真实模型，不作为已可发送的接口资格。

候选 `IntentProof={intent_id,prepare_operation,generation:'1',prepared_record_sha256,request_sha256}` 指向准确immutable首intent record；SHA/代次规则同上。首记录body SHA与完整Submission request SHA是两个摘要，不能互换。

| 候选动作 | Index/session最短责任 |
| --- | --- |
| `editor_intent_prepare` / `editor_intent:{operation_id,request_json}` | 在业务前冻结一次prepare transport与完整原Submission；parent暂停期间核exact publication/pins。ACKlost只显式原prepare retry，不提前业务send；ACK proof要与原card/op/wire/pub/snapshot/expected revision匹配 |
| `editor_intent_list` / query `{after,limit,card_id}` 与 `editor_intent_read` / ref `{intent_id,prepare_operation}` | 启动发现pending与closed committed context，逐项取完整原wire/准确historical publication/原literal计划。DTO所需语义必须完整明确；list summary不是restore资格。跨进程不读currentcards拼回基线 |
| `editor_save` / `{request_json,intent:IntentProof}` | 使用新增实际registered外层，但Submission字符串不改。核proof和额外UTF8预算后仅发送固定actual save wire；issued/cancel CAS与独立S1 snapshot准入由native实现 |
| `editor_commit_inspect` / `{request_json,expected_revision}` | 沿现模型相同原Submission和准确原预期revision；保完整第一次inspect wire供Unknown显式retry。registered intent/current draft只是恢复定位，不改原被检查业务payload |
| `draft_continue_business` / `business_handoff:{request_json:完整Handoff literal}` | literal一次冻结schema/intent proof/active parent proof/flat child Write/plan operation/retirement operation。child Write采用latest full raw、source_kind0、准确业务revision、expected_generation0；完整source bytes由native历史生成，UI不添加不存在的source字段或current baseline |
| `draft_continue_business_retire` / `business_retirement:{request_json:完整Retirement literal}` | 使用native已经durable生成/绑定的Retirement literal；不能client在childACK后新生成retire身份或重拼body。firstchild/current/link/latest确认后才显式派发 |
| `editor_intent_close` / `{request_json:完整Close literal}` | 固定close operation与disposition。`cancel_prepared`仅未issued；`saved_exact`在UI revoke/exact守卫后通过native close-plan和固定discard完成；`handoff_retired`仅真实child/parentretirement已证。close Unknown重试同literal，不执行旧ordinary discard替代它 |

`EditorBusiness`现 `originalRequest`是原完整Submission字符串，`originalSave`是 **未带intent** 的冻结outer save。新候选不能直接把该getter当实际已发送registered wire，也不能修改Submission以塞proof；50003仍绑定原Submission SHA及publication SHA。最窄可选择新Intent/session协调器做固定transport适配，而不扩大原业务投影：

1. 保原model、originalRequest与其originalSave不变；收到并强验证prepare proof后，一次冻结实际registered outerSave。额外proof及其转义计入实际完整512KiB预算，不能只依旧model的不带proof预算检查。
2. send hook只将 **逐字节等于该model原originalSave** 的输入映射到这一个registered outerSave；不能接受同card/op但不同raw/proof/source/JSON的任意请求。retrySave重复相同映射，实际Workbench收到的字符串始终一致。其它action不混入此映射。
3. inspect原wire保持模型输出，首次用于核对时独立冻结/保存其完整transport；retryInspect只同原wire。任何额外异步存储都发生在相应真实派发前，并遵守前述parent暂停/queue顺序，不能让hook内部延迟造成“已入列”的假判断。
4. 跨进程read恢复exact Submission、immutable publication、prepared proof与实际registered save/inspect原transport归属后，再 `EditorBusiness.restore`，初始Unknown。不能仅从新版本模型的JSON字段顺序重造所谓“原实际outer”。替代实现可窄扩model绑定immutable intent及restore实际outer，但仍要相同字节/预算/状态测试；本轮不实施任何一个分支。

按候选设计，native意图准备记录将保存原Submission；如果产品承诺恢复**完整实际外层transport**，还需明确其持久位置：issued独立记录保存原actual outer literal，或注册不可变化的versioned唯一encoder且实际回传字节核对。仅inner SHA不能证明外层literal完全相同。包含`prepared_record_sha256`的registered outer不能再放进同一首prepared body来计算自身proof，避免自引用；可以在后续issued record存并保持首generation1 proof指向immutable prepare历史。first/retire/close计划里的embedded literal同样独立保存，outer有固定版本编码或准确存储，不能只存operation后猜原key顺序。

Native proof/read DTO的最终字段还需明确：准确prepare历史/phase/current状态与计费、原wire/expected revision/恢复publication、固定plan/Retirement/Close literal、准确first child/current-active/link、普通business effect与intent/plan/cleanup effect分离。ETS不得把某个内部plan事务 `committed`当business或child已committed；必须验证对应实际历史结果。这里列的是必要语义，不宣称候选已经返回该完整DTO。

当前缺失：Workbench strict DTO类型与分支；Index owned business/context和全文source0 row准入；native business link/retirement/View及其ETS强校验；完整原请求durable journal/发现/history读取/条件回收；跨进程pending handoff/retire原wire；准确上下文恢复与Stable workspace owner。后续以Root确认的最终native wire为准，再写新的独立model与实际Index调用，不能提前把以上候选名称称实现。

| 验证组 | 最少有意义场景 |
| --- | --- |
| actual Index methods + actual EditorBusiness | unchanged既有卡Save建立publication；create全部raw/pub/stage/assets同一原op；普通V2非空pendingtodo拒而保raw；native-owned source0 child仍多行capture/formatter/Save使用continued；foreign/current marker不能准入 |
| owner/epoch/业务事实 | 字段检查/hash/journal等待期间lateedit取消未发proposal；入队后S2持续capture并保原wire；away/back text/selection仍epoch失配；view换owner后历史ACK只更新原request，不清新editor；完整坏DTO/回复预算保持Unknown |
| Store原intent故障 | intent写前/持久后但业务前/业务后reply前/child前/首child后/退休后checkpoint前逐处kill+reopen；prepared S1业务absent而parent推进S2后，原wire有独立intent准入且source CAS仍正确；原JSON空白/key顺序/转义不变；sameop不同raw但normalized相同拒；已commit+latermutations/draftretired可按原wire重核 |
| 真实handoff source与pins | createkind1→自己exact历史source0；edit带matching prior origin0 pins推进后raw仍能保留，未确认source资产无权借digest；confirmed parent proof+dirty complete S2仍能正确建child；S2/S3继续保存且latest/root非递归；真实ancestry、unpublishedpin/order/alias/metadata/byte验证、unselectedimport阻断、错card/op/source/ref重复拒；firstACK后currentadvance/inactive不复活 |
| 预算与恢复 | outer escape final-fit/overflow、pub snapshot/wire合计quota、16/256边界、unknown namespace/corrupt/truncated sidecar或Storerecord保留诊断；list完整分页；超过2^53的canonical u64无Number舍入；root在child引用时不可GC |
| closing/manual | exactSuccess revoke前/后latefullraw remount→business child；incomplete/composition停清理；manualdiscard明示consent且view已撤销；第二次childdiscard不沿旧父回调；不能把未交付SDK queue当已排空 |
| SDK与实际产品 | 新DTO/model actual API26编译，finalproduction source/native/HAP重新冻结；实际device create→lateS2→child→continued→重启/S3/附件与关闭矩阵。主机模型或隔离SDK不是此设备验收 |

既有 v22 158 Rust、23 ETS及742全模型是原发布范围，本文未新跑任何测试，不扩写为上述矩阵通过。当前接续设计与全面UI/Flutter/Windows目标持续OPEN；专用分支发布不合入main。
