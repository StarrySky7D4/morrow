# M03 首片接入设计：真实 Core → 宿主 HTTP 增量流

状态：仅设计与只读源码核查，等待宿主合同评审；未实现、未构建、未运行 M03。
本文件不改变 M02-002 的冻结候选或尚待联合签收的状态。

## 结论与复用范围

可以直接以冻结的 `upstream/p02-integration-004/codex-work` 为只读 path dependency，
从真实 `ModelClient` 入口进入真实 `ResponsesClient`、上游 SSE/Responses 解析器。
首片不需要复制或改写 Core，不另写 SSE parser，也不把调用 `ResponsesClient` 单层
测试冒称经过 Core。该副本来自固定上游 `44fe510ce3ee61c8ef623adcbf89b901c73ddd61`，
含已封存的 12 个资格接缝改动；本设计引用的是这个具体集成副本，不是其他旧批次。

可以复用普通 native 连接的宿主启动、私有 stdin/stdout、首次期限、身份绑定、退出与
双 EOF 观察原则，以及受限 I/O 辅助代码作为只读依赖/明确来源。**不能复用 client001
原 exe 来收 HTTP 流**：它只支持 read-own-session 和8种控制消息，内部 Driver 也不是
公开的通用 stream dispatcher。后续须在新版本目录实现会话 driver/网络 adapter，
消费宿主评审后的权威 Capnp 生成绑定；不改 capnp001，不用其 Data/nonce 字段夹带 HTTP，
不另立插件 Schema、JSON guest wire、Net/IO DLL 或代理公网出口。

首片限定：同一台机器上、由本批创建的一个 literal loopback 地址与随机固定端口、一个
`POST /v1/responses` 模拟端点，无 credentials/cookie/proxy/DNS/公网/重定向/工具执行。
模拟端点只代替远端 Responses 服务；HTTP socket、宿主网络执行、Capnp 管道、Core
请求构造和上游流解析都必须真实。是否授予这一有界 HTTP capability 由宿主可信审批决定，
不能由 guest 声称 approved、endpoint、plugin-id 或摘要来产生授权。

## 已核查的真实调用点

下列路径均相对上述冻结副本 `codex-rs/`；精确当前 SHA256 与来源核对记录见同目录
`source-audit-001.json`。行号是此次固定源码的定位，不指代未来版本。

| 位置 | 已有能力与本批接法 |
| --- | --- |
| `core/src/client.rs:480,552,613` | `ModelClient::new`、`with_network_backend(Arc<dyn ModelNetworkBackend>)`、`new_session` 均公开；新入口注入唯一宿主 backend。 |
| `core/src/morrow_network.rs:25,41` | object-safe `execute/stream/connect_websocket`；`ModelHttpTransport::Injected` 原样调用 backend。 |
| `core/src/client.rs:1186,1685,1798,2258` | 注入路径先于默认 transport，restricted feature 缺注入即拒绝；`ModelClientSession::stream` → HTTP 分支 → `ApiResponsesClient::stream_request`。 |
| `codex-api/src/endpoint/responses.rs` | Responses 请求编码、POST、Accept、会话 headers 仍由上游生成；`stream_encoded` 接 `spawn_response_stream`。 |
| `codex-api/src/endpoint/session.rs` | request 经 `into_prepared` 与 auth 后调用 transport.stream；外有 telemetry/retry 包装，不能漏审自动重发。 |
| `http-client/src/transport.rs:28,30,36` | `StreamResponse { status, headers, bytes: BoxStream<Result<Bytes, TransportError>> }` 正是适配目标。 |
| `codex-api/src/sse/responses.rs:37,73,521,528` | 真正 `eventsource_stream` 解析字节、JSON Responses 事件；event channel 容量1600。 |
| `core/src/client.rs:2386,2389,2423` | Core 继续映射真实 `ResponseEvent`，另有1600项事件队列；不是 UI 代码。 |
| `core/src/client_common.rs` | `ResponseStream::Drop` 取消 Core mapper；mapper 丢弃 API stream 后，SSE task 的 `tx_event.closed()` 才能终止字节消费。 |
| `core/src/test_support.rs:216,223` | 公开 `responses_metadata` 可提供合成测试元数据；直接 metadata 构造器是 crate-private，不假设外部可调用。 |
| `codex-client/src/retry.rs:93` | `0..=max_attempts`；本版本设置0才是一共1次发送，不能误用1表示单次。 |

