# P-01 固定源码审查（G0 第一切片）

日期：2026-09-28。状态：**已完成所取得固定文件的静态审查与清单；P-01 完整构建闭包资格尚未完成，P-02 未执行。**

本报告的源代码依据是本仓库 `upstream/reference/codex/`、`upstream/reference/cc-switch/` 内的真实固定提交文件。计划中的接口名称不是上游能力证据。未运行上游程序、cargo build/fetch、登录、模型调用、MCP 服务或 SubagentBridge；未读取本机个人账号、MCP 配置和密钥。Morrow checkout 与原计划未修改。

## 1. 来源和核验边界

| 来源 | 固定提交 | 已核验文件 | 许可证 |
|---|---|---:|---|
| https://github.com/openai/codex | `44fe510ce3ee61c8ef623adcbf89b901c73ddd61` | 209 | Apache-2.0；保留原 LICENSE、NOTICE |
| https://github.com/farion1231/cc-switch | `846de29c13ac4d65f164db8c15dd5fd58e29f972` | 13 | MIT；保留原 LICENSE |

先检查了 Morrow 文档/研究候选路径及 CodeXProjext 的 temp/plugin 范围，没有找到可直接复用的固定源码。公开 Git 的单提交浅提取失败；两份固定 codeload 下载超时。改用 GitHub 只读连接器按明确 ref 获取文件，保存原字节，并对全部 222 个文件计算 Git blob SHA-1 与连接器返回值逐一比对，另计算 SHA-256。字节一致，无修改文件、无 upstream patch。当前目录是 **partial_snapshot**，不是完整 Git checkout；不能将空 `upstream/codex/.git`、`upstream/cc-switch/.git` 或不完整 tar.gz 用作已取得固定提交的证据。

`receipts/upstream-inventory.json` 保存来源、路径、许可证摘要、失败获取事实和限定结论。`receipts/upstream-codex-tree.json` 与 `receipts/upstream-cc-switch-tree.json` 是这次冻结快照的完整文件集合；含 path、sha256、git_blob_sha1。逐字节验证了该集合，未证明整个上游提交已下载。后续新增源须形成新快照清单并重新锁定，不能悄悄补文件绕过预检。

`upstream/codex-git-tree-complete.json` 实际是请求失败留下的截断 JSON，**不可使用**。CC Switch 的完整 Git tree 元数据成功解析，1454 项且 truncated=false，但这不代表源文件全部下载。所有失败产物保留以说明取源过程，不进入可构建源集合。

已读取 Codex 根 AGENTS.md。只读审查不触发其中修改后格式化/测试要求；本任务也明确禁止全局格式化和运行上游。codex-rs、core、core/src 的 AGENTS.md 固定路径检查返回 404，保留在获取回执中。

## 2. 真正的构建与依赖范围

固定 Codex `codex-rs/rust-toolchain.toml` 指定 **1.95.0**，组件 clippy、rustfmt、rust-src；workspace edition=2024、license=Apache-2.0。取得 147 个 workspace/path 包的 Cargo.toml、workspace manifest 和 Cargo.lock（1471 package records）。CC Switch 指定 edition=2021、rust-version=1.85.0；取得其 Cargo.lock（750 records）。这两份上游锁是审查输入，不是插件原生/Wasm 的最终锁。

`receipts/upstream-codex-dependency-closure.json` 对每个包保存完整 normal/build/dev/target 依赖边、features、optional、default-features、版本/路径/git 信息，以及固定锁的依赖记录和 workspace patches。下表是 normal+build 的保守传递 **path** 闭包，包含所有 target 与 optional 边，包含根自身；dev 边另行记录但不计入此数。所有这些 path manifest 都已取得，missing_path_manifests=[]。这不是运行 Cargo 解析出的目标/feature 最小闭包，也不是已编译证明。

