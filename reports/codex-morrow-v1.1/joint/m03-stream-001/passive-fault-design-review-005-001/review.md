# fixture003 / harness005 被动故障前置联审

日期：2026-09-30（Asia/Shanghai）。结论：三个场景的设计可形成限定、可证伪的资格检查；下列谓词及证据要求应在冻结后逐项兑现。**本报告不是新宿主、fixture003 或 harness005 的实现签收，也不安排运行。** 未读取正在修改的005最终代码，未修改实现、旧候选或旧证据，0 HTTP、0 build、0 runtime test、0 Git。

已保存13份阅读输入快照，核对 guest proposal manifest 的5份资料摘要。宿主算法说明来自已固定 lifecycle004 candidate-002 基线及 guest fixture002；不将它们冒充003新实现。未重扫19份历史输入或旧 fixture79项输入。[输入清单](input-manifest.json)及[快照](inputs/host-interface-001.md)固定本次读取内容。

## 已对齐的设计与冻结前要求

旧宿主 seams 设计H0使用 chunked、要求故障前消费 ACK；guest003提案采用 **Content-Length8192 + 短正文 + FIN关闭**，并以真实 delta交付与parser消费见证触发，不要求小前缀达到正常credit/ACK门槛。主协调已确认005采用后者，不提前settle/强发credit、无静默fallback。冻结后应明确旧H0 framing/ACK要求不适用于这一新切片。

主协调也已确认：guest可以累计4B+8B构成12B部分帧，使用真实read id/count和IO重放链；只在各自时钟域比较；ACK不替代owner实际关闭；host将保存完整目标raw_hex并标为未sent、不走普通WriteCompleted。这些是**已选设计，实际落实仍待第二次审查**。

冻结前应补齐或明确以下证据契约：

1. H1 Challenge remaining的取样必须引用计算remaining所用的同一个Instant，或给出实际取样区间；不要把稍后的事件发布时间当取样时间。原deadline值与Parent/SendContext/guard/gate实际引用一致，不能仅回显`expiry_not_renewed=true`。
2. H2保存完整目标frame原字节和12B prefix原字节、真实OS issue/reap及owner实际close事实。close请求ACK、witness matched或supervisor看见marker都不能证明endpoint已关闭。
3. 明确close事件时间的含义。若H1 at_ns是supervisor收到owner事件后的时间，事件只有在**实际close后**发布、且该观测仍早于原D时，才可保守证明close早于D；观测晚于D不能反推实际close也晚于D。需要细分时，导出owner同域deadline/操作区间或真实host-origin操作时间，不跨域减算。
4. 固定plan canonical JSON字节、域分离字符串与execution-config加入方式；仅回显plan SHA不证明其进入批准配置。plan的nonce/spec改变应改变实际批准config，不能只哈希旁路文件。
5. 各场景结果分别保存目标达到、真实原因、业务/协议结果、消费结算及资源回收。证据完整与全资源回收是有限签收前提；cleanup Err、缺ACK或Unknown不能填成clean。

## 通用机械检查

- 固定新host/guest/harness manifest与exe、原schema/锁/输入摘要、完整spec与唯一新profile/op/grant/session/attempt/nonce；一次Prepare/Commit及最多1个POST，无自动重试或重发。nonce为非零64个小写hex字符，不能使用资料中的全零示例。
- G2每条JSONL行必须完整含LF、append-only、容量/字节有界，ordinal连续且唯一；校验mode/scenario/nonce/spec和验证完成的admission identity。Core阶段有多条真实事件时，按event kind/stream/delta ordinal识别目标，不能误用第一条非delta事件。
- marker必须引用真实frames/IO/audit/business事件，并核对数组索引、operation id、byte count、摘要和原阶段；`core_event_observed`、排队、server flush都不能代替`core_event_delivered`与真实parser消费。
- 大uint64 session/epoch保持整数；H2 identity SHA精确使用指定6键顺序、compact UTF-8 JSON，禁止经float或JavaScript Number往返。marker行SHA基于原行字节含LF，不基于重新序列化JSON。
- host-authority、pipe-owner、guest与harness各自单调时钟分别使用。跨域只建立原始事件/消息/实际IO的因果引用，不能直接减时间或宣称期限同步；ns是记录单位，精度未测量。
- 所有失败保留实际first reason、worker结果、sticky data/control failure、Unknown/EOF/material状态。后到20/26/Close25不得覆盖首次持久原因。

## D：原期限自然到达

令H1原点为host admission.created，原deadline offset为D。

