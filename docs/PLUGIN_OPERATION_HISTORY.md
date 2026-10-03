# 插件操作历史查询（W13 实验能力）

2026-10-03。本能力只向当前获批的 managed Wasm 插件返回一个既有 HTTP operation 的无正文状态。继续使用原 IO v1 Schema 和 `morrow_io_v1.call`，不增加 ABI、不重建原冻结插件；完整 SDK 仍未冻结。

## 插件发送什么

原 Rust SDK 的 `io::Request::new(call_id, io::Action::QueryOperation { operation_id })`、C 的 `mp_io_request_v1.kind = MP_IO_QUERY_OPERATION` 和 `operation_id` span、C++ 的 `io_request::query_operation(call_id, operation_id)` 都已有此编码。保持请求原始字节，按既有 SDK 回复解码器核验新查询的 call ID、Schema 和请求摘要；不要用原 HTTP submission 的 call ID 或摘要接收查询回复。

operation ID 必须是非空、有效 UTF-8、符合 Store identity 规则的原身份，最长 256 字节。本次通过实际 Wasm import 验证 Rust SDK 编码的请求；没有将三语言历史查询 guest 的实际执行计入资格。旧 SDK 中关于默认 dispatch 为 Unsupported 的注释属于原冻结快照，普通 raw／brokered 路由仍不支持此查询。

## 宿主如何授予只读范围

宿主先对当前插件实例作新的批准并得到 `IoBinding`，再调用 `io_history::OperationHistoryGrant::issue(manager, host, instance, binding, expected_command, now)`。expected command 是宿主持有的完整原操作事实，限定一个 operation ID、原 subject／package digest、协议、请求／target／批准摘要与原配额。它不能从插件自行宣称的操作 ID 推导授权。当前能力要求同一包摘要；跨版本包或新 owner 的恢复策略尚需单独实现和验证。

原 approval SHA 只参与历史事实的相等性核对，绝不是当前批准。Grant 绑定原 manager／host／connection generation、当前实例、当前 HTTP capability 和原绝对截止时间；不能序列化、传给另一 owner 或借此延长权限。应在将 host／instance 移入对应 `IoWorker` 前签发，与 worker 使用同一绑定。

可在新签发且尚未共享的 grant 上设置 `with_live_guard`；它必须是有界、纯、不可重入的宿主检查。探针失败会永久撤销 grant，所有 clone 共享撤销；恢复不能复活旧 grant。当前 grant 的创建不预留新业务资源，也不产生配额退款。

使用 `IoWorker::submit_operation_history(input, grant, timeout)` 执行既有任务。只有这条显式只读路由允许 QueryOperation，没有 HTTP backend、业务 prepare 或 durable dispatch claim。宿主应同时维持 managed task declaration 和实际 IO 预算，查询失败、取消或撤销均不得自动重发原业务操作。

## 状态和隐私

| 核验到的原历史 | 返回 |
| --- | --- |
| 不存在，或初始 ownership 事实不可证明 | NotFound；无法区分其他 subject 的记录 |
| Prepared | Pending |
| Unknown | OutcomeUnknown；不修改原历史、不重放 |
| Cancelled | Cancelled |
| Observed 且原 request／response、摘要、关联均可验证 | 原 status；Completed 带原 HTTP 100–599 状态 |
| 仅人工 reconciliation、原帧缺失／损坏、已有归属下历史损坏 | EvidenceUnavailable |
| 同一 subject 的原 command pins 不匹配 | Conflict |

Completed 表示原传输结果已完成，HTTP 503 仍返回 Completed／503，并不表示远端业务成功。没有足够证据时不能用 200 或 Completed 填空。与另外一个 operation 或 owner 不匹配的 grant／请求直接拒绝，不暴露状态。

所有回复均不含 body、headers、reference 或 capability references，offset 为 0、eof 为 true；严格回复上限 256 字节。Store 在同一只读事务内完成归属、至多三条历史和有界原帧核验，查询不新增 intent／evidence／reservation／outbox，也不成为第二份权威历史。

## 配额与最终交付

查询在查库前同时预留请求开销和回复上界，沿用原 per-job／worker／lease 预算，不重复计数、不退款。实际 import 后、Ready 和最终 read 再取样原授权时钟并检查权限；撤销、过期、manager generation 改变或取消后清除保留回复。此前已交付的结果不能被追溯撤回。

成功匹配实际 guest completion 后，`JobReport.operation_response` 提供类型化状态，`operation_frame` 保留严格校验过的新查询回复。取消或权限失效时二者一同清空；仍需遵守最终 read 的容量限制。历史 OutcomeUnknown 不把本次只读查询报告成新的外部 Unknown effect。

## 当前验证边界

见 [W13 实现与证据](../reports/reconstruction-2026-10-03/windows-operation-history.md)。Windows x64 Release、普通合成 Store、实际 managed Wasm import 已验证；测试中的既有 Broker→合成 HTTP 503→durable Observed→新 Query 链只生成一次原效果。

尚未验证查库中途撤权的独立故障方法、真正换 owner 后重新批准的恢复、三语言新查询 guest、生产 protected owner／GUI、Web／Android／Linux，以及全种类 operation 的统一历史。发现／分页／清理策略、产品授权页面、流／文件／服务操作恢复也不由本接口覆盖。不得据此宣称整个 SDK 已冻结或所有平台可用。
