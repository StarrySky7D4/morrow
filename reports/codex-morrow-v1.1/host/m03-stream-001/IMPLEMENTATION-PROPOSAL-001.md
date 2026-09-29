# m03-stream-001：受控本地 HTTP 增量交付方案

状态：2026-09-29 只读源码核查后的接口提案，等待主协调评审。没有创建或修改运行时、Schema、客户端，没有发出 HTTP 请求或运行测试。M02-002 候选继续冻结；其联合最终验收状态不由本文升级。

## 1. 首片目标与选择

通过新版本原生连接，将插件已固定的一个 HTTP 请求交给宿主；宿主只访问本批创建的 `http://127.0.0.1:<随机端口>` 固定端点，POST 正文无凭据、无真实内容。收到响应头后立即提供状态/头，正文在服务器 EOF 前到达插件。宿主只处理字节，不解析 SSE、模型事件或 Responses 语义。插件随后由真实 Codex transport/SSE 消费链解释字节。

推荐：沿用002的批准、独占、撤权与真实进程收尾机制，在新版本增加**显式 HTTP 请求授权扩展、Core 已有 attempt 存储适配、现有 network_node 客户端的流式出口**。不将旧 read-own-session 批准视为网络批准，不建立独立 HTTP 守护服务，不让 guest 自报 URL/approved 获得权限。首片单会话、单 operation、单 attempt，批准的发送次数为1；没有自动重试、重定向、WS、上传流、账户恢复或外部服务。

所有以下新路径均为评审后的落点建议，当前尚未创建：

- `native_session_stream_001/`：从冻结002派生的新宿主版本，增加 native HTTP grant/attempt adapter 与两通道调度；旧 source/kit 不变。
- `network_node_stream_001/`：既有 network_node 传输组件的版本化增量副本/提取模块，仅增加流式出口；保留与原客户端的逐文件来源及差异。核心校验共享在这一新版本内，不能另写放松校验的裸 reqwest 路径。
- `contracts/experimental/agent_host_v3_http_stream/`：唯一宿主权威 Cap'n Proto Schema、生成绑定、限额、向量、manifest。本文不是 schema 冻结。

原 network_node/client 的私有 execute 没有可直接调用的增量出口。将 `send_raw` 返回的完整 Vec 再切块，无法满足目标，明确排除。新组件可回馈既有网络组件的后续集成另行评审，不因首片派生版本通过即声称生产应用已接入。

## 2. 具体复用与缺口

路径均相对实际工作区 `morrow/build/io-safety-refactor`，行号为本次核查。

