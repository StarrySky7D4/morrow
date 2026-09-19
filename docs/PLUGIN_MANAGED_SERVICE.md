# IO-D2：原生宿主显式发布受管插件服务

状态：当前隔离工作树已实现独立类型化服务帧、原实例发布／监听批准、远端主体 scope 与原生服务适配；**Windows 本机限定验证通过**，结果见 [本轮报告](../reports/managed-service-2026-09-19.md)。本文描述实现合同，不预报测试通过数量，也不代表公网部署、完整 SDK 或主应用产品接入完成。

这是可信原生宿主显式选择包、实例、处理器、地址和路由后发布服务；不是 guest 自行动态注册路由。完整目标仍见 [API 节点](PLUGIN_API_NODE.md) 和 [网络 API](PLUGIN_NETWORK_API.md)。

## 独立服务契约与包声明

`core/schemas/service.capnp` 是独立的固定 v1 契约，没有改写已有 `io.capnp` 或冻结 SDK 原件。请求携带版本、schema 摘要、call ID，以及 `Invocation { service, handler, principal, method, target, headers, body }`；响应携带版本、schema 摘要、call ID、完整原始请求帧 SHA-256 和 `Reply { status, headers, body }`。摘要采用 schema 的 LF 标准化字节。摘要只证明关联与完整性，不是签名或认证。

`core::service::Request` 保留原始帧；`Response::decode` 同时核对调用 ID 和完整请求摘要。解析有帧、遍历与嵌套限制，拒绝尾随数据、错误 UTF-8 文本、版本与摘要不符。`principal` 是宿主完成认证后填入的身份事实，解码成功不会授予权限。

`IoDeclaration.service_schema_sha256` 为 Protobuf tag 7 的可选 bytes：空值保持旧 IO 声明兼容；非空必须精确等于服务 schema 摘要，且声明 `HttpPublish`。声明仍只是批准上限。服务处理器必须存在于该包的 IO handlers 中，不能把普通纯转换处理器登记自动视为发布许可。固定服务 schema 尚未形成新版 C/C++/Rust SDK 稳定承诺。

| 数据 | 当前硬边界 |
| --- | --- |
| 请求／响应完整帧 | 128 KiB |
| 请求／响应正文 | 64 KiB |
| 每侧头数量／总量 | 64 项／16 KiB；总量含名称、值及每项 4 字节分隔成本 |
| 方法 | GET、HEAD、POST、PUT、PATCH、DELETE、OPTIONS |
| 目标 | 有界 origin-form；拒绝跨 authority、反斜杠、fragment、空白／控制字符、坏 percent 转义 |
| 响应 | 状态 200–599；204／205／304 不得携带正文 |

输入移除认证、Cookie、代理和连接／分帧头，包括 Connection 指定的逐跳字段；core 再拒绝不允许的头。输出拒绝连接／分帧和全部 `proxy-*` 头，不允许 guest 控制监听连接或 HTTP 分帧。响应头值保留合法原字节，正文不要求 JSON；原生入站请求头仍走严格文本路径，合法非 ASCII 头值暂不能保证完整兼容；这些内部原件是 Morrow 协议帧，不是 TCP/TLS 报文录制。

## 原实例与三层授权

1. Registry／Manager 的实际 IO 类别批准和 `IoBinding` 先存在。发布需要 `HttpPublish`，监听需要独立 `HttpListen`；任何出站调用另需自己的能力与资源批准。
2. `ServiceGrant::issue` 固定 service／handler 与实际 Manager、Host、ManagedInstance、绑定和资源租约；`ListenerGrant::issue` 单独占监听资源。`ServiceHost` 持有同一个 `IoWorker::spawn_managed` 创建的 worker。路由与 listener 必须属于该 worker 的实际实例；同包的新实例不能借用旧批准。
3. `Principal::new` 由宿主配置主体 ID、Bearer token、允许的 service 集合与期限。服务端只保存 token 摘要，认证后构造不可由普通请求自报的 `AuthorizedRequest`。主体只能调用其 service scope；clone 不续期、不生成新批准，撤权共享生效。

Principal 的 service scope 只允许调用服务。需要读写卡片时，宿主显式配置 content_route 与 ServiceContentPolicy，将实际主体范围和原逐对象内容 grant 相交；受控 exchange 已通过本机限定验证，见 [内容权限合同](PLUGIN_SERVICE_CONTENT.md)。普通服务仍拒绝内容 exchange，不开放原始 SQL；持久账号和主应用范围管理仍待接入。