冻结 `core/src/morrow_network_qualification.rs::run` 不能直接复用为成功入口：它固定
fixture.invalid 并把 `Ok(stream)` 标作 `unexpected_successful_stream`，没有消费事件。
冻结 `codex-api/tests/sse_end_to_end.rs` 使用内存 fixture transport，只作事件格式参考；
它不证明真实宿主 HTTP 或增量交付。

## 拟新增的最小插件结构（合同通过之后才实现）

1. 新的无 UI qualification executable 直接依赖同一冻结 Core/API/http-client/protocol
   图及既有 restricted feature。读取的 profile、临时 cwd 和环境由可信本批 harness
   创建；不读取个人 Codex 配置、凭据或真实内容库。独立 Cargo home/lock/target，旧输出
   不作为可写 build target；沿用已锁定 vendor/forks，不静默更新版本或并入宿主 Tokio 图。
2. 创建 `ModelClient`，注入新 `HostModelNetworkBackend`。provider 为确切 loopback URL，
   `supports_websockets=false`，request/stream retries=0，无 auth manager/所有 auth 字段，
   request compression=false，trace disabled，tools 为空，仅短合成 prompt/model。
   `execute` 与 `connect_websocket` 明确 Unsupported；不得转回 Reqwest、WS 或旧 fixture。
3. 新入口直接调用 `ModelClientSession::stream` 并逐项消费返回值，断言真实
   `OutputTextDelta`、`OutputItemDone`、`Completed` 及 usage/response_id；不调用旧 refusal
   probe 再把它的文字结果改成成功。元数据用现成公开 test_support 构造，属于测试输入，
   不冒充完整 agent/UI 配置。
4. backend 保留上游已准备的 method/url/headers/body 字节，使用
   `Request::prepare_body_for_send`/已有 prepared 状态，不重新拼 JSON 或重复压缩。
   宿主返回状态与 headers 后即可构造 `StreamResponse`；正文由按需 `ByteStream` 逐块
   提供 `Bytes`，不能先 `read_to_end`、转完整 String 或等待 HTTP EOF。
5. 将 SSE content-type 策略放在插件 Responses 适配层：成功状态必须是规范化后的
   `text/event-stream`（允许适当参数），缺失/错误类型在解析前取消正文并报错。固定上游
   SSE parser 当前没有执行这个校验。宿主仅传递通用 HTTP 元数据，不理解 Responses。
6. 非2xx必须映射 `TransportError::Http` 并保留 status/允许的诊断 headers，受限错误正文
   达上限即停止读取；不能作为成功 StreamResponse 喂给 SSE。超限、超时、断线、撤权有
   清晰错误类别，不伪装 EOF/Completed；不为了兼容构造假的 reqwest 连接错误。

## 提交宿主的最小接口需求

下表是行为需求，**不是字段号、RPC 编号或另一份 Schema**。消息命名、序列化、大小、
准入位及向后兼容由宿主唯一权威合同决定。本轮尚未收到可消费的新合同。

| 通用操作 | 插件必须得到的语义 |
| --- | --- |
| 打开一次 HTTP 操作 | 宿主固定授权上下文内接收完整 method/目标/headers/原始 prepared body；有 request/operation 身份、单次消费与明确 accepted/未发送/发送结果不确定边界。只有宿主执行 socket。 |
| 响应起始元数据 | status、受限完整 headers、opaque response handle；与原会话/授权代次/操作绑定。收到 headers 不代表 body/业务成功。 |
| 有界 Read/credit | 指定最多字节数，只允许有限 outstanding；返回原始 byte chunk、单调序号/offset，不能按 UTF-8 或 SSE 重切文本。明确无数据、EOF、错误，空块不能无限占用轮询。 |
| ACK | 对确切已交付 chunk/offset 释放传输 credit；拒绝 future/跨流 ACK，duplicate ACK 是否幂等由合同固定。ACK 不授权重发 POST、不续期、不等同 UI 已显示或模型任务完成。 |
| Cancel | 在数据队列满、消费者不 poll、请求未获得 headers、Read 挂起时仍可由控制路径到达；取消回复区分已接受取消与 HTTP body/操作实际关闭。幂等取消不能重启操作。 |
| 终态与释放观察 | 互斥 EOF/error/cancel/revoked/expired；可查询操作关闭结果。native 会话 owner 的退出/双 EOF 与 HTTP body 的结束分别记账，不把 Drop/HTTP EOF 当整个会话释放。 |

