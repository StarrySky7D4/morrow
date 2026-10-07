# dev22 独立业务保存协调器

2026-10-07。此次只新增 `entry/src/main/ets/model/EditorBusiness.ets` 与 `tool/editor-business-model.test.cjs`，以及本报告和测试日志。没有修改 Index、Workbench DTO、既有 draft/fork model、Rust、shared；没有执行 Git、NDK、产品 HAP、安装或设备操作。

生产模块已冻结为 **26,767 bytes / SHA256 `6469D1F2AF4E2DA6B9066B52917E658BC13ECECB4EF2A4E6B92FBB4AD0D996CF`**。它尚未接入 Index。原生实现仍由另一个 worker 独占验证；本报告不代表 native 构建或采用，也不代表业务成功后的 source0 子草稿已实现。

## 实现的协议与 API

`EditorBusinessCoordinator.prepare(mode, business, exactPublicationRecord, hooks)` 只开放新 `create` 与普通已有 V2 `edit`。它在任何异步检查前拷贝完整业务字符串、原 source 和完整 DraftRecord，生成一次实际 Submission JSON；后续 save/retry 不重新排序 key、裁剪原文、替换 publication 或生成 operation。普通已有 V2 edit 明确拒绝非空 legacy todos。

prepare 要求 publication 是自己准确的 active/current generation；scope card/source kind/revision/完整 source、save operation、canonical request SHA 和五字段 raw/category/stage 均对应。完整 TextValue 仍留在 publication 中，业务只提交其原文本；IME composition 没有完成时拒绝业务提交。实际 `EditorFieldPolicy.requireValues` 对完整冻结字段执行 native Unicode16 计数和字段上限检查。它不会把 raw journal 的结构合法性当作业务字段合格，未知/不完整计数回执会拒绝准入。

`restore(actualOriginalRequestJson, exactHistoricalPublicationRecord, hooks)` 保留原完整字符串字节，开始于 Unknown。历史 publication 必须是准确的原 active save，但 current 可以已经退休或推进；恢复不以当前 journal 仍 active 为条件，也不重新执行当前 owner 的字段准入。它不从相同文本重造原 request。恢复后仍须通过真实 native 原 command/history 检查才能获得确认。

`save()` 仅发首次冻结请求；`retrySave()` 仅对 Unknown 明确重发同一完整外层 save wire。`inspect(expectedRevision)` 发只读原 wire 核对；其 revision 必须与 original source revision 的准确后继一致。`retryInspect()` 只重发原 uncertain inspect wire。save 和 inspect 在途不能被误当作彼此的 transport。Absence 只记 snapshot absence，不把已发 Unknown 原业务改成确定未提交，不生成新操作或自动重放。已确认历史事实不会因为后续 readback 失败而变成未提交。

hooks 将冻结的 view/session owner（`isCurrent`）与完整 input epoch（`isExact`）分开。prepare/首次 save 要求二者均成立；历史 ACK 不因 owner 替换而丢失。`mayConsume(fullValues)` 还核完整 `sameValues`，包含选区、affinity、direction、composition、category/stage、附件 origin/order/aliases；相同文字经过来回编辑也不能跳过 epoch。此门禁仅授权调用者消费自己已确认的冻结输入，不能替代业务 source CAS、持久子草稿交接或 SDK callback 队列证明。

## 严格历史结果与接续资格

只读取独立 `Reply.editor_commit`；`Reply.cards`、当前列表及 `receipt_revision` 的混合内容不会成为基线。DTO 在任何异步摘要验证前完整深拷贝。校验 card/operation、publication 全 tuple、source/result revision、原 receipt/event、严格资格、四文本/category/stage、完整 source、TaskId 唯一性和非空/字节限制、选中附件完整顺序与 name/MIME/length/SHA。null、稀疏或缺失的附件集合不被当作完整结果。

客户端独立验证三项真实摘要定义：

- `request_sha256 = SHA256(actual complete Submission request_json UTF8 bytes)`，包括原空白、换行、重复 todos、source、root/context 和 publication。
- `publication_sha256 = SHA256("morrow.hmos.editor-publication.v1\\0" + framed(card) + framed(draft) + generation_u64_LE + framed(save_operation) + requestSHA32)`；frame 是 u64 LE 字节长度再原 UTF8 bytes，不是 JSON hash。
- `content_sha256 = SHA256(完整 historical_card.source hex 解码 bytes)`，不是 source hex 文本的 hash。

