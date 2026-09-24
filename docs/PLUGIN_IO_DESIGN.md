# 插件网络与文件接口设计

2026-09-15 排期修订：正式出站与入站授权、当前实例绑定、持久意图及 Unknown 核对由 [ROAD-07](ROADMAP_UPDATE_2026-09-15.md) 统一承接；复用已有原生传输，完整 NET／NODE 范围不变。对方无去重／查询接口时必须保留不确定结果与人工核对，不能承诺自动确认或安全重发。

状态：test.50 后续实施设计，2026-09-14。**本文件不表示网络、文件系统授权或 IO 重放已经实现。** 第一方实现默认 AGPL-3.0-only；第三方 SDK 使用规则沿用既有许可文件。本设计及其后续补充不表示已修改运行契约、数据库或冻结包。

依据：[统一架构基线](ARCHITECTURE_BASELINE.md)、[SDK 兼容候选](PLUGIN_SDK_COMPATIBILITY.md)、[SDK 与 UI](PLUGIN_SDK_AND_UI.md)。目标是 C、C++、Rust Wasm 插件经过同一个可信宿主取得明确授权的 IO 能力；插件不能直接获得操作系统文件句柄、任意本机路径、网络 socket 或宿主凭据。正式内容仍只有现有核心 Store 一个权威来源。

当前实现增量（2026-09-16）：[声明与实例准入](../reports/road-07-io-admission.md)已扩展为[真实 guest IO 与受管文件读取](../reports/track-a-integration-2026-09-16.md)。现有 IO schema 保持不变，`core::io` 支持 Read/Finish/Cancel；`morrow_io_v1.call` 由专用 Runner 导入，FileBroker 使用当前 Manager／Pool 实例、Registry 批准、共享预算与原资源租约。管理入口每次执行一个准确请求并核对实际完成结果，不走纯转换注册。调用限制、测试和平台边界见验收报告。

这些是实验性实现，不代表下文整套异步 API 已实现。submit/poll、OS 资源选择、文件变更、正式 guest 网络、持久材料与恢复仍见 [后续编码看板](DEVELOPMENT_BOARD.md)。已有 network_node 原生 HTTP／HTTPS 客户端和服务节点继续复用；它们不自动拥有 guest 授权。IO 扩展不纳入 guest-v1-rc1 冻结承诺。

## 完整网络能力目标补充

用户进一步要求完整网络 API 对接。最终范围以 [完整网络 API 设计与任务](PLUGIN_NETWORK_API.md) 为准：通用 HTTP 方法、原始/JSON/表单/multipart 请求、账号认证与 OAuth、上传下载、分页限流、流式 HTTP/SSE、WebSocket、恢复与录制重放，以及各平台真实能力。GET 仅为首个内部验证步骤，不是网络交付终点或 SDK 稳定门槛。第三方 JSON 是架构允许的外部接口格式，不改变自有 Cap’n Proto／Protobuf＋LZ4 的分工。

本文 16 MiB/30 秒等是普通请求首期策略；完整能力使用可声明的传输/长流 profile 和有界资源/期限，不能把这些数值冻结为大型上传或连续流的永久上限。所列网络 capability 名称仍为草案；完整候选须补 HEAD/OPTIONS 等方法范围、独立 streaming/WebSocket 声明、认证类型和平台能力发现。

## 1. 已有实现与准确切入点