| 现有代码 | 已有能力 | 本片实际使用 / 所需扩展 |
| --- | --- | --- |
| `network_node/src/client.rs:26,153,212` EndpointPolicy::new / Client::send_raw / execute | 精确 origin/method，禁止危险请求头、代理、自动 retry/redirect/解压；解析并钉住目标地址；检查实际 remote_addr、正文/头/时限/并发 | 复用同一前置校验与连接器构造。增加 `send_stream` → Headers + 逐片 Body + NetworkEof；collect API调用同一底层，避免策略分叉 |
| `network_node/src/client.rs:295-306` | 已使用 bytes_stream，但累加到完整 body Vec 后才返回 | 当前不具备对外流式交付。必须把队列/credit 的等待放到下一次网络 poll 前，不能仅增加回调后继续无限读取 |
| `network_node/src/managed_http.rs:130,305` HttpEndpoint::approve / HttpRouter::prepare | 不透明 endpoint、批准摘要、目标拼接、HTTP请求→Command | 策略与构造次序可复用；其 ManagedInstance/IoBinding 不能用于原生 session，不能伪造 Wasm 实例 |
| `plugin_runtime/src/http_io.rs:75,122,200` HttpGrant::issue / authorize / HttpCallGuard::check | 原 managed owner 校验、撤权、期限、预算、交付前复查 | 作为语义基线；新增 sealed NativeHttpGrant adapter 与002实际 owner/epoch/原批准绑定。不是直接复用这个不可构造的 managed grant |
| `plugin_runtime/src/io_execution.rs:226,479` Broker::begin / claim_dispatch_checked | 活跃 reservation、完整材料校验、先 durable Unknown 后副作用、唯一 claim | 保留其次序和失败分类；Broker与ManagedInstance耦合且结果为整Vec，需要 native adapter 调用下列共享 Core primitive，不能说旧Broker已支持stream |
| `core/src/io.rs:258` Request::encode_http_submit | 规范既有 HTTP 请求字节、method/relative target/headers/body/opaque endpoint | 直接用于内部请求材料和 Command.request_sha256。新IPC外封装与旧IO请求摘要分离，不把新wire digest冒充旧IO digest |
| `core/src/io_intent.rs:87,147,159` Record / Recovery | Prepared→OutcomeUnknown→Observed；Unknown仅ReconcileOnly | 直接复用，不再建一份 JSON attempt journal |
| `core/src/store/io_intent.rs:251,310,323,365` | 后续容量预留、受权append、严格唯一发送claim、历史query | 在002 owner持有的原Core Store上执行；不另开一个同identity Store争抢自己的pin；授权callback绑定native live grant且不重入Store |
| `core/src/store/io_evidence.rs:234,333,367` | 请求/响应原文预留、Protobuf+LZ4材料、摘要查询 | 请求先保留；完整响应可在首片64KiB上限内保留原文后写Observed。部分正文不冒充完整Response，不覆盖Unknown |
| `network_node/src/stored_http.rs:161` StoredHttpEndpoint::resolve | 原Store pin、持久endpoint、固定deadline和依赖撤权 | 端点持久模型可复用；approve路径仍是ManagedInstance，需native live adapter。不读取本机现有endpoint/credential记录，测试Store新建 |
| `native_session_owner_002/src/authority.rs:388,453,566,614` | immutable批准、one-shot claim、独占、撤权、收尾 | 新版本内扩展，不修改002；HTTP worker真实退出及新数据通道EOF加入owner释放条件 |
| `workbench_host/src/http_tasks.rs:100` Workbench::start_http | 产品路径固定submission、registry revision、StoredHttpEndpoint、拥有runtime的worker | 用作产品接入方向和去重语义基线。此片不调用 UI/真实库，也不宣称此入口完成新stream接入 |

## 3. 原生授权适配与唯一状态来源

002批准仅capability1。新版本的宿主批准对象必须另外明确绑定：native grant/issuer/profile/slot/instance generation，plugin/角色/原operation，endpoint reference及其revision/policy SHA、精确method/relative target、规范请求 SHA/字节数、无credential标记、发送预算1、响应/头/排队预算、首次批准的总deadline。

这一扩展需新版本 native批准持久 Protobuf（不是修改002 schema）；guest不能新建、扩权或替换该对象。role/plugin标签仍来自受控host批准输入，未获得生产身份认证。新网络能力必须由新版本明确协商，旧v2客户端不自动升级；普通client001保持原验收用途，新HTTP adapter/driver另建。

唯一归属：native ledger只管批准/owner；既有Core Store的 io_intent/io_evidence 管 operation/dispatch/材料。两个库之间不宣称原子分布式事务：先持有native owner；每次Core受权写入使用同一original live permit；最后一次授权校验与开始副作用串行化在原owner控制域，撤权ACK与effect fence有明确顺序。若native record或Core commit任一不确定，停止新发送且保留owner/Unknown；不能用另一个库的“正常”覆盖它。控制调度不能长时间持有数据库锁或正文队列锁。

