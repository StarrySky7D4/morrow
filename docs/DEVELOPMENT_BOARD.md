# 后续编码看板

更新：2026-09-20。基线：test.52 开发线 457e023 与隔离分支 `codex/io-safety-refactor` 的 Track A 修补／重构；应用版本仍为 `0.1.9-test.52+56`。本看板随代码提交维护，是当前任务状态入口；总架构与退出门槛见 [主路线](FUTURE_ROADMAP.md) 和 [执行路线](ROADMAP_UPDATE_2026-09-15.md)。版本号、编译和测试数量不替代产品验收。

状态含义：已验子集＝对应限定实现通过；下一项＝可开始编码；待前置＝须先通过列出的门槛；可并行＝不修改正在整合的核心契约；研究＝不得作为运行后端上线。本轮隔离修正与验证证据见 [修正报告](../reports/io-safety-refactor-2026-09-19.md)；前置提交608ccc3已同步到同名远端开发分支；本轮后续改动的本地验证不代表已发布或主应用端到端验收。

## 已验子集

| 编号 | 范围 | 证据及仍未覆盖的范围 |
| --- | --- | --- |
| IO-A | 当前 IO 声明、Registry 批准、Manager／Pool 实例绑定 | [准入报告](../reports/road-07-io-admission.md)；声明不是资源授权 |
| IO-B1 | 当前协议 Read/Finish/Cancel、raw Runner IO、固定字节 FileBroker、预算／撤权／回收 | [整合验收](../reports/track-a-integration-2026-09-16.md)；15 codec + 12 raw + 15 managed 回归包含在核心396／运行时271项内；不是异步作业或选择器 |
| IO-C0 | Store v16 意图历史及后续意图／审计逻辑预留 | [意图记录](IO_INTENT_RECORDS.md)；没有受保护请求／响应材料或真实效果核对 |
| ROAD-04a | test.52 宿主中英文界面 | [i18n 范围](I18N_PREVIEW.md)；插件消息、RTL、业务值迁移未整项通过 |
| SDK-BASE | 冻结 C／C++／Rust 原包兼容 | 冻结基线为36固定文件／13原包对；本轮执行结果见修正报告；新 IO 仍实验性 |

## 编码队列

