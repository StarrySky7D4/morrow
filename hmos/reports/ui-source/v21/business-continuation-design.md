# dev21 业务成功后的准确接续：development 合同设计

2026-10-07。本轮只读检查真实 Flutter、HMOS Engine 和共享 Store/transaction 源；本文件是待实施的接口设计，不是新增后端、SDK、NDK、HAP或设备证明。生产源码、旧报告和冻结 shared 未修改。完整 Flutter 目标继续 OPEN，不能把上一版 raw fork 作为用户长期只能保存冲突原文的替代终态。

## 当前源码事实

Flutter `build/io-safety-refactor/lib/main.dart:5210/5224/5325` 在完整 TextEditingValue（文本、UTF16选区、affinity、direction、composition）变化时推进代次。业务 S1 保存冻结 `_frozenFields/_frozenDraft`；成功回执到达而 live S2 已变化时，保留控制器并打开准确业务成功后的后继会话，不再次执行原业务保存。

`plugins/workbench_native.dart:2875` 的 `_NativeEditorSession` 保留原 operation、完整 fingerprint、raw fields、submitted attachments 和 `_pendingBytes`，拒绝改变未确认原请求。`_save` 业务提交后读取当前内容；`_matchesConfirmed:3092` 要求自己原 confirmed 对象、owner、target、完整 JSON snapshot、准确 revision+1、非历史回执、非删除和 legacy格式。`_openLegacyEditor:2100` 使用明确 expectedRevision，并再次核 live revision。`_currentAfterCommit:1804` 会将较新内容标为 `historicalReceipt`，不会把该内容与旧回执当作可接续的确认基线。

Flutter legacy `todos` 业务投影为 splitLF → Dart trim → 去空 → 首次去重；完成集合按相同文本成员关系求交。`plugins/versioned_editor_adapter.dart:144` 拒绝已有V2的非空legacy todos/completed/editor.todos。因此 HMOS 将新卡legacy原文适配成真实V2 Tasks是明确的后端差异，不能声称Flutter已有V2全篇任务编辑器已提供这种功能。

HMOS `rust/src/lib.rs:520` 先对同card/original operation读取准确历史事实，仍通过原Core完整command比较阻止改过重试；但成功回复只返回 `receipt_revision`，`cards:self.cards()` 是**当前**全集。原操作revision1与当前卡revision2可能同时出现。禁止由二者拼一个后继source。

Core `Store::operation_commit`（`shared/core/src/store/evidence.rs:239`）返回原持久command及Receipt，验证容器、command hash、card/operation和evidence；它不是plugin授权或protected capture。CreateCard命令含准确初次完整Card bytes。SetVersionedContent含准确source_card、title/body/preview/attachments；用 `VersionedContentChange::propose` 在命令内原source重建结果，Core `decode_commit` 已核结果content SHA、revision和附件SHA。因此可以不读currentcard地重建准确业务结果。

还有一个不能假装已存在的边界：当前Core不保存Engine JSON wire，也不保存业务使用的 `draft_id/generation/draft_operation` tuple。50001仅绑定非空create原todos身份；它不是原JSON/完整raw选区的提交证明。旧历史可以准确证明原**持久Core command及结果**，通过兼容原请求重建验证其业务语义；无法追溯证明某个仅语义相同的JSON字节序列或raw publication tuple就是实际使用的那一份。

## 本次源代码检查点的准确状态

用户要求立即同步GitHub后，本阶段收敛为**设计文档检查点**。`editor_save`、`editor_commit_inspect`、`continued_todos`没有落盘生产实现，没有新dispatch、module、Cargo依赖或测试；`lib.rs`和`create_todos.rs`仍是下表原SHA。没有对这个设计执行Store、NDK、HAP、SDK或设备验证。本文件不能作为新后端已发布或后继S2已能业务保存的证明。根代理当前界面修复交付不依赖这些待实现入口。

