# dev22 editor business：独立只读审查

本报告核对 [v21 业务接续设计](../v21/business-continuation-design.md) 与本轮新增 Rust/ETS 实现。首轮观察为 2026-10-07T10:39:41Z；其后在 10:47:34Z 开始复核最终冻结源和新增实际证据，结论及完整最终 hash 见文末。首轮候选身份保留为修复审查的记录。本审查没有修改生产源或测试，没有运行 Git、设备、NDK、HAP 或任何测试验证；仅读取实现代理的实际日志与产物证据。

审查范围内未发现需要停止源码检查点交付的历史结果混用、自动重放或附件权限放宽。已发现并推动修复一个最终容量问题；弱旧历史兼容范围和客户端回执校验的窄加固项见下文。这里的资格始终是 `development_editor_wire_v1`：可信开发适配器重建原 Core 操作的请求身份核验，**不是 protected capture、生产认证或对任意 Core writer 的权限证明**。

## 实际审查边界

本轮读取了新 [editor_business.rs](../../../rust/src/editor_business.rs)、其 [实际 Store 测试源](../../../rust/src/editor_business/tests.rs)、[EditorBusiness.ets](../../../entry/src/main/ets/model/EditorBusiness.ets) 和 [实际 ETS 模型测试源](../../../tool/editor-business-model.test.cjs)，以及 `lib.rs` 的 dispatch/FD 入口、`create_todos.rs` 的共用投影。依赖核对涉及既有 `editor_draft` 当前/历史 publication、Core `operation_commit`、`VersionedContentChange`、Properties/Task 原件扫描和完整 Card/outer attachment 编码。

该新业务模块在观察时尚未由产品 `Index.ets` 引用。它没有实现设计中的 `draft_continue_business` / `draft_continue_business_retire`，没有新增 Slot15/16 的业务来源交接；现有 kind1 raw fork 仍不能自动成为可保存 S2 的业务 source0 后继。不能把本模块的 source/host 资格写成已经完成保存后迟到输入的全流程设备验收，也不能将正在验证的旧包当作包含本模块的新包。

## 准入、标记与历史证明

| 检查面 | 本轮实际观察 |
| --- | --- |
| 模式 | `create` 要求 create action、空 source、无 continuation；`edit` 要求现有完整 source、空 legacy todos、无 continuation，并拒绝带 owned create/continued 标记的 source；`continued_todos` 只接受自己的准确 strict create/continued 历史基线。完整 source 的 card ID、idea 类型、V2 格式、未删除状态和 canonical bytes 被检查。 |
| publication | 首次保存使用当前 active 的准确 generation/save operation；已提交重试和检查使用该 save 的不可变历史 Slot。五个完整字段、无 composing、category/stage、source 和 canonical Write request SHA 必须一致，不能把 live 草稿的新一代值拼入旧业务请求。已退休/推进的当前草稿不使原业务历史失效。 |
| 50003 | 新登记的 length-delimited 字段是固定 domain、schema、mode、完整内层原 `request_json` UTF8 SHA 和 canonical publication SHA。重复字段、错误 wire/domain/schema/mode 被拒绝。实际搜索 HMOS Rust/shared 的 `.rs`/`.proto`，只在新模块发现该登记。50001 原 create todos 身份与无关字段保留；50003 不扩展为 protected lineage。 |
| publication SHA | native 与 ETS 都使用带 NUL 的 domain、长度 LE64 framing、完整 card/draft/save IDs、canonical u64 generation 和原 Write 的 32-byte SHA。ETS 十进制除法编码 u64，避免大于 2^53 的 generation 被 Number 舍入；不是用 JSON 重排后的 hash 代替协议原件。 |
| 准确历史结果 | 从 `operation_commit(card,operation)` 的不可变 command/receipt 重建 CreateCard 或原 source 的 VersionedContentChange，核完整结果 bytes、card/revision/content SHA。随后用原 publication/source/原业务投影重新构建完整 command，逐 byte 比较 stored command，并核 mode/两项 marker hash。当前卡只提供 `live_matches` / `live_revision` 诊断；它不能替代历史结果。 |
| 原操作重试 | 同 wire 在 current 已推进、原 raw 已退休时仍只返回原历史 receipt/result。根 original create 字符串固定，continued 只携带最新六字段基线，避免嵌套整个前驱链；原 request 的 whitespace/key order 也属于身份。查到历史 committed 后，后续 proof/当前读取失败不能重新声明该业务未提交。 |
| 未知结果 | ETS 保留完整 save/inspect wire，分别显式 retry。只读 absence 是某次快照，不能解除并行原保存的不确定性、制造新 operation 或自动重试；已知 commitment 不会被后来缺失、损坏或超限回复覆盖。 |