| 顺序／编号 | 状态／优先级 | 模块与前置 | 可评审产物与退出证据 |
| --- | --- | --- | --- |
| 1 / IO-C1 | 已验存储子集 / P0 | core 证据存储；依赖 IO-C0 | Store v17 受保护原件、原容器身份和共享容量预留；读取／幂等重试有界校验；满额、真实满盘、撤权、崩溃重开、材料缺失均可解释，旧签名原件不改写 |
| 2 / IO-C2 | 已验 broker 子集 / P0 | runtime broker＋core，沿用 IoBinding | 同 operationId 唯一活跃执行、请求匹配、原代次退休及恢复核对；现用 Store 严格认领，两个独立宿主竞争同一操作时仅新提交成功者可外发；重复提交、并发绑定、发送边界中断不导致重发，历史记录不恢复授权 |
| 3 / IO-B2 | 已验调度＋托管准入＋持久子调用 / P0 | runtime 作业调度＋独立契约路由 | 有界 submit/poll/read/cancel、Ready 最终交付撤权、声明预算、温和排空已验；已接真实 Manager/IoBinding 的撤权与原实例共享 job/bytes；[托管证据](../reports/managed-io-jobs-2026-09-19.md)。同一作业子调用已贯通 Prepared／发送边界／Observed，无重复计费；[接线证据](../reports/brokered-io-jobs-2026-09-19.md)。HTTP端点与原实例资源批准已接真实传输；后续连接主应用、持久批准与其它资源 |
| 4 / IO-D1 | 已验本机 HTTP/TLS 出站子集 / P0 | guest→Manager/IoBinding→broker→network_node | [托管 HTTP](PLUGIN_MANAGED_HTTP.md)：原实例端点批准、精确 origin/方法/凭据引用、真实 POST/状态/重复头/原件、发送后断线不重发与 Ready 撤权已验；[持久端点批准与Windows系统保护凭据](PLUGIN_OUTBOUND_AUTHORITY.md)已接线，仍待主应用、真实提供者核对、路径范围和更多平台 |
| 4 / IO-D2 | 已验本机受管服务子集 / P0 | broker＋network_node 受管服务 | 独立 service 帧／声明 tag 7、真实 Manager 的发布与监听批准、同 worker 路由及 Principal service scopes 已接线；这是宿主显式发布，非 guest 动态注册。本机 HTTP/TLS 的认证／冲突／额度／撤权／节点关闭已验；[持久请求](PLUGIN_SERVICE_HISTORY.md)已接同一 Store 的原子准备／唯一认领／响应原件重试／TTL，真实断线重启恢复已验；[内容权限交集](PLUGIN_SERVICE_CONTENT.md)已通过实际 HTTP 读写与重放验证；[只读状态查询与稳定配置](PLUGIN_SERVICE_RECOVERY.md)已接线；[入站批准与认证解析](PLUGIN_SERVICE_AUTHORITY.md)已接原Store及真实HTTP/TLS；Unknown核对、跨平台凭据提供者、UI 与新三语言 SDK 未完成，见 [实现合同](PLUGIN_MANAGED_SERVICE.md) |
| 4 / IO-D2a | 已验原生内容子集 / P0 | 远端主体与内容权限交集；依赖 IO-D2 | 原逐对象 grant probe＋service policy＋真实 principal scope；7类命令保留原事务授权，Ready／重放复验，范围变化同key冲突，HTTP实际读／改名／重启重放已验；持久配置记录已接原Store，入站批准解析已接线，主应用 UI仍待接入，见 [合同](PLUGIN_SERVICE_CONTENT.md) |
| 4 / IO-D2b | 已验配置／查询子集，整体进行中 / P0 | 持久服务配置与恢复操作 | 原Store v18保存稳定namespace、主体／批准引用与修订CAS；新实际grant恢复journal；原worker只读查询不认领、不执行，真实HTTP重启与响应边界已验，见[合同](PLUGIN_SERVICE_RECOVERY.md)。Store v19入站认证摘要／发布批准、原拥有者写锁与撤销、原worker配置修改和HTTP/TLS绑定已验；出站受保护凭据已接Store v20及原worker；下一项主应用配置，再补Unknown核对、因果关系、跨进程时钟高水位和证据退休 |
| 4 / IO-D3 | 下一项，可独立推进 / P0 | 平台文件适配＋broker | 系统选择、目录枚举、创建／替换／删除，资源越界／替换冲突／撤权／崩溃结果核对；固定读取保留兼容测试 |
| 5 / IO-E1 | 待 B2/D1/D2/D3 契约验收 / P1 | sdk/rust、sdk/c、sdk/cpp | 三语言类型化 IO、同一正负向量与独立仓库插件；旧原包原样执行；新扩展单独形成兼容候选 |
| 5 / IO-E2 | 已验类别、凭据、端点与Rust任务状态子集，整体进行中 / P1 | workbench_host＋Flutter 管理界面 | [管理接口](PLUGIN_IO_MANAGEMENT.md)已接私有协议与真实Registry：声明／批准分离、明确保存／撤销、修订校验与重启恢复；原Store有界分页、Windows凭据与[具体端点批准](PLUGIN_ENDPOINT_MANAGEMENT.md)录入／替换／停用已验。[Rust应用任务状态](PLUGIN_APP_IO_TASKS.md)已接原Storage所有权、Busy与恢复；[HTTP任务消息与关闭](PLUGIN_APP_HTTP_TASKS.md)已接私有通道和Dart接口；[HTTP任务页面](PLUGIN_APP_HTTP_TASKS.md)已验Windows真实Rust guest/凭据/响应与设置重挂；下一项API节点管理与Unknown证据核对，用户资料无隐式迁移 |
| 5 / ROAD-08-IO | 待 C1/C2 与实际后端 / P0 | 录制证据与独立验证器 | A→B→IO→内容提交→封存→删除安装来源→隔离重放；真实故障、合法退休与缺材料分类；重放禁止实际外发 |
| 6 / IO-E3 | 待基础双向 IO / P1 | NET-2–8／NODE-4–7 按各自依赖 | OAuth／多账号、上传下载、分页限流、流/SSE/WebSocket、webhook、持久服务与 TLS 运维；每个 profile 单独验收 |

## 可并行及研究

