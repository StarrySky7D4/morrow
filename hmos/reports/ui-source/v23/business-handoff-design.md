# 严格业务成功后的 source0 草稿交接：待授权 development 合同

2026-10-07。只读现有 `editor_business.rs`、`editor_draft.rs`、fork/staging/pin、protobuf、Core immutable command/receipt，以及历史 `v21/business-continuation-design.md`。**本文件是实现提案，未修改生产、测试、协议或构建输入，未执行Git/设备/NDK/HAP。** v22 已实现并验证 strict business foundation；本文件中的 intent/交接 API 尚未实现。完整目标 OPEN。

## 源码约束和本阶段结果

v22 `editor_save/inspect` 能验证原完整 Submission 字符串、immutable publication、实际原 Core command/receipt 和历史结果 Card，当前卡仅用于相等/冲突诊断。50003只存mode与两个hash，不存原请求；当前ETS `EditorBusiness` 原wire仍在内存，不能跨进程找回。原publication active gate会因较新raw代次改变而失效，历史publication gate只允许已证明提交的重建，不能直接作为首次写入授权。

现ordinary source0 fresh draft从current card采集source；同draft后继保持原完整source。origin0若已有metadata完全相同的prior pin，先核该pin即可；只有没有可复用pin时才核live完整source并从card导出。不能把所有旧origin0输入都判作必然失败，也不能保证新选中、尚未确认的旧source资产能在业务推进后继续导入。

现raw fork13/14只保父source，不能把kind1新卡草稿转成准确业务结果的source0。新能力必须：先durable保存实际原business wire及发布快照权限，严格核其真实提交，首child.source_card使用准确历史结果，保存最新完整S2 raw及已确认选中pins，然后条件退休准确父。S2不要求先在旧source成功flush；first child.values可与confirmed parent.values不同。selected identities/aliases/order须是confirmed parent pins的准确有序子集；未确认附件、独立Pending/Ready/Unknown/spool不得被假装已交接。

## 固定接口候选

所有外层及嵌入JSON按完整UTF-8字节计512KiB，不重新排序或改写已冻结literal。SHA用小写64hex，代次/revision用canonical十进制u64。严格拒未知字段、不同action夹带其它envelope，以及不成对的Unicode surrogate。新数据结构不使用protected ParentLink/Retirement。

```text
IntentProof = {
  intent_id, prepare_operation, generation:"1",
  prepared_record_sha256, request_sha256
}
DraftProof = {draft_id,generation,save_operation,request_sha256}

action:"editor_intent_prepare"
editor_intent:{operation_id:固定prepare operation,request_json:完整原Submission literal}

action:"editor_intent_list"
editor_intent_query:{after:"",limit:1..16,card_id:""|确切card}

action:"editor_intent_read"
editor_intent_ref:{intent_id,prepare_operation}

action:"editor_save"
editor_save:{request_json:同一个原Submission literal,intent:IntentProof}

action:"editor_commit_inspect"
editor_commit:{request_json:从intent取回的同一个原literal,
  expected_revision:原native候选给出的准确预期revision}

action:"draft_continue_business"
business_handoff:{request_json:完整冻结Handoff JSON literal}

Handoff JSON = {
  schema_version:1,intent:IntentProof,
  parent:DraftProof,child:现完整Write,
  plan_operation:固定intent plan operation,
  retirement_operation:固定parent retire operation
}

action:"draft_continue_business_retire"
business_retirement:{request_json:由native计划时生成并持久的完整Retirement literal}

Retirement JSON = {
  schema_version:1,intent:IntentProof,plan_operation,
  handoff_request_sha256,
  parent:DraftProof,child_draft_id,child_operation,child_request_sha256,
  operation_id:预先绑定的retirement operation
}

action:"editor_intent_close"
editor_intent_close:{request_json:完整固定Close JSON literal}

Close JSON = {
  schema_version:1,intent:IntentProof,operation_id:固定close operation,
  disposition:"cancel_prepared"|"saved_exact"|"handoff_retired",
  parent:null|{proof:DraftProof,discard_operation:固定operation},
  plan_operation:""|原handoff plan operation
}
```

`editor_save`增加的intent是原Submission之外的授权envelope；50003仍绑定同一完整Submission SHA及publication SHA，不扩大业务body。无intent的v22已提交历史仍可strict inspect/retry。若同card/business operation已有intent记录，新首次save必须受该记录active/closed门禁，不能删掉envelope绕过已持久取消；旧未使用intent的新操作可保留v22兼容入口，但**不具有本阶段可恢复的intent/handoff资格**。原wire不能因为parse后语义相同而换一份。