`continued_todos` 基线必须等于完整历史 result；root 原 create 的完整 command、marker 和 50001 身份重建核对。普通 task/favorite 路由虽然会保留未知 marker，但其 source/output marker 没有本次变更，不能通过 continued baseline 的直接标记变更条件。当前没有发现旧公开 Engine 路由可直接写任意 50003；这只是已检查的适配器路由范围，不能据此声称共享 Core 的任意可信写入者都被隔离。

## 全量任务、附件与最终预算

投影先完整校验原 1000 graphemes / 100 原始 LF 行，再按实际 Dart trim、去空、首次去重规则处理。相同 normalized label 保留准确旧 TaskId、completion 与原 Task raw payload；移除 label 追加原 ID 到 retired 集合；新增 label 生成绑定 card/root/本次 operation/完整 S2 raw/index 的新 ID，并拒绝 active/retired ID 复用。改名是 legacy 成员删除加新增，不把 view row ID、位置或相近文字推断为真实 V2 TaskId。

**发现并已改代码的具体问题：** 首版逐项调用 task/category/text 纯 apply，每一步都会检查 Properties 64KiB。若同一次保存把约 60KiB 的旧正文缩短并增加待办，最终结果可能合法，但“旧正文仍在、新任务已加入”的中间结果会先被拒绝。交换调用顺序也不能覆盖文字增长加任务删除等逆向组合。这是提交前的容量拒绝，没有发现局部业务写入。

实现代理接受此问题后，代码已经改为 owned raw scanner 一次组完整 final Properties。独立重核确认：其保留 unrelated/favorite/Origin 字段；Task20 原件按实际 ID/label 对应复用；原 retired22 raw 保留并追加新退休 ID；selected Asset10 按同 ID 原件复用或仅替换已知 1..4 子字段，保留 nested unknown；common/category/stage/icon/color 全量替换后加入准确 50003，再进行完整 Properties decode/budget。完整 Card 的外层未知字段、关系和附件同 ID 原件继续由既有 VersionedContentChange/Core 保留。其后实际读取最终 final-fit / reverse-overfit 和 unknown-field Store 测试源与 PASS 日志，限定结果见文末；没有用代码复核代替测试执行事实。

附件来自准确 publication 的有序 pins/资产投影。没有新增 URI/任意 path 权限，也没有从 current Card 的全资产猜回 raw selected order 或 aliases。业务写入仍是一次普通 Core create/edit transaction，不能把多个任务变更命令串行执行当作全篇原子保存。

inner Submission、转义后的外层请求以及完整成功回复各自使用 512KiB UTF8 bytes 上限；original create 字符串嵌入 continuation 后的实际大小也计入，不只检查内层或 UTF16 length。完整 reply 过大整项失败，不省略 tasks/assets/source 后声称检查成功。128 tasks、2048 UTF8 bytes/task、4096 retired IDs、Properties64KiB、Card8MiB、event16MiB、raw/附件预算继续独立存在；grapheme 合格和较大的 Card 上限不会解除更紧的实际请求或 Properties 限额。

## ETS owner 与可行动事项

`prepare` 调用现有实际 `EditorFieldPolicy.requireValues`，冻结原业务、publication 和 complete TextValue，异步字段/hash 检查前后都要求原 view owner 与 input epoch。首次 `save` 再检查两项；ACK 历史事实不因 owner 改变丢失，但 `mayConsume` 还要求完整 sameValues，因此同值编辑绕回或选区/IME 更改不能消费旧快照。`restore` 保留已发原 request 的字符串 bytes 和历史 publication，不重新以 current raw 制造替代提交；`continueTodos` 只从该实例自己的 qualified create/continued receipt 派生，并接受新 proposal 的独立 exact epoch。较新的 current 或外来动作仍由 native source CAS 拒绝，不自动 rebase。

两个已经直接反馈实现代理的窄事项：

1. **旧 edit 历史兼容范围必须限定。** 新 `publication` 要求 todos/category/stage 与全部 raw、无 composing 相等；旧 edit route 的 gate 只核四个文字字段，且业务 todos 必须为空，旧 edit 还保留 source category/stage。因此某些原已提交旧 edit（例如 raw 仍有 pending todos 或 category/stage 沿旧 source）不能在新路径取得 `legacy_semantic_only`。这不提升其强资格，也不导致原内容丢失；不应声称新 inspect 能恢复所有旧 edit。强模式不得为兼容而放宽此门禁。
2. **稀疏 asset 回执与空 Task text 已加固。** 初读的 ETS asset `forEach` 会跳过数组 hole，length 仍可能与 pins 一致；真实 native JSON 不生成 hole，但控制注入的 receiver fixture 可以暴露不完整 inventory。实现代理已改逐 index 检查、拒 null/hole 与空 Task text。下述冻结源已独立重读这些修改；不是将注入 fixture 误标成 native JSON 数据丢失事件。