## 发布与执行路径

宿主先颁发服务、监听及可选出站端点批准，再移交原实例给 managed worker。`ServiceHost::new(worker, timeout, RouterFactory)` 共享一个实例、队列和累计预算；`route` 固定 service、handler、方法和路径。`ManagedNode::bind` 或 `bind_tls` 核对同一 owner，将发布许可绑定到 listener 后创建实际节点。同一节点的所有路由必须属于同一个实际 worker，不能仅凭相同包名混接。一个监听批准只能 activate 一次；已经通过 owner 校验并 activate 后的 bind 失败也消耗该批准，owner 校验失败则不消耗。监听监控经 worker 的原 Authority 同锁取时与校验，不使用独立时钟。

普通 HTTP 仅 loopback；TLS 由宿主显式提供地址、证书和私钥，保留 TLS 校验和 Bearer／scope 检查。不存在隐式公网监听、自动防火墙配置或 guest 选择监听地址。本轮仅验证 Windows 的本机 HTTP/TLS，不能从可配置地址推导公网部署已通过。

收到获准请求后，宿主剥离原始认证头、填入实际 principal，构造服务 Request，并调用 `IoWorker::submit_service`。普通 route 的固定调用序号不等于持久幂等键；需要恢复时，宿主显式使用 durable_route／submit_service_durable，并保存稳定 namespace，见 [持久请求与恢复](PLUGIN_SERVICE_HISTORY.md)。请求不额外重建一套插件实例或批准权威；排队、执行、Ready 与最终 read 使用同一原实例和配额。

**零次 IO import 是合法服务计算**：guest 可以直接返回绑定请求的服务 Reply。若发生 IO，则每次 import 仍走受控 `BrokerRouter`／`RouteContext`，出站服务需另获 [HTTP 端点批准](PLUGIN_MANAGED_HTTP.md)，实际副作用继续遵守 Prepared → 发送边界 → Observed／Unknown。最终服务 Reply 可以由多个计算／调用结果组合，不要求等于最后一次 IO 响应，但必须精确绑定原服务 Request。

服务完成帧另计字节，不能通过零 IO 绕过输出额度。队列饱和可返回 429；失败、取消、无效完成帧不伪装成成功。HTTP 4xx／5xx 是可表达的业务响应。主体在等待和交付处重新检查，作业也持续受发布、监听、原实例和已使用出站资源的存活约束。已交付字节不能因之后撤权被追回。

## 停止、租约与事实边界

ServiceGrant／ListenerGrant 撤权以及 Manager 停用、批准变更、移除、Drop 都会使旧绑定失效。服务调用句柄取消与节点关闭只能请求停止；已经到达上游或完成的业务效果不能被宣称回滚，发送边界后的不确定结果仍须核对。

`ManagedNode` 的监督任务持有监听租约，通过实际 `Node::shutdown` 收尾后释放其副本；Drop 仅发取消信号，不直接终止持有租约的监督任务。其它仍存活的 grant／route 副本可能继续占有资源，clone/revoke 本身不退款。这里保证监听任务的租约生命周期，不宣称所有已接受连接、插件计算或外部业务均已回滚。`ServiceHost::shutdown` 有界等待 worker 退出后归还原 Host；超时不是线程已退出的证明。

## 仍未完成与验证入口

- 持久服务／监听批准、主应用发布 UI、证书在线轮换、OAuth/OIDC/mTLS、持久账号与内容范围管理界面。
- 入站唯一认领、保存结果重试和崩溃后的保守恢复已完成限定验证；独立状态查询、待核对结果处理及整条入站身份与内容提交的审计关联仍待建立，见 [持久请求与恢复](PLUGIN_SERVICE_HISTORY.md)。
- 大正文／流式上传下载、SSE/WebSocket、异步持久任务、多 worker 调度与完整平台矩阵。
- 新服务契约的 C/C++/Rust SDK、正式示例生成、冻结原包兼容门禁与稳定版承诺。既有三语言纯转换节点示例不是本契约的三语言验收。

代码入口：`core/src/service.rs`、`plugin_runtime/src/service_io.rs` 与 `io_jobs.rs`、`network_node/src/managed_service.rs` 与 `server.rs`。核心专项为 `core/tests/service_codec.rs`；运行时和真实 HTTP/TLS 的范围与结果见 [限定验证报告](../reports/managed-service-2026-09-19.md)。
