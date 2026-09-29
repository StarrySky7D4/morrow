# fixture003 被动故障观察提案

状态：**proposal-only，接口待核对，未实现、未构建、未运行**。日期：2026-09-30，Asia/Shanghai。

本批只新增 `receipts/m03-fixture-003-design-001/` 的设计资料。fixture002、历史回执、harness011 与 v3 Schema 保留原件。宿主工作树只读；没有 HTTP、宿主修改或 Git 提交。本提案不能作为三场景已通过的证据。

目标是让真实 Core 的 ModelClient / Responses / SSE / 原生 HTTP 路径正常消费，再观察自然期限或真实对端故障。fixture003 使用新的 `passive-observe` 模式，场景为 `authority-deadline`、`network-abort`、`pipe-partial-close`；不调用 fixture002 的 hold、reserve hold、release-consumer 或 DataBound 读暂停。实际消费者可以接收期限/故障前已合法交付的 delta，不能撤回该输出或注入人工 ResponseEvent。

具体接口见 [interfaces.md](interfaces.md)，三个完整示例见 [scenario-specs.example.json](scenario-specs.example.json)。`input-manifest.json` 固定本次阅读的输入，`proposal-manifest.json` 固定本批资料，`proposal-check.json` 仅记录资料检查。

## 预算与原期限

每场景另建本机批次、另建 session/attempt、独立一次批准、最多 **1 个 POST**；这轮预算为 **0 POST**。无重试、重发、续期、重新 Prepare/Commit、账户、凭据、代理、公共网络或收费 API。未来实际运行必须由主会话绑定新的宿主/guest/harness 候选，不得将 fixture002 或 harness011 原地改为新场景。

| 项目 | 固定边界 |
| --- | --- |
| 目标 | `http://127.0.0.1:PORT/v1`，仅既有 Responses 路径；无重定向、其他 origin、用户信息或查询参数 |
| HTTP | 一次 POST，request body ≤32768 B、response body ≤65536 B、header ≤8192 B / 32 项；无 Authorization、Cookie、凭据 |
| 消费/缓冲 | Core 事件队列 8，credit 16384 B，命名管道 buffer 1024 B，parser chunk 1024 B；已有 wire/host body chunk 上限保留 |
| 批次 authority | 提议继续使用原批准 lifetime 10000 ms；从原 admission 创建开始计时，不能从 POST、响应头、fault arm 或第一次观测重新计时 |
| guest 原期限 | 保留首次控制读取起点 + 已验证 Challenge.remaining_ms 构造出的唯一 Admission deadline；所有 Operation/driver/parser 等使用该 deadline，后续 frame.remaining_ms 不刷新它 |
| host 原期限 | 原 Parent.deadline 同时用于 HTTP claim、SendContext / LiveGuard、network/write effect fence；保留原持久批准与 generation/provenance |
| 清理观察 | 原 Core cleanup 2000 ms；原 Close/control/evidence 共享一次绝对 500 ms；断连解释等待至多 500 ms 且受原 guest deadline 截止。均不赋予新的业务 IO 权限 |
| 证据 | spec ≤4096 B；128-entry 非阻塞队列、总记录 ≤128、单条 ≤8192 B、events 文件 ≤1 MiB；身份/摘要之外不记凭据或任意事件文本 |

host/guest 使用各自 Instant 时钟。guest 从 Challenge 推导的 deadline 不是跨进程绝对时钟同步证明：首次读可能晚于 host 创建/发送 Challenge。需要分别记录 host 原创建/expiry/remaining 取样时间与 guest 首读/推导期限，不能直接相减两域 elapsed_ns 或宣称两期限完全相等。记录建立后的期限未修改，以及真实先后原因；本批不改期限算法，也不将握手或启动成本从 host TTL 中扣除后重新补给。

任何前置条件未在原期限内成立，单批结果为 `unreached` 并保留日志。不得自动加 TTL、重开请求或改变触发点直到通过。证据溢出/写失败若引发本地取消，也必须记录真实原因，不能签收为自然 deadline 或对端故障。

## 三个场景

### 1. authority-deadline：响应头后不 EOF，等待原期限

1. 本机服务器接收并完整记录唯一 POST，发送真实 200 SSE head、`Content-Length: 8192` 和一个有效 OutputTextDelta 的 SSE 前缀（≤1024 B）。不发送 Completed、`[DONE]` 或余下正文。
2. 服务器保持 socket 打开；整个 HTTP body 保持未完成，直到 host 原 authority 自然到达。没有 operator revoke、人工 Stop 或测试关闭来替代 deadline。
3. guest 正常读帧、解析与消费，不暂停。要求真实 ResponseHead 被接受；有实际 prefix/delta/匹配 credit 时记录这些事实。不能用“server 已 flush”单独证明 guest 已读到 head。
4. 原期限后仅观察既有取消/清理与真实 EOF/退出。服务器最后的关闭必须记录为收尾，发生在 host 自然 expiry 观察之后；不得把它作为故障原因。