本次没有独立执行正在编写的测试套件。读取测试源码确认其覆盖原 wire 换字节、publication/基线改动、current 推进、copied marker、absent/Unknown、历史恢复和 client owner 变更；controlled ETS hash/receipt fixtures 只能说明 coordinator 行为，不能替代 native Store 重建、实际 SDK 编译或设备保存/重启结果。

### ETS 冻结复核

其后实际重读冻结 `EditorBusiness.ets`，SHA256 为 `6469D1F2AF4E2DA6B9066B52917E658BC13ECECB4EF2A4E6B92FBB4AD0D996CF`（26,767 bytes）；对应测试源为 `1875F562443CDC02B319B2E6019B15034A73060E48D3834E0AA06FCB295DB517`（28,548 bytes）。除上述库存加固，完整回复 DTO 在异步 source digest 前已经独立 deep copy，避免发送方在等待 hash 时改变已核对身份/source；committed/absent 的 effect、receipt revision 与 success error 字段也执行一致性检查。此冻结模块复核没有新增 blocker。

实际读取并核 hash 的 [stage1 模型日志](editor-business-model-stage1-tests.log) 为 **22/22 PASS、0 fail、0 skip，3143.2051ms**，日志 SHA256 `3B6BD3C1A054379A7AD9211BD8777C373AD3589E62A8EAD933418B18A791CC54`。这是实现代理运行的实际 ETS 模块测试，本审查未重新执行；其 hash/receipt fixtures 受控，不是 native Store 或 SDK/device 资格。其后若追加跨实际 native DTO 的检查，应另保留结果，不能改写此 stage1 身份。

## 首轮实际读取身份

| 候选输入 | SHA256 |
| --- | --- |
| `rust/src/editor_business.rs` | `1E16FDAD4E99105D535E7284037D7B2159FC9FF03CF6BB7162422ADBD67BE316` |
| `rust/src/editor_business/tests.rs` | `90C6BE53593087996FBEDAE00BA2F3E1504DE91814DC65A6519E742C59D7A6D8` |
| `rust/src/create_todos.rs` | `0F8AFF1BED67AFACF9F61BFECD6F5D3EE92AA3EE39C24C1E554D183471370125` |
| `rust/src/lib.rs` | `F2A3BC120EB4CA5348EF311DC9A8FFF2A2F46A62B78A0732F0CE23ECB33F717F` |
| `entry/src/main/ets/model/EditorBusiness.ets` | `BC37E4465C393E092AAE43DB70D40CC3FF18A49AAA9D19CA6A7F5826E0A29C0B` |
| `tool/editor-business-model.test.cjs` | `983A01BF7E14CDD2CA530361E98F471903B1DA4217AB7AECBF32397C1B5474B4` |

这些首轮身份只界定当时的实际读取，不能冒充以下最终冻结输入。

## 最终冻结源与证据复核

最终只读复核重新读取了 native 的 `historical` / `publication` / `continuation_root` / `project_todos` / `final_properties` / `prepare` / `execute`，以及实际 Store 新增测试、共用 todos helper 和最终 ETS receiver 追加项。重复提交仍先查不可变操作，使用原完整 wire、原 publication/save Slot、原 source 重建 command；已经 committed 的请求在 current 推进/原 raw 退休后不会再次写业务。continued 投影仍保留 label 对应的原 Task payload/ID，known common fields 与 selected assets 全量在单次事务写入，未发现最后冻结时退回逐步中间预算或未知字段整体重编码。

最终完整身份已直接从磁盘读 bytes/SHA256，与 root 提供冻结身份一致：

| 最终输入 | bytes | SHA256 |
| --- | --- | --- |
| `rust/src/editor_business.rs` | 40,865 | `BE299723AF695C603CC8F1C41AFCC621FE0CF7EE94327903CD1EF5B1B86940C1` |
| `rust/src/editor_business/tests.rs` | 39,984 | `AE37C89C01A126CFDA071BDB4468E798157C191C741D34A068412718F890B7EF` |
| `rust/src/create_todos.rs` | 4,661 | `4C03E1118582F8930B75FF534480D92665BFE0D7920B7799860AAF233B0A5615` |
| `rust/src/lib.rs` | 57,367 | `F2A3BC120EB4CA5348EF311DC9A8FFF2A2F46A62B78A0732F0CE23ECB33F717F` |
| `entry/src/main/ets/model/EditorBusiness.ets` | 26,767 | `6469D1F2AF4E2DA6B9066B52917E658BC13ECECB4EF2A4E6B92FBB4AD0D996CF` |
| `tool/editor-business-model.test.cjs` | 30,012 | `27E49FF7E78E72A707F33DB41D3A54065F24CD3EE4BDEC165D6D9F9E6B5FD19C` |