所有 canonical u64 revision/generation 按十进制逐位处理；不会通过 Number/BigInt 生成 wire。完整原 Submission、保存和核对外层 JSON 的转义 UTF8 大小分别核 **512KiB**；reply 也拒绝超出 native 完整 JSON 预算的 DTO。root 嵌入字符串的额外转义同样计费，不裁剪字符串或返回子集结果。

准确 native producer 仍是 command/完整 protobuf 重建、Core receipt、source 内容投影和出版历史权限的权威。ETS 的摘要/结构校验不是独立 protected 授权；受控 transport 不能充当真实 Store 或设备证据。

`legacy_semantic_only` 可以只读保留原历史结果，旧 edit 的 category/stage 保留行为单独兼容；它的 wire/pub hash 必须为空，不能取得严格消费或接续资格。仅具有 marker、文本相同或 copied current Card 的内容也不能由该协调器自行提升为 own strict receipt。兼容并非覆盖全部旧 edit：例如旧 publication 仍有 pending todos，或 raw category/stage 与原业务不同，仍会被完整 publication gate 拒绝，不能为此放宽强模式。

`continueTodos(nextBusiness, exactNewPublicationRecord, nextHooks)` 只能由本实例已经验证的 strict create/continued receipt 派生。它使用准确历史 Card source、latest original operation/revision/四摘要六字段，以及恒定的第一次 strict create 完整原字符串；不递归嵌入每一代 Submission。旧 owner 必须仍属于同 view；新 proposal 必须核自己完整 epoch/raw。live_matches=false、普通 edit、外来 card/source/op、无当前 publication，以及 kind1 原 raw fork 都不能作为该 factory 的新业务基线。它仍只是新业务 proposal，不自动创建/认领 source0 草稿或退休父草稿。

`continued_todos` 的真实 TaskId/completion/未知字段投影由 native 原子业务入口验证。该模式的 Flutter legacy 文本成员规则和已有 V2 TaskId 重命名语义不同，不能宣称已实现 Flutter 已有 V2 全篇编辑器等价。

## Fresh 有界验证

命令：

```powershell
& 'C:/Program Files/Huawei/DevEco Studio/tools/node/node.exe' --test hmos/tool/editor-business-model.test.cjs
```

`editor-business-model-stage1-tests.log`：**22 / 22 PASS，0 fail，0 skipped，3143.2051ms**。日志 SHA256 `3B6BD3C1A054379A7AD9211BD8777C373AD3589E62A8EAD933418B18A791CC54`。对应测试源 **28,548 bytes / SHA256 `1875F562443CDC02B319B2E6019B15034A73060E48D3834E0AA06FCB295DB517`**。该 stage1 日志保留原身份，没有被后续追加的实际 Store DTO 测试覆盖。fresh 模块 transpile 无语法诊断；实际 SDK 编译另见下文的隔离检查，不能用未 import 该模块的产品 build 代替。

测试实际加载新的 ETS 模块、既有 EditorDraft 与 EditorFieldPolicy；UTF8/SHA 由受控 Node 平台提供，native DTO/field RPC 是明确受控 fixtures。覆盖完整原文与元数据冻结、异步 owner/epoch 撤销、Unknown 原 save/inspect 重试、snapshot absence、初次已知拒绝/后续 Unknown 不降级、历史当前卡区分、post-retirement 重启恢复、弱历史资格拒绝接续、两次恒定 root/latest baseline、u64 超过 2^53 至 max、非法 receipt/source/DTO/任务/附件、完整转义预算、DTO 摘要 await 期间的 mutation 以及没有 currentcards fallback。

初轮 19/20 的唯一失败是预算用例数据不足以令实际外层超过 512KiB；扩大 fixture 后保留原预算断言。最终 22 项没有删去或缩小该断言。本模型测试不是 native Unicode16/Flutter 对照、Store crash recovery、完整业务 UI、SDK 或设备验收。

