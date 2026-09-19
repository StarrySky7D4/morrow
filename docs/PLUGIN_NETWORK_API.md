# 插件完整网络 API 对接能力

修订日期：2026-09-14。状态：用户要求的完整目标、设计与待实施任务；**完整 guest 网络运行接口尚未实现；新增原生传输原型范围见 [双向 API 节点](PLUGIN_API_NODE.md)**。本文补充 [IO 设计](PLUGIN_IO_DESIGN.md)，取代将 HTTPS GET 首切片当作网络最终交付的理解。test.50 已通过的插件管理验证不构成联网证明。公共 schema 和冻结原件保持不变；后续双向传输原型不等于完整 SDK 已接入。

## 1. 完整交付的含义

第三方开发者应能用 C、C++、Rust 插件完成账户连接、调用第三方 API、上传附件、接收分页及流式结果、处理失败并把结果提交为 Morrow 内容；不要求修改宿主、编写 Dart 插件或借助 MorrowCloud 才能接入一个普通 API。宿主提供统一的网络、凭据、授权、任务、证据和 UI 服务，插件负责服务端协议及业务适配。

HTTPS GET 是内部首个连通性用例，不是最终能力范围，也不足以冻结网络 SDK。常规 HTTP API、认证、上传下载、SSE 和 WebSocket 都进入完整验收。gRPC 和专用签名等使用显式 profile，单独实现和声明平台能力；“完整”不表示自动支持任意互联网协议、任意服务端私有认证或无限资源。

| 能力 | 必须形成的开发者接口 | 完整验收 |
| --- | --- | --- |
| HTTP 请求 | GET、HEAD、POST、PUT、PATCH、DELETE、OPTIONS；相对路径、查询参数、有界自定义头、取消、超时 | 真实服务收到正确方法、顺序与重复参数、正文和目标；不能只测 GET |
| 数据格式 | 原始 bytes、UTF-8/text、JSON、URL encoded form、multipart、二进制、Protobuf；可扩展 media type | JSON API、表单登录、混合字段/文件上传、二进制下载均可运行 |
| HTTP 响应 | 原始 status、重复 headers、可见 trailers、内容类型、正文流、最终 URL/重定向说明 | 4xx/5xx 保留正文；空正文、非 UTF-8、超量、截断、解压失败可区分 |
| 认证与账户 | API Key、Bearer、Basic over TLS、OAuth2 code+PKCE、refresh、适用时 device flow；隔离账户会话 | 连接账户、使用、过期、刷新、退出和撤销；秘密不返回 guest |
| 实际服务接入 | REST、GraphQL over HTTP、可配置 Base URL、服务专有参数及错误解析 | 不为每个供应商改宿主；下一页 URL 与上传 URL 仍受批准范围约束 |
| 大文件 | 有界上传/下载流、进度、临时资源、取消、条件续传；multipart 分块生成 | 超过普通内联上限也能处理，不在 Wasm 内存组装整件 |
| 流式结果 | 任意有界字节流、逐步 UTF-8/NDJSON 辅助、SSE 事件 | POST 发起的流式 API 可边接收边显示、暂停消费、取消和报告部分完成 |
| 实时双向 | WebSocket 文本/二进制、子协议、连接/发送/接收/关闭、背压 | 多消息、分片、断连、慢消费者及主动撤权；不自动重发业务消息 |
| 调度与恢复 | 异步 job、每账号/目标队列、并发/速率预算、退避建议、稳定操作身份 | 429、503、断网、应用退出/重启、授权撤销不造成无界重试或重复外部变更 |
| 开发与 UI | 三语言辅助库、服务适配样例、诊断工具；账户、授权、进度、错误与取消控件 | 同一原包通过实际主应用完成配置→调用→流式更新→核心内容提交 |

## 2. 数据与协议边界

依据 [架构基线](ARCHITECTURE_BASELINE.md)的“不可控第三方接口”例外，第三方 JSON、XML、表单、文本和二进制协议可按其要求发送和接收。JSON 辅助编码/解析位于插件 SDK 或服务适配层，保留深度、字符串和总字节限额；读取 JSON 不是运行任意代码。XML 适配禁用外部实体和外部资源读取，不能绕过文件/网络能力。