| 当前文件 | 已有基础 | 新增工作 |
| --- | --- | --- |
| `core/src/plugin_package.rs`、`core/schemas/plugin_package.proto` | 校验包摘要、ABI、三种 mandatory feature、七种内容能力、预算 | 独立 IO 声明与 schema 摘要；未知 feature 继续拒绝，不能仅放宽 feature 数量 |
| `core/src/plugin_package/registry.rs`、`core/schemas/plugin_registry.proto` | 唯一持久选择、批准、依赖锁、revision CAS；当前 v1 canonical PB | 新版 IO 批准状态、原子迁移与旧版读取；不另开授权配置权威 |
| `core/src/dispatch.rs`、`core/src/lifecycle.rs` | `HostBinding`、`ConnectionBinding`、实例 Ready、七种 `GrantKind`、批准上限、前后授权与撤权信号 | IO 资源租约及上限，复用真实连接身份但独立于卡片授权；不将文件路径塞进 card_id |
| `plugin_runtime/src/lib.rs` | 固定导入白名单、任务先 read 后 complete、内存边界、共享 host_calls、取消 | 仅 IO 模式新增固定导入；异步 IO 作业的有界提交／查询，不阻塞执行线程等待整个网络请求 |
| `plugin_runtime/src/package.rs`、`manager.rs`、`instance_pool.rs` | 包准备、真实连接批准上限、控制变更先撤权、实例清理 | IO 准备／执行入口；关闭实例同时取消作业、拒绝迟到输出、回收资源 |
| `plugin_runtime/src/shared_objects.rs` | 不可变对象、租约、映射、实际宿主绑定 | 可复用固定结果传输，描述本身仍不是访问权限；无自动依赖转授 |
| `workbench_host/src/plugin_catalog.rs` | 第三方包导入、启用、独立零对象 grant 转换和表单 | 可信选择文件／网络目标、显示实际授权范围、启动 IO 任务；不能默认给所有已启用插件 IO |
| `sdk/rust/src/{wasm,ffi}.rs`、`sdk/c/include`、`sdk/cpp/include` | 三语言固定字节契约、受控 Wasm transport、所有权和释放边界 | 独立 IO codec、transport、C ABI 与 C++ RAII；暂不新增 TS／JS 插件 SDK |

`Runner` 目前只有 `morrow_v1.exchange`、`morrow_task_v1.read_input/complete`、可选 `morrow_dependency_v1.call`；不提供 WASI 文件或网络。现有取消检查发生在执行／导入边界，纯循环受 fuel 限制，fuel **不能中断任意阻塞的宿主回调**。因此不能直接在现有 `Exchange` 闭包里增加无界同步 HTTP 客户端。

## 2. 契约与兼容策略

已新增实验 `core/schemas/io.capnp`，构建时检查 schema，并由 `core/src/plugin_package/io.rs` 提供 IO 版本 1 及精确摘要。包要求 `io-v1`，guest ABI 仍为 2。运行期 codec／`core/src/io.rs` 尚未实现；该 IO 扩展尚未冻结，未来契约变化必须显式更新版本／摘要，不能重编旧候选掩盖不兼容。保持 runtime **7**、task **3**、UI **1**、dependency **1** 的原 schema、摘要、解码与路由不变，保持 `sdk/compat/guest-v1-rc1` 所有原件不变。新宿主对原包默认 IO 权限为空；旧宿主遇到 `io-v1` 明确拒绝。

包元数据建议追加独立 `IoDeclaration`（预编译 PB，例如 `io_manifest.proto`，由 Manifest 新字段承载），包括 schema 版本／摘要、申请的 IO 类别、具有外部效果的 handler 名及预算。不能复用现有七种 `Capability` 的数值。保留旧 Manifest 的读取和原件编码；扩展主加载器不修改冻结目录里的历史 schema 副本。Registry 新版本存 IO 批准子集，与包选择及依赖锁同一次 CAS 提交；旧 v1 迁移为空 IO 批准，不恢复任何运行期引用。

已实现声明类别：FileRead、FileList、FileCreate、FileReplace、FileDelete、HttpRequest、HttpListen、HttpPublish、CredentialUse、WebSocketConnect，具体数值见 io_manifest.proto。列举不由 read 推导，replace 不等于 create，delete 不由写权限隐含；出站、监听、发布和凭据使用分开批准。方法、目标与路径约束由后续资源授权收紧，不能从 HttpRequest 得到任意 URL 或凭据。识别类别并保存批准仅证明声明兼容；全部 IO 执行入口当前仍不可用，不将批准上限当作具体对象授权。

IO 运行期请求包含 version、schemaSha256、callId、resourceRef／jobRef、operation union；响应绑定**实际请求原帧** SHA、callId、稳定状态码、有界载荷及 EOF。宿主从实际连接确定调用者，不相信包 ID 或请求自报实例。引用由宿主随机生成，绑定 host、连接代次、包摘要、对象类别、权限、期限、预算；猜中引用值也不能跨连接调用。持久化只保留历史引用说明，不把它重新当有效授权。