| 编号 | 状态 | 下一步与边界 |
| --- | --- | --- |
| ROAD-01b | 可并行 / P0 | 补主应用／测试／平台固定源码构建回执；不把本轮源码整合写入旧预览归档 |
| ROAD-02/04b | 可并行设计 / P0 | LiteralText／MessageRef、命名空间、任务语言上下文、坏包和RTL向量；固定接口后才接插件 UI；不新增 TS/JS 或动态 Dart 插件 |
| ROAD-12/AND-01 | 可并行探针 / P0 | Android Rust/Wasm 引擎提取与执行域能力；编译、设备、隔离分别记证据，不能继承 Windows 通过状态 |
| ROAD-05/06 | 可并行固定接口验证 / P0 | 真实跨进程 A/B 故障、证据容量与退休；存储／schema 变更需与 IO-C1 串行整合 |
| QUIC-RESEARCH | 研究 / P1 | 先统一迁移开关、IPv4/IPv6共享端口计数及层次依赖；再提交真实 socket/TLS/传输原型，下载的计数器／布尔模型不接产品 |

## 每次合入检查

记录基线和实际范围，复核 schema／数据库／授权唯一权威；跑改动相关回归、冻结原包完整性及执行兼容。影响共享模块时检查 wasm32 编译，原生 IO 另报平台资格。完成一项只移动该子项状态，ROAD-07、完整 SDK 和 M0–M7 不因局部通过整体勾选。测试版继续沿 test.x 推进，0.2.0 仅在声明范围达到既定门槛后评审。

## 本轮恢复边界验收

持久入站与跨宿主唯一外发的限定结果见 [报告](../reports/service-history-2026-09-19.md)：core 499、runtime 356、network 72 项通过，均无失败；5个 ignored 为父测试实际启动的崩溃子进程入口。三 crate 严格静态检查、默认 wasm32 库编译与冻结 SDK 原件检查通过。不是全平台运行、新 SDK 稳定或完整插件产品验收。

## 本轮内容权限验收

[内容服务报告](../reports/service-content-2026-09-19.md)：核心510、运行时370、网络78项通过，均无失败；主应用宿主116个测试入口通过（含3个既有子进程入口，不将父测试内的子进程输出重复累计）。实际 HTTP→Wasm→core 读取／改名、主体隔离、权限收窄后缓存拒绝和重启原回执恢复已验；旧 dispatch 采时保持兼容，新增 guarded 入口执行最终授权检查。IO-D2b的配置／只读查询进展见下；IO-D3 文件系统后端可沿固定 Broker 边界独立推进。

## 持久配置与恢复查询进展（2026-09-20）

原Store的配置修订、原worker查询及实际HTTP入口已贯通，证据见[本轮报告](../reports/service-recovery-2026-09-20.md)。所有查询保留原授权、容量与TTL；Missing不写记录、Prepared不认领、Unknown不重发，Observed返回原件。配置仅为期望状态，主体认证与资源批准引用不能恢复旧权限。下一编码顺序：主应用授权／凭据与任务配置 → 有证据的Unknown核对与完整因果链 → 证据退休；三语言SDK和主应用UI依赖这些契约继续推进。

## 持久入站授权进展（2026-09-20）

[批准合同](PLUGIN_SERVICE_AUTHORITY.md)与[验证报告](../reports/service-authority-2026-09-20.md)：原Store v19记录认证摘要和精确发布批准，原实际实例重新准入；配置或认证更新通过原worker执行并阻断迟到交付。数据库副本、独占模式与显式原生VFS遵守同一授权锁。仅Windows本机HTTP/TLS及存储故障测试通过；没有主应用发布UI、出站秘密保险库或全平台运行结论。

## 持久出站授权进展（2026-09-20）

[出站批准合同](PLUGIN_OUTBOUND_AUTHORITY.md)：端点批准和系统保护凭据保存在原Store v20，Windows使用与审计密钥隔离的DPAPI域；实际插件实例与凭据使用权限在解密前复核，修订更新继续走原worker并撤销旧活动授权。后续主应用需提供端点批准、凭据录入／轮换、任务与恢复界面；其他平台凭据提供者、OAuth及完整网络SDK继续独立验收。验证结果见[本轮报告](../reports/outbound-authority-2026-09-20.md)。

## 主应用 IO 类别管理进展（2026-09-20）

[接口合同](PLUGIN_IO_MANAGEMENT.md)与[验证报告](../reports/plugin-io-management-2026-09-20.md)：主应用支持独立保存／撤销网络和文件类别，保持内容批准与启用状态；真实 Flutter→Rust 进程重启恢复通过。切换工作台后的迟到回包与旧关闭失败已隔离。类别批准不代替资源授权，也没有接通实际网络任务。

