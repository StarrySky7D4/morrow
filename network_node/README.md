# Morrow 原生双向 API 节点（开发原型）

版本 `0.1.9-test.51`，2026-09-14。该版本仅属于新实验组件，主应用仍 `0.1.9-test.50+55`。第一方 AGPL-3.0-only；无新增 unsafe。当前原型提供 HTTP 客户端与带认证的 HTTP/HTTPS 服务；完整插件网络/服务系统尚未完成。[完整双向目标](../docs/PLUGIN_API_NODE.md)

## 当前实现

- `client::Client`：受信任调用方指定精确 origin 与允许方法，支持 GET/HEAD/POST/PUT/PATCH/DELETE/OPTIONS、原始正文、重复业务头与完整 HTTP status；4xx/5xx 仍返回响应。目标 DNS 全量检查后钉定连接，禁止隐式代理、重定向、重试与解压。默认 HTTPS 公网，显式本机 HTTP/HTTPS profile 仅允许 loopback；TLS 使用内置 WebPKI 信任根；自定义受信任根有界导入且保持证书/主机名验证，不自动读取 Windows 系统证书库。
- `server::Node`：普通 HTTP 仅监听本机 IPv4/IPv6；TLS 通过 `TlsIdentity` 和 `bind_tls` 明确配置证书/私钥与监听地址，可选择非本机地址，不隐式改防火墙或发布公网；所有路由先 Bearer 认证，按精确路径与方法分派。检查 header/正文/输出、并发、超时与取消，停机撤销交付并关闭 socket。最多 2×并发数量的接收连接，每个连接固定期限 2×请求超时，半头/空闲/keep-alive/TLS 未完成握手不能永久占位。每连接独立推进 TLS 握手，单个慢握手不阻塞 listener 接收其他请求。
- `plugin::PluginService`（可选 feature）：启动方明确选择固定包与 bytes→bytes 处理器，只接受无内容能力/无依赖的包。创建全新独占数据目录，拒绝已有目录/文件；Worker 以零内容批准执行，远端不能选择其他包/handler或获取内容对象。处理器失败返回固定 422，内部错误和认证头不传给插件。
- `morrow-api-node`：提供 plugin 模式及 relay 模式；可作为本机 API 节点，也可将获认证请求转发到启动方固定的上游，再返回上游结果。relay 仅转发 content-type/accept 业务头，节点认证信息不转发。

默认请求 body 1 MiB、响应 body 4 MiB、header 16 KiB、并发16、请求超时30秒；类型层另有有界最大配置。当前正文完整缓冲，读取网络分块时检查总量，不是 guest 流式 API。响应头仅接受可表示为文本的值；不承诺保留网络逐字节报文顺序/原始 HTTP 帧。

## 构建与测试

```powershell
cargo test --manifest-path network_node/Cargo.toml --locked --offline --features plugin-adapter --target-dir build/network-node
cargo clippy --manifest-path network_node/Cargo.toml --locked --offline --features plugin-adapter --all-targets --target-dir build/network-node -- -D warnings
cargo build --manifest-path network_node/Cargo.toml --locked --offline --release --features plugin-adapter --target-dir build/network-node
python -B tool/verify_network_node.py
```

实际测试使用本机临时端口、合成认证信息与新目录，不访问真实账号。`tests/bidirectional.rs` 运行真实两节点链：外部请求→本节点→上游节点→冻结的 C/C++/Rust Wasm→原路返回，比较空输入/二进制/Unicode 的精确处理结果。它证明 HTTP 传输与显式纯转换适配，**不证明 guest 已获得通用出站网络接口或完整服务发布授权**。

## 启动本机插件 API

先准备只有当前用户可读的 token 文件，内容为至少32字符的随机秘密（长度校验不能保证人为密码有足够熵）；不要把 token 写入仓库、命令参数或日志。程序从文件读取，标准输出只显示监听地址。

```powershell
build/network-node/release/morrow-api-node.exe plugin sdk/compat/guest-v1-rc1/rust-transform.mplugin build/my-api-data bytes.reverse C:/private/node-token.txt 127.0.0.1:8787
```

`build/my-api-data` 必须不存在；程序创建其内 `workbench.db`。父目录属于受信任启动环境，不宣称可以抵御同一系统账号恶意替换目录。端口占用等后续失败可能留下新目录，程序不自动删除它；核对后换一个新目录重试。

向 `POST http://127.0.0.1:8787/v1/invoke` 发送 `Authorization: Bearer <token>` 和原始正文，成功响应为二进制逆序结果。使用 C/C++ 包替换参数即可验证相同处理器；这几个冻结示例只是资格包，不是完整业务 API 插件。

`relay <固定上游URL> <token文件> [本机监听地址] [--allow-local-upstream]` 将 `/v1/invoke` 常用方法请求发往固定上游。上游 origin/method 来自启动配置，入站请求不能覆盖；入站 URL 的 query 不拼接至上游。没有自动携带上游凭据、自动追随跳转或重试。需要上游认证时，嵌入方应通过受信任 Client 的 header 配置；当前 CLI 不实现账户/凭据管理。

也可启动 HTTPS 版本：

```powershell
build/network-node/release/morrow-api-node.exe plugin-tls sdk/compat/guest-v1-rc1/rust-transform.mplugin build/my-https-api-data bytes.reverse C:/private/node-token.txt C:/private/node-chain.pem C:/private/node-key.pem 127.0.0.1:8788
```

证书链与私钥均为 PEM、有界读取并校验，证书必须匹配客户端使用的主机名。默认仍本机地址；需要接受其他设备请求时，在 TLS 模式明确指定适用的监听 IP/端口。证书签发、DNS、防火墙和网络映射由用户部署配置，本轮没有自动开放任何公开监听。客户端不要关闭证书验证；内部 CA 可通过受信任根配置接入。

Ctrl+C 执行有界停止。首期服务只有一个节点 token；远端主体、每路由 scope、持久发布、证书在线轮换和公开环境部署验收尚未接入。已实现 TLS transport 不代表公开网络生产部署资格。

## 验证范围及缺口

当前实现是可运行的受信任原生传输底座，和 app 的原有 Manager、Registry、正式 guest IO import、网络证据与 UI 服务管理仍有待集成。原 guest 协议与冻结二进制不变，本轮不修改 Flutter、正式用户资料库或应用版本。

完整目标仍包括：正式出站/监听/发布权限、远端主体与内容授权交集、外部效果持久意图/Unknown查询、API Key/OAuth账户管理、multipart辅助/大型流/SSE/WebSocket、webhook签名与恢复、TLS 部署/证书轮换和各平台实际验收。当前 Error 的取消/超时/传输失败不证明远端没执行，不会自动重试；没有持久结果恢复。不要把普通原生HTTP调用描述为完整插件服务SDK稳定。

本轮实际结果见 [首次原生验证报告](../reports/network-node-initial.md)：33 项 Release 测试、strict Clippy 和 4 次实际可执行文件流程通过；不是整个网络 SDK 的完成声明。