必须新增的可复用API建议：`HostAuthority::approve_http(fixed_request, endpoint, limits)` → 不可反序列化NativeHttpGrant；`NativeAttempt::prepare(store, grant)`；`claim_send(store, grant)` → 单消费SendTicket；`open_stream(ticket, checked_client, sink)`；`inspect_attempt`为历史只读；`cancel`只限制原许可；`complete_transport`验证network EOF、材料/终态提交、数据交付和worker join各自状态。这些名称是拟定接口，不是现有函数。

## 4. 真实调用链与发送边界

1. 受控host选择本批loopback服务器精确端点，固定完整HTTP输入；批准对象先确定原始期限。native进程通过新版本握手和实际PID/channel绑定，不能通过guest字节补批准。
2. 插件真实 ResponsesClient 构造请求 → 注入HttpTransport::stream → 新native bridge，把同一固定operation/attempt与规范method/target/headers/body提交。插件重试配置为0，并将拒绝/Unknown作为不可自动重发错误传播；不制造401/429/426触发恢复。
3. Host比较固定输入；用旧 `Request::encode_http_submit` 生成内部规范帧，建立 `Command { operation_id, subject, package_sha256, capability=HttpRequest, protocol_sha256=io::schema_digest(), request_sha256, approval_sha256, target_sha256, request_bytes, response_limit }`。新wire schema摘要另绑定到native批准。
4. 在原Store写 Prepared、预留request/response材料与terminal容量、保留规范request原文；所有容量不足或commit未知均在网络前拒绝。
5. `propose_dispatch_boundary` 后仅 `claim_io_dispatch_local_authorized` 的明确Ok允许取到一次SendTicket。先 durable OutcomeUnknown，再 DNS/connect/send。此标记是保守发送边界，不能解释为服务器已收到POST。
6. 在effect fence再次检查原grant、epoch、期限、取消，然后调用经过共同校验的Client增量出口。发送前已提交Unknown但随即崩溃也不自动重发；重复Commit、同attempt、旧operation重新包装为新attempt均拒绝。没有HTTP库retry/fallback。
7. Headers立刻交付，随后正文按credit增量交付。HTTP EOF只证明完整网络响应；在预算内保留完整响应，写材料并提交Observed。4xx/5xx或302也可为Observed response，不是业务成功。客户端ACK证明消费字节，不证明持久存储或业务效果。
8. 网络/IPC中断、取消或超限发生在boundary之后时保留Unknown（可附临时诊断但不声称完整Response）。进程重开只能Inspect/ReconcileOnly，不能Start/Commit重发。首片不实现实际远端业务核对，也不实现stream跨崩溃续读。

## 5. 控制与正文的通道选择

仅把 `select! biased` 放在一条已塞满的OS管道上不足以保证Cancel ACK交付。推荐保留002衍生版本的stdio作为控制通道，另建每会话一条本地**双向数据named pipe**，只承载请求块/响应块和数据握手。取消、Inspect、credit ACK、terminal状态通过控制通道。两个通道均有独立有界reader/writer状态，不共用“等待正文写完”的mutex。

数据pipe由host提前创建，随机名称、first-instance、拒绝远程连接，DACL仅当前测试用户和SYSTEM；名称/一次性channel nonce只经已绑定的控制握手传递。服务端连接后用GetNamedPipeClientProcessId取得实际连接PID，与本次持有句柄的child PID比较，再做schema/session/epoch/grant/channel nonce绑定。不接受guest自报PID；失败即关闭，不能自动接纳另一个连接者。需要小型受审Windows平台封装，若无法证明DACL/peer PID/真实pipe背压，本片不能给“独立控制”通过信用。未宣称同用户强攻击者或句柄委派隔离。

每次正文写采用可取消的非阻塞状态机。取消到达：先停止新HTTP读取/交付许可，触发network CancellationToken，停止/丢弃尚未交付正文；控制ACK不等待数据pipe清空。已写入OS的数据可能仍在接收端缓冲，不能承诺撤回；以accepted offset、写完成offset与取消ack generation明确边界。部分frame取消直接关数据通道，不拼接后续frame。ACK不等于network worker已结束；Release要求worker join、数据通道闭合及child实际exit/stdoutEOF/stderrEOF。

