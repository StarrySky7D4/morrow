# M03 接入补充001：与宿主接口及主协调要求对齐

状态：设计补充，未实现/构建/测试；原 `integration-design-001.md` 保持原样。
本文件优先于原方案中的数值草案、单数据槽假设、ByteStream Drop 的粗略取消描述，
以及“尚未收到宿主提案”的历史状态。

只读消费的宿主提案：
`morrow/build/io-safety-refactor/reports/codex-morrow-v1.1/host/m03-stream-001/IMPLEMENTATION-PROPOSAL-001.md`，
SHA256 `957b58bc8e321849a85996d806e1cda8e669bd0109ffd19fb2d4d33de0df713f`。
后续新权威 Schema/生成绑定/manifest 仍未冻结，本文不替代它。

## 动态真实请求与显式批准

普通 native 会话只是控制底座。read-own-session/capability1 **不包含网络权限**。
既有宿主 HTTP grant 绑定 Wasm ManagedInstance；不能伪造 Wasm owner 给 native 使用。
采用宿主拟定 sealed NativeHttpGrant，复用原 Core Store 的 io_intent/io_evidence；
native ledger 只管批准/owner，不再复制一套 attempt journal。插件无任何批准能力。

真实调用的顺序为：

1. native 准入后启动真实 Core。Core 构造 Responses Request，EndpointSession 完成
   prepared bytes/auth（此片无auth），调用注入 backend 的 `stream(request)`。
2. backend 提取同一请求的 method、精确URL、保留重复值及顺序语义的原始headers、实际
   prepared body，校验总量，然后通过宿主新合同的 Prepare/RequestChunk 提案。
   这一阶段只有本地有界输入与摘要；无DNS、connect、HTTP、Core dispatch claim。
   **wire Proposal 不等于 Core io_intent::Prepared。**
3. host 收齐并验证实际字节，自算 proposal_ref、body SHA、transport_request SHA；
   trusted harness 从 host 只读查询完整提案，核对本批固定loopback/端口、method/path、
   所有header、无凭据、model/input/tools/stream等合成fixture约束。
   host比对的对象是收到的原始请求，不是 guest 自报摘要，也不重新拼JSON替换正文。
4. trusted harness 经宿主 operator 接口明确 `approve_http(proposal_ref, expected摘要,1次预算)`。
   固定输入变化就拒绝；批准绑定parent session/epoch、原operation、endpoint和原native
   deadline。等待审批持续消耗原期限，不能在 HTTP approve/Commit/Read/ACK 时续期。
5. `stream()` future 在 Approved/Denied/取消/expiry 之间等待；Approved后才发一次Commit，
   经宿主真实live grant校验、原Core材料/意图预留及唯一 Prepared→Unknown claim 后发送。
   获得 ResponseHead 后返回真正的 `StreamResponse`，不等EOF。未知结果只Inspect，不重发。

不能用启动前编造的固定body替代步骤1；也不能把“fixture匹配”默认当作批准。测试harness
是此片明确的可信操作方，不是生产审批UI/正式CLI认证。该调用链可复用冻结Core，业务
模型元数据与prompt本来就是测试输入；并不证明主产品运行时已注入网络backend。

## 对齐后的限制与通道

| 项目 | 本片评审基线 |
| --- | --- |
| 请求正文 | ≤32KiB，实际prepared bytes计数；不得截断或重新编码来压入额度 |
| 响应正文 | ≤64KiB；不采用原草案256KiB |
| 响应headers | ≤8KiB/32项，值按原字节，重复项保留，不用to_str/lossy转换 |
| 错误正文 | ≤4KiB诊断窗口，超限停止；是否有限drain必须显式区分策略和结果 |
| 最大data块 | ≤8KiB，可请求≤1KiB的消费块用于真实UTF-8/SSE断边测试 |
| in-flight credit | ≤16KiB，absolute ACK不能重复累计；host正文queue两片 |
| 通道 | 既有控制stdio + 新宿主创建且认证的named data pipe；不是单pipe biased保证 |
| 总期限 | 受parent native首次期限限制；审批、body提案、ACK、末尾drain都算在内 |

已只读核对冻结004实际 `http_direct` 的 prepared body 长度 **22025字节**，SHA256
`de3dc0d590e17b7aa6e1a584fdf3740f3b7b7911bd3d3796b3b84ab669bf8e3a`，所以32KiB能容纳
这个已观测输入，16KiB不能。新M03请求会换loopback/元数据，长度与SHA必须重新实测；
不能把旧body直接复制到新请求或预先保证所有真实模型请求都小于32KiB。