建议固定导入 `morrow_io_v1.call(i32,i32,i32,i32)->i32`，输入及整个输出区各最多 128 KiB；先核对地址、长度、溢出、重叠、任务阶段、计数与取消，再复制输入和进入 broker。输出后重新取得 Wasm memory view；回复长度、结构、关联摘要全部验证。返回正长度为帧、负值为固定 transport 错误；普通 IO 拒绝写在结构化响应内，不伪装成业务成功。

共享现有 host_calls 硬上限，每次 IO submit/poll/read/cancel 都计费；另有 IO 累计读写字节、总作业、并行数与实际时间预算。首版建议单块 64 KiB、每实例 8 资源／4 作业、每作业最多 16 MiB、实例 IO 累计最多 64 MiB、最长 30 秒；这些是**待实现与测量的初始硬上限**，声明和用户策略只能收紧。协议支持有界 offset、分页游标、EOF，不以截断数据冒充成功。 全宿主还须有独立总准入（初始建议 32 个并行作业／256 MiB 未完成 spool），防多实例各自合法却合计耗尽资源；已发布证据沿用持久归档配额，失败／取消回收暂存但不删除已提交历史。

### 2.1 声明、批准与实例准入（已实现，执行后端待接入）

- Manifest 新 field 18 承载独立 IoDeclaration，Selection 新 field 5 承载独立 IO 批准。`io-v1`、ABI2、声明版本与精确 IO schema 摘要必须同时匹配；重复／未知类别、错配 handler 和超预算拒绝。保持原内容 Capability／GrantKind 数值不变。
- Registry 文件升为 schema 2，复用 selection.morrow 的锁、revision CAS 和原子持久化。严格读取 v1 后迁移为空 IO 批准，旧内容批准及依赖锁保留；v1 若携带新 IO 批准字段则拒绝。迁移独立处理，不能被“业务状态无变化”分支跳过；成功推进 revision，失败保留原文件。旧宿主明确拒绝 v2，不承诺数据库降级兼容。
- 包升级继续禁用；保留的批准仅为旧批准与新声明的交集，新增能力不自动批准。IO 批准变更进入现有 Manager 控制路径，先校验请求，再撤销相关实例，最后持久化；写入失败不能恢复旧绑定。无效请求不应误停实例，无变化请求可以幂等返回。
- 私有 IoContext 附着实际实例 Control，重复绑定共享同一预算。绑定至少关联 Manager、Host、Connection、包摘要、批准、期限、撤权及取消状态。启停、升级、移除、批准／依赖变更、实例关闭及 Manager 销毁使相关旧引用失效；无关实例不应误停。
- 出站请求、监听、路由发布和凭据使用分别建模；不得从 HttpRequest 推导 HttpServe，也不能由只读方法名称推断无副作用。HttpRequest 只作为出站上限，具体方法与资源仍需单独约束；声明类别覆盖当前 NET／NODE 分工，但不代表流式和服务后端已经可用。
- runtime 定义平台中立 broker 接口，由 network_node 实现网络后端，避免 runtime 反向依赖 network_node。IO handler 与纯转换入口明确分开；在执行、意图和审计未就绪时拒绝相应入口，不能退到普通 Runner 或纯任务证据捕获路径。

本轮验收已覆盖包字段／feature／摘要组合、Registry v1→v2 与持久化失败、跨 Manager／Host／实例与过期绑定、重复绑定预算和 Pool 重启；真实 IO 在途请求与迟到结果仍需接入作业后验证。原 sdk_frozen_compat／sdk_frozen_dependency 直接运行旧原件，不重编。完整执行次序见 [IO-A–IO-E](ROADMAP_UPDATE_2026-09-15.md)。

## 3. 宿主与 SDK API 草案

以下 Manager／Pool 绑定入口已实现；IoBroker、Runner IO 执行与 Pool::run_io_task 仍是建议签名。`IoBinding`、`SelectedFile`、`AuthorizedEndpoint`、`ResourceRef`、`JobRef` 均不提供 guest 构造宿主授权的方法。

