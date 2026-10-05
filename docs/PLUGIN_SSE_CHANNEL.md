# 原宿主批准的 SSE 通道：W14 实验接口

本文保留 W14 的原生 source 合同与历史验证范围；后续 typed payload 和 Windows 回归分别记录。最新范围见[项目状态](PROJECT_STATUS.md)，完整 SDK26／G04 仍 OPEN。

2026-10-03。这是显式 opt-in 原生可信宿主能力；普通 guest 的 channel 声明不授予网络权限。完整 SDK 尚未冻结，Workbench discovery 中的生产 network backend 仍关闭。本指南覆盖无凭据的有界 POST SSE，不改变旧 channel、SubmitHttp、IO 或基础 SDK 的 ABI、schema、imports 与 pins。

## 接入位置与生命周期

可信原生宿主启用 `network_node_stream_001` 的 `managed-channel` feature，使用 `managed_sse::{Approval, SseSource, EventEnvelope}`。这不是 guest 的 HTTP import，不允许用 `bind_io` 绕过旧 channel＋IO 组合拒绝规则。旧 `Source` 和 `Manager::bind_channel` 调用形式不变。

1. 用当前 Manager、原 HostRuntime、原 ManagedInstance 绑定 `Kind::Events`、非 duplex、具有非空 checkpoint scope 的原 ChannelBroker。宿主选择 scope、预算和原期限，历史 checkpoint 不产生权限。
2. 创建不可变 Approval：精确 POST RawHttpRequest（完整 URL、路径/query、原 header bytes/body）、唯一 operation_id、非零批准 epoch、原绝对 std::Instant deadline、NetworkProfile、HTTP/decoder/编码累计预算。当前批准可收紧原 broker 的期限，不能续期。
3. `SseSource::approve` 或 `approve_with_live_guard` 以期望 selection revision 核验当前 owner／实例／包与原 broker，并且只允许在无 producer、无原 frame 时附着一次。缩窄 probe 必须有界、纯、同步且不可重入；它也在原 ACK 最终 Store guard 中调用，不得获取 queue、Store 或 clock 锁。
4. `source.start(manager, &mut original_host, instance, broker)` 在同一原 Store 保留逻辑请求 material、Prepared 与后续容量预留，严格取得 dispatch claim 后才创建 native producer。输家、错误实例、撤权、Unknown、冲突或不确定提交均不能 POST；这次 start 尝试被消耗，不自动重试。
5. 原 managed invocation 仍通过原 `morrow_channel_v1` Receive／ACK。每个 frame 入队后必须等该原 frame 的真实、精确、已提交消费 ACK 才请求下一事件。首事件允许有界预取；不宣称首个 Receive 前零网络读。不要在 UI 或 Wasm host callback 线程里等待 native completion。
6. 分别读取 `source.completion()/wait_completion`、原 broker snapshot 与实际 reaper。回调 completion 本身不证明 native thread 已 join。只在 HTTP worker 原 receipt 标记 joined、broker CleanupProof::Joined 和所需状态均实际成立后确认相应资源退出。

SourceGrant 持有原 manager／broker／实例／包／selection 及完整 Command 身份。显式 revoke 先撤销批准，再关闭原 queue；guest Control 可以仍保持 active，但 queued Receive、ACK 和最终提交/交付 guard 必须拒绝。普通 EOF 的业务关闭与撤权分离，合法 Closed reply 和 task completion 不因正常 EOF 被丢弃。

## 事件载荷

`ChannelFrame.bytes` 是 `network_node_stream_001/schemas/sse_event.capnp` 的独立 version1 envelope：schema SHA256、data、event、完整 provider ID、hasRetry 与 retry。旧 channel schema 没有新增字段。W14 旧三语言示例仅拥有、摘要和 ACK 整个 bytes；C05 后续新增独立 Rust／C／C++ [typed SSE payload SDK](PLUGIN_SSE_EVENT_SDK.md)，按同一原生 schema 解码 data／event／id／retry，并 ACK 原 Frame 字节。它不改原 channel import，不把 codec 身份当作 required feature 或网络权限；旧 SDK 与旧 guest 原件保留。

provider ID 可以超过旧 cursor 的长度界限，不能截断或拿来授予 resume 权限。当前 cursor 是宿主根据原 source epoch／批准 epoch／request digest／序号／事件 bytes 生成的32字节 opaque digest。retry、ID 和 `[DONE]` 均为惰性元数据/文本，不触发重连、Last-Event-ID、重放、续期或模型完成判定。