| 谓词 | 可核原始证据与判定 |
| --- | --- |
| D1 原deadline绑定 | created_offset=0，D=lifetime_ms×1,000,000；本批原TTL10000；Parent/gate/HTTP SendContext/guard实际引用同一原deadline，后续不修改。guest另外核`guest_deadline_offset=first_read_offset+Challenge.remaining_ms×1,000,000`且仅构造一次；不要求guest offset等于host D。 |
| D2 Challenge真实取样/发送 | 同H1域取样T，raw Challenge的remaining等于`floor(max(D−T,0)/1,000,000)`，或与真实取样区间相容；完整发送事件晚于取样且原期限前，tuple与实际guest接受一致。后续header remaining不刷新guest期限。 |
| D3 场景已开始 | 同一唯一POST，真实200 head已接受、合法未Completed SSE前缀实际delta交付且parser>0。保持Content-Length8192正文未完成，无Completed/[DONE]，允许故障前合法输出，不能事后撤回。 |
| D4 原expiry首因 | H1实际expiry/effect事实在D或之后，requested20、首次持久source2/reason20与真实ledger一致；此前没有人工revoke/stop/H2动作、有效HttpCancel19、pipe错误、Close25或其他先行原因。不是仅看到最终error20。 |
| D5 server没有抢先终止 | 服务器一直保持未完成响应；server收尾由已经收到的真实host expiry事件触发。用harness同域“收到expiry原行→发起server收尾”证明顺序，记录原行/事件引用；不得沿用旧handler的5秒自动返回、早期socket/body timeout或测试关闭去制造原因。 |
| D6 效果截止与终局 | 核原guard和fence拒绝D后新准入效果、关闭ordinal后无新准入写，保留已准入操作的实际completion/reap。迟到completion不是新issue；只记录supervisor观测时间时不能夸称精确OS issue时刻。HTTP EOF/material均false，不把未确认parser消费伪补为peer ACK。 |

guest首因按真实竞争保留：本地原Deadline、Stop/Denied20所致Deadline、或合法gen2 HttpTerminal20所致HostCancelled均分别记录。NativeFailure等其他原因抢先时，不授予“原期限先关闭业务入口”资格；host的自然expiry事实仍可作为竞争分支保存。若guest HttpCancel19先持久化，后来20不能回写；本目标为unreached，而非改期望通过。

原expiry可使宿主发送Stop20后关闭控制端，RequestClosed/消费ACK/Close ACK可能无法送达。必须解码实际发送完整帧及guest接受；排队Stop不算已发送。缺确认、sticky control Unknown、cleanup Err/Unconfirmed保持原样。有限负例签收可证明“自然expiry已观察且资源实际回收”，**不能授予clean-close或产品成功**。

## N：短Content-Length正文后网络FIN关闭

| 谓词 | 可核原始证据与判定 |
| --- | --- |
| N1 真实未完成响应 | 一次POST与已接受的同一连接，200 SSE，Content-Length8192、无Transfer-Encoding；实际成功写出正文0<N<1024，无Completed/[DONE]，声明长度未满足。prefix原字节与host/parser chunk/实际delta摘要关联。 |
| N2 真实消费先于关socket | 完整G2目标delta delivered与parser>0见证、原IO/audit/business引用均有效；harness收到两见证后才对该accepted connection发起close。不是仅观察server flush，不要求尚未到8192门槛的早期CreditState。 |
| N3 固定关闭方法与连接 | 保存accept/POST/close的同一connection标识、实际shutdown/close调用区间与返回值、实际sent bytes；不启用未批准SO_LINGER/RST。wfile/rfile/socket的所有引用最终收尾，不能只调用高层server.shutdown或关闭listener。 |
| N4 原expiry前真实Transport | close前真实host未撤权/未expiry的snapshot，随后同响应体真实network_error/Transport分支、requested26、首次source2/reason26及effect fence事实；保守要求同H1域Transport及关闭业务gate事实早于D，无其他先行原因。受信harness同域close/收行顺序提供应用层因果引用，不用于计算跨域网络延迟。 |
| N5 无成功误报 | network Failed、intent Unknown、HTTP EOF=false/material=false；不能因短正文FIN得到正常EOF/Observed。将普通close-delimited合法EOF、deadline20、RST和partial data Protocol区分。 |

原始socket调用记录能证明本端请求有序关闭；如果没有包级/ETW证据，报告应称“固定方法的短Content-Length对端关闭及实际Transport”，不能声称已抓到wire FIN或RST。若实际使用另种关闭方式或错误类型，保存真实分支，不静默更换方法直到通过。

Close25可在26之后出现，首次26必须保留。guest首因、Core terminal及全帧边界disconnect解释遵循原生产语义，不把26统一映射成CoreFailed，也不清除已经成立的Protocol。最终真实zeroCredit/匹配CreditState、RequestClosed、Close ACK有则核原tuple/seq/gen/分类，无则明确未确认。

## P：真实BodyChunk的12B前缀后pipe关闭