```rust
// 可信本地；校验当前 Manager 选择与批准，绑定真实 Host/Connection/撤权信号。
Manager::bind_io(&self, host: &HostRuntime, instance: &ManagedInstance,
                 expected_digest: [u8; 32], expected_revision: u64,
                 requested: &BTreeSet<IoCapability>, expires: u64,
                 now: u64) -> Result<IoBinding>;
Pool::bind_root_io(&self, manager: &Manager, host: &HostRuntime,
                   session: &Session, expected_digest: [u8; 32],
                   expected_revision: u64, requested: &BTreeSet<IoCapability>,
                   expires: u64, now: u64) -> Result<IoBinding>;
IoBroker::grant_file(&mut self, binding: &IoBinding, selected: SelectedFile,
                    access: FileAccess, expires: u64, now: u64) -> Result<ResourceRef>;
IoBroker::grant_http(&mut self, binding: &IoBinding, endpoint: AuthorizedEndpoint,
                    expires: u64, now: u64) -> Result<ResourceRef>;
IoBroker::exchange(&mut self, binding: &IoBinding, request: &[u8],
                  now: u64, cancel: &Cancellation) -> Result<Vec<u8>>;
IoBroker::revoke(&mut self, binding: &IoBinding) -> Result<()>;
Runner::new_io_task(module: &[u8], limits: Limits) -> Result<Runner, Fault>;
Runner::run_task_with_io(input: &[u8], exchange: Exchange, io: IoExchange,
                         cancel: Cancellation) -> IoTaskRun;
Pool::run_io_task(&mut self, manager: &Manager, host: &mut HostRuntime,
                 session: &Session, input: &Invocation, broker: &mut IoBroker,
                 clock: impl FnMut() -> u64) -> Result<IoTaskReport>;
```

`IoBinding` 只由 Manager 根据当前真实 ManagedInstance 创建，字段私有，无公开构造或反序列化恢复入口。requested 集合只能收紧当前批准，不能作为批准来源；裸 Connection、包声明或调用者传入的 enum 集合都不能生成授权。Pool 包装入口复用已有宿主／Manager／会话关联检查。身份、引用类别、任务归属错误在推进可信时钟／扣除别人的预算前拒绝；有效身份的已执行请求正常计费。每个实际 IO 阶段及最终交付复核 Ready、批准、租约、撤权和 deadline；依赖 A→B 不自动把 A 的 IO binding 给 B。

第一版 IO 与动态依赖同时出现的包可明确 `UnsupportedCombination`；不要把缺 IO 回调转给普通 Runner。后续组合执行仍要逐节点自己的 IO binding、共享总预算和取消传播。外部表单仍只做声明式交互；IO 用独立用户任务启动，不能在预览表单时自动发网请求。

三语言共用 `IoRequest::encode`／`IoResponse::verify(request_bytes, response_bytes)`；Rust 提供 `io::submit/read/poll/cancel` 类型化包装，C 提供 `mp_io_request_encode`／`mp_io_response_decode` 与已有 SDK 释放规则，C++ 只包一层 RAII。guest 辅助 API 不包含 `grant`、打开任意 OS 路径、设置批准或选用宿主连接。错误码至少区分 Denied、Revoked、Expired、UnsupportedPlatform、InvalidPath、Quota、NotFound、Conflict、Pending、Cancelled、OutcomeUnknown、EvidenceUnavailable。

## 4. 文件访问与变更

可信文件选择器返回具体文件或目录，由宿主平台适配器保留实际句柄／授权对象。guest 收到显示名称及不透明引用，不收到可用于打开任意路径的绝对目录。选择动作只能满足本次用途，不能被插件按钮文字替代授权事实；已批准的同范围操作可按现有授权继续，不增加每块重复确认。

文件 read 针对已选文件；目录模式在独立授权后允许相对路径。约定 UTF-8、`/` 分隔、不得空段、`.`、`..`、前导 slash、反斜杠、NUL／控制字节、盘符、UNC、冒号 ADS、Windows 特殊设备名称。不得把 URL 解码或 Unicode 正规化放在路径检查之后；不支持的名称返回不可表示。列表名称、长度、文件类型和游标同样有界，末页明确 EOF；普通目录列举不宣称天然一致快照。