下面仅冻结原子业务基础的JSON形状。`draft_continue_business`及专用proto/link/retirement仍是后续设计，**本阶段明确不实施**，避免与当前rawfork/lifecycle接线同时扩展协议。完整业务接续目标仍OPEN。

## 冻结的接口方向（待实现）

最低可行路径包含一个真正的原子业务保存入口、只读原提交检查、持久后继写入和条件父退休。既有create/edit、raw fork和protected ParentLink不偷偷改变语义。

共同 DraftProof 是 `{draft_id,generation,save_operation,request_sha256}`；card由外层绑定，generation为canonical u64十进制，SHA为小写64hex。Proof指向准确immutable raw save历史，不指向current列表或view row ID。

### 1. 新的原子 `editor_save`

```text
action = "editor_save"
editor_save = {request_json: 完整冻结Submission的JSON字符串}

Submission = {
  schema_version: 1,
  mode: "create" | "edit" | "continued_todos",
  business: {
    action, id, operation, source,
    title, description, hypothesis, conclusion, todos, category, stage
  },
  publication: DraftProof,
  continuation: null | {
    root_request_json: 一次strict create的完整原Submission字符串,
    baseline: {
      operation, revision, command_sha256, content_sha256,
      request_sha256, publication_sha256
    }
  }
}
```

`request_json` 保留实际客户端冻结的**完整Submission字符串**，一次冻结后不得重新排序key或改空白再作为同一操作重试；其内容已经包含mode、原business、完整publication和continuation context。严格解码既有业务字段；内部action必须与mode相容，id/operation/source/publication参数一致，不允许draft/query/file/生命周期动作嵌套。`create` 复用当前真实新卡投影（含create_todos），`edit` 复用普通四字段/附件投影且拒非空todos；`continued_todos` 只开放给下面核实的本development创建序列，执行完整原子V2投影。

为了能在重启后准确核实**完整实际提交字符串及publication选择**，新路由需要在同一最终business body登记小型独立identity marker。建议新HMOS-owned legal field50003（本轮再次fresh `rg`在`hmos/rust/src`及`hmos/shared`全部Rust/proto中未发现使用；50001仍是create原todos，测试无关50002必须保留）。Marker仅包含domain/schema、mode、实际完整`request_json` UTF8字节SHA256和canonical publication proof SHA256；operation/card/source/raw/context已经被完整原字符串SHA绑定，**不把整份raw/source/root字符串重复塞进body**。这是development原请求身份，不是protected签名/capture digest。没有marker的旧create/edit字节不改写、不补造证明。

marker计入已有Properties64KiB及event/body预算；对唯一tag/domain/wire-type/schema严格校验，遇到该tag已有不同domain或重复值拒绝，不覆盖未知扩展。请求/回复仍各自遵守既有512KiB完整JSON限制，嵌入字符串的JSON转义也计入实际外层预算。marker本身不保存原wire；客户端必须保留实际完整原字符串，重启后提供它才可通过SHA、publication及完整Core command比较恢复准确提交资格。不能仅凭相同业务语义补造原JSON字节身份。

普通favorite/task/category/edit等后续操作可能按原scanner保留50003，但保留旧marker不证明这些新操作使用了原editor请求。inspect必须核原完整wire内operation等于正在检查的原operation、mode/whole-wire SHA/publication SHA均一致，并重建完整Core command；不能仅因历史body里有50003就把其它路由提升为严格editor提交资格。

continued context只携带恒定的原strict create字符串和最新历史baseline六字段，不能递归嵌入每一代旧Submission。root必须`mode=create/continuation=null`并逐项重建原CreateCard、50001及实际TaskId。baseline由`operation_commit`重建准确command/result、核六字段、要求mode=create或continued_todos、50001与root一致且无migrated origin；business完整source必须等于此历史result。continued的本次marker必须与其命令source的marker不同，普通task/favorite等复制marker不能成为新qualified baseline。普通strict edit不得把已有create/continued marker的source洗成owned任务序列，必须使用continued模式。