| 谓词 | 可核原始证据与判定 |
| --- | --- |
| P1 计划批准绑定 | qualification feature显式开启、plan one-shot、canonical plan进入原execution config，nonce/spec与actual admission相同；generation1、active grant、原gate开且未到期。普通构建拒绝CLI/action；feature但无plan走普通driver，无环境变量或旁路文件fallback。 |
| P2 原真实frame | host保存首个实际BodyChunk完整raw_hex；离线校验4B LE声明长度+4=full_len>12、Capnp kind/tuple/sequence/body范围/原payload、完整frame SHA。不能用仅SHA或人为构造另一个短帧代替实际编码帧。 |
| P3 单次真实12B OS完成 | prefix=raw_full[:12]、SHA相等；same pipe owner的实际requested12、operation id/ordinal、completion12/errorNone、reap一次。若pending未完成、短成功8B或error，目标12B未达到；不能补发以凑12。原完整frame complete=false。 |
| P4 guest累计真实部分帧 | 从上一完整frame边界开始，重放实际Read issue/completion链，成功字节依序拼成同一prefix；framer buffered12、expected=4+declared_payload=full_len>12且push=None。可4+8、更多合法分段或实际其他id/count；不要求单次Read12。最后read id/count对应真实记录，无完整BodyChunk accept/parser/credit。部分帧尚不能验证完整wire tuple，其marker身份来自已验证admission与同pipe前一完整边界，不能称已认证部分frame envelope。 |
| P5 匹配见证导致实际close | 受信harness先验证完整G2及IO引用，再转交compact projection；host验证plan/真实tuple摘要、prefix已reap、原gate/expiry、唯一marker。owner实际处理matching witness后关闭同一endpoint，保留实际cancel/reap/close及trigger来源；ACK仅接受请求。original expiry、撤权、500ms部分帧timeout或其他失败抢先则unreached，不能事后把其close归给witness。 |
| P6 没有伪整帧与尾部 | 不出现该原frame的普通WriteCompleted/data_frame_sent，不重发tail，不在故障后发新data；physical12B不推进HTTP body issued/OS-completed/peer-consumed/parser。reserved保留真实body范围，网络received按实际记录；不能把body_end当已发送。 |
| P7 独立部分帧失败保留 | 保存guest真实data worker Err与sticky first-data-failure、framer残留、actualEOF/error或control-close-with-partial分支；不使用完整帧边界disconnect豁免。即使HostCancelled先赢、aggregate Unknown，后续真实data failure仍存在。NativeFailure/HostCancelled按真实顺序保留。 |

若control先到，可能先发生`close with partial data frame`，而非再产生guest零字节Read EOF；分别记录该分支与EOF/error分支，不能补造没有发生的IO。只有worker返回Err不算join；必须finished-only实际join并保留Err。如果原部分帧500ms timeout先发生，它是另一个原因，不能签收witness-close目标。

12B成功是**OS按12B请求完整成功**，同时原协议frame被截断；不证明OS对完整frame请求返回非零短成功completion。这一已知缺口仍另待真实证据。

## H1/H2信任边界

H1是观测导出，不能修改期限、原因或生产gate。来自候选源的事实与原始frames/ledger/IO相互核实；布尔宣言或最后error code不足以证明因果。event timestamp可能为观察时间，owner实际操作与发布关系需由冻结源证明。

H2是**有资格作用的受信operator输入**，不是guest协议消息或额外HTTP批准。host无法认证guest文件，marker_sha仅完整行引用；root harness承担G2验证和转交。摘要与execution config提供候选/批次完整性绑定，不能宣传为guest签名、跨进程密码学证明或同用户隔离。

compact输入≤原1024B、nested/outer拒绝未知字段，exact实际uint64 identity canonical SHA、nonce/spec、frame长度/prefix、one-shot/replay/expired/revoked检查均应在进入owner前完成；owner处理时继续检查实时原gate/期限，防止queued command过期。H2独立容量1 typed channel，无任意bytes/path/handle/PID注入；没有见证不延时续期。普通FrameWrite约束不放宽，测试prefix是独立分支，不借普通成功路径推进body进度。

## 结果、回收与第二阶段检查

`expected_fault_observed`只代表预期故障被观察；目标前置条件或首因未达到为`unreached`。伪EOF/Observed、丢失真实data错误、原frame假完成、尾部重发或期限修改为`failed`。原因已达到但证据不完整、线程仍live或缺实际reap/join为`unconfirmed`，不能仅因“预期失败”就签收。

分别核guest data、3个control、evidence writer的work result/finished/joined/retained，host pipe/network、server/handler/controller、child实际wait exit/双EOF及owner Released；cancel/kill请求不是完成证明。ledger独立只读核完整parent/HTTP/owner绑定、首因source/reason、原TTL、budget1及response cap；guest不能证明自己退出，owner Released也不能替代guest join或消费ACK。

第二次审查应核冻结feature/default构建及拒绝负例、本地真实12B pipe回收、H1实际Instant取样、plan/config完整绑定、003pass-through observer没有hold/暂停/改变原门、独立data诊断不清除原错误、共享预算不续期、harness原始IO/frames重放与三场景机械谓词。然后才分别安排单POST运行。旧candidate-001/A010失败与所有旧限定资格保持原判；整体M03/G0/SDK不因本报告升级。

[纯参考谓词](predicate_reference.py)仅对归一化的**合成事实**作31项纯检查，覆盖24个误报变异、大uint64、累计4+8、缺join/观察证据等。frame字节是明确标注的synthetic stub，不是实际Capnp证明；底层事实以后仍须从原始证据抽取并离线解码。[pure-check.json](pure-check.json)不验证producer003/H1/H2/005实现、不启动任何进程/网络、也不授予候选或运行资格。