可触发接缝在 **服务器保留未完成响应**，无需新协议或 host 故障接口。观测接缝包括原 authority、network Timeout/guard、持久撤权原因、Stop/HttpTerminal 的实际发送/接受、guest 原 latch、credit/Close 和线程回收。

期望 host 自然 expiry / timeout 路径为 reason20；无 HTTP EOF、无 Observed/response material。guest 首因按竞争事实记录：合法 Stop/Denied20 可先记 Deadline，合法 gen2 HttpTerminal20 可先记 HostCancelled，本地计时可先记 Deadline；不能把所有 error20 归一化为 Deadline。如果别的失败先发生，不能签收为“原期限首先关闭业务入口”。

当前宿主 expiry 可清掉/替换待发 HttpTerminal 为 Stop20，并在原 expiry 后放弃 Close ACK；后到 credit/Close 可能没有确认。允许资料完整地证明这些限制，**业务仍为失败/Unknown，清理可为 Err/Unconfirmed**。真实 host/guest join 或 Released 单独证明，不补造 CreditState、RequestClosed、clean ACK。是否足够达到主会话的有限签收标准由主/联合复核确定。

### 2. network-abort：实际传输开始后网络对端异常关闭

1. 发送与场景1相同的 head / 不完整 `Content-Length: 8192` body；只有一个真实 delta，避免 Complete/EOF 被提前解释为正常终局。
2. 本机 harness 从 guest 的非阻塞 `core_event_delivered` 观察到真实 delta，并观察到真实 consumption 的 `parser_yielded_bytes >0`。两个被动witness绑定本批身份/nonce/spec摘要，触发前host未expiry/revoke；host实际network/body帧事实另行关联。这里不要求故障前CreditState已确认非零消费：现有正常credit可累计到8192 B才发送，小于1024 B的前缀可能尚未触发，不能为测试提前发送credit或把缺ACK当作未开始传输。
3. harness 自动关闭服务器已接受的这一 socket，记录实际 sent bytes、声明长度、close 方法和时间。默认采用短 Content-Length body 后 FIN；由于声明正文未完成，这应走 Transport 错误，不能依赖“未声明长度的 EOF”来制造错误。若另选 SO_LINGER/RST 必须另行固定配置并证明，不能把普通 FIN 写成 RST。
4. guest 持续正常读/消费/控制；不以 hold/release 文件或人为 Read 暂停制造前置条件。

触发接缝在 **新 harness 自有 loopback server 的 close-on-witness**，不需要 network library 的人工 Error 返回、不需要新 wire kind。观测接缝包括 server socket 实际关闭、host 的真实 bytes_stream Transport、持久 intent/原因、原控制帧、guest 的取消/数据结束和实际回收。

当前 host 的 Transport 通常映射 reason26，intent Unknown，HTTP EOF=false、response material=false。最终必须以实际网络错误和 ledger 记录为准；不能先锁定 reason19 或对 guest 首因进行归一化。若 expiry 先于 server close/Transport 或没有实际 parser 消费，场景未达到。若实现错误把短正文当作正常 HTTP EOF/Observed，场景失败。

gen2 terminal 可在 guest 数据端完整帧边界断开之前/之后到达。保留原来至多500ms的解释等待、禁止恢复业务交付；允许真实控制解释原断连，不允许用迟到控制清除已成立的 Protocol。最终零 credit/匹配 CreditState、Close 与 RequestClosed 的要求不豁免：可成功清理，也可真实失败/未确认；不把业务 Unknown 改为成功。

### 3. pipe-partial-close：真实完整帧的前缀写出后 data peer 关闭

1. 建立真实 guest/server 连接，真实 DataBound、批准/Commit、ResponseHead 和网络 body。guest 正常 read；保持 control lane 的独立运行。
2. 新 host **测试专用**入口拦截第一份真实 `BodyChunk` 的完整编码帧，保留原 declared/full length、sequence、原帧 SHA、真实 body offset/end。完整帧必须 >12 B；只向该 session 的既有 data pipe issue **原帧前12 B（4 B length + 8 B payload前缀）**。
3. sole data owner 记录这次12 B OS write 的真实 id/ordinal/completion/reap。新 harness 观察 guest 被动 `data_partial_frame_observed`（缓冲12 B、expected=4+declared length>12、prefix SHA 与 host 相同）后，自动给该 owner 一个身份绑定的 close 指令。没有 guest release 文件或暂停开关。
4. owner 在原期限内关闭同一个 data endpoint；取消并实际 reap 自己剩余的 read/write 操作。control lane 按真实既有故障流程继续。不得为了固定首因而抑制合法撤权/终局控制，也不得伪造 HostCancelled 或 clean EOF。