首片建议参数供宿主评审：每会话1个 HTTP 操作、每次 Read 不超过1KiB、1个未确认数据块、
请求正文总量16KiB、响应正文总量256KiB、错误诊断正文4KiB、headers 总量8KiB且条数受限。
这些是建议，**不是已接受合同或现有能力**。若单帧无法容纳 request/headers，须由宿主
决定有界分段或新 frame 上限；插件不能越过 capnp001 的2KiB payload限制夹带数据。
总量上限要按实际收到的字节计数，不能只相信 Content-Length；所有阶段共享首次批准的
总期限，idle/per-frame deadline 另设，不把每次 Read/ACK 当授权续期。

宿主需确认取消与数据发送竞争时的处理：已完整发出的最后块可标记已交付；部分写不能
拼接控制 frame 或盲目重发。当前 M02 的部分写/控制饱和没有完整资格，不继承不存在的证明。
若单个 OS pipe 阻塞导致控制无法物理到达，要明确超时关闭/Unknown 的安全结果，而不是
宣称任何情况下都可即时 Cancel。

## 背压、解析缓冲与取消

新会话 driver 持有有界数据槽和独立控制队列；Tokio 业务任务只通过异步 channel 与其
交互，不在 async worker 上直接阻塞 stdio。复用的 ProcessIo 本身是同步接收接口，需有
专用 driver 线程或等效实现；其 worker Drop 不等于 thread join，不能据此宣称清理完成。

ByteStream 至多发出一个未完成 Read。上块已交给 parser 且下一次 poll 需要数据时才
归还对应 credit/发下一次 Read；取消/终态时释放剩余 credit 的规则须在合同中固定。
“ACK”表示适配层不再需要宿主保留该块，不能谎称下游全部释放内存。SSE parser 可将字节
留在 UTF-8/行/event buffer，API 与 Core 各1600项队列又能提前读取；故 **W=1 不等于
UI 慢消费时整个链路只有一个块**。首片用严格累计字节上限控制最坏输入量，记录 reader
credit/缓冲高水位，并分别测“ByteStream 不 poll”和“Core 事件消费者慢”两个场景。
1600项是项数上限，不是按字节内存上限；不能只凭 RSS 或队列存在就宣布全产品有界。

保留上游 eventsource-stream 0.2.3 的实际增量 UTF-8、SSE 边界处理。该依赖会缓存未完成
UTF-8/行/事件；它没有本片可直接配置的每事件字节额度。首片总字节上限是保障的一部分，
不能另写简化 parser 改变语义。若需要更细额度或发现实际解析缺陷，应另提可审查的新
依赖/Core版本，不改此冻结副本。源码现有行为还包括跳过部分坏 JSON 事件，以及
`response.incomplete` 的单独错误处理；验证必须对照真实上游结果，不能自定容错规则。

取消所有权归业务 request/task，不归 UI widget。测试入口显式持有请求取消句柄和 cleanup
receipt future；用户/测试取消先触发请求 token，再 drop Core stream。Core Drop → mapper
退出 → API stream 关闭 → SSE task 退出 → ByteStream Drop 会触发 adapter 的非阻塞取消
信号。driver 继续读控制/终态，显式 await cleanup receipt 后才能报告资源完成；Drop
本身不等待。UI若后续接入仅订阅事件，关闭视图与取消业务任务必须是不同操作。

## 单次 POST 与结果不确定

Core provider 的 request_max_retries/stream_max_retries 均设0；当前 RetryPolicy 的
max_attempts=0仍执行初次请求一次。无 auth manager，不走401 token refresh；禁 WS，禁
redirect，禁止 adapter/host reconnect 自动 Start。业务入口也不放任何采样重试 loop。
唯一模拟端点记录实际 POST 次数和接收 body SHA。在503、429、401、headers前断线、首块后
断线、Cancel、撤权、ACK丢失情况下均核对没有第二个 POST。提交后连接断开表示结果可能
不确定，只报错误/Unknown和已有操作身份，不自动复制请求。ACK 的重送/核对不能变成
HTTP 请求重送；持久安全恢复不在本首片实现范围内。

