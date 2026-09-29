# M03 接口补充002：请求批准、发送边界与数据通道完成

状态：2026-09-29 设计待评审，不是 Schema/运行时 READY。原 IMPLEMENTATION-PROPOSAL-001.md（SHA256 957b58bc8e321849a85996d806e1cda8e669bd0109ffd19fb2d4d33de0df713f）冻结；以下内容优先于其预先固定完整请求、写完成/取消和 parser drop 的简略描述。只读对齐插件 integration-supplement-001.md（SHA256 b0ff63e549684911aafcca7228bad8f034218bc3e29ea7206e88939414e554ff）。独立 network_node_stream_001 的实现已获主协调另行授权；新 native grant/IPC 的运行实现仍待接口评审。

## 1. 真实 Core 动态请求先提案，可信操作方后批准

1. native 会话按原批准期限启动；capability1 只提供控制底座，不产生网络权限。
2. 真实 ModelClient/ResponsesClient 构造 Request 并进入注入 backend 的 stream future。Prepare/RequestChunk 仅有界收集原始 prepared body、method、完整目标和原始头值，不解析/重组 JSON，不进行 DNS/connect/HTTP，也不建立 Core dispatch claim。wire Proposal 不等于 Core io_intent::Prepared。
3. Host 收齐字节、校验 offset/长度，自算随机 proposal_ref、body_sha256 和 transport_request_sha256。后者覆盖域分隔标识、session/epoch/operation、method、精确 URL、有序头名/原始值及 body，所有可变字段带长度，最终编码由唯一新 Schema 冻结。guest 自报摘要只有比较用途，不是批准。
4. trusted harness 只读取得完整提案，独立核对本批 loopback/端口、method/path、所有头、无凭据、合成 model/input/tools/stream；再从 host operator stdin 明确 approve_http(proposal_ref, expected_transport_sha256, budget=1, limits)。不能由“匹配 fixture”自动批准；任何字节改变均使旧批准失效。harness 不写 guest control，不与 guest 共享 writer。
5. HTTP 批准绑定 parent grant/issuer/profile/slot/epoch、同一 operation/attempt、endpoint/policy revision、提案摘要和预算。到期为 min(parent 原始 monotonic deadline, 更短的显式子期限)，审批等待消耗原期限。approve/Commit/Read/ACK/drain 都不续期。
6. backend 等待 Approved/Denied/取消/expiry；Approved 后仅一次 Commit。Host 构造既有 Request::encode_http_submit 原文和 Command，写请求材料、Prepared 及容量预留，再严格唯一 claim 为 Unknown。transport_request_sha256、body_sha256、Core request_sha256 是不同对象，分别记录，不能混用。

允许表必须包含本片真实 Core 必要的非授权头：accept/content-type/originator/session-id/thread-id/x-client-request-id/x-codex-turn-metadata/x-codex-window-id，逐项批准合成值。禁止 Authorization/Cookie/Proxy-* 和 guest Host/Content-Length/Transfer-Encoding 等；不能删除真实 Core 头再声称原请求完整。HTTP 库增加的传输头和排序单独记录。旧004观测 body 为22025字节只能说明该输入可容于32KiB；新请求重新实测。

## 2. SendTicket 与撤权的精确顺序

唯一持久 claim 的明确 Ok 才产生一次 SendTicket；CommitUnknown/重复 claim 均不发送。Unknown 先于首个网络 future 的 poll，所以 Unknown 不证明 POST 已离开本机。ticket 只能由原 owner 的传输 supervisor 消费，不能复制或从磁盘重建，重启只 Inspect/ReconcileOnly。

owner 控制域提供短小串行 effect fence：在一个临界区中检查原 live grant、期限、revocation generation，消费 ticket，并登记一个唯一 in-flight network operation。随后才允许该 operation 进入异步执行。临界区不跨 DNS/connect/body await、不等正文队列、不持数据库事务等网络。