新 `StreamLease` 必须拥有原并发 semaphore permit、原 deadline、guard及 network JoinHandle；返回 ResponseHead 时只借出元数据，不释放permit，也不重新起一个完整timeout。permit至少保持到网络EOF/取消后的worker实际退出及关闭收尾，不能因调用方取走head或丢弃一个临时reader就计为可复用。数据channel或调用方drop触发取消，但由原owner监督join；不能把取消请求当退出。保留原raw response header value bytes，不能经to_str或lossy UTF-8转换。

## 6. 待评审的最小新wire（仅字段草案）

新Capnp目录major/revision由合同冻结时确定；不能给旧schema原位加字段，或在JSON/私有blob里藏网络控制协议。

| 消息 | 必需字段/行为 |
| --- | --- |
| 通用Envelope/Hello | major/revision、schemaSHA、session、instanceEpoch、grant generation、request sequence、capability协商；server challenge沿用真实artifact/config/PID绑定语义 |
| DataChannelOffer/Bind | 本地pipe locator、nonce、用途data、最大frame；原session/epoch/grant匹配，实际PID验证在OS侧 |
| HttpPrepare | operationId + attemptId（批准绑定，不允许guest换ID重试）、endpointRef、policy revision/hash、method、relativeTarget、受限headers、requestBytes、canonicalRequestSHA、bodySHA、deadline剩余显示值、maxResponseBytes |
| RequestChunk/CommitRequest | byte offset、bounded Data；Commit校验完整长度/摘要；重复commit不再send，返回同一历史阶段或拒绝 |
| ResponseHead | status UInt16、头名Text/值Data列表（保留重复项）、已验证actual endpoint/remote address元数据、attempt、phase；3xx以原响应交付，无跟随 |
| BodyChunk | attempt、chunk sequence、absolute offset、bounded Data；不声明SSE事件边界，禁止因UTF-8分片而lossy转换 |
| ReadCredit/ACK | absolute consumed offset、有限credit bytes；单调且不超过已交付字节，不能重放credit扩容；重复ACK幂等不加额外额度 |
| Inspect/Cancel/Close | 独立控制sequence及attempt；不续期、不创建请求；Cancel ACK说明revocation generation/已接受offset/worker仍待结束 |
| Terminal/Release | transportPhase、HTTP EOF事实、received/delivered/acked offsets、error分类、durable phase Prepared/Unknown/Observed/CancelledBeforeDispatch、worker exit/channel closure、releaseConfirmed。Unknown与HTTP status不能混用 |

首片固定数值建议：请求正文≤32KiB，响应正文≤64KiB，响应头≤8KiB/32项，data chunk≤8KiB，IPC frame≤32KiB，正文queue两片，in-flight credit≤16KiB，独立控制queue≤8，单stream/单发送，总期限≤10s且不超过原批准。部分帧等待≤500ms，控制ACK实验目标≤500ms；该数值是测试阈值，不是Windows实时保证。不得通过取消/ACK/poll更新总期限。

内存计量必须分开：队列、pending write、reqwest当前Bytes、完整响应证据镜像。64KiB完整材料镜像是有限证据用途，不得为了存材料先读到EOF才发首片。上游库/OS socket buffer不等于2×8KiB队列；记录可见高水位和暂停poll次数，不伪称全进程内存硬上限。单次库chunk异常大时超限拒绝；实现/测试需核实这可能已发生短暂库分配，不能隐藏在队列指标外。

请求头只允许本地夹具必要的Content-Type/Accept/自定义非授权测试头；Authorization/Cookie/Proxy-*、Host、Content-Length/Transfer-Encoding等guest控制头拒绝。响应头长度、数量、原始值字节有界；不保存cookie，不加载账户信息，首片不启用压缩解码和trailers语义；原始Content-Encoding可报告但不解释为SSE。非法/缺失长度与提前EOF按真实HTTP库结果分类，不能依赖HTTP200当完成。重定向第一跳的状态/Location仍受限返回，第二目标请求计数必须为0。