仅 `canonicalize(path).starts_with(root)` **不够**：打开过程必须使用平台可证明的根绑定解析，防中间 symlink／junction／reparse 替换与 TOCTOU；目标文件身份也需核对，不能只验证父路径。首切片只支持已选单文件，快照到宿主有界 spool 后按固定长度和摘要读取。外部文件复制期间可能变化；除非后端提供真正快照／适用的锁，记录的是**宿主实际固定的字节**，不能仅凭两次 stat 称为某一时刻外部文件快照。无法实现可靠目录解析的平台不开放目录能力。

写入分阶段：`beginWrite(target, disposition, expectedIdentity/version, expectedLength)`→分块 staged→`commitWrite`→查询稳定结果。create 使用不可覆盖语义；replace 必须专门批准且复核预期对象，平台不能提供条件替换时返回不支持，不能偷偷改成无条件覆盖。删除首期仅明确选定普通文件，禁止递归删除；rename／移动、符号链接、设备与可执行文件加载不在首版。文件变更不等于卡片变更，导入附件仍须走既有内容提交 API。

外部写入与 SQLite 审计不能成为一个原子事务。先将稳定 operationId、目标说明、原请求摘要与预期效果写入持久意图，再执行副作用，再保存结果；崩溃间隙标 `OutcomeUnknown`，通过目标状态与已录制凭据核对，不能猜测未写入或自动换 ID 重试。写入撤权后丢弃未发布 staged；已完成外部效果不能因取消而被宣称回滚。临时文件／spool 删除仅限宿主管理的精确范围。

## 5. HTTP 访问与网络边界

首切片 HTTPS GET：宿主批准规范化 origin（scheme／host／port）、方法、可用路径范围与字节量；插件可在范围内构造请求，但不能把 `http.get` 视为任意目标通行证。禁止 URL userinfo、非 HTTP scheme、任意代理、任意 socket、guest 自设 Host／Cookie／Authorization 等敏感头。查询参数可能含秘密，展示与审计需使用脱敏描述，实际重放所需原帧另受证据访问策略约束。

原生后端在发出请求前检查解析后的全部目标地址类别，并钉定实际连接目标；默认拒 loopback、私网、链路本地、组播及已知内部凭据服务目标。公开 HTTPS 目标和用户明确授权的本机服务分开批准；本机例外不能继承给远程重定向。DNS 变化、代理远程解析、连接复用、IPv4-mapped IPv6 均须验证真实连接符合目标策略，不只检查 URL 字符串。

默认关闭重定向。若后续开放，逐跳重新验证目的地址、scheme、origin、方法与授权，最大 3 跳，跨 origin 不传凭据；不能先自动跟随再检查最终 URL。TLS 验证不由 guest 关闭。状态码、headers、压缩前后字节、解压后总量、上传 body、响应 body 分别有界；headers 建议总 16 KiB、最多 64 个，拒 CR/LF 注入。流读取随累计预算停止，超额返回错误并保留“已产生部分外部效果”的事实。

连接／总时间／空闲时间分别限制；取消触发底层作业取消并撤销交付，不能只设置 Wasm flag。异步 worker 不持有 `&mut HostRuntime`／数据库写事务等待 IO，不跨 await 保留 guest memory 指针；poll 返回 Pending 时不忙等到无界，宿主调度下一任务，guest 自身不能在旧 Wasm Store 中无限挂起。后续 POST／PUT／DELETE 必须另授 send、采用上述持久意图与未知结果合同；HTTP GET 也可能被远端设计成有副作用，因此不自动重试已发出的请求。

凭据保存在平台凭据存储，由宿主限定目标后注入；guest 只持 `CredentialRef` 且需 `credential.use`。默认不继承宿主 cookies、不返回认证头、不打印密钥或含秘密的 URL。使用凭据访问服务意味着该服务能接收到凭据，必须与授权目标一致；禁止把重定向、插件日志或错误串变成凭据导出通道。

## 6. 审计、录制与隔离重放