| 顺序/状态 | 精确承诺 |
| --- | --- |
| 本 owner 撤权已应用先于 fence | fence 拒绝；未 poll 网络 future，本 attempt 没有 DNS/connect/send |
| fence 先于撤权 | operation 已获一次执行资格；撤权触发取消，拒绝新 attempt 和新交付，不能证明 POST 未发送，不能撤回 OS/socket 已接收的字节 |
| 外部 host 持久撤权 | 返回 Persisted/ApplicationPending；在原 owner 实际应用前不能称 CancelAccepted，更不能称网络已停止 |
| CancelAccepted/RevokeApplied | owner 已设置取消、更新 generation，拒绝新的 body write issue/网络读取需求；已 issue 的异步 I/O 可在 ACK 后完成或到达 |
| NetworkQuiesced | 此层 network worker future 已结束并 join；不等于远端未执行，不等于所有 HTTP 库内部任务/内核缓冲均已撤销 |
| Released | 另需所有 data overlapped operation 完成回收、通道关闭、child OS exit 和 stdout/stderr EOF，以及持久 owner release 成功 |

DNS/connect await 监听原 cancellation/deadline，阶段切换前复查 guard。check 然后 await 不是 OS 原子发送锁；CancelAccepted 不是“ACK 后绝无网络字节”的保证。库已启动的 I/O 仍可能推进，边界后取消保留 Unknown，禁止重发。

每次数据 WriteFile issue 和 owner 撤权应用也按同一 generation 顺序授权。记录 reserved_offset、issued_offset、os_completed_offset、peer_consumed_offset，四者不同；仅已 issue 的操作可晚完成。接收方丢弃取消 generation 的晚到字节，不能拿晚到数据产出新业务事件。部分 frame 取消关闭数据流，不拼接重用。ACK/Inspect 始终不等 data write。

## 3. 数据通道平台封装与实际背压

计划新增独立小型 Windows 平台封装，受审 unsafe FFI 仅限该封装，外层保持 forbid(unsafe_code)。接口建议 create_private_pipe、accept_bound_child、issue_write、observe_completion、cancel_pending、join_and_close；这些不是已实现函数。

CreateNamedPipeW 使用 FILE_FLAG_FIRST_PIPE_INSTANCE、FILE_FLAG_OVERLAPPED、PIPE_REJECT_REMOTE_CLIENTS，显式当前测试 SID + SYSTEM DACL，非继承句柄；禁止默认安全描述符。一次连接，GetNamedPipeClientProcessId 匹配持有进程句柄的真实 child PID，并校验随机 nonce/session/epoch/schema/grant。不能以名字、guest PID 或同用户 DACL 单独证明身份；不宣称同用户强攻击者或句柄委派隔离。[管道安全](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)、[客户端 PID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid)。

本地锁定 tokio1.53.1/mio1.2.2。已读 mio-1.2.2/src/sys/windows/named_pipe.rs 的 Write 实现：仅将整 buf 排入异步写也可返回 Ok(buf.len())；flush 返回 Ok；Drop 特意不取消写。因此 Tokio try_write/poll_write 的 Ready/Ok 和 Drop 不能分别充当 OS 写完成及取消完成的证据。

封装直接拥有每个 OVERLAPPED、独立 event、不可变 buffer 和句柄；最多一个 pending write、一个 pending read。WriteFile 返回 ERROR_IO_PENDING 后，用 GetOverlappedResult(bWait=false) 的 ERROR_IO_INCOMPLETE 观察仍 pending；只有完成结果记录实际 transferred bytes。CancelIoEx 仅提出取消请求，随后仍必须等待正常/ERROR_OPERATION_ABORTED/其他错误完成，才能释放 OVERLAPPED/buffer/handle。ERROR_NOT_FOUND 不单独证明已回收。[CancelIoEx](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelioex)、[GetOverlappedResult](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-getoverlappedresult)。

