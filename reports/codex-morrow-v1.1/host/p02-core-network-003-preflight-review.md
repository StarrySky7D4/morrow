# P02 Core network 003：宿主只读预审

日期：2026-09-28。审阅对象为插件 `receipts/p02-core-network-003/preflight.md` 与固定 Codex `44fe510ce3ee61c8ef623adcbf89b901c73ddd61` 源码。输入摘要见同目录 `p02-core-network-003-preflight-inputs.json`。本文未审阅尚未交付的实现，也未运行构建、探针或网络请求。

结论：五案计划对应真实 Core 控制流，可继续形成有限拒绝资格证据；不能据此宣布宿主网络适配、P02/G0/J00 或产品通过。计划正确区分本地合成 426 与宿主回执，没有以成功对象填补 003 缺口。实施前应落实以下边界。

## 1. 认证来源在 transport 之前，空 AuthManager 不充分

`core/src/client.rs:497-503` 创建 provider 后无条件调用 `collect_auth_env_telemetry`；`login/src/auth_env_telemetry.rs:38-53` 使用 `std::env::var` 读取认证环境变量值并转换为 presence。关闭 inference traces、不配置 env_key 或传入空 AuthManager 都不会阻止此读取。没有读取当前机器变量进行本次审计。

此外，`model-provider/src/auth.rs:187-194` 会在 provider.auth 存在时自行创建 external bearer manager；`provider.rs:373-391` 还有 Bedrock、gateway OAuth 分支。`codex-api/src/endpoint/session.rs:147-154` 在调用 HTTP transport 前执行 apply_auth。因此 Unavailable factory 只能作为网络后备拒绝，不能证明此前没有凭据读取或认证动作。

公开夹具应显式固定 auth/gateway_oauth/aws/experimental_bearer_token/env_key/env_http_headers 为 None，requires_openai_auth=false；使用普通 Responses provider、公开固定 URL/headers、空 request_contributors、无 attestation provider，并明确禁止配置加载与遥测 exporter 初始化。若验收要求不读取本机账户密钥，运行进程必须不继承有关认证值，或使用明确的 qualification 注入边界提供公开 telemetry 值。不能以“只记录 presence”冒充“未读取值”。这是实现待确认条件，不是已发生泄露的判断。

## 2. 独立 preconnect 分支尚未列入五案

`core/src/client.rs:1451` 的 `preconnect_websocket` 是独立公开入口：只建立连接、不发送 prompt；其 426 分支在 1507 附近调用 try_switch_fallback_transport 并返回 Ok。该路径静态上也通过 websocket_connection/connect_websocket，若注入位置正确会被拒绝边界覆盖，但五案没有执行它的分支。

可保持五案并明确列出 preconnect 的通用失败/426 分支未测；如补充用例则另计数量。prewarm 不能代表所有预连接路径。成功连接复用、断线重连和 auth owner/cache 仍属未测。

## 3. 注入与计数必须证明选中的实际路径

HTTP override 应在 `build_api_transport:1176` 的 create_client_for_route/ReqwestTransport 构造之前选中；WebSocket override 应在 `connect_websocket:1211` 的真实 API connect 前选中，并说明截获发生在 headers/telemetry/timeout 构建之前还是之后。不能失败后改走默认网络后端。HttpTransport 同时要求 execute 与 stream（`http-client/src/transport.rs:36-45`）；即使五案只覆盖 stream，注入 execute 也应明确拒绝，不能漏回具体客户端。

EndpointSession 存在请求重试；`codex-client/src/retry.rs:25-42` 将 Connection/Network/Timeout 视为可按策略重试，Policy 不重试。夹具宜固定 request_max_retries 和 stream_max_retries 为 Some(0)，并断言调用顺序、数量与错误传播。拒绝语义不得伪装成 401/429/5xx 引入认证恢复或额外重试。仅 backend 计数不能证明 OS 全局无网络旁路。

五案最低证据应包括：

1. WS disabled：真实 Core stream → Responses 请求构建/序列化 → HTTP 拒绝；WS 调用为零。
2. 通用 WS disconnect：初始 WS enabled；WS 一次拒绝、HTTP 零次，错误返回且没有启用 HTTP fallback。
3. prewarm disconnect：连接拒绝、无成功连接或 frame，错误返回。
4. stream 合成 426：真实匹配分支自然切换；顺序 WS426 → HTTP 拒绝；同一个 ModelClient 的 new_session 随后仅走 HTTP。不得由测试直接调用 force_http_fallback。
5. prewarm 合成 426：当次仅 WS426，setup Ok 且无 HTTP/成功 stream/frame；后续真实 stream 才进入 HTTP 拒绝。

上述 426 应携带 local_injected_error 来源，不能称为 host003 HTTP status 或真实服务器响应。预热的 setup Ok 也不是模型响应成功。默认具体后端保持原路由属于静态设计要求，五个拒绝用例不验证其产品行为。

## 4. M-03 / M-08 合同归属

003 的 ResourceRef/字节流不是 HTTP 或 WebSocket 完整合同。M-03 后续需定义获准目的地、method/URL/受限请求与响应 headers/status、重定向与传输阶段，以及若声明支持 WebSocket 时的 upgrade/frame/control/close；M-08 关联账户权限、凭据和认证挑战/恢复。HTTP body、SSE 与 Responses 模型解析仍归插件，秘密不进入普通诊断。WebSocket 能力也可显式未提供，不能静默直连补齐。

本批应保留成功 HTTP/SSE/WS、认证恢复、Realtime/unary、真实 IPC、代理/TLS、完整 Core loop、OS 隔离等未验证项，不扩写 003 或生产后端。宿主唯一 schema 和 host-kit-003 不变；本轮只新增宿主预审报告与输入摘要，没有修改插件、联合验收或既有交付。