新增版本化 `io_observation.proto`（PB 原件＋LZ4），记录声明／批准决策摘要、真实连接的历史说明、顺序、原 request／response 或受限数据引用、目标类别、预算、时间与阶段结果。秘密原件需要加密或不保存；选择不保存会让重放标记不完整，不能用脱敏数据声称逐字节重算一致。历史 token 永不恢复为 live 权限。

只读 IO 可复用 `read_capture`／`read_archive` 的准备、分片、Ready、失败与容量基础保存有序实际响应，但需新增 IO 语义验证，opaque part 并非自动成为 TaskEvidence。副作用不能挤进“完成读取”日志伪装成普通 read；新增独立审计事件种类和 Store／封存／快照完整性闭包，必要时显式提升数据库版本。审计无容量时在副作用前拒绝，发出后记录失败只能报告待核对。

现有纯任务 `TaskEvidence` 与 `projection` 不含 IO 响应链。`io-v1` 模块必须被纯转换捕获／回放入口明确拒绝；新 `IoTaskReport` 的输出也不能伪造纯转换 proof。IO 结果形成卡片时，经后续版本化宿主投影及当前内容 grant 提交，不能另建“插件内容库”。

离线录制式重放使用固定旧包、原任务输入、按实际顺序匹配的 IO 请求与录制响应；默认不启动真实网络、不写原文件、不加载生产凭据。首次 request、顺序、输入摘要或预算分支不同立即报告 Mismatch；缺材料为 EvidenceUnavailable；原执行 OutcomeUnknown 不能被回放推断为成功。签名只证明记录来源与完整性，不认证远端服务诚实、文件在复制时没有变化或用户已经收到结果。

## 7. 平台能力矩阵（目标，均待资格测试）