## 必须真实运行的验收矩阵（当前全部 not_run）

| 场景 | 必须保存的证据与判定 |
| --- | --- |
| 完整成功链 | 真实 Core 构造 POST → host socket → loopback 响应 → Capnp raw chunks → 上游 parser → Core typed events；精确正文、中文/emoji delta、最终 item/usage/Completed一致，POST计数1。 |
| 首片早于 EOF | 服务端发出首个有效 delta 后等待测试控制屏障，只有消费者实际观察到 Core OutputTextDelta 才释放后续响应和EOF；记录跨进程屏障因果与各自单调时钟，不直接比较不同进程时钟原点。 |
| UTF-8 与 SSE 边界 | 中文3字节/emoji4字节跨实际 adapter块，`data:`、JSON escape、CR/LF/空行拆开；一次块含多事件；记录真正进入 parser 的块长度/摘要。TCP write 不等于 read边界，不能只靠sleep声称已拆分。 |
| HTTP 非成功 | 401/429/503/重定向返回受限Http错误，SSE成功事件0，诊断正文有界，POST计数不增加，不读凭据、不跟随Location。 |
| 错误/缺失 Content-Type | 2xx+JSON/缺header 在插件入口拒绝，发送Cancel并观察资源关闭；不能把任意200当SSE。合法参数类型接受。 |
| 早断/未完成事件 | response.completed前EOF、半UTF-8 EOF、半SSE、response.failed/incomplete按上游产生错误；不假Completed，无重试。 |
| 慢 ByteStream 消费者 | 一块未ACK时不再发超额Read、不拉取完整body，取消仍可服务；记录最大outstanding与实际内存/字节额度，不把parser预取当UI消费。 |
| 慢 Core 消费者 | 用真实1600项队列与有限总字节测试预取/背压及取消；不改队列容量做一个更容易通过的假路径。总上限先触发时如实报告，不能宣称已填满队列。 |
| 取消各阶段 | headers前等待、Read挂起、已交首块、queue满/不poll；主动业务取消与drop分别运行，查driver取消确认、host资源终态、endpoint关闭，检查没有晚到token被追加。 |
| 有界与到期/撤权 | body/header/错误body额度、首次期限与idle期限分别触发；未ACK/部分接收下撤权后禁止新Read；旧operation handle跨会话拒绝，不能变成新的POST。 |
| ACK/顺序/方向 | 少量确切合同负例验证duplicate/future/cross-session ACK和重复chunk规则，记录last-delivered/last-acked；不重复已有充分通用codec测试。 |
| 无默认出口 | 缺backend、注入拒绝、WS/execute调用均fail-closed；模拟端点和host记录之外不允许网络目的地，不把未做全机网络审计说成全机隔离。 |

mock endpoint只发协议数据，测试本身不得合成 Capnp 成功回复绕过真实host。上游 parser
fixture/trait unit test 可作为定位补充，不能计为上表真实路径通过。测试材料、HTTP bodies
与 raw frames 均留本批受限目录，标准输出仅协议或受限摘要，禁止个人上下文与secret。

## 实施前待宿主/联合审查的决定

- 新权威 Capnp版本及生成绑定、准入能力、request/response句柄、元数据/正文分段限制。
- Read/ACK credit 的精确责任、EOF与最后ACK顺序、Cancel确认和实际释放的区别、部分写结果。
- 受控loopback目的地/方法/body总量如何进入原批准记录；unknown操作与重新准入是否安全拒绝。
- 不允许自动POST重试的全层配置，以及禁代理/重定向/凭据的真实宿主HTTP执行入口。
- 新driver的取消/任务清理与普通native会话 owner 关闭如何衔接，不能省略M02退出/EOF条件。
- 成功Core新入口的完整resolved graph与源码身份记录；未来实际构建规模可能仍覆盖Core大图，
  本轮没有重编，不能以“依赖可复用”声称已有新可执行物。

当前交付仅上述设计、只读核查与源身份清单。M03实际成功路径、真实endpoint、传输adapter、
新native driver和上述测试均未实现/未运行。M02、G0/P02/J00/G1、产品0/2与84not_run均不升级。