## 7. 成功和失败的最小证据矩阵

| 场景 | 必需可观察证据 |
| --- | --- |
| 正常分块 / 实际Core消费 | 本批server记录POST一次、发送首片后等待可信harness看到插件首片回执才发剩余/EOF；记录head、first-byte、last-byte、EOF四时刻。不能仅比较同一host日志时间。独立Capnp解码与server字节/插件消费一致 |
| 真实SSE边界拆分 | server把SSE/UTF-8多字节边界拆开；宿主原字节搬运；真实Codex parser在EOF前生成事件；插件最终完成，宿主不识别业务完成标记 |
| 实际数据pipe背压 | 固定peer握手后停止读取数据pipe；继续受控发送足量数据直至实际Write Pending，并同时满足app queue满/credit耗尽；不足以触发就记未触达，不能以nonreading-peer名称冒充背压 |
| 背压中的Cancel/Inspect | 停读数据而继续读控制；独立控制响应在测试预算内；body read/交付停止，network worker实际退出后才Release；未ACK尾部不冒称已撤回 |
| 原批准期限 / revoke | Delay/ACK/Read不续期；同owner实际撤权后新交付拒绝；endpoint-only与session整体撤权分开。控制ACK后的禁止交付边界需精确，不仅看最后一条日志 |
| 400/500/302 | status及受限headers在EOF前可用；HTTP失败不是transport重试，302目的地B收到0次，原A只一次 |
| 提前断流 / POST后断连接 | server已读完整request再断开；Unknown持久存在；插件错误传播，不自动第二次POST；故障后重复Commit/新连接仍不能发 |
| 崩溃在boundary前后 | boundary前失败无HTTP；after-commit-before-send允许保守Unknown但server0；server接受后kill仅本批host，重开Unknown且不自动发送。未完成M02 owner reconciliation时只Inspect，不清除锁来跑重发 |
| 输入/容量负例 | body digest/offset、method/path/header、owner/session/attempt、重复credit、超限/坏frame、pre-send存储容量失败均拒绝；边界前server0 |
| 收尾/兼容 | Cancel不是release；网络join+新channel关闭+child exit+双EOF各有证据。旧001/002所有冻结输入不变，新协议不被旧client误接受；无公网和凭据路径 |

此矩阵是待实现检查，不是已有通过项。首次执行只在固定源码/生成器/合同/host/client/server清单封存后进行，联合工具只观察本批已知PID/handles，不全机盘点。HTTP server是受控夹具，不是产品侧替代后端。

## 8. 评审门与未关闭项目

请先评审三个接口决策：A. native授权通过sealed adapter复用Core attempt primitive，明确不伪造ManagedInstance；B. 新版本提取共同HTTP校验后提供stream出口，旧send_raw不当流式；C. 控制stdio与独立认证data pipe分离。批准接口后再落实新Schema、实现和固定测试工件；目前停止在本文方案。

仍保留：M02生产审批UI/正式CLI认证/安装身份、完整crash恢复、全局owner/强隔离；M03 TLS/公网DNS/代理/WS/认证恢复和无限响应；M04真实用户业务写入；M06任意执行。M02/M03均不能由首片升级为产品完成，旧G0/P02/J00/G1及0/2、84 not_run不变。

参考核实：本地Cargo固定reqwest0.12.28、tokio1.53.1，代码实际调用bytes_stream/retry::never。Tokio有界队列的文档说明满时send等待，不保证独立控制通道，因此另外设计OS数据通道：[Tokio channel](https://docs.rs/tokio/1.53.1/tokio/sync/mpsc/fn.channel.html)。连接PID需OS取证：[GetNamedPipeClientProcessId](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid)。这些文档不是本方案实现或测试的证据。
