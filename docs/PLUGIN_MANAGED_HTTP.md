# IO-D1：托管插件的有界 HTTP/HTTPS 出站

状态：Windows 本机真实 HTTP/TLS 路径已接线；这是首批有界出站能力，不是完整网络 SDK。验收范围与实际结果见 [本轮报告](../reports/managed-http-2026-09-19.md)。原生宿主显式 API 节点发布已接入并通过本机限定验证，见 [IO-D2](PLUGIN_MANAGED_SERVICE.md)；持久服务发布、账户服务、大文件流和主应用 UI 仍沿 [完整网络目标](PLUGIN_NETWORK_API.md) 与 [API节点目标](PLUGIN_API_NODE.md) 推进。

## 授权来源

插件包声明、Registry/Manager 的 HttpRequest 类别批准与具体资源批准同时满足才可发出请求。`HttpEndpoint::approve` 是可信宿主的显式运行期资源批准入口，必须提供真实 Manager、Host、ManagedInstance、原 IoBinding 和端点策略；不从 guest 数据直接创建批准。当前资源批准只在内存中存在，尚未接入持久配置和授权界面。

`EndpointApproval` 固定一个 origin、允许的方法、网络 profile、请求/响应/头额度、期限、完整响应帧上限，以及可选的凭据引用/头值与自定义 TLS 信任根。当前批准涵盖该 origin 下所有路径，尚无更细路径范围规则。公网 profile 只接受 HTTPS 公网地址；本机 HTTP/HTTPS 必须显式选择，仅准 loopback，TLS 仍验证证书和主机名。DNS 全答案集校验后钉定连接，禁止系统隐式代理、重定向和自动重试。

`HttpGrant` 持有原实例的受控资源租约；包相同也不能换到另一实例、Host 或 Manager 使用。端点只向 guest 交付 lowerhex 引用，引用本身不是独立承载授权的令牌。可信宿主须每次会话提供新生成的不可预测秘密；端点引用绑定策略、包及单调序号。旧引用不会恢复新实例或重启后的批准。

## 运行流程

1. 宿主在移交实例前调用 `HttpEndpoint::approve`，然后以原 binding 启动 `IoWorker::spawn_managed`。
2. 将端点引用交给 guest；guest 通过现有 IO 帧提供操作 ID、方法、相对路径/查询、业务头、正文和可选凭据引用。
3. 宿主用 `endpoint.router(tokio_handle)` 提供 `BrokerRouter`，调用 `submit_brokered`。Tokio runtime 由宿主管理且须持续运行；网络驱动从专用 IO 线程调用，不在 Tokio task 内嵌套阻塞。
4. 路由核对原实例与引用、方法、凭据和额度。错误请求在持久发送边界前拒绝；guest 不能直接提供认证/代理/Host/分帧头，也不能覆盖已配置的凭据头。
5. 原始 guest 帧按 IO-C1/C2 保存并提交 OutcomeUnknown 后，才执行网络。请求期间监测原绑定、作业取消、期限与独立端点撤权；网络错误、超限、无法保留完整结果均不假装安全重试。
6. 完整响应转换为 Morrow HTTP 帧并保存为 Observed，最终交付受原作业和全部资源检查器约束。资源检查器保留至 Ready 的 read/drop；此时端点撤权或到期同样清空正文、头及任务载荷。

`deadline_ms=0` 使用端点期限，正数进一步缩短本次网络等待；排队和总任务期限仍由作业控制。取消无法撤回已经到达远端的副作用，发送边界后的失败保持 Unknown 并要求核对。已观察或未知的操作 ID 不再发送。普通 HTTP 4xx/5xx/302 是收到的响应，保留状态与正文；302 不触发第二次请求。

## 凭据与原件

`Credential::header` 支持由宿主提供的 Bearer、Basic、API Key 或其它合法头；实际引用必须精确匹配且原实例须同时获 HttpRequest 与 CredentialUse。值仅在传输前注入，不出现在 guest 请求帧或受保护请求原件中。请求原件与响应原件是 Morrow 协议帧原字节，不是 TCP/TLS/HTTP 原始报文；不得声称保留网络逐字节顺序、TLS记录或 trailers。

响应头允许重复项与合法非 ASCII 字节；`Client::send_raw` 保留头值 bytes，旧文本 `send` 接口保持原严格转换。远端本身可能在响应中回显凭据，故本层不承诺对任何响应都能自动剔除秘密。尚无系统凭据库、OAuth刷新或持久账户服务。

## 额度和生命周期

每个获准端点占一个 resource；活跃 Broker 调用再占一个 resource，复用父作业 job。声明中 max_resources 至少为2才可同时持有一个端点并执行一个调用。clone、revoke 不提前释放仍被持有的资源；所有端点/检查器副本释放才归还该资源。

作业累计仍为 input + 每次请求帧 + 每次预留完整响应帧上限，不因响应小而退款；网络正文/头另受更紧约束。当前为内联有界 profile：请求/响应正文最多64KiB、头总量最多16KiB、完整响应帧最多128KiB；响应头数/单字段还受 core codec 约束。声明长度与实际分块都检查，HEAD/204/304不会仅因广告长度而虚构正文超限。请求头仍经过原生文本 HeaderValue 路径，不能保证任意非UTF-8字节输入。

## 后续门槛

- 宿主主界面/持久资源批准、凭据库、路径范围、本地局域网独立 profile及平台策略。
- 实际提供者幂等/状态查询核对、应用重启后的恢复展示、证据访问策略与配额退休。
- 在已接线的原生服务发布／Principal service scopes 上补持久发布、细粒度内容权限交集与主应用接入；受控文件变更/选择/枚举。
- 三语言类型化扩展、大文件/流/SSE/WebSocket、OAuth、多账号、录制隔离重放。

上述范围未因本机传输通过而完成；冻结旧 SDK、应用版本和用户数据库不在本轮变更范围。