自有内部消息继续 Cap’n Proto，自有正式配置、批准、任务恢复、审计、缓存与证据继续 Protobuf＋LZ4；网络原始正文可以作为不透明 bytes/资源引用保存。不得为接 JSON API 增加第二套 JSON 权威内容库，也不要求第三方服务器接受 Morrow 的私有二进制格式。

公共网络契约在首次候选冻结前必须包含请求、响应、作业、流、认证引用、取消与结果状态，不能把 API 固定为 get(url)->text。建议以下模型，名称和字段尚未成为已发布 ABI：

- `EndpointRef`：绑定已批准 origin、路径范围、实际实例/包、平台能力与期限。插件可构造范围内路径和查询，不能构造宿主授权。支持用户批准的自定义 Base URL、本机/局域网服务 profile；普通公网批准不隐含这些目标。
- `RequestSpec`：规范化 method、目标引用、路径/查询、header 多值序列、body 来源、认证/会话引用、超时/预算、重试建议和可选远端幂等键。
- `BodySource`：empty、bounded inline bytes、受控上传资源或有界 producer stream；长度可未知，以明确 finish 和背压交付，区分空正文与尚未完成。资源和网络出口分别授权，读取卡片不自动等于允许上传其内容。
- `ResponseHead`：HTTP status、可见 header 多值序列、协商协议及明确的可观测字段标志；可见性受平台限制时不能填造原始信息。
- `NetworkJobRef`、`StreamRef`、`SocketRef`：由宿主创建并绑定真实连接、代次和预算，不可从持久记录恢复为有效权限。字节偏移和事件序号使用 UInt64。
- `JobEvent`：queued、request-started、upload-progress、response-head、body-available、completed、failed、cancelled；说明本地阶段与可能的远端效果，不把 delivered/sent 当作远端已提交。
- `NetworkOutcome`：传输状态、HTTP status、业务结果和证据状态分开。HTTP 200 不保证 GraphQL/业务成功，HTTP 4xx/5xx 不伪装成无法联网；连接失败也不能假设远端没执行。