解析规则沿用有界 SSE：严格 UTF-8、跨块字符、CR/LF/CRLF、多行 data、空 data 事件；只接受 HTTP200、规定的单一 text/event-stream MIME 和 identity encoding。未结束 block 不在 EOF 作为事件发出。envelope aggregate 输入与编码 bytes 分别受限，元数据、version、schema digest、UTF-8、尾随 bytes 不合法均拒绝。

HTTP 总响应量、decoder 总量与 encoded channel 累计量分别计费；精确 ACK、撤权和 cleanup 不退已收取额度。frame 受原 broker max_frame_bytes 与 envelope64KiB 较小者约束。一个交付 chunk 的视图界限不代表 reqwest/hyper/kernel 所有 backing buffers 的总内存界限。

## 历史、首因与资源事实

| 事实 | 可以证明什么 | 不能推出什么 |
| --- | --- | --- |
| 原 Store strict claim | 本次源取得恰好一次 dispatch 权限 | 已 POST、远端完成、重放许可 |
| 原 ACK journal/checkpoint | 精确原事件 bytes、request/response receipt 和 cursor 被消费提交 | provider/model/远端业务完成、重新批准 |
| HTTP EOF | 原传输 body 到达 EOF | parser 完整、未截断、业务 Observed |
| decoder成功并达到HTTP EOF，且Finish.truncated=false | 有界解析完整结束且未截断 | 远端业务成功、业务 Observed、完整响应 material |
| HTTP Completion.worker_joined | 本层实际 Tokio worker 已 join | hyper私有任务/远端回滚/native producer 已 join |
| broker CleanupProof::Joined | 原 native producer 的实际 JoinHandle 完成 | 业务成功、生产 protected owner Released |
| NoWorker／NoProducer | 实际未创建相应 worker | 虚构的成功 join |

正常 EOF、所有事件 ACK 完成也仍将 streaming HTTP intent 保持 OutcomeUnknown。不能用空 Response 构造 Observed，不能把本地重开历史视为跨进程 crash/恢复资格。SourceGrant 的批准 SHA 中期限 pin 是进程内历史身份，不能转成跨重启活权限。

`Completion.outcome` 保留首个来源/解析错误，SSE 和 transport cleanup outcome 单独返回。source 层授权检查将到期归为 Source(Expired)、显式 close 归为 Source(Closed)、原授权失活归为 Source(Denied)。但 Completion 保留第一实际观察到的错误：限定撤权／Stop 回归接受 Source(Denied) 或 Sse(Transport(Denied))，显式 source.revoke 场景还实际观察到 Sse(Transport(Cancelled))；这些场景均复核原 grant 为 Denied，不把所有取消统一映射成同一首因。cleanup 的 Cancelled/Timeout 不覆盖首因。调用者必须分别处理历史 Unknown、结果交付与资源回收，不能重新换 operation ID 来恢复未知外发。

`Client::send_stream_with_receipt` 在 opening failure 显式返回 NoWorker 或实际 Worker cleanup receipt。旧 `send_stream` 仍返回相同 Error，复用原执行路径，不新增重试或权限。drop lease 只取消，不是 join；drop 一个 SseSource clone 也不是显式 revoke 或业务完成。

## 当前拒绝与待完成项

拒绝 raw Authorization、Cookie、Proxy-Authorization、API-key 和 Last-Event-ID 注入。当前没有 Account/OAuth、凭据租约、signer、自动 cookie 或账号刷新接口。PublicHttps／LoopbackHttp／LoopbackHttps 是明确不同的目的地策略，HTTPS保留正常 hostname/certificate 验证；本阶段只实测普通合成 loopback HTTP，未提供真实 TLS 或公开 API 资格。

旧 IO/WebSocketConnect 仍 Unsupported。后续已有有界原生 WS、changes source 及独立 typed WS／SSE SDK；这些不等于 Workbench 生产 owner／批准页、公开 guest 目录 import 或完整目录产品接入。流上传与 blob 后端、一般异步依赖组合、生产 changes／恢复绑定、完整跨进程恢复和其它平台继续推进，不能把此 bridge 扩称为完整 SDK 已冻结。

具体实际命令、三语言新旧产物区别、三轮原失败与最终134方法见 [W14报告](../reports/reconstruction-2026-10-03/windows-sse-channel.md)。