native派生intent_id：独立host prefix `morrow-host-editor-intent-` 加domain/frame(card,business operation) SHA，不能由UI路径或当前卡marker生成。prepare operation原样持久并全局唯一；其它operation不得重用prepare/business/publication/child/plan/retirement operation。prepare proof指向immutable首Card/Slot generation1，其record SHA是原canonical protobuf body digest，单独校验Core command/receipt。不是caller自报权限。

新增reply字段候选`editor_intents:IntentView[]`和`intent_next_after:string`；旧reply省略它们，不把新metadata塞进当前cards。完整IntentView为`{proof:IntentProof,card_id,business_operation,expected_revision,phase,current_generation,current_active,repeated,request_json,publication:DraftRecord|null,handoff_request_json,retirement_request_json,close_request_json,close_disposition}`。ID/revision/hash/literal均string，两个current/repeated为boolean；phase取下面五个持久phase。list仅summary：三个action literal与request_json为空、publication=null；read逐项提供完整原wire/准确historical publication和已durable plans。不可用phase替代原业务inspect，关闭的intent也不声称其business一定提交。

DraftRecord新增nullable `business_link`/`business_retirement`，其它scope/values/assets/current/history字段维持原shape。link DTO为`{schema_version:1,parent:DraftProof,intent:IntentProof,plan_operation,handoff_request_sha256,child_operation,committed_operation,committed_revision,command_sha256,content_sha256,request_sha256,publication_sha256}`；retirement为`{schema_version:1,child_draft_id,operation_id,business_link}`。child.scope.source就是准确historical result；handoff ACK不再重复回另一份完整CardView，避免source/raw容量无必要翻倍。完整原business DTO仍通过既有editor_commit_inspect取得。

## Durable intent 和原snapshot授权

采用独立有界host-owned protobuf journal，与普通business卡、草稿、protected会话不同。首prepare在一次普通Core元数据事务内保存：完整原Submission literal、准确publication tuple+canonical request SHA、原candidate command/result SHA与预期revision、验证过的原selected pin inventory及实际blob引用。原publication的完整TextValue/选区/composition/assets/scope通过其immutable save history重建；read不把当前raw替换进原restore record。无需在每个child link重复存512KiB wire或整份publication raw。

prepare先复用strict projection的全部business、source、five-field/no-composition、final body/Task/JSON限制，核active准确publication并逐pin验证bytes。intent原子持有其原selected blobs（≤20），这样S1发送期间父可继续capture/确认S2，移除S1 pin不会把原发送快照变成无权限。新首次editor_save从**已核active intent**取原wire与immutable pub，重新比较完整candidate command、原source及所有pin元数据/bytes；这是独立development snapshot授权，不是把普通history gate放宽成任意首次保存。修改intent proof/wire、缺失pin、legacy弱资格均拒绝。

发送前必须收到prepare ACK；prepare Unknown不发送business。重启list/read可找出原已durable intent，不自动重放。list只回有界summary，read逐项回实际原wire和准确historical DraftRecord恢复元数据，必要时wire与publication两个有界读reply分别提供；完整restore不得因容量不够被省略后冒称有效。

为关闭和业务首次发送竞争，native在原business mutation前先用intent自身CAS持久固定`issued`记录；其operation从intent/card/business固定domain派生，确切payload可从首prepare重建。只有issued ACK已证明完成才进入业务mutation。`cancel_prepared`只能CAS取消尚未issued的intent，issue与cancel对同intent generation竞争，输方不能执行business。issued Unknown保留原wire/pins，不以absence snapshot断言撤销已在途写入。原请求explicit retry先核实际business history，首次才受active issued权限；客户端不得换新operation重试。

持久phase最少为`prepared → issued → handoff_planned/close_planned → closed`。business/child/parent的committed与current状态由实际Core history重新派生，phase本身不证明这些操作提交。Unknown没有自动超时关闭或自动重放。已经issued但原business尚未证明提交时，普通discard/close须明确阻断并保全原raw/intent，等待显式原请求核对/settlement；本阶段不假装能撤销在途业务，也不添加共享Core取消操作。

## 准确首child、祖先和pins

新`DevelopmentBusinessLink` / `DevelopmentBusinessRetirement`占Slot15/16；13/14 raw fork与11/12 protected字段意义不变。新link仅存准确intent首proof、plan operation与literal SHA、active parent proof、fixed child first operation，以及native已核business operation/revision/request/publication/command/content SHA。不存重复大wire。两种incoming link互斥；一个Slot可有incoming rawfork13和outgoing business retirement16，或incoming business15和outgoing rawfork retirement14，不能误拒合法跨阶段链。两种outgoing retirement互斥。

native从intent原wire重新执行严格原提交检查：`operation_commit(card,original operation)`必须有实际原command/receipt，qualification必须development_editor_wire_v1，完整command与50003两hash一致；CreateCard或SetVersionedContent.propose重建准确historical result。child.source_kind必须0，source_revision必须result revision，source_card由native取完整historical result bytes；客户端不提交或拼current baseline。legacy/absent/损坏history/错误expected revision均不能交接。

