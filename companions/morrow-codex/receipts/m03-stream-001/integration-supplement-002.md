# M03 补充002：真实 Core 入口、完成语义与请求清理

2026-09-29。新增说明，原 integration-design-001.md、integration-supplement-001.md 和 source-audit-001.json 保持冻结。本说明是 pre-wire 源码设计边界，不是 native HTTP/Core/SSE 运行验收。

只读对齐 host INTERFACE-SUPPLEMENT-002.md（SHA256 01dcdbd4ceeecfc1dbe0a392e63e221acfe1fd99157aa592284e8d357f9a68f9）和 joint interface-alignment-002.md（SHA256 0a5430c599ef0e3e33ceb5b2dd03184d1e3990d1a300a3c450c784c5179665b3）。唯一新 wire/schema 仍由 host 提供；未冻结草案不生成绑定。

## 真实 Core 完成并不等于整轮成功

固定004源 codex-api/src/sse/responses.rs 的 response.incomplete 分支：reason 为 interrupted 且 response 可反序列化时，实际输出 Completed(end_turn=Some(false))；其他 reason 返回 Stream 错误。新消费者保留 Option<bool> 原值。不能用“所有 incomplete 都错误”的旧简写作为新矩阵预期，也不能把任一 Completed 当成整轮成功。

新独立 crate qualification/m03-stream-001 调用真实 ModelClient::new → with_network_backend → new_session → session.stream，消费真实 ResponseEvent；不自写 SSE parser。Prompt/ModelInfo/metadata 均为固定合成夹具。WebSocket 关闭，request/stream retries 为0，auth manager 为空，provider 不带凭据；每个 Operation 的 backend stream 仅可进入一次。真实401/429/5xx及自然426多入口和后续session的无重复发送仍需将来实际 server 计数验证，本轮不重复旧拒绝套件。

Core typed terminal、transport drain、请求清理和外部session release分别保留；即使 Completed 已观察，随后用户取消/期限耗尽仍使 transport drain 失败并禁止继续读/产出业务事件。若HTTP完整材料已经持久Observed，后续parser/IPC错误不能倒退该历史事实。product_success_claimed固定为false。

## prepared request 和原期限

backend从Core Request.prepare_body_for_send取得实际字节和原始header值，32KiB请求上限；response_limit取Core声明的更低值与64KiB的最小值，进入后续host提案/批准摘要。只允许该批精确127.0.0.1非零端口 /v1/responses POST，禁止凭据、代理及guest framing头。必须先Prepare完整字节，再由trusted operator明确批准，再一次Commit；本地匹配不是授权。唯一deadline来自原native会话，等待批准、读、ACK和Completed后drain不能续期。

头上限8KiB/32项，error诊断4KiB，最大块8KiB、夹具可要求1KiB。成功响应须一个text/event-stream Content-Type；本adapter没有解压层，拒绝非identity Content-Encoding。data credit固定16KiB、host队列2片；Core和API各自的1600事件队列不等于有界字节背压，实际OS pending仍待唯一native实现与独立停读屏障。

## pending read、分类ACK与清理生命周期

ParserDetached只记录parser退出，由RequestTask仲裁；它不是用户取消。真正Completed后，业务owner可在同一操作/原deadline/原额度内接管未ACK尾部，通用Read/ACK到HTTP EOF，尾部不再送模型。用户取消、owner Drop和期限优先。

内部NativeHttpSession.read契约要求session-owned reader持有pending操作和结果。取消一个read等待future只脱离waiter，相同offset的drain接管必须得到同一块，不能丢失或重复dispatch。这是未来adapter必须实现和实测的义务，WireNotReady占位没有实现该能力。

ACK提供absolute offset及parser_yielded、drain_discarded、error_body_consumed三类累计值；parser_yielded仅表示传给真实parser，不表示语义消费。cancel generation之后的晚到丢弃由driver单独记账，不能伪称parser或drain，也不能重新发放credit。wire须校验四类单调和与consumedOffset、issued/completed等各层offset关系。ACK入队与host确认分账。

请求cleanup只等待已启动HTTP worker实际结束及该请求data connect/read/write回收、通道关闭等事实。活着的guest不能等待自身child exit、stdout/stderr EOF或ownerReleased；这些由外部host/operator在guest退出后观察，内部回执session_release可以None。它不自动使请求清理失败，也不证明session已释放。cleanup观察最多2秒，不延长HTTP授权；超时为CleanupUnconfirmed。

wait_cleanup借用oneshot receiver等待，实际Ready之后才take。外部select/timeout取消一次等待后仍可再次取得同一回执；新增单元测试只验证这个本地生命周期，不启动Core/HTTP。first_cancel_reason保留首次取消原因，避免后续错误覆盖用户取消或期限。

## 当前验证边界

本片先交付真实Core依赖编译与必要本地单元测试，结果以新build/test日志为准。可执行入口在wire未就绪时明确返回WireNotReady；没有成功mock transport、没有独立私建Schema、没有真实账户、公网或付费API。host独立stream组件的通过结果不代表本片native/HTTP/Core/SSE已串通。原产品0/2、84not_run、G0/P02/J00 blocked及G1未通过均不升级。