本节当时的下一项为原Store有界元数据列表与凭据录入／轮换；完成状态见下节。禁止另建运行时或数据库绕过唯一权威；Unknown核对、因果链、文件系统及完整SDK继续保持原退出门槛。

### Web 构建阻断及修复

上一轮 Flutter Web JavaScript Release 因 Cap'n Proto 反射代码的64位schema ID无法精确表示为JavaScript数值而失败，历史记录见[故障报告](../reports/plugin-io-management-2026-09-20.md)。本轮生成器保留原生反射，并为Web提供精确十六进制／BigInt身份侧表；Web关闭可选int反射，不修改消息布局。UiEvent三个UInt64字段使用两个UInt32传递，真实Chrome边界向量和完整Web JS Release均通过，见[修复报告](../reports/credential-admin-web-2026-09-20.md)。这不代表所有运行期UInt64路径、浏览器存储或Web插件IO均已验收。

## 主应用凭据管理进展（2026-09-20）

原Store v20在同一读事务进行有界分页和快照核对；主应用Windows凭据面板完成新建、替换、停用和进程重启恢复，仅返回元数据。原DPAPI保护、原Store CAS和撤权协调器继续是唯一权威；保存凭据不会启用插件或创建活动网络授权。[报告](../reports/credential-admin-web-2026-09-20.md)记录真实Flutter→Rust、缓冲区清理、页外损坏、混合记录空页及Web修复证据。IO-E2仍为部分完成，新SDK未冻结。

下一编码顺序：完整Storage交接底座（进展见下节）→ 主应用可恢复任务状态机 → 原端点批准管理与短响应start/poll/read/cancel → 真实主应用HTTP链路、撤销与重启核对。文件系统后端可沿固定broker边界独立推进；凭据期限不代替活动授权，Unknown结果不能自动重发。

## 受保护存储的 IO 所有权交接（2026-09-20）

[所有权合同](PLUGIN_IO_OWNERSHIP.md)：IoWorker 可移交完整 HostOwner，Windows Storage 保留原审计 Session、签名身份、数据库／身份／Registry 租约。独立准入 IO 实例而不拆取 Pool 根；异常结束归还原容器、执行／断连／维护状态及必要的待清理实例。原运行时默认 API 保留，维护失败不把已发生的 HTTP 效果改写为未执行。

这属于 IO-B2／IO-E2 的宿主底座子集。真实受管 Wasm→本机HTTP→原审计Store→封存／重开已验，详见[报告](../reports/io-owner-2026-09-20.md)。CLI／Flutter 主应用命令循环尚未使用新交接入口，不能标记主应用网络任务完成。下一项是在主应用中建立显式的存储在线程中／停止待退出／已取回状态，再接端点批准和短响应任务协议；文件系统、API节点管理、Unknown核对与完整SDK保持原门槛。


## 2026-09-20 主应用任务所有权状态

[应用合同](PLUGIN_APP_IO_TASKS.md)：Workbench Rust 入口可使用原 Manager／Store 独立准入 IO 实例，启动一项受管任务并非阻塞查询、读取、取消、回收和恢复；StorageSlot 对现有内容方法显式提供 Busy，协议提前拒绝依赖存储的文件／上传／批准副作用。Ready 仍须最终读取授权校验，停止请求不冒充线程退出，退出诊断与实际修复分开保存。

本轮验证记录见[报告](../reports/app-io-tasks-2026-09-20.md)。这不是 CLI／Flutter 已有网络任务界面：下一项具体端点批准管理 → 私有任务消息、界面状态与 EOF／关闭待退出 → 实际用户路径、撤销和重启核对。文件系统、API节点、Unknown核对、因果链、三语言SDK与平台资格保持原有退出门槛。原任务句柄不持久化，不恢复旧授权，不自动重试外部效果。

## 主应用端点批准进展（2026-09-20）

[端点管理合同](PLUGIN_ENDPOINT_MANAGEMENT.md)与[验证报告](../reports/endpoint-admin-2026-09-20.md)：原Store完整政策分页／保存／停用、私有协议、Dart原生适配、中英文界面及进程重启恢复通过。保存只记录批准，不启用插件、不解密秘密、不建立连接；冻结SDK不变。下一项明确为CLI／Flutter短响应任务消息、关闭待退出及实际用户请求路径；IO-E2整体仍进行中。