original publication和active parent是两项不同证据。parent current Slot必须active、generation/save operation/canonical request SHA/完整Slot等于指定immutable save，且其frozen source与原business提交前source完全相同。它可为原pub同draft较新代次，也可为真实rawfork/此前business child链的active后继。按incoming link追溯immutable首save与准确父proof，所有跨fork边要求既有conditional parent retirement已证实；同draft需要准确pub generation≤当前代次。到达original publication才停止，不能把同card/revision另一会话认成祖先。visited/draft/op有界，拒loop/多successor/缺失retirement；累积最多256 identities，不无限递归。

first child全部selection用专用origin4；只继承active parent已确认库存的有序子集，完整aliases、name/MIME/length/SHA不变并逐byte验证pin。已发表项另核原pub→历史business assets/outer BlobRef的准确映射；late未发表项只能凭active parent确认pin授权，不能从历史card全资产列表重建选择。原pub存在而已被S2取消选择的项不会自动加入child。未选Pending/Ready先用原staging reconcile处理已consumed项，剩余独立owner明确拒绝；不借旧URI重新导入、自动弃import或伪造origin0。

Handoff entry在child mutation前先在intent自身CAS持久完整原Handoff literal和固定Retirement literal，称plan。plan包括完整first S2 raw（全部TextValue/category/stage）和confirmed selection。此时business已证明提交，可把intent活跃blob引用从原S1 selected snapshot切换为first-child selected snapshot（仍≤20），原prepared wire/metadata保存在immutable prepare历史并计逻辑bytes。Plan同一事务持有这些pins；parent与intent共用blob也按各自逻辑预算计费。

plan durable后父new save/import/ordinary discard/第二child均封住（rawcapture仍由UI保留）；first child以plan的确切Write、准确历史业务source、plan retained pins与parent proof写入新的generation1 journal。first S2 raw由plan和child持久，S3由Root仅在sameowner/lease/epoch guard成立时交给child writer保存。child ACK后后继普通save用origin3并保留incoming business link；绑定不主动format/trim原raw。

原Handoff Unknown字节可从intent plan恢复，包括原child ID/first operation/parent proof/所有raw与selection，不重新生成身份。成功但回包丢失后读immutable first child，回历史first ACK及current_generation/current_active；current较新不覆盖新raw，current inactive不reactivate。不同literal即使字段语义相同也不能重试计划。Plan内部持久化效果与被请求child operation的effect分开，不能把plan提交当child已提交。

## 条件退休、关闭和准确recovery

Retirement literal在plan中已经durable；native严格对literal SHA/全部fields及stored link。核first child history generation1、expected_generation0、完整Write/request SHA/link/准确source；核current child仍active且source/link不变（可较新代次），first pins从plan保有、current pins从child保有，均验证bytes。parent仍为原准确active full Slot、没有另一个successor、独立imports已解决，才在一笔父journal事务内写16 retirement marker并释放父pins。已提交retire的相同literal返回原history；不同operation/proof拒绝，不走ordinary discard代替。父与子是两事务，plan又是独立事务，**不宣称跨进程原子hand off**。

close固定literal也先durable在intent close-plan，便于restart准确恢复：

- `cancel_prepared`：未issued且无handoff/close计划，native intent CAS关闭，保留wire tombstone；不声称取消已在途业务。
- `saved_exact`：真实strict原business已提交，指定parent fullraw/ordered aliases/pin inventory与原pub完整raw一致，准确owner lease cutoff由Root负责。close-plan封父，准确fixed discard CAS后关闭intent。任何已确认S2 raw/selection/assets变化拒绝该路线，改走business child；SDK未交付callback并非native可证明的queue drained。
- `handoff_retired`：准确first child及fixed parent retirement已实际提交，释放intent计划pins；不会discard活跃child或覆盖其新raw。原child后续由用户明确discard仍可关闭资源，不删除已提交business恢复context。

closed只把active charge/pin引用释放，原wire、prepare/plan/close操作及root/latest business context留在有界immutable历史；旧intent/draft身份不reactivate。list/read必须能区别active pending恢复与closed committed context。原close/retire读失败保持已经证明的effect，cleanup失败不反称未提交。首child创建后普通child discard在parent退休前仍拒绝，避免最后唯一raw/pin副本消失。

## 容量和Store边界

建议新intent与普通draft**合计**active≤16、完整逻辑active≤64MiB、累计identity≤256，保持既有门槛不放宽。每个active intent计完整immutable prepared request+必要original metadata、current plan/close literal元数据和selected blob字节；历史pointer不是扣除仍需要的原wire预算的理由。每个选中快照≤20，first-child raw保留各字段512KiB/总protobuf4MiB/UTF16选区规则，selection aliases规则不变。最坏原wire+first raw嵌套导致512KiB外层超限时明确整项拒绝、保留父/intent，不截断、不假称成功。所有新success/restore reply也须完整有界，写前预检，不因reply省略而给错误ACK。