这里必须新增 **host sole-owner 内部故障接口**，不能从外进程抢句柄、另开 pipe 或将 `Command::Write` 的 Vec 简单截短：当前 FrameWrite 会把截短 Vec 看作一份新的“完整帧”，可能错误发出 WriteCompleted/data_frame_sent。入口只能用于新的测试候选，生产/普通路径拒绝启用，接口见 H2。

必须分别记录 prefix 写入完成和**原完整帧未发送完成**；不发原帧 WriteCompleted/data_frame_sent，不重发余下字节，不把12 B wire前缀换算成 HTTP body已交付/peer已消费，也不自动推进完整 body_end。真正 body offset 的 reserved/issued/OS-completed/peer-consumed 维度仍分别保留。

guest 应记录部分 framer 后实际 read EOF/error，保留 `unexpected data EOF/error` 等真实 Protocol/IO 失败和 worker Err；不满足完整帧边界断连的解释豁免。首因可由 NativeFailure 赢，也可由先到的合法 HostCancelled 赢，均记录原始路径；独立 data failure 不能因 aggregate 已为 Unknown 而消失。部分帧不能解析成 BodyChunk、不能作为合法响应字节 credit。

负例观察完成可证明“发现 Protocol 并实际回收”，但请求业务保持失败；data cleanup Err 与 local_data_thread_joined=true 可以同时成立。缺 ACK 或线程仍活跃时记录 Unconfirmed，不能用 owner Released 补全 guest join。

该场景是**故意截断原帧后 peer关闭**，不证明 OS 对原完整 WriteFile 返回过非零短成功 completion，也不完成 pending-write/cancel 的全部临界竞态验证。未观察到12 B部分帧，或 fault触发前expiry/其它错误先发生，均不能签收这一本场景。

## 结果分维度与签收边界

| 维度 | 必须保存的事实 |
| --- | --- |
| 原因 | 真实 `task.audit.first_cancel_reason`；latch 原子 transition、source/stage、合法控制 kind/sequence/generation/code；host 首次持久原因/provenance；不相互覆盖 |
| 业务 | 原 Core terminal、真实 delta/Completed/error、transport drain、aggregate first error、intent、HTTP EOF、material；Unknown 无白名单改写 |
| 协议 | independent sticky control failure、matching ACK、data worker 原结果、framer 部分字节/长度/EOF、原 IO error；即使先有 Unknown 仍记录后续 data失败 |
| 清理 | 实际 zero-credit/匹配 CreditState、RequestClosed 资源事实、request cleanup 返回值、Close 返回值；没有的确认保留为没有 |
| 回收 | 每个 OS id 的 issue/poll/cancel/reap，真实 finished/joined/handle_retained；data、3个control、evidence writer；host network/pipe/server/child 的实际 join/exit/双EOF/Released 分别由外部证据证明 |
| 场景 | guest只报观测完整性；`scenario_reached`/故障因果/有限签收由新 harness 和联合证据判断；每个 guest结果均 `runtime_qualified:false`、`product_accepted:false` |

没有统一的 `gate_closed && stages_complete` 正例标准；不得复用 harness011 的 A/B 断言。新结果分别表达 `expected_fault_observed`、`unreached`、`failed` 或 `unconfirmed`，不与请求业务成功混为一谈。证据不足或真实资源未回收时不能仅凭“预期是失败”判通过。

## 最小实现范围与后续检查

接口核对后才派生独立 `qualification/m03-fixture-003/`、`native/m03-fixture-003/`、`receipts/m03-fixture-003/` 和独立 out 候选；只复制构建必须的 guest源，复用已固定的只读 upstream/contracts。新 Core pass-through observer 和 native诊断钩子仅进入003源，不覆盖002或当前宿主 companion。真实宿主接口/新harness由主会话另行安排，不在插件工作树代改。

后续先做本地无HTTP检查：strict v2 spec拒绝旧模式/多余字段/重放；真实事件观察不hold且不改变交付门；first原因保留全部20/26/Protocol先后路径；部分framer EOF保留worker Err但实际join；缺零credit ACK不clean；observer溢出/退出和500ms共享预算不续期。再冻结003源/锁/构建/本地检查候选，绑定H1/H2/harness的新摘要，由主会话批准/安排每场景单POST运行。这份资料本身没有执行这些检查或授予运行权限。