这一恒定容量方案依赖**development trusted producer的同事务marker归属与严格准入**，不是仅靠marker自证全体历史TaskId的protected权限链。baseline字段不能代替对原Core command/result的检查，也不能从TaskId前缀或current相同文本认领任务。任意有完整Core写权限的其它作者不属于该development准入的认证威胁范围；如果将来要对这种作者证明可独立验证的每代任务血缘，必须另外设计有root/op/task权限的持久provenance协议，不能把当前紧凑两hash marker夸大成该能力。

这里只调用一次授权 `host.create_content` 或 `host.edit_versioned_content`。各种pure projection可以在内存中组成最终body，但不能发送多次task_add/remove/rename/reorder/category命令，再把中间可见状态宣称成原子保存。Core一次事务同时提交完整卡片、tasks、附件、原command/receipt/outbox。

### 2. 只读 `editor_commit_inspect`

```text
action = "editor_commit_inspect"
editor_commit = {
  request_json: 与editor_save完全相同的原完整冻结Submission字符串,
  expected_revision: 原业务回执的canonical十进制revision
}
```

检查顺序：

1. `operation_commit(id,original.operation)`；不存在仅返回snapshot absence，不能创建原业务操作，也不能以absence证明并行请求不会以后提交。读取/解码失败保留Unknown。
2. 严格重建原请求的完整Core command。必须使用原publication历史和原source、相同纯业务投影/marker；逐byte等于stored command，card/op/mode/expected receipt对应。查到Committed也不能跳过完整payload比较。
3. 从CreateCard或SetVersionedContent重建历史结果Card，核canonical完整编码、id/type/format、原source revision、receipt revision、content_sha256、ordered attachment_sha256以及完整Properties。不能从current卡组装结果。
4. Marker存在才可回 `qualification="development_editor_wire_v1"` 并核其canonical publication SHA、实际完整原wire SHA和模式；旧route历史最多回 `qualification="legacy_semantic_only"`。同raw相同normalized结果、不同publication/composition/alias证明的问题不能被旧历史假装已经核实。严格后继接口不接受旧弱资格；若需给旧历史开发兼容接续，必须单独明示这种语义资格，不能默认为完整实际原请求证明。
5. 最后可只读比较live `card.encode()==historical_result.encode()`，返回 `live_matches` / `live_revision`；这仅是新业务CAS诊断，不能替换历史result。缺失/较新/已删都不能自动rebase。

成功结构由`Reply.editor_commit`单独返回准确历史CardView/完整source与receipt：`{commit_status:"committed", qualification, card_id, operation, source_revision, revision, command_sha256, content_sha256, request_sha256:<实际完整原wire>, publication_sha256, publication, historical_card, live_matches, live_revision}`。普通`cards`当前全集不能冒充这里的历史结果。只读动作自己的effect与被检查业务commit_status区分；业务已证明提交而随后读当前卡失败不能反称原业务未提交。可避免在普通UI展示这些内部字段。

所有digest都来自native原件，不接受client任意宣称committed；`expected_revision`只是严格一致性条件。元数据不能代替完整source/command比较。回复容量不足整项报错，不省略Card/任务/附件后冒称有效inspect。

### 3. 后续持久 `draft_continue_business`（本阶段不实施）

```text
action = "draft_continue_business"
continuation.schema_version = 1
continuation.child = 完整现Write；new draft_id/operation；expected_generation="0"
continuation.parent = 准确active handoff DraftProof
continuation.editor_commit = 原editor_commit_inspect完整输入
continuation.expected_command_sha256 / expected_content_sha256
```

native重做inspect，不把外层客户端传入的上一条reply当权限。初次业务后继Write的source_kind必须0，source_revision必须准确业务receipt revision，完整source_card由原历史结果生成；禁止客户端source/currentcards自动提升。保存最新全部raw TextValue/category/stage/选中附件，不改变业务内容、不清空todos、不format/trim raw。