| 候选根（codex-rs/） | path 包数 | 保留/替换/排除决定 |
|---|---:|---|
| core | 90 | 保留唯一会话循环候选；先改真实注入点，不能直接整库进入 A 或 Wasm |
| exec-server | 39 | 保留 ExecBackend/事件语义参考；A 必须替换本机后端与环境创建 |
| file-system | 15 | 保留 ExecutorFileSystem 接口参考；实现交给宿主/B |
| model-provider | 48 | 保留 provider 语义；不把它当成已完成网络隔离 |
| model-provider-info | 16 | 保留必要配置类型；禁止随意 env/base URL/凭据外发 |
| models-manager | 41 | 替换目录下载与缓存；未受控网络不得启动 |
| login | 34 | 原生流程后续保留；传输/凭据 store 必须替换，本阶段排除运行 |
| codex-api | 17 | ResponsesClient 泛型传输是真接缝候选，仍有非纯依赖 |
| codex-client | 3 | 客户端/SSE参考；依赖 http-client，不直接视为纯 Wasm |
| http-client | 2 | HttpTransport 是真实底层接缝；生产 ReqwestTransport 排除于 fake 路径 |
| thread-store | 35 | 保留 ThreadStore/LiveThread 接口；替换 LocalThreadStore |
| rollout | 27 | 格式/导入参考；正式模式排除独立可写本地权威 |
| history | 15 | 纯历史逻辑候选；需继续拆类型和外部依赖 |
| protocol | 14 | 仅挑必要数据类型；完整包不能自动当纯逻辑 |
| apply-patch | 40 | 仅解析/预览候选；实际文件写入和进程路径交给 B |
| utils/stream-parser | 1 | 无外部 normal/build 依赖的候选；并未证明它等同所需 SSE parser |

JSON 另含从各 path 闭包外部依赖出发的 Cargo.lock 保守可达集合；不明确版本的边包含所有同名锁版本。它不能代替 Cargo 的 platform/feature 解析。没有下载注册表 crate 源码、执行其 build.rs 或审计全部第三方传递实现。此缺口必须保留。

workspace patch 固定了 crossterm、tokio-tungstenite、tungstenite 的 git rev；不能移除 patch 后声称使用同一固定依赖，也不能跟随 latest。core 明确启用 Tokio process/rt-multi-thread/signal；musl 目标引入 vendored OpenSSL，unix 引入 shell-escalation；Windows sandbox 包也在 path 闭包内。native 与 wasm 必须各自 workspace、各自锁和输出目录；当前 90 包闭包不可机械纳入 Wasm。

对候选闭包的 90 个默认 build.rs 路径做了固定 ref 检查，四个存在并已取得，其余 86 个返回 404；manifest 的显式 build 字段仅指向其中两个默认路径。没有执行这些脚本：

