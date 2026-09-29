# M03 transport-kit-001 限定独立复核

2026-09-29。固定六项stream测试独立执行6/6通过；没有发现阻断该受限传输层候选的实际失败。仅传输层资格，不是新native IPC、真实Core/SSE、系统管道Pending或产品链通过。

## 身份、来源与执行

- handoff SHA256 `f069be46cea410f6e7538c6d217020f77f7d14b79f9d76e6e4b08609d362f727`；manifest `c19503dbb991377fd7df1a66618ab4a19c7a8658fc7e78529349ac5bce16abec`；producer receipt `0c4b74cc41dc39455869d5e8003eca336728a44c52b9b8ec9247cd8f21a28db2`。
- 测试exe `stream-454c3e399a964f0e.exe` SHA256 `47886ab6739357522b8fb2c4da78f1b0d091822048a65861ae755518e6104d54`。副本仅写入本joint新目录，各测试独立进程、精确名称过滤、单线程、无继承凭据环境、独立profile/tmp，实际使用本批loopback。6次均exit0、无超时、无强制终止。
- 每个进程只读取Popen返回的已知OS句柄，独立记录PID、创建FILETIME、实际image路径及文件摘要，等待真实退出。没有全机进程扫描；文件摘要不是加载后image attestation。服务器和客户端是测试exe内部任务，不宣称独立服务器进程/抓包证据。
- 运行前后2311个输入摘要匹配，包括2267个冻结旧输入、kit/source、原target测试exe及交接记录；5个provenance原件同时匹配当前原文件和封存原文。独立重建extraction.patch与交付完全相同。
- 旧10项策略测试仅引用包名变化，断言/夹具原文保持一致；本次不重复执行。新client逐段差异保留origin/method/特殊地址、危险framing头、无proxy/retry/redirect/解压、remote_addr及额度策略；raw输入由HeaderValue::from_bytes严格校验，重复头append。collect调用同一stream出口。
- 源码/lock/binary关联依据生产方构建前后摘要、真实Cargo编译及运行日志中的精确test exe路径、生产方artifact摘要、kit副本和联合执行副本摘要。本次没有独立编译；生产回执未提供完整compiler-artifact JSON依赖图，因此不能把这种关联表述为独立可复现编译/全图构建证明。只读Cargo fingerprint提供附加本地依赖名称信息，也不是独立构建证明。

## 六项实际结果及语义边界

| 测试 | 独立结果及实际覆盖 |
| --- | --- |
| first_chunk_precedes_server_eof_and_permit_survives_head_and_eof | 通过。server发送one后等待真实next_chunk返回才获闸门发送two/EOF；headers后及EOF后finish前新请求均Limit，finish后许可可再次准入。验证raw请求0xff、raw响应0xff及重复响应头；首片因果是transport消费，不是Core delta。 |
| original_deadline_expires_while_waiting_for_demand_and_does_not_renew_at_head | 通过。90ms延迟headers，220ms原绝对deadline，无demand仍到期，返回Timeout并join；过去deadline拒绝。没有把头到达当续期。 |
| cancel_pending_body_read_and_cancelled_next_future_do_not_lose_demand | 通过。首次next_chunk future被30ms timeout取消后，后续调用仍取得原请求的x；之后pending body read收到token取消，返回Cancelled、join且非EOF。 |
| response_limit_streaming_unknown_length_and_small_delivery_chunks | 通过。未知长度chunked 20000字节完整，20001字节Limit；每次交付≤8KiB。并非OS/socket/完整进程8KiB内存上限。 |
| live_guard_revocation_is_checked_even_without_consumer_demand | 通过。无demand时真实guard变Denied，worker终态可见、后续读拒绝且join；这是测试guard，尚非native durable grant/revoke fence。 |
| dropping_head_future_and_lease_cancels_owned_worker_without_false_join_receipt | 通过。headers前future abort与headers后lease drop分别使server端连接读结束，随后许可可再次准入。证明eventual cleanup/许可释放，不生成虚构的显式join receipt。 |

StreamLease.pending把oneshot receiver保存在lease，支持取消next_chunk future后接续；worker每次只响应一个demand，当前Bytes可保留剩余量。SendContext共享原绝对期限，5ms轮询guard并监听token，不因需求或head续期。WorkerExit保留permit直到lease join/丢弃结果；Drop只发取消并放弃handle，不能等同join证明。

## 首次15/16失败的复核

生产首轮日志确为10旧项和5新项通过，drop测试在末尾1秒外层等待处Elapsed，非此前server收尾断言。首轮与最终生产before摘要比较仅tests/stream.rs变化，client/lib/stream实现、Cargo与provenance输入未变。

最终测试先等server任务收尾，再以独立100ms SendContext探测下一请求；Limit表示仍未取得许可，Transport或Timeout表示已越过许可准入。该检查目标是许可释放，而非要求Windows关闭端口必须在1秒内给连接错误，修正合理。源码在try_acquire前也检查期限，因此调度长时间停顿时Timeout单独并非绝对准入证明；本次实际短时运行结合源码与server收尾支持该测试的限定结论，不据此推导精确调度/OS网络回收界限。原始失败保持，未改候选或重试到绿；联合六项各执行一次。

旧测试源码未以独立文件保存在该首轮回执，仅有其摘要与panic定位，故不能重建那次精确代码diff；“仅测试文件变动”可由摘要比较确认，具体修正逻辑依据最终源码与失败位置核对。

## 锁文件差异须显式保留

直接依赖精确版本保持请求中的固定值，但新独立lock并非原lock的纯删除子集。以下7个registry条目版本不在原lock：

| crate | 原lock相关版本 | 新lock版本 |
| --- | --- | --- |
| cc | 1.4.5 | 1.4.7 |
| cfg-if | 1.0.4 | 1.0.5 |
| cpufeatures | 0.2.17、0.3.0 | 新出现0.3.1 |
| find-msvc-tools | 0.1.12 | 0.1.13 |
| smallvec | 1.16.0 | 1.16.1 |
| syn | 2.0.119、3.0.5 | 新出现3.0.6 |
| unicode-ident | 1.0.24 | 1.0.26 |

相同name/version/source的条目checksum没有变化。这是已冻结候选的依赖差异披露，不是本次更新依赖；不修改lock，也不凭版本断言语义等价。最终集成仍须固定实际resolved graph及来源。

## 后续native适配必须承担的边界

- 该层是trusted transport，SendContext::trusted不是guest网络授权，允许的generic headers也不是M03无credential审批白名单。native适配须单独执行授权、单次持久claim/effect fence、头条数及本片32KiB/64KiB等额度。
- Completion.delivered_bytes统计成功发进oneshot，不是parser消费、pipe OS完成或ACK；已经发入oneshot的chunk可能在之后取消时仍被接收。新adapter必须在其交付栅栏/取消generation重查并丢弃晚到业务数据，不能直接以该计数证明撤权后无交付。
- received_bytes在整块累计超限时不纳入该超限chunk；peak_upstream_chunk另记该块大小。它表示接受的计数，不是服务器/网卡精确收字节量；证据应区分超限块的实际观察长度。
- 没有独立验证reqwest内部任务/内核在worker退出后的完整静止；worker_joined仅对应本层JoinHandle。无TLS、公网DNS、跨平台、持续OS pending、native grant、Core/SSE、UI或真实用户数据运行。
- M03完整矩阵仍not_run；本报告新增transport层限定信用，不将它改成Core全链通过。M02 partial，G0/P02/J00 blocked，G1未通过，产品图0/2，原84项not_run，产品通过计数0。