必须区分两种parent proof：原S1 `editor_commit.publication`证明原实际业务提交的raw输入，而 `continuation.parent`证明当前仍active、完整current Slot等于其save历史的raw/pin权限。它们可以是同draft的不同generation；若当前parent是rawfork子，需验证不可变真实祖先链（最多既有256累计journals）回到publication，不能把同card/同revision的任意另一个draft当作原会话。原业务source与publication完整冻结source严格相同；原sourceKind1 create只能向其自己准确created Card转成sourceKind0。

初次selected pins仍用专用origin4，只能继承active parent确认库存的有序子集/准确aliases/完整name/MIME/length/SHA。业务S1已发布的pins须逐项匹配历史business assets+outer附件；S2才确认、未发布的parent pins也保留真实权限，不从历史card全资产重建选择。未选Pending/Ready按现fork规则明确拒绝，不能冒用URI或自动丢弃。首child ACK后后继普通保存用origin3保留新link。

采用独立development business link/retirement类型（建议Slot15/16，13/14 rawfork和11/12 protected含义不动），绑定active parent proof、original publication proof、原business op/requestSHA/commandSHA/contentSHA/revision、固定child first operation、todo模式/root归属。普通fresh/save/origin范围不放宽；新save/import/discard/second-successor门禁同时识别两类development successor。

若live已经变化，可以保留准确历史source的新child raw并回明确business source conflict，或在写前拒绝并保留parent；这是实际外部冲突。正常own-success且live_matches时必须得到可实际保存S2的source0 child，不能继续使用source1 rawfork作为长期终态。此状态不授权自动拉取current task列表/资产/来源。

### 4. 后续 `draft_continue_business_retire`（本阶段不实施）

使用原固定退休operation、上述完整link及准确child first receipt/current-active证据，确认child complete raw+pins durable后才退休准确parent。要求current parent完整Slot/CAS不变、唯一successor、准确原history及未选staging已解决。child first历史ACK可与current child较新代次并存，但旧ACK不能覆盖新raw；Unknown只保留原完整请求，不能制造新operation。

沿用两阶段crash-safe顺序，不声称cross-process原子交接。parent+child并存按现16active/64MiB/256identity完整计费，不借业务成功扩大quota。父退休成功仍由child保存await期间S3，不能立即dispose live controllers或再次discard造成同样竞态。

## 新卡todos → 实际TaskId → S2全篇原子保存

kind1 create有原文S1（含空/重复行）和真实V2任务两个不同层次。检查原CreateCard时必须复用现 `create_todos::prepare(card,original_create_operation,S1_raw_todos)`，核完整ordered `(TaskId,text,completion,legacy provenance)`；非空原文核body50001 domain/digest，empty旧形状必须无该field。初次全部Incomplete/originNone，不能从view rows或current相同文本猜原TaskId。

`continued_todos`只适用于上述准确验证的HMOS-owned创建序列。fresh已有V2或migrated Origin任务不能因为看到相同文本就进入该模式。本阶段基础设计以root原strict create和最新准确历史baseline、development producer准入核归属；将来持久business child还需自己的immutable link，当前不存在该link实现。不能从current TaskId前缀猜。后续每次editor_save/inspect/child继续绑定最新自己的strict business结果；foreign task panel动作令原source CAS冲突，不自动把多出的任务纳入owned集合。

最小明确语义采用Flutter legacy文本成员规则：先完整检查S2原1000graphemes/100行，再splitLF/Dart trim/去空/首次去重。原normalized label仍在S2时保留其确切旧TaskId、completion和原未知Task字段；按S2首次顺序排序。移除label则将确切旧ID加入retired_task_ids；新label创建新ID，Incomplete且没有迁移证据。ID由card、root create operation、本次固定原save operation、完整S2 raw身份和首次normalized index确定，严格拒active/retired/origin复用。**label改名在此legacy语义中是删除+新增，不猜row ID、位置或LCS为原TaskId；相同文本成员才继承completion。** 这不是V2详情面板TaskId rename语义，必须明确模式。