旧案真实非授权header名包括：accept、content-type、originator、session-id、thread-id、
x-client-request-id、x-codex-turn-metadata、x-codex-window-id。新批准要核对真实Core产生的
合成值并明确允许；不能悄悄删改这些header后声称原始请求完整透传。禁止Authorization、
Cookie、Proxy-*和guest控制Host/Content-Length/Transfer-Encoding；HTTP库可根据已批准
body自行形成传输层长度。headers完整性和HTTP线上的排序/库增加字段要分别记录。

data pipe由host提供唯一新Schema声明的locator/nonce，host用OS获取的连接PID匹配其实际
child句柄，并绑定session/epoch/grant；插件不以自报PID替代。插件只有一个session driver
写native control，只有其data worker写data。**harness只写host进程的operator stdin**，
绝不直接写child control，也不与guest共享writer。ACL/非远程/首次连接/拒绝额外连接等
均按host合同验证；同用户恶意进程/继承句柄强隔离仍是边界。

## 具体consumer接口及终态所有权

以下是插件内部职责，不是第二份wire协议；名称待实现时采用普通Rust类型。

- `HostModelNetworkBackend::stream(Request)`：单请求状态机
  Idle→Proposing→AwaitingApproval→CommitPending→Streaming→Terminal/Unknown。
  同一request task的第二次stream调用拒绝，不新造proposal/attempt来绕过单次提交。
  `execute`/`connect_websocket`一律 Unsupported，不转回默认transport。
- 业务 `RequestTask` 拥有 ModelClientSession、请求CancelToken、driver handle、原截止时间
  和 cleanup receipt future；把真实Core事件送入有界consumer。它独立于UI，UI只是观察者。
- `HostBodyStream: Stream<Item=Result<Bytes,TransportError>>` 只消费原operation的chunk；
  控制Read/absolute ACK，不解析UTF-8/SSE、不制造模型event。交给上游eventsource-stream
  的实际块需记录seq/offset/长度/hash，必要时把一个8KiB host块切成≤1KiB不可变切片；
  ACK不能超过已实际交给parser/或明确discard-drain的offset。
- `cancel_and_wait()` 向driver发幂等取消并await明确cleanup receipt；调用方drop不能执行
  async等待，所以业务RequestTask Drop只发非阻塞取消信号，资源仍由supervisor监督到
  worker结束/channel关闭。截止期限、已接受offset和不确定状态保留，不把Drop当释放。

**必须区分内部 parser结束与用户取消。** 冻结 `process_sse_with_treatment` 处理
ResponseEvent::Completed 后return；其ByteStream可能先于HTTP EOF被drop。若一律把内部
ByteStream Drop立即转Cancel，正常模型完成就可能被错误记为Unknown。也不能等待HTTP EOF
后才把首个模型delta交给调用方。

设计采用业务RequestTask持有transport lease直到分类明确：

| 信号 | 行为与资格 |
| --- | --- |
| 内部ByteStream Drop/ParserDetached | 只报告“解析器不再请求字节”，交业务supervisor与Core终态裁决；不自行认为用户取消/HTTP EOF/已释放。仍有原deadline兜底。 |
| Core真实Completed | 首片立即投递typed Completed；supervisor在原operation/预算/期限内继续通用有界Read/ACK收尾到HTTP EOF，丢弃解析后尾部但保留byte计量/host证据。没有新POST或新授权。 |
| Core错误、无Completed的流结束 | 显式Cancel；如果网络本已EOF，按真实transport终态记账，不能把parser失败改成HTTP未知或成功。模型成功与transport完成分别记录。 |
| 用户取消/业务RequestTask Drop | 立即触发独立控制Cancel，丢弃晚到业务事件；不等待数据pipe释放credit，也不等待UI队列腾空。 |
| HTTP EOF、材料Observed、workerjoin | 与Core Completed分开记录；只有各自事实成立才报告相应阶段。Completed之后drain超限/断线/expiry仍可能Unknown，不能倒推业务/持久完成。 |