native worker 从 fresh actual Store 导出 [create/continued fixture](editor-business-store-fixture.json)，包含原完整 `request_json`、准确 native draft_read publication View 和真实 editor_save Reply；该文件 **16,708 bytes / SHA256 `7930BBE56C010ED33D80806703B3CFF73D211FD4D631BEFD4681AE7E46D12C70`**。其 Store exporter 单独执行证明在 `editor-business-store-fixture-tests.log`，不是由 ETS 手拼回执。

在 stage1 的 22 项之外追加实际 fixture 解析：两种 mode 分别恢复原字符串/历史 publication，经 inspect 受控接收真实 native DTO，逐项核 original wire SHA、canonical publication SHA、完整历史 Card bytes SHA 与 strict qualification，未使用任何 currentcards。最终 [模型日志](editor-business-model-final-tests.log) 为 **23 / 23 PASS，0 fail，0 skipped，3157.9661ms**，**2,900 bytes / SHA256 `8F5CC41A5862A7F6F545B20D8FA69AFF565337CB4769120972596BE3FE2B22A7`**。最终测试源 **30,012 bytes / SHA256 `27E49FF7E78E72A707F33DB41D3A54065F24CD3EE4BDEC165D6D9F9E6B5FD19C`**，生产模块 SHA 未改变。该追加项是 actual native DTO 与 ETS receiver 的跨层兼容验证；回放受控，不声称实际 transport、设备或当前 Store 状态。

## 隔离 API26 compile-only 验证

根代理另行授权新独立 harness 后，创建 `business-sdk/` 中的三个 prepare/verify/build 脚本，复用 `tool/checkpoint-sdk-smoke/prepare.cjs` 的 fresh copy 能力，没有修改旧 preparer/旧 harness/任何产品入口。独立入口明确 import 此新 model，通过真实 `prepare`/`restore` 构造，以及 save/retry/inspect/continueTodos/mayConsume 和完整 DTO public type 引用，使 SDK 实际检查业务模块。受控 field/transport provider 均直接拒绝运行；入口没有调用 runtime references。

独立 API **26.0.0** 构建 **SUCCESS 11.654s**（`business-sdk/sdk-build.log`）。**311 copied inputs / 5 generated wrappers** 在构建前后完整 bytes/hash 验证 **PASS**，两份 verify JSON 完全相同（SHA256 `70810DBC9F9F2AAB32C526BEF7C98FEC8423387A8D85CFAB5713E950DCDA87A3`），当时没有 concurrent production source 差异。尤其 frozen EditorBusiness 与 EditorDraft/EditorFieldPolicy/Workbench/Attachments/Markdown/native DTS 七项实际类型依赖保持准确 source bytes。

[新 manifest](business-sdk/business-sdk-source-copy-manifest.json) 为 **199,502 bytes / SHA256 `8132DD56F344A3E155840CE79EDFFFB1E65FD9D141245E505A31D0C1DEE8CF7E`**。只有 base fresh generated 的 `AppScope/app.json5`（隔离 bundle/version）和 `CheckpointSdkSmoke.ets`（实际新接口引用）被再次改写；manifest `rewritten_wrappers` 精确登记每项 before/after bytes/hash 与来源，所有 generated 条目都更新为最终字节身份。其余三项 ability/module/main_pages wrapper 保持原生成内容。scripts 没有加入当前产品 build-inputs。

隔离 bundle `dev.morrow.hmos.editorbusinesssdk` 的 unsigned HAP **25,774,084 bytes / SHA256 `8A09E79FEE2A57BFFA1A0BF505DCE54FB68504D5831B0532B87B5B59BACAE621`**，留在 `.build/checkpoint-sdk-smoke/business-dev22-2026-10-07T10-44-44-764Z`。它是 compile-only artifact，不是产品包。复制的现 CPP/native archives 只满足 SDK 链接；没有作为新 editor_business runtime/native identity 或采用资格，**runtime/device/installation 全部 NOT_RUN**。

## 仍开放

Index 业务接线、真实业务 source0 子草稿/祖先权限接续、条件父退休、完整 live S2/S3 保存与关闭流程尚未由本模块实现。现 kind1 raw fork 保原 source 的冲突状态不能作为业务接续终态。SDK 未交付 callback 队列、Flutter 完整原 TextEditingValue/IME 平台差异、产品全流程编译/runtime、native adoption、签名/安装/设备、完整 Flutter 与 Windows 目标均不由本报告通过。发布仍限定专用分支，不合入 main。