| 平台 | 文件授权适配 | 网络适配 | 首期边界 |
| --- | --- | --- | --- |
| Windows 原生 | 系统选择器＋宿主持有文件对象；目录需 reparse 安全解析 | 宿主 HTTP worker，可实现 DNS／地址和逐跳策略 | 优先完成单文件固定读取与 HTTPS GET，禁止把进程权限当插件权限 |
| Linux 原生 | 文件选择／桌面 portal，目录使用可验证的相对根解析 | 宿主 HTTP worker | 独立执行路径／symlink／取消测试，不能由 Windows 通过推定 |
| macOS 原生 | 沙箱环境保留 security-scoped URL 的访问生命周期 | 宿主 HTTP worker，遵守应用网络权限 | 不把 URL 字符串等同于有效安全范围授权。[Apple](https://developer.apple.com/documentation/security/accessing-files-from-the-macos-app-sandbox) |
| Android | SAF 单文档／document tree，使用 provider 对象而非伪造文件路径 | 宿主 HTTP worker，遵守实际应用权限 | provider 可能远程或不提供原子替换，按能力返回不支持。[Android](https://developer.android.com/training/data-storage/shared/documents-files) |
| iOS／iPadOS | document picker＋security-scoped URL | 宿主网络适配与平台限制 | 生命周期和后台取消单独验收，不能等同永久磁盘路径。[Apple](https://developer.apple.com/documentation/uikit/providing-access-to-directories) |
| Web | 用户选择的 File／Blob；支持的环境可给文件句柄；OPFS 为 origin 私有存储 | Fetch、CORS、AbortController；不能取得原生 DNS／连接 IP 控制能力 | 默认不跟随重定向、不继承凭据；无法满足“禁止内网地址”的强策略时明确 Unsupported，不能把 CORS 当网络访问控制 |

Web 的 manual redirect 可产生不可读 `opaqueredirect`，所以首版以 `redirect: error` 拒绝重定向，不假称可检查每跳；CORS 控制脚本读取结果，不保证请求没有发出。[Fetch](https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API/Using_Fetch)、[CORS](https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/CORS)。OPFS 不代表访问用户的任意文件系统，也不能因浏览器不用逐次弹窗就给所有插件共享全部 origin 数据。[OPFS](https://developer.mozilla.org/en-US/docs/Web/API/File_System_API/Origin_private_file_system)。

## 8. 最小完整实施顺序与验收

1. **独立契约与三语言 codec**：新增 IO schema／PB 声明／Registry 版本，IO import opt-in；保留旧加载器路径。验证新旧包共存、feature／摘要错误拒绝，整个 frozen guest gate 不重建原件。
2. **真实实例资源 broker＋选中文件读取**：实现 Manager 批准、宿主选择文件、固定原件、64 KiB 分块、EOF 与取消，C／C++／Rust 实际 guest 逐字节读回；关闭／升级后旧引用失败。这一步必须贯通工作台真实入口，不能只有 broker 单元测试。
3. **HTTPS GET 完整切片**：固定授权 origin、无重定向、实际异步 worker、预算／超时／取消、三语言示例；用本地合成服务在显式本机测试授权下覆盖成功与恶意响应，不依赖公网。与文件读共同保存录制响应，删除源文件／关停服务器后离线重放仍匹配。
4. **副作用接口**：先完成持久 intent／未知结果／签名事件与恢复测试，再开放 file.create／replace／delete 和 http.send；没有这组边界前 UI 不显示为可用能力。
5. **平台资格与分发**：分别验证 Linux／Apple／Android／Web 适配；开发工具追加 IO 模板及 manifest preflight；发布准确支持矩阵。首切片不混入目录树、递归操作或任意 TCP。长连接/SSE 与 WebSocket 是完整网络能力的后续必做工作包，不能以首切片通过将它们无限后置；后台生命周期按声明平台单独验收。

| 正向证据 | 必须配套的负向测试 |
| --- | --- |
| 原 guest-v1-rc1 所有包直接运行 | 未声明 IO 却导入、错误函数签名、未知 schema／版本、WASI fs/socket、旧功能被无意增权 |
| 三语言同一请求／响应与实际文件 bytes | 非对齐 0..7、截断／尾字节／巨段／遍历上限、地址溢出、缓冲重叠、complete 后调用；边界失败不得进入后端 |
| 单文件分块 EOF、合法目录分页 | 跨 Host／实例／包摘要、旧代次、过期／撤权、guess token、offset 溢出；路径逃逸／ADS／junction 竞态；不足预算不伪造 EOF |
| GET 正常响应／合法授权凭据注入 | DNS 改绑／私网目标／IPv6 映射／代理、跨 origin 重定向、TLS 错误、头注入、压缩炸弹、超量流、慢连接／空闲超时 |
| 实例关闭回收、后端取消 | 调用中撤权与升级、取消后迟到响应、依赖传递权限、并发共享总预算耗尽、终止失败不得重新授予旧资源 |
| file.create／条件 replace／delete 与 HTTP send 结果查询 | 请求／效果／回执每个崩溃边界，已执行后超时、错误同 operationId 重试、变更用户文件不匹配、未授权递归／覆盖 |
| 固定 IO 记录＋签名／备份＋断源重放 | 原件被换、少片／换序／错请求绑定、凭据泄漏、证据额度满、缺材料、Unknown 误报成功、重放真实连网／写磁盘 |

下一阶段的具体首个代码入口是 `core/src/io.rs`＋`plugin_runtime/src/io_broker.rs`＋Runner 的显式 IO 模式，再由 Manager／Pool 和 Workbench 接通真实文件选择。不能以开放未授权 WASI、普通目录路径拼接或在现有纯转换回调里直接联网代替该接口。

## IO-C 持久化增量

意图记录现已接入 Store v15 与现有审计／快照链，具体格式、幂等历史读取及限制见 [意图记录设计](IO_INTENT_RECORDS.md)，验证见 [持久化验收](../reports/road-07-io-intent-store.md)。当前记录不含受保护 IO 原件，也未预留整个操作的后续完成容量，不能作为外发许可。上文完整 IO 范围和实际后端验收要求保持不变。

## 2026-09-24：三语言 SDK 接入

已在现有宿主能力上补充 C／C++／Rust codec 和 Wasm 调用包装，详见 [IO SDK](../sdk/IO_API.md) 与 [专项报告](../reports/plugin-io-sdk-2026-09-24.md)。本轮不修改原 IO schema、不扩大旧兼容基线；Read／Finish／Cancel／SubmitHttp 与仅有编码形状的 SubmitFileRead／Poll／QueryOperation 明确区分。完整文件写入、异步恢复与网络流式能力仍开放，不能将 SDK 包装完成理解为本文所有设计均已实现。