背压测试固定请求 inbound/outbound buffer 各1024字节，并用 GetNamedPipeInfo 记录实际值；Windows buffer 参数是建议，可能取整/扩容，不能断言8KiB必然阻塞。[CreateNamedPipe buffer 语义](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea)。测试 peer 完成认证后，以独立控制回执确认暂停 data read，再发实际正文。credit 固定16KiB；8KiB块、至多两片应用队列，pending write 单独计量但占同一 credit reservation，不能再次累计额度。

实际 OS 背压的通过门：有确定 write op id/offset/length、WriteFile pending、已知 peer 停读屏障成立、至少两个相隔25ms的 ERROR_IO_INCOMPLETE 观测且无完成，其间控制 Inspect/Cancel 仍能处理。单个 ERROR_IO_PENDING 可能只是异步调度，不能独立通过。记录实际 buffer/issued/completed/ACK；不要求“队列满、credit耗尽、OS pending”恰好同时成立，三者是不同观测。

若固定16KiB窗口没有触达持续 pending，该项未通过；不偷偷扩大credit、不用假ACK、不换巨型frame。需要另提有明确内存上限的新测试配置评审后重测。真实慢 Core 消费与专用停读 peer 分开验；上游两个1600事件队列不能由16KiB窗口推出已满。

## 4. 模型完成与 HTTP EOF 分账

插件 RequestTask 持有原 operation transport lease。ParserDetached 仅暂停新增需求，由仍存活的业务 supervisor 等真实 Core terminal，原期限和外部取消优先。Core Completed 后允许在同一授权、额度、期限内通用 Read/ACK bounded drain 到 HTTP EOF，尾部不再生成模型事件。Core 错误/无Completed EOF/业务 owner drop 则明确 Cancel；若网络早已完整 EOF，保留真实传输事实，不能用 parser 错误反改。

宿主不识别 SSE/Completed。只有实际 HTTP EOF 和完整响应材料持久化才可能 Observed；模型 Completed 不升级它，HTTP Observed 也不等于模型成功。network worker/permit 生命周期覆盖 drain/取消实际退出，不在 ResponseHead 返回时结束。

## 5. 最小联合可观察事件

每个事件含 run/session/epoch/operation/attempt、单调本地序号/单调时间原点、原 deadline、generation；只写合成测试摘要/计数，不在生产诊断日志泄露正文。不同进程时间不能直接当因果。

| 事件组 | 关键字段与独立证据 |
| --- | --- |
| ProposalComplete / HttpApproved | 三种摘要、原始长度、批准expected值、原deadline；审批前 server连接/POST=0 |
| ClaimCommitted / SendFence / NetworkStarted | durable revision、一次ticket、effect ordinal；NetworkStarted不称ServerAccepted |
| ServerRequestReceived / FirstChunkConsumed | server实收摘要/请求计数；真实Core首delta回执驱动server EOF闸门，不能只看host时间 |
| Head / ChunkReserved / WriteIssued | 状态/头摘要、received/reserved/issued offsets、queue及pending buffer分别高水位 |
| WritePending / WriteCompleted / Ack | OS op id/结果/实际字节、停读屏障、重复pending观测、absolute消费offset；不混同enqueue与完成 |
| RevokePersisted / RevokeApplied / CancelAccepted | 明确pending/application、generation、最后授权write ordinal及各offset |
| ParserDetached / CoreCompleted / DrainStarted | 插件侧所有；host仍只处理通用字节，没有业务完成控制权限 |
| HttpEof / EvidenceObserved / NetworkQuiesced / ChannelClosed / Released | 每项独立事实、错误、join/完成回收/OS exit/双EOF，缺一不假报owner release |

需要的新执行场景：审批前零请求、改proposal拒绝、原期限耗尽；撤权分别赢/输fence及DNS/connect等待；真实首片屏障；持续OS pending时控制可用和取消后回收；Completed先于EOF及parser错误分别收尾。当前没有运行这些IPC/Core全链测试，也没有发布新Schema或native candidate。