实际读取以下日志，结果不相加为产品覆盖率：

| 限定证据 | 已读取结果 |
| --- | --- |
| [native focused](editor-business-rust-tests.log) | 14 PASS / 0 FAIL / 2 default ignored，2.12s。包含实际 Store 全 common 字段原子修改、原 wire/root/publication/baseline 变动拒绝、current advance 后准确历史重试、最终可容纳成功/最终超限拒绝且 raw 保留、严格旧 edit 兼容限制。 |
| [实际未知字段和附件检查](../../../rust/src/editor_business/tests.rs) | focused 中 strict edit 逐 raw payload 保留 Task20、Asset10、Properties50002、outer Card/BlobRef/Relation 未知字段；重新打开后实际 attachment export bytes 精确相等，再同 wire 重试不增加业务操作。completed Task 的 continued 原件保留另有 pure projector 检查，它不授权借外来 task edit 的 copied marker 接续。 |
| [Store subprocess 故障](editor-business-store-crash-tests.log) | 单独 1 PASS，3.37s；测试源实际遍历 create/continued × 7 边界，共 14 vectors。重开实际 Store 后核全篇旧/新内容和原操作，再同 wire 重试。不是设备 crash 资格。 |
| [Store DTO exporter](editor-business-store-fixture-tests.log) | 单独 1 PASS，0.21s。真实 create/continued 原请求、publication、完整 reply 导出至 [fixture](editor-business-store-fixture.json)，16,708 bytes / SHA256 `7930BBE56C010ED33D80806703B3CFF73D211FD4D631BEFD4681AE7E46D12C70`。 |
| [实际 Dart 对照](create-todos-dart-compare-tests.log) | 单独 1 PASS。实际 [reference](create-todos-reference.json) 共 50 records，Dart3.12.0 / Characters1.4.1 / Unicode16.0.0，capture exit0。这是全文 LF/trim/blank/first-unique 投影对照，不是 Flutter controller/IME 全流程证明。 |
| [完整默认 Rust](editor-business-full-rust-tests.log) | library155 + attachment binary3，共 158 PASS / 0 FAIL；9 default ignored，分别81.45s / 5.51s。上面的 focused 属于其子集；ignored fixture/crash/Dart 对照另有独立结果。 |
| [最终 ETS](editor-business-model-final-tests.log) | 23/23 PASS / 0 FAIL / 0 SKIP，3157.9661ms；SHA256 `8F5CC41A5862A7F6F545B20D8FA69AFF565337CB4769120972596BE3FE2B22A7`。新增项从 actual Store fixture 受控回放 create/continued DTO，核完整原 wire、publication SHA 和历史 source bytes SHA；不是一次真实 transport/device 调用。 |

独立读取 [API26 compile-only 结果](business-sdk/sdk-build-result.json) 与 [构建日志](business-sdk/sdk-build.log)：11.654s SUCCESS，隔离 bundle `dev.morrow.hmos.editorbusinesssdk`。构建前后 verify 均 PASS，311 copied inputs / 5 wrappers，7项实际类型依赖身份一致，`live_source_differences=[]`；两份 verify JSON SHA 相同 `70810DBC9F9F2AAB32C526BEF7C98FEC8423387A8D85CFAB5713E950DCDA87A3`。此证据只限定新增 ETS import/API 实际 SDK 编译；其复制的旧 CPP/native archives 不证明新业务 Rust 已进入产品包。本审查没有启动这次构建。

**最终结论：本次已读取的冻结源码没有发现阻断分支源码检查点推送的真实数据丢失、owner 绕过、历史回执混用或隐式重放问题。** 旧 edit 弱检查兼容限制已加入实际 Store 测试，拒绝时仍保持原 `effect=committed`；不把这种限制隐去，也不为兼容放宽 strict 原请求资格。最后再查产品 Index/Workbench，没有引用此新业务模块或新 action。新业务 source0 raw/pin 交接、条件父退休与 Index 接线仍未实现；现 kind1 raw fork 不可充当业务 rebase 或 child。新 native adoption、产品 HAP、安装、真实业务保存/重启/IME 设备验收和 protected 资格不在本结论中，完整 Flutter/HMOS 目标仍未完成。