## HTTP任务与关闭接线进展（2026-09-20）

[接线合同](PLUGIN_APP_HTTP_TASKS.md)：真实Rust HTTP-forward guest仅允许原输入帧的一次转发；原Store解析端点与凭据、原实例授权、非阻塞私有任务控制及Dart接口已接入。CLI关流等待实际存储回收，Dart不再五秒强杀。下一项为用户任务表单、状态及错误恢复页面；随后实际用户路径、API节点管理与文件系统，不把本阶段解释为完整网络UI或SDK稳定。验证见[本轮报告](../reports/http-task-control-2026-09-20.md)。

## HTTP任务页面进展（2026-09-20）

明确提交、状态、结果、取消、原实例恢复及完成确认已接插件库。当前包handler来自原目录，不从权限类别推断。设置收起保留原后端结果和未知状态，恢复观察只查询；新鲜Local/无TaskKey时才可显式归档未知尝试，不代表远端回滚。[页面报告](../reports/http-task-ui-2026-09-20.md)记录85项Dart回归、真实Windows凭据/HTTP表单及Web构建。下一顺序：API节点配置与发布控制 → 跨重启Unknown证据核对/因果链 → 文件系统后端 → 三语言IO SDK候选；整个IO-E2仍进行中。

## API节点管理底座进展（2026-09-20）

已接Workbench Rust配置、一次性认证令牌、精确发布批准与停用，原Store提供整表校验的稳定分页；保存不启用、不监听。新15项定点测试、宿主完整165项及原存储40项回归通过，见[报告](../reports/service-admin-2026-09-20.md)。尚未接私有消息或Flutter管理页面。

下一编码顺序细化为：[管理合同](PLUGIN_SERVICE_MANAGEMENT.md)中的消息/模型/双语表单 → 常驻监听租约与单请求预算分离 → ServiceHost保留完整原Storage的泛型化与统一停止回收 → 真实入站/查询/故障用户路径。现有30秒实例寿命和长期占用内容库的Busy不能作为常驻节点最终方案，也不能通过自动反复绑定或旁路数据库绕过；解决后再验Unknown持久证据、文件系统及三语言IO SDK。IO-D2b与IO-E2整体保持进行中。

## API节点私有消息进展（2026-09-20）

七个管理动作已连接Rust宿主、私有Cap'n Proto与Dart原生适配；一次令牌独立所有权、回复缓冲清理、连接失败封锁和历史IPv6 scope读取已有回归。真实Windows进程证明配置/批准在原库重开后保留、轮换和修订冲突有效，保存不会自动监听。详见[消息接线报告](../reports/service-wire-2026-09-20.md)。

下一项收敛为双语配置/认证/批准页面及一次令牌呈现；之后仍按管理合同完成常驻租约、原Storage调度和监听/worker真实回收。当前没有新增服务管理页面、主应用监听入口或三语言IO稳定承诺。版本保持不变，本阶段仅本地提交。

## API节点管理页面进展（2026-09-20）

双语配置/认证/发布批准表单已嵌入插件库，支持逐对象范围编辑、一次令牌呈现与清理、目录变更保留草稿和明确重绑定。未知写入留在原后端会话，重新打开页面不自动重发或解锁。Windows真实表单已验证创建、编辑、轮换、批准更新、停用及原库重开；115项相关Dart回归、语言包检查与Web构建通过，见[页面报告](../reports/service-ui-2026-09-20.md)。

下一项是管理合同中的常驻监听租约与每请求预算分离，随后完成ServiceHost完整Storage所有权、统一访问调度、监听监督/worker真实退出与恢复，再连接启动/停止及真实入站用户路径。现有页面只管理期望配置和批准，IO-D2b/IO-E2整体仍进行中；Unknown证据核对、文件系统和三语言IO SDK继续保持原门槛。

## 服务运行所有者适配进展（2026-09-20）

独立前置ServiceHost完整所有权适配已落地：泛型HostOwner、构造失败归还原worker、非阻塞停止请求、真实join后返回完整WorkerExit，以及可取消等待但不丢失owner的shutdown_owned。原HostRuntime构造和bind空路由调用保持源码兼容。真实Windows原Storage上的HTTP执行、端口冲突和封存失败修复已验；监听关闭与worker归还分别检查，见[所有权报告](../reports/service-owner-2026-09-20.md)。