| build.rs | 真实行为与输入 |
|---|---|
| build-info | 读取 TARGET，设置 CODEX_BUILD_TARGET |
| code-mode-protocol | 读取 src/grpc/*.proto，调用 vendored protoc/tonic 生成；缺文件不能构建 |
| skills | 遍历 src/assets/samples，输出 rerun-if-changed；资产尚未完整取得 |
| windows-sandbox-rs | 读取 CARGO_CFG_TARGET_OS/ENV/ABI、CARGO_MANIFEST_DIR；Windows manifest/linker 参数 |
| CC Switch src-tauri/build.rs | tauri_build::build 与 Windows manifest 处理；从纯转换构建图排除 |

## 3. 执行接缝及已经发现的旁路

以下路径以 `upstream/reference/codex/codex-rs/` 为基准，行号来自本次锁定原字节。

- `exec-server/src/process.rs:223` 定义真实 `ExecBackend::start`，:238 定义 `start_with_network_policy_decider`；:199 的 `ExecProcess` 包含 read/write/signal/terminate 及事件订阅。它不是计划凭空创造的 trait。
- `file-system/src/lib.rs` 定义 `ExecutorFileSystem`、`EnvironmentAccess` 等接口；exec-server/lib.rs 重导出。文件能力需要独立替换，替换启动进程接口不能替代文件访问控制。
- `exec-server/src/environment.rs:675` 的 exec_backend 字段私有；:759 的 local 构造创建 LocalProcess；:1136 返回 backend。不存在已经核验的“随便给完整 A 塞 fake backend”公开构造路径。`environment_provider.rs:63` 读取 CODEX_EXEC_SERVER_URL；未设置/空值会选择本机环境，不能拿环境变量缺省充当 fail-closed。
- `core/src/unified_exec/process_manager.rs:1322` 仅在 remote 或 exec_server_shell_snapshot 条件下走 backend，:1329 取得接口，:1349/:1352 调用 start；**:1413 的另一分支直接调用 codex_sandboxing::spawn_process**。`core/src/exec.rs:959` 还调用 spawn_child_async。只给 ExecBackend 写 fake 并不会证明无本机后备。
- `exec-server/src/local_process.rs:424` 调用 sandboxing::spawn_process；PTY、进程事件、OutputClosed/Exited 等都仍是本机实现候选，不能误说已由 B 承载。
- `core/src/unified_exec/mod.rs:1` 附近说明上游有经自身批准后降级 sandbox 的重试语义；Morrow 适配不能把它当作宿主 permit 重用许可。A-only 必须先撤出执行工具并拒绝执行入口。

P-02 执行探针应在真实调用点接入 fake B，并同时封锁/替换上述 local 分支。另须补齐 Hooks、AGENTS/Skills 发现、MCP 启动、Git 辅助、shell snapshot、code-mode 进程和全部文件实现的源审查及运行记录。当前只有主路径和已发现旁路的静态证据，**没有动态无旁路证明**。

## 4. 网络接缝与覆盖面

- `http-client/src/transport.rs:36` 的 `HttpTransport` 有 execute/stream；`codex-api/src/endpoint/responses.rs:26` 为真实 `ResponsesClient<T: HttpTransport>`，:41 的构造接受 transport，:135 起沿 EndpointSession 发 `/responses`。这是最小真实网络截获探针的候选入口。
- 但 `core/src/client.rs:1181` 的辅助函数返回类型写死 **ReqwestTransport**，:1197 从 HttpClient 创建它，:1758 创建 ResponsesClient。:1233 使用独立 WebSocket client；:2218 的 stream 有 WebSocket/HTTP 路径。因此只测试泛型 ResponsesClient 只能标“endpoint 级探针”，不能声称完整 loop 网络已替换。
- `model-provider/src/provider.rs:141` 的 ModelProvider 是 provider 行为接口，不是覆盖所有网络的宿主 transport。`core/src/session/mod.rs:821` 与 `core/src/client.rs:497` 仍调用 create_model_provider。替换宿主 adapter 要保留模型语义在插件侧。
- `models-manager/src/manager.rs`、`model-provider/src/models_endpoint.rs`、`core/src/compact_remote_v2.rs`、`codex-api/src/endpoint/models.rs` 是模型目录/远端 compact 的额外候选；不能用一次 Responses 截获替代它们。
- `login/src/device_code_auth.rs:74/:118` 发送认证 HTTP；`login/src/server.rs:204` 打开系统浏览器，:1026/:1036 发送 token exchange。认证 refresh/revoke 的完整实现尚未全部取得，明确待续审。浏览器导航必须单独管理，不归入普通模型流。
- `login/src/auth/default_client.rs` 和 `http-client/src/outbound_proxy.rs` 包含 route/default proxy/custom CA 的 fallback。`core/src/state/service.rs:66` 还持有文件上传 client pool，并包含 analytics/telemetry、network proxy 等初始化。未受控后台网络须在资格 profile 禁用或全部接入后验证。

P-02 的断开测试必须让 fake transport 明确失败、禁止 direct reqwest/socket 后备，并保留实际请求记录与账户/端点/attempt 关联。此报告只定位源码，未创建第二份宿主 StreamTransportPort 合同。

## 5. 存储接缝与单权威要求

- `thread-store/src/store.rs:93` 定义真实 `ThreadStore`；create/resume/append/persist/read/list/fork/archive/delete 及附件、项目、元数据等都有接口。:68 的 PersistContext 允许部分背景排队，不能把返回成功统一解释成宿主 durable。
- `thread-store/src/live_thread.rs:143` 调 create_thread；:204 调 resume；:302 调 persist_thread；:309/:318 调 flush/shutdown。:191 仍有对 LocalThreadStore 的 downcast 特殊路径，需在适配评审中明确封锁或实现相容拒绝。
- `core/src/thread_manager.rs:521` 的 ThreadManager::new 实际接受 `Arc<dyn ThreadStore>`（:532）；这是可利用的注入点。`core/src/session/mod.rs`、`core/src/state/service.rs:93/:95` 持有 live_thread/thread_store。
- **不能使用默认 `thread_store_from_config`**：`core/src/thread_manager.rs:449` 起会构造 LocalThreadStore，:479 起可启动旧 rollout 迁移和压缩工作；:500 的 InMemoryThreadStore 仍接 state_db。fake 内存存储不代表正式唯一存储。
- `thread-store/src/local/mod.rs:126` 明确 local filesystem/SQLite 后端，:147 持 SQLite pool；`rollout/src/recorder.rs` 保留 JSONL/文件写入。正式 A 不得把它们与 Morrow Store 同时保持可写。
- `core/src/state/service.rs` 还有 state_db、thread_extension_data、image_store 等；`thread_manager.rs:511` 有 SQLite agent graph store。会话 store 单点替换不覆盖所有旁存。
- `login/src/auth/storage.rs:174` 有 FileAuthStorage；:409 的 AutoAuthStorage 含文件回退；:518 起根据 mode 选择。它与历史存储不同，但同样不能悄悄使用个人 CODEX_HOME/默认 keyring。原生认证阶段必须明确宿主凭据 namespace/失败语义。

最小存储探针应调用真实 LiveThread + fake ThreadStore 的 create/append/persist/resume/fork/archive 路径，并在 fake 禁用后拒绝，不生成可继续恢复的第二份 rollout。尚未执行；也尚未验证完整 SessionServices 初始化无旁存。

## 6. CC Switch 转换子集

`receipts/upstream-cc-switch-dependency-closure.json` 记录取得模块、直接 use 行、上游完整 manifest/lock 和候选排除项。两份转换文件不是自足 crate：

| 候选模块（src-tauri/src/） | 直接关联 | 决定 |
|---|---|---|
| proxy/providers/transform_codex_chat.rs | codex_chat_common、provider::CodexChatReasoningConfig、proxy/error、json_canonical、tool_media、serde_json | 保留文本/function-tools转换候选；逐项能力拒绝，不照搬完整实现 |
| proxy/providers/streaming_codex_chat.rs | transform、codex_chat_common、codex_responses_sse、proxy/sse/json_canonical、bytes/futures/async-stream/log、tokio::pin! | 重做有界字节输入/状态输出边界和终态逻辑；不能把异步网络wrapper直接叫pure parser |
| proxy/providers/codex_chat_common.rs | serde_json，含 think 标签处理 | 仅保留显式字段/类型映射；排除文字标签推断隐藏推理 |
| proxy/providers/codex_responses_sse.rs | bytes、serde_json | 保留显式事件编码候选 |
| proxy/json_canonical.rs | serde_json、sha2 | 只用于明确允许的稳定编码；工具参数原字节仍单独保留 |
| proxy/tool_media.rs | json_canonical、serde_json | 本期文本/function子集优先拒绝不支持媒体；不默默搬移媒体 |
| proxy/sse.rs | 纯字符串/字节帮助函数 | 必须经 BOM/CR/LF/CRLF/UTF-8分片/预算测试后才能采用 |
| provider.rs | app_config/codex_config/grok_config/pi_config 等 | 仅提取 :403 的纯 CodexChatReasoningConfig 类型候选，排除整模块 |
| proxy/error.rs | axum 等 HTTP response 边界 | 替换为纯错误类型，排除 axum 依赖 |

上游转换的已核对语义风险：

1. `streaming_codex_chat.rs:849` 附近遇到 `[DONE]` 直接 finalize；:857 附近坏 JSON `Err(_) => continue` 静默丢弃。
2. :889 起 EOF 后有正文却没有 finish_reason 时填造 `length` 并 finalize，不能作为上游真实终态。
3. `transform_codex_chat.rs:2064` 的 response_status_from_finish_reason 除 length 外一律 completed，包含 None/未知 finish reason。
4. streaming 中 InlineThinkMode 与 split_leading_think_block 从文本标签推断 reasoning，不符合本项目只保留上游显式且获授权字段的要求。

因此仅保留带许可证的来源参考和经后续测试选定的纯逻辑；排除整 Tauri crate、build.rs、UI、控制台、代理路由/failover、全局配置写入、reqwest/rusqlite/OS集成。最终原生/Wasm同源差分、名称碰撞、多工具交错、原始参数与完整终态均尚未执行。

## 7. 已产生的证据与下一步

- `receipts/upstream-inventory.json`：来源锁建议和失败/部分取得事实。
- `receipts/upstream-{codex,cc-switch}-tree.json`：冻结文件集及双摘要。
- `receipts/upstream-codex-dependency-closure.json`：147 manifests、16个候选闭包、所有边/feature/target/build/lock/patch资料。
- `receipts/upstream-cc-switch-dependency-closure.json`：转换模块和完整上游锁的静态记录。
- `receipts/upstream-side-effect-candidates.json`：668处关键词候选，涵盖 env/fs/process/net/init；包含注释/测试，**不是完整可达性或无副作用证明**。
- `upstream/reference/*-fetch*.json`、`receipts/upstream-*-source-*.json`：固定ref获取及404探测原回执；未冒充完整源码。

继续条件：M-00/唯一宿主 kit 交付并审查；取得可构建的固定上游最小真实源码闭包及固定依赖；选择一条上游真实调用链做最小 fake 探针。优先从 ResponsesClient<T:HttpTransport>、LiveThread/ThreadStore 和执行分支全部拦截入手，但 endpoint/trait 单测不得升级宣称完整 Codex loop 接入。随后检查全部初始化和后台通路并做断开失败路径测试。

当前明确阻塞：完整源码/依赖源和 target-feature 解析资格；真实 adapter 接入；真实 exec/net/store 截获及断开无后备证明。P-00 的完整来源门槛不能因此放行，G0/SDK 冻结/84项验收均未宣称通过。

## 8. 后续宿主交接与实际准备尝试

静态来源审查之后，插件主代理收到正式 `host-kit-002`，在 `receipts/host-kit-review-002/review.json` 记录了180个文件、19对向量及1个拒绝向量核验，结论仅 `qualification_only`，不是 SDK 冻结。随后宿主发现最大事件批次的累计遍历预算问题并暂停002最终签收；`receipts/host-kit-review-002/superseded.json` 保留并撤回该初步审查的可用基线状态。等待新候选，真实 adapter 接入仍未完成。

随后主代理实际运行固定1.95.0的 `cargo metadata --locked --offline --no-deps`，指向本次快照真实 `exec-server/Cargo.toml`；使用插件目录内独立 CARGO_HOME 与 target。`receipts/p02-readiness-001/result.json` 记录 exit=101、status=blocked；缺少 workspace 传递路径中 `utils/rustls-provider/src/lib.rs`，没有通过伪造 lib.rs 或缩减成假 loop 绕过。此项是 P-02 准备检查，**不是编译成功或 fake 接缝执行**。因此本次工作流不能描述为“从未尝试 Cargo metadata”，也不能将这次有证据的失败升级为 P-02 通过。