内部ParserDetached不得无限等待：由仍存活的业务supervisor收Core terminal或取消；失败、
task死亡、外部取消、原期限到达均进入清理。所有正常路径/错误路径都保留cleanup future。
竞态处理必须固定：parser可能已发送Completed进队列并drop字节流，而Core消费任务尚未
看到Completed。此时只标记Detached并暂停新增数据需求，保留有限未ACK尾部；不能抢先
取消，也不能推断成功后任意继续网络。业务取消、request owner释放及原deadline优先于
任何排队Completed；事件转发等待慢消费者时也要同时监听取消，不能让UI队列锁阻塞控制。
supervisor真正观察Core Completed后才切换到bounded-drain，按同operation消费并ACK剩余
字节避免credit死锁，尾部绝不再产模型事件。Core错误或无Completed的Core EOF立即走
Cancel；若终态迟迟无法分辨，则到原期限保守终止，不无限挂起。独立验证须分别跑
Completed→ParserDetached→稍后HTTP EOF，以及parser error→Cancel两条真实竞态路径。
API/Core各1600项事件队列和eventsource内部缓存保持原样；它们可能预取到总限额，不能把
16KiB传输credit冒充全进程内存上限。慢Core consumer与数据pipe停读是不同测试。

## 全层不自动重试：具体已读路径

| 层 | 风险与首片措施 |
| --- | --- |
| `codex-client/src/retry.rs:93`、EndpointSession telemetry | `0..=max_attempts`；设置0才总共1次。模型provider `request_max_retries=Some(0)`，不只依赖host reqwest no_retry。 |
| Core HTTP `client.rs:1697-1702,1817-1845` | HTTP分支有401 recovery loop。provider无auth/aws/gateway/env/command配置、auth_manager=None；默认provider recovery返回NotConfigured，不能返回Recovered后continue。401仍需真实POST计数1验证。 |
| WS stream `client.rs:1962` | 426被自然转换为FallbackToHttp；新backend不能以假426表达Unsupported。返回不可恢复Unsupported/Build错误，provider.supports_websockets=false。 |
| WS preconnect `client.rs:1547` 与 prewarm `client.rs:2241` | 也是426降级入口。本片不调用preconnect/prewarm，不复用旧synthetic426 backend，不用禁用flag手工伪造成功；如误入，明确拒绝且HTTP计数0。 |
| Core公共 `stream` 与下一session | WireApi仅Responses；禁止WS。业务只创建本次请求一次，不在错误后再次new_session/stream；共享attempt lease拒绝重复提交，caller显式新业务任务不是自动重试。 |
| HTTP状态426 | 在已选HTTP分支保留TransportError::Http(426)，不走WS fallback；实际端点验计数1。400/302/401/429/503也保留真实状态。 |
| provider/业务采样重试 | stream_max_retries=0，不进入完整agent sampling retry loop；这也是“Core API切片”不等于完整agent接入的限制。 |
| adapter/宿主/库 | Prepared body仅一次；Commit未知不重发、不换attempt/连接。host唯一Unknown dispatch claim、no_proxy/no_retry/no_redirect，各有独立证据。ACK重送不能变HTTP重送。 |

原004资格案 `stream_426_natural_fallback_and_next_session` 的真实调用记录包含一次
websocket.connect 后两次http.stream（第二次来自明确next_session），证明这些路径确实
存在；它们当时全是拒绝探针，本轮不复跑、不把那个记录当成功网络证据。

## 本补充增加的真实验证要求

1. 准入后先真实构造请求、Prepare待审批时端点连接/POST均为0；明确批准同一原始提案后
   才一次POST。拒绝、改字节、审批超时、native过期时均不能发送。
2. 端点实际消费的body与Core prepared body、host proposal、host request evidence一致。
   请求总量32KiB负例不得以截断“修复”；header原字节/重复项/400/302透传分类独立验证。
3. 首片-回执-EOF屏障必须由真实Core OutputTextDelta触发；实际parser chunk边界需记录，
   不把server多次write当作管道/TCP必然分段。
4. Completed早于HTTP EOF单独用屏障测试：真实parser已结束，但host lease保持，bounded
   drain在原额度/期限内到EOF；用户取消和drain超限仍准确分类，HTTP材料Observed不是
   Responses业务成功，反之亦然。
5. 用户取消/RequestTask Drop、headers等待取消、慢Core消费、实际data pipe Pending、
   paused parser/未ACK时均检查独立Cancel与cleanup；harness和guest绝不共写control。
6. 400/302/401/426/429/503、收到完整POST后断线、首块后断线、重复Commit/换连接都保存
   真实server计数，没有自动第二次POST。3xx第二地址只作为受控无调用观察点，不新增公网。

仍未完成：新权威合同、认证data pipe、新native grant与stream实现、真实Core成功可执行物、
本轮任何运行验收、生产UI/CLI认证/恢复/全产品接入。当前不启动依赖待定wire的实现。