请求头与响应头保留允许范围内的重复值和顺序，不使用会覆盖同名字段的普通 Map。Host、Content-Length、Connection 等由后端管理；Cookie/Authorization/签名等经目标绑定的凭据服务注入。插件可设置获准的自定义业务 header，包括供应商的版本头和幂等头；不要因只支持固定白名单而迫使普通 API 改宿主。[HTTP 语义](https://www.rfc-editor.org/rfc/rfc9110.html)

GET/HEAD 正文不进入基础 profile；扩展方法需新声明，未知方法/必要字段明确拒绝。CONNECT/TRACE 与任意 socket 不因“完整 HTTP API”自动开放。HTTP/1.1、HTTP/2、后续 HTTP/3 属后端协商能力，不让 guest 操作底层帧。

## 3. 认证、凭据和服务适配

凭据应保存在宿主平台凭据存储。插件取得受限 `CredentialRef`/`AccountRef` 并请求宿主调用，不能读回秘密；持久授权、账号元信息按正式二进制格式保存。隔离键至少含用户、插件、账号、服务目标和凭据类型，不能用一个全局 cookie jar 或 token 覆盖多个账号。

| 方式 | 宿主职责与验收 |
| --- | --- |
| API Key / Bearer / Basic | 用户在宿主凭据控件录入；按已批准 header/query 位置注入。query key 和预签名 URL 在 UI/日志脱敏；Basic 只在获准 TLS 目标使用 |
| OAuth2 code＋PKCE | 宿主启动系统浏览器，绑定 state、PKCE、issuer、redirect 与一次性登录会话，校验回调后保存 token；插件不采集用户密码。采用当前 OAuth 安全实践及原生应用授权方式。[OAuth 安全实践](https://www.rfc-editor.org/rfc/rfc9700.html)、[原生 OAuth](https://www.rfc-editor.org/rfc/rfc8252.html) |
| 刷新与退出 | 按账号串行合并 refresh，避免并发刷新破坏 token 轮换；失效转待重新授权。刷新成功不意味着可自动重放原业务请求；退出取消绑定 job 并废止后续注入 |
| Device flow | 服务支持时按返回 interval、slow_down、expiry 有界轮询；取消即停止。不把它作为有浏览器平台唯一入口。[Device Authorization](https://www.rfc-editor.org/info/rfc8628/) |
| Client credentials | 只使用用户/受信任服务配置的机密凭据；分发到桌面/Web 的插件不内置“可保密”的 client secret。需要服务器代管时使用用户选择的独立服务，不默认依赖 MorrowCloud |
| Cookie 会话 | 单独授权隔离 jar，宿主解析/保存/发送 cookie；插件可使用会话但不能枚举其他服务或宿主浏览器 cookie；Web 能力另行报告 |
| HMAC / OAuth1 / 自定义签名 | 版本化受限 signer profile 对获准目标、方法、规范化请求及 body 摘要签名，并由宿主发送；不能提供可签任意字节或导出秘密的通用接口。缺 profile 明确报告不支持 |
| mTLS / 企业 CA / 代理 | 作为独立平台配置 profile，由用户选择证书/信任/代理，guest 不关闭 TLS 验证、不读取私钥；底层能力及代理解析边界需独立验证 |

服务适配可描述认证服务器、token 服务器和资源服务器；它们分别校验范围，不把同一账号理解为可向任意域发送 token。Discovery、下一页 URL、文件下载/CDN、预签名上传链接均为不可信输入，扩张目标范围需要宿主决策；不能给新目标复制原站认证头。

JSON API、GraphQL、服务错误结构、分页字段和业务重试规则由插件类型化适配。宿主提供可复用的 pagination iterator、backoff 与诊断元数据，但不猜测任意 JSON 的 next/error 字段。OpenAPI 可作为开发期导入生成 C/C++/Rust 服务代码与声明建议；生成后仍走同一能力接口，不能运行期动态安装 schema 或自动批准 servers 列表。[OpenAPI 规范](https://spec.openapis.org/oas/latest.html)

## 4. 上传、下载与流式 API

上传来源是已批准文件资源、经过出口授权的卡片附件或 guest 提供的有界数据流。multipart 支持重复字段、文件名/媒体类型与二进制资源；边界和转义由 SDK/宿主辅助实现，不让不可信文件名注入协议头。结束后请求已发出但结果未知时保留操作身份，不能重新选文件就当作新安全重试。

下载先进入受限临时资源或流消费者，正文完整性与长度检查后再经文件写接口/核心内容 API 提交。部分正文不能冒充完整附件；保存到用户路径仍需文件授权。续传需检查 Range/If-Range、ETag/资源身份和实际返回范围；服务器忽略 Range 返回 200 时不能盲目追加。上传断点协议由具体服务适配，不承诺所有 POST 可续传。

将 IO 设计中的 16 MiB/30 秒保留为**初期普通请求策略**，不能冻结成完整网络 API 的全局硬上限。大型上传下载和长连接使用单独批准的大小、持续时间、并发、缓冲/磁盘配额与租约策略；长连接可在原授权范围内续期，但仍有每窗口/每连接/全宿主预算。流消费者慢时暂停生产或明确超额终止，不能无界缓存。

SSE 包含标准的 event、data、id、retry、注释与多行拼接，处理跨块 UTF-8、CR/LF 和空行边界；事件不得按每个网络 chunk 错切。`Last-Event-ID` 仅在明确重连策略下使用，记录每次连接，不保证恰好一次。通用字节流解码同时支持 POST+认证头产生的 text/event-stream，不能只包浏览器 GET EventSource。[SSE 标准](https://html.spec.whatwg.org/multipage/server-sent-events.html)

NDJSON/流式 JSON 不等同 SSE；SDK 提供独立有界解码器，保留非法/截断事件错误和已交付片段。流式 UI 更新是即时展示，正式内容保存仍是核心命令；可取消时明确已显示、已保存和远端可能继续执行的范围。

WebSocket API 必须覆盖文本与二进制消息、子协议、队列水位、发送接受状态、接收、关闭码、消息限额与异常断开。原生后端按适用协议管理分片与控制帧；浏览器未暴露的握手 header、ping/pong 控制不能伪造成已支持。重连后创建新连接代次，业务消息不自动重发；会话恢复、ACK/去重是服务协议的责任。[WebSocket 协议](https://www.rfc-editor.org/rfc/rfc6455.html)

## 5. 异步生命周期、错误与外部效果

所有网络等待交给实际异步后端；Wasm 导入仅提交/查询/读取已就绪结果。guest fuel 不能取消阻塞宿主调用，因此不能把完整客户端塞进普通同步 exchange。需要可持续的 IO 作业上下文或显式 continuation 任务语义：poll Pending 后把执行权还给宿主，由事件唤醒后续调用；不能通过在一个 guest 调用内忙等耗尽 fuel，也不能把重新执行整个任务当作恢复。

必须冻结以下不变量：真实实例身份与权限绑定、请求摘要关联、流序号、一次终态、取消后拒绝迟到交付、升级/停用/退出后的撤权、资源回收和状态查询。异步队列/线程实现、缓冲大小、调度器和 HTTP 库不属于公共 ABI。

错误至少区分授权拒绝、平台不支持、非法请求、DNS/连接/TLS、连接/首字节/空闲/总超时、预算、响应截断、解码、HTTP 错误、服务错误、用户取消和远端效果未知；保留已发送/已收到/已交付的有界观察。错误文案不泄露凭据、原敏感 URL 或服务器未审查的巨大正文。

处理 429/503 可给出 Retry-After、限流状态和重试建议；自动重试要同时满足用户策略、服务契约、尝试/总时间预算与可重播正文。相同本地 operationId 不等于远端幂等；只有服务支持相应幂等键/查询时才能据此恢复。认证刷新、redirect、SSE reconnect 和 SDK retry 都计入同一请求链预算，不得层层相乘。

所有方法都记录可能外部效果，不能仅按 GET/POST 判定远端副作用。具有效果的请求先保存稳定意图和必要录制预算，再发出，最后记录观察结果；崩溃窗口使用 OutcomeUnknown。已经发出的请求无法因本地取消保证远端回滚，也不能用本地数据库事务包装成“恰好一次”网络调用。

审计记录 method、脱敏目标、批准/账号说明、时序、请求/响应实际内容引用、阶段和字节预算。需要保密的原件按独立策略加密或不录制；不录制要明示重放不完整。重放读固定记录，不发真实网络请求、不刷新真实账号、不重连 WebSocket；SSE/WS 按已录事件顺序重放，缺失原件或不匹配立即报告。

## 6. 平台与服务边界

| 后端 | 完整能力实现要求 | 不能承诺的等价能力 |
| --- | --- | --- |
| Windows / macOS / Linux | HTTP/TLS、账户保护、异步流和 WS 各自跑实际测试；公网/本机服务/代理为独立 profile | Windows 测试不能代替其他系统；DNS/路径/凭据行为不同 |
| Android / iOS / iPadOS | 网络任务接生命周期与系统限制，后台能力单独声明，文件用平台授权资源 | 应用退后台/被杀后无限保持连接；移动沙箱内凭据不可直接视为桌面文件 |
| Web 直接调用 | Fetch/Streams/AbortController、CORS 允许的 header/body；浏览器 WebSocket 和适用 OAuth 路径 | 原生 DNS/IP 钉定、任意 header/cookie/证书/代理、全部响应头、原生 WebSocket 鉴权头；不能声称 CORS 确保请求无副作用。[Fetch 标准](https://fetch.spec.whatwg.org/) |
| 用户选择的网关 | 服务器提供明确的代理/账户隔离和证据路径；用户了解目标与数据处理范围 | 静默上传全部请求、秘密或资料到 MorrowCloud；网关不属于全平台核心必需依赖 |

Web 无法满足批准策略时拒绝或提示需要原生版/用户配置的网关；不能关闭浏览器策略来补兼容。能力发现必须能细分 streaming upload、可见 headers、认证类型、WS 子协议和取消语义，开发者才能给出可用交互。

gRPC 另设 profile：原生后端覆盖 unary、server/client/bidirectional streaming、metadata、trailers/status 和 deadline；浏览器使用明确的 gRPC-Web 适配及实际模式限制。能发送 Protobuf bytes 不等于支持 gRPC，HTTP 200 也不代替 gRPC status。按后端分别列退出门槛，不宣称与普通 Fetch 等价。[gRPC 生命周期](https://grpc.io/docs/what-is-grpc/core-concepts/)、[gRPC-Web](https://grpc.io/docs/platforms/web/basics/)

用户进一步要求 Morrow 自己作为 API 节点。入站服务、webhook 接收、服务发布与节点协作现已纳入 [双向 API 节点及 NODE-1–NODE-7](PLUGIN_API_NODE.md)，与本文客户端能力并行；公网监听仍需单独配置与验证，不在普通插件启用时隐式开放。任意 TCP/UDP、VPN、邮件/数据库驱动等不混入 HTTP/实时 API SDK 的完成声明，可以后用独立能力模块扩展。

## 7. 执行任务与退出门槛

以下是完整工作包，尚未整项验收；NET-1的有界HTTP出站与NET-6的持久发送分类已形成 [托管HTTP子集](PLUGIN_MANAGED_HTTP.md)，不包含账户/流/三语言新接口或主应用。编号是工作包，不绑定某个 test.x 发布次数。IO-1 的 GET 跑通只能标内部进展。网络完整交付至少要求 NET-1 到 NET-7 在声明平台达到相应验收；NET-8 以独立 profile 验收并准确披露，不能借“不属于基础 profile”声称任意网络 API 已兼容。

| 任务 | 依赖 | 交付与必须通过的证据 |
| --- | --- | --- |
| NET-1 契约与完整 HTTP | 原 IO 实例/授权底座 | method/多值字段/body/job/stream/错误契约、C/C++/Rust codec 与实际请求；新旧包共存、无增权、异步取消；同时设计持久意图后才能开放变更请求 |
| NET-2 凭据与账户 | NET-1、平台凭据服务 | Key/Bearer/Basic、OAuth code+PKCE/refresh/device、多账号隔离和退出；合成 OAuth 服务覆盖错误 issuer/state、并发刷新和撤权 |
| NET-3 常见服务适配 | NET-1/2 | JSON/form/multipart/raw、REST/GraphQL、分页/限流/自定义 Base URL；三语言完整插件从主应用使用，4xx/5xx/业务错误真实显示 |
| NET-4 流与大型传输 | NET-1/3、文件 IO | 上传下载、背压、取消、条件续传、NDJSON、SSE POST；跨块编码/异常截断/慢消费者/预算及进度一致 |
| NET-5 双向实时 | NET-1/2/4 | WebSocket 文本/二进制/子协议/队列/关闭；无消息自动重发、撤权后无交付、进程终止资源释放 |
| NET-6 恢复和录制重放 | 随 NET-1 开始，覆盖 NET-2–5 | 发出前/后、结果保存前/后崩溃注入；服务器实际副作用计数、Unknown 查询；停服后 HTTP/SSE/WS 录制离线匹配，禁止真实外部效果 |
| NET-7 开发者与平台资格 | NET-1–6 | 同语义三语言 SDK/示例、账户与任务 UI、服务诊断、至少一类真实外部服务适配验收、各平台明确支持矩阵及兼容原件；尚未获准的真实凭据/账号不在开发测试中使用 |
| NET-8 专用 profile | 相应底层能力 | 签名/隔离 Cookie/mTLS/代理/gRPC/开发期 OpenAPI 逐项测试，不把未知配置默认放行；至少签名扩展契约在网络 SDK 稳定前确定 |

默认自动测试用可重现的本机合成 HTTP/TLS/OAuth/SSE/WS 服务，只有测试绑定拥有精确本机授权；生产默认政策不为测试放宽。实际外部服务兼容测试使用明确提供并授权的账号/端点，不产生未经授权的真实写入。最终验收需要真实请求与 UI/内容提交证据，模拟 transport、只验证 codec、只完成设计均不能标为网络 API 已支持。

冻结候选须同时有：C/C++/Rust 二进制固定样例、畸形输入与跨实例/撤权测试、方法/认证/流/恢复的语义矩阵、平台能力发现及迁移规则。HTTP 客户端库、provider 列表、分页解析器与策略阈值可以独立演进；请求关联、权限范围、流顺序和终态不可在不升版本时改义。