prepared/issued/plan/child/retire/close是各自单Core事务；同一事务的metadata+pin引用必须一起提交。Core event/body/card、operation/evidence、所有既有64KiB business/Task budgets不变。phase变更的CAS与固定原operation保障精确历史retry，不能拿current仅revision当原command；内部plan/cleanup各自effect独立。SQLite crash-safe不等于任意多个native owner的跨object同时原子检查。单进程SESSION serialized owner仍要求Root遵守；并行外部变化导致proof/CAS冲突时保留两份durable记录，不自动覆盖/退休更新父。64MiB是活跃逻辑quota，不承诺整个历史数据库文件大小。

## 保存后再打开和详情操作：不可把拒绝当终态

exact-close或用户discard child不删除原business/root wire。后续重新打开当前仍等于准确latest own历史result的卡，可从closed intent读原wire→strict inspect→完整result相等检查，建立**fresh** source0 draft/context，绝不复活inactive旧draft。fresh普通origin0 pins仍由实际current card authority导出，closed intent/history只有metadata资格，不授予新export权限。

用户之后会勾选完成、收藏、改类别、TaskId rename，再打开全文编辑。v22 copiedmarker门禁会拒这种较新baseline；这只证明不能偷认copiedmarker，**永久拒全文编辑不是完整追平终态**。后续必须增加用户明确“编辑当前准确revision”的新会话授权与原子V2全篇投影：由host读取current真实完整Card并冻结full source/CAS；核实际TaskId/completion/unknown/retired/Origin和附件权限；以真实TaskId rows编辑/排序/删除/新增，不用文本位置、旧raw行ID或marker前缀猜rename。可要求原own root/history作为恢复context，但不能只因旧marker仍在就认当前操作属于原请求。当前编辑中的外部变化仍是冲突，不能悄悄rebase。

这项需要独立“显式当前revision open + 真实TaskId原子save”合同，特别重复Task label、rename、完成状态与migrated provenance，不能直接放宽本阶段continued_todos的legacy文本成员集合语义。本轮先闭合严格own-success source0交接和durable恢复；此后产品路径必须完成，完整目标继续OPEN。参见并行 [Index integration design](index-business-integration-design.md)。

## 需要改动的owned源及验证入口

待Root授权后最小native改动：

1. 新`rust/src/editor_business_intent.rs`及tests，新`editor_business_intent.proto`，用现有vendored protoc/prost生成，不新增Cargo依赖；intent prepare/read/list/issue/plan/close、actual retained refs、固定literal/history/capacity。
2. `editor_business.rs`提取read-only strict qualified result/helper，新增exact registered-intent admission（无marker/body变大），首次active snapshot与历史重建分开；`lib.rs`最小Request/Reply/action分派和internal journal过滤。
3. 新`editor_draft_business.rs`及tests；`editor_draft.proto`15/16独立类型，`editor_draft.rs`slot shape/read/history/pin/save/预算/联合successor/discard门禁窄接入；不改model原`validate_request`与protected规则。
4. `editor_draft_fork.rs`共享祖先/联合successor/check，允许合法incoming/outgoing混合但拒多successor；`editor_draft_staging.rs`检查plan冻结父和原import owner；`draft_bridge.rs`完整proof/link/retirement/View与native intent恢复DTO。generic NAPI execute已可携JSON，无新FD或C++符号必要。
5. Root/Index worker随后接新intent与source0 writer、sameowner lease/raw/epoch、原wire跨进程恢复；旧UI/模型不能在新库采用前发送未实现action。NDK/HAP/device由Root另行执行与归档。

必须有actual Store/reopen与failure测试：prepare ACKlost及changed originalwire；issue/cancel同intent CAS竞争；prepare→父S2推进/移除S1pin→原S1单business；kind1 raw多行创建→actual TaskIds→source0 first S2全raw→conditionalretire→continued单事务S2/S3；strict edit源资产/late unconfirmed限制；pub祖先错会话/loop/copiedmarker/legacy拒绝；未选imports、不变alias/order/三方source与pin bytes；current先进explicit historical conflict；first ACK currentadvanced/inactive；16/64MiB/256与完整JSON/refusal不丢raw；parent/child/intent export bytes及raw UTF16 selection/composing全文；closed context reopen不reactivate。subprocess在intent prepare/issue/plan/child/parentretire/close每笔真实事务的七边界突然退出，重启只核固定原history，明确各段独立而非多对象原子。

上面的API/DTO/预算均为可实施候选，Root确认后才冻结生产接口和开始实现；本报告不宣称已有intent pins、source0业务后继或设备资格。