完整最终Properties还包含全部common fields、category/stage、selected附件投影和实际ID集合。一轮pure计算后一次 VersionedContentChange提交，source是准确自己原business Card；同原operation重试在current更晚/已删除时仍重建原command，不把current body当重试材料。删除/新增/排序/类别不分成多个Host命令。

初次50001保留为root原create身份。新continued保存还需同一业务body的50003原请求marker通过实际完整Submission字节SHA绑定完整S2原文/context，并绑定准确publication SHA，避免blank/duplicate变动normalized相同却偷换原提交；它不能覆盖50001或无关50002。当前批准的marker仅schema/mode/两hash，**没有额外root+完整ID集合的持久描述**，不能引用尚不存在的描述充当权限证明。保留source全体未知Properties/Task/outer Card字段、relations/favorite/icon/color以及真实retired IDs；不typed re-encode覆盖整个树。

128任务、单项2048UTF8 bytes、4096 retired IDs、完整Properties64KiB、Card8MiB、Core event16MiB、raw4MiB/各字段512KiB及JSON512KiB均独立保留。最终预算或新marker导致超限即在业务提交前整项失败，不截断原文/任务/marker，也不因为grapheme合格放宽backend bytes。

## 需要验证的完整工作块

1. 实际Store原create/edit/continued command重建、完整receipt/content SHA；current外部变更与历史结果区分，原history读损坏仍不假称未提交。
2. 改过raw却normalized相同、publication tuple/selected alias/source/operation变动、marker重复/错误domain、其它card commit、非own TaskId、migrated任务、TaskId退役后复用均拒绝；无写入/子集成功。
3. kind1带真实多行/空白/重复todo创建→准确inspect→source0 child→修改S2字段/todos/附件→单业务commit。保留相同label ID及completion，新增/删除/顺序/未知字段/retired IDs全文验证；再次继续S3，不停在仅冲突rawfork。
4. Unknown原request重试、成功但current读失败、child newer/inactive历史ACK、未选import、预算双active和256身份、完整bytes export/reopen。
5. actual Flutter原raw normalize/completion成员对照与完整TextValue保存；V1文本成员/V2真实TaskId适配差异单独记录。actual Store subprocess对一个业务事务及两阶段接续各边界中断，证明全篇任务/body/附件/marker/receipt/outbox一致。
6. Index只有原owner/raw/epoch相符才更换writer；保持live controls和完整S2/S3、source状态、自己的原operation，不用 `attachDraft(currentCard)` 重新选择全资产。SDK/设备/IME/安装资格另外验，不由纯检查器推断。

## fresh读取来源

| 本轮实际原件 | SHA256 |
| --- | --- |
| `build/io-safety-refactor/lib/main.dart` | `6203B7465F320DC8463DFACB5387EA8B437EC3EC1458CCA7E215E44B8F65CB1E` |
| `build/io-safety-refactor/lib/plugins/workbench_native.dart` | `984FE512761E9E2C66A6CAA0275B40D6CE142D06727C2B3C0EC65227D8C7F77E` |
| `hmos/rust/src/lib.rs` | `AD144F2C7106AE876A2B482679E84116A2DC4E2BB8D7F89D1A9275CAA0731D6D` |
| `hmos/rust/src/create_todos.rs` | `ED0CF6965C7C5AB2A98E2A92B582A650DD4E756564D5D7563E87BB51C1BD3900` |
| `hmos/shared/core/src/store/evidence.rs` | `3B4E4CDE8596F956F849FC24532C1E8ADC4FF5CC18DF65F38C15AEA7012CCBF1` |

这些是本轮文件bytes身份和只读源码依据，不是新测试结果或全工作树资格。本轮未运行Git/设备/NDK/HAP，也未执行或实现上述新API。