后续按[常驻运行方案](PLUGIN_SERVICE_RUNTIME_PLAN.md)实施：显式版本化运行租约与每请求预算 → 包含原Pool/Manager/内容状态的WorkbenchState和统一有界调度 → 长耗时等待可暂停与工作台共存 → 主应用启动/停止及完整故障用户路径。本轮没有放宽30秒旧声明，也没有新监听按钮；常驻期间内容Busy仍未解决，不能标记常驻API节点完成。

## 服务有限运行租约进展（2026-09-20）

`service-run-v1`独立版本化声明与可信宿主 `bind_service_run`已实现：有限时长上限暂为一小时，每请求仍受原IO短期限；原实例一次签发、真实时钟截止、回退/过期失效及原累计字节账本保持。真实同一监听器31秒后执行第二个不同请求，旧30秒绑定不再是新profile的限制；旧包行为不变。详见[有限运行报告](../reports/service-run-2026-09-20.md)。

这是常驻节点的前置原型，主应用尚无启动入口。一小时稳定性、累计作业总额、显式续租与完整WorkbenchState调度仍未完成；不能用原型替代服务期间内容界面的可用性验收。下一步继续完整工作台所有权和调度，同时收敛版本化运行预算与续租；IO-D2b/IO-E2整体保持进行中。

## 服务累计任务与字节预算进展（2026-09-20）

`service-run-budget-v1`增加明确的累计任务保留次数和字节声明，可信宿主通过新入口批准更小额度；原IoContext统一计费，取消/丢弃/取结果不退款，单独资源占用不计任务。最后一个获准任务不会因任务额度耗尽而失去交付资格；四条字节计费路径均执行宿主上限。旧profile不重解释，新字段和feature成对验证。详见[累计预算报告](../reports/service-run-budget-2026-09-20.md)。

下一项是显式续租的原身份/修订/额度更新规则，以及完整WorkbenchState和有界调度；主应用监听按钮、服务期间内容界面的响应和未知结果核对仍未完成。IO-D2b/IO-E2继续进行中，当前没有SDK稳定承诺或发布动作。

## 原声明内显式续租进展（2026-09-20）

预算profile的可信宿主续租入口已接原IoWorker与ServiceHost：固定原Manager/Control/包、当前批准、原grant和两级期望修订；不创建新实例，不清除累计账本。更新只能扩大当前批准并保持在首次签发确定的声明总时长、任务和字节上限内。所有旧绑定副本共享新期限，单请求、认证、发布和历史结果期限保持独立；停止/排空、撤权、到期和时钟回退不能恢复。详见[续租报告](../reports/service-run-renewal-2026-09-20.md)。这是有限原型，不提供无限续租或稳定SDK承诺。

当前下一编码顺序：抽出持有原Storage/Pool/Manager/内容会话/undo/暂存的完整WorkbenchState → 原执行者的有界命令队列与管理容量预留 → 长IO等待期间的可暂停执行和工作台共存 → 主应用显式启动/状态/停止/修复及真实HTTP/TLS用户路径。前一步不能通过只给IO worker转移裸Storage或在旁路Store写内容替代。随后继续Unknown证据核对、完整文件系统、三语言IO SDK候选及各平台资格；IO-D2b/IO-E2整体仍进行中。

## 宿主命令预留通道进展（2026-09-20）

原生执行者已增加独立8项宿主命令保留，覆盖排队、执行和Ready未读；取消后的未知结果、原时钟复验与两条队列轮转都有专项。`ManagedHostOwner`允许原Manager随整个拥有者移动，失败仍归还原对象。真实Windows组合拥有者在同一HTTP监听期间保留Storage/Pool/Manager，查询原Store已执行记录并完整回收，见[验证报告](../reports/owner-commands-2026-09-20.md)。这是调度前置，不是全部工作台内容方法已在线程中运行。

下一步收敛为：内部Manager的续租/撤权管理命令 → 完整WorkbenchState提取及现有内容命令接入 → 长IO等待可暂停 → 主应用界面和故障路径。内部Manager目前不能供外部旧续租方法借用，这一缺口不能通过复制管理器解决。宿主handler与旧IO router仍同步执行，预留队列不等于长任务期间的响应时间保证；主应用内容Busy尚未消除，IO-D2b/IO-E2及完整SDK门槛保持未完成。
