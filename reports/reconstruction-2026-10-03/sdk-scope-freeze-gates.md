# W07 当前 SDK 覆盖与冻结剩余门槛（只读）

## W14 最新限定推进（2026-10-03）

无凭据 `managed-channel` 原生 POST SSE source 已接原Store strict claim与原Receive/ACK；新三语言guest真实消费，17新方法及117相关回归共134通过。原ABI/pins/SDK327/frozen57不变，HTTP EOF/ACK/join各自记录，业务Unknown不重放。G02现在是 **IMPLEMENTED_EXPERIMENTAL_POST_SSE_SLICE / REMAINING_OPEN**：WS、Account/认证、TLS/公开API/平台与Workbench生产owner/目录仍缺资格。下方W07矩阵是原静审底本；其中“公共SSE未实现”仅描述当时状态，当前以 [W14报告](windows-sse-channel.md) 和 [接口指南](../../docs/PLUGIN_SSE_CHANNEL.md) 的精准限定能力为准。

G07延续W13只读HTTP单operation历史查询，不因SSE EOF扩大Observed/恢复/重放。其它组维持完整原目标；原SDK014 channel产物未找到，新guest不替代其原件资格。没有把 `network_backend=false` 或生产binding翻成available，整个SDK仍OPEN。云端Linux汇合代码未修改，云端Close199不计本阶段Windows证明。

日期：2026-10-03。结论：**OPEN_FULL_TARGET_SDK**。本轮未运行测试、插件业务、网络、CI、GUI；未打开真实 DB、protected Session、DPAPI、账户密钥；未修改生产源码、Git index、提交或推送。W05 原生 codec／W06 分发单独验证，本只读报告不重复其工作，也不把两项通过当成完整 SDK 冻结。

源码范围：Windows SDK 限定 worktree，分支 codex/windows-sdk-qualification-20261003。
HEAD：63f38d4a8a5bf453248dc7532197bee6980dc86f；HEAD tree：a8a74bc4563fae735e9b695f50db4e979ac1393d。
待提交来源 tree：8e77d5d6f448291ab3029976aad07a2c4d6b93f8，来自 w02-w04-delivery/source-seal/source-identity.json 的实际封存／临时索引还原记录，并非本轮重新计算或已提交 HEAD。
来源记录为11个 modified文件＋3份新增report。source-inventory.json保存本轮实际读取的主要静态输入摘要；不宣称全仓库无漂移。

## 当前证据各自能证明什么

| 现有记录 | 支持范围 | 不足以关闭 |
| --- | --- | --- |
| 2026-10-03 takeover base9＋dependency3、原provider | 旧C/C++/Rust有界内容／Transform／UI／批准slot运行；原件未重编／重封 | 第三方真实接入、公开新能力、全平台 |
| takeover mapping7／objects14／reader9，descriptor10＋9 | Windows受管共享映射及描述符限定范围 | 非Windows映射；公共guest大对象／零拷贝API |
| transport HTTP4＋service2，57固定原件恒同 | Windows本机真实七方法／bytes／合成凭据注入／续租撤权期限／Unknown原库重开 | POST SSE／WS／OAuth／跨进程完整业务核对／GUI |
| runtime7套76个唯一测试 | 受管服务绑定／累计预算／续租／durable jobs（脚本后端） | 真实网络或产品全链；不能与六包相加 |
| SDK-only三语言消费者 | 独立复制SDK依赖闭包、本地回调所有权、成功与失败不重试 | 正式分发、第三方、原bounded runner、恶意native隔离 |
| 当前Rust SDK92 reported ok | Windows x64编解码／FFI／本进程合成guard-page，1项按需导出body未运行 | 92项能力、产品、普通token或跨平台稳定 |
| SDK014历史20项11pass／9fail/error | 旧产物的真实生产检查点，保留失败 | 当前候选channel PASS；已有源码修复亦不能自动升级结果 |
| Linux stage15 | owned VFS、crate-private guarded card/ACK slice、pidfd/controller基础和合成wire | 公共protected Store、完整native owner／GTK/Flutter产品、全部业务方法 |

计数不得相加为通过率。当前三个2026-10-03报告为首要本地限定证据；stage13/14/15、SDK014分别保留各自产物身份。

## 26需求逐项现状

状态含义：IMPLEMENTED_BOUNDED为真实公共有界接口存在；EXPERIMENTAL为真实接口存在但独立能力／产品资格未闭合；OPEN_IMPLEMENTATION为所需公共能力确实缺失；OPEN_QUALIFICATION为基础实现存在但当前产品／平台证据不足；DEFERRED_PRODUCT为具体CCswitch/Codex/Cloud产品集成，按用户顺序在SDK／平台之后推进。

| 需求 | 当前判定与源码锚点 | 剩余门槛 |
| --- | --- | --- |
| F01 卡片读／创建／编辑／提交核对 | IMPLEMENTED_BOUNDED：core/schemas/runtime.capnp::Request七命令；sdk/rust/src/protocol.rs | 当前第三方内容adapter／主应用入口资格；一般workspace／关系／draft／批量不在七命令内 |
| F02 有界JSON转换 | IMPLEMENTED_BOUNDED能力；sdk/rust/src/task.rs、portable/cc-switch-pure | DEFERRED_PRODUCT：image-capability样例不能证明原Responses caller接入 |
| F03 POST SSE／增量终态 | OPEN_IMPLEMENTATION：sdk/rust/src/io.rs::HttpRequest为整帧；channel仅host给定本地source | 新网络source、SSE跨块parser、public route／三语言原包／UI提交闭环 |
| F04 WebSocket | OPEN_IMPLEMENTATION：core/src/io.rs:410解为Unsupported；无公开Socket生命周期 | 独立WS profile和backend；文本/二进制/分片/control/子协议/close/背压 |
| F05 取消／到期 | IMPLEMENTED_BOUNDED：Cancellation、io::Read/Finish/Cancel | 不等于任意job取消；当前产品竞态／实际资源退出分别证明 |
| F06 credit／ACK／关闭／join | EXPERIMENTAL：sdk/rust/src/channel及plugin_runtime/src/channel.rs | OPEN_QUALIFICATION：正式Workbench binding／真实网络源及多层队列；不能称公共channel不存在 |
| F07 授权changes／cursor | OPEN_IMPLEMENTATION：runtime七命令无subscribe/watch/listChanges | 独立授权变更源、分页/保留/epoch/ACK/CAS／新grant恢复；本地Events不授予内容来源 |
| F08 能力发现 | IMPLEMENTED_BOUNDED：workbench_host/src/sdk_profiles.rs::descriptor | OPEN_IMPLEMENTATION：逐平台backend能力、IO/service/mutation完整descriptor、多版本路由；当前实验项只recognized |
| F09 有界原bytes输入输出 | IMPLEMENTED_BOUNDED：task、dependency_call | 大对象/无限消息另profile；新type不授予权限 |
| F10 大对象／零拷贝 | OPEN_IMPLEMENTATION公共guest能力；shared_objects内部存在，shared_memory Windows mapping存在 | 公共blob/transfer lease、分块累计额度／摘要／取消；非Windowsmapping仍Unsupported |
| F11 Create/Delete | EXPERIMENTAL：sdk/rust/src/mutation.rs::Action、独立import／budget | 当前Windows完整审批／选择器／最大内容故障／其它平台资格；Replace缺失 |
| F12 同步多层纯依赖 | IMPLEMENTED_BOUNDED：dynamic_dependencies::run_graph、GraphLimits、RoutedOutput | 深度/总calls、linked取消和delivery再验已经存在；异步组合不属于它 |
| F13 异步跨插件服务 | OPEN_IMPLEMENTATION：Runner额外imports互斥；同步graph没有公开异步service协议 | 显式组合profile、根deadline/cancel/共享累计budget、队列/reentry/join |
| F14 HTTP出站＋认证入站有限service | EXPERIMENTAL：io、service、service_resources；当前transport六原件已验 | 当前主应用／TLS／凭据生命周期，完整NET矩阵和各平台 |
| F15 完整Cloud同步 | OPEN_IMPLEMENTATION共享changes/blob/私有同步state等；DEFERRED_PRODUCT Cloud插件 | 按普通插件权限构建；禁止Store/SQL/签名权或品牌核心特例 |
| F16 授权bytes处理并提交 | IMPLEMENTED_BOUNDED；private capture_provenance/captured_cards存在 | 公共capture profile及来源权限；private接线不等于第三方capture SDK |
| F17 基础声明式UI | IMPLEMENTED_BOUNDED：ui.capnp六节点、UiSession | 当前原包到Flutter输入/焦点/撤权/关闭；业务提交另授权 |
| F18 富UI／媒体／平台窗口 | OPEN_IMPLEMENTATION公共能力；现有六节点无Canvas/媒体租约/native窗口 | 独立UI profile、fallback/unknown规则、资源关闭、输入法/可访问性/平台资格 |
| F19 exec／PTY／子进程树 | OPEN_IMPLEMENTATION公共能力；IO capability十项不含exec | 需单独明确native/exec授权与隔离；DEFERRED_PRODUCT Codex |
| F20 多turn模型/auth/writer会话 | DEFERRED_PRODUCT；task现一次Invocation→Completion | session事件／模型HTTP／writer／exec共同授权及恢复；不能把task3改成长时义务 |
| F21 OAuth／多账号刷新 | OPEN_IMPLEMENTATION公共Account/OAuth profile；opaque凭据HTTP已具备 | PKCE/state/issuer、串行refresh／轮换／注销／多账号隔离／预算 |
| F22 list/watch/rename/replace／附件导入 | OPEN_IMPLEMENTATION：core IO多动作Unsupported；TargetBroker::replace_controlled明确UnsupportedConditionalReplacement | 正式resource profile和满足条件替换的backend；无可靠原子条件时继续Unsupported |
| F23 Unknown／历史／回收 | IMPLEMENTED_BOUNDED内容operation查询、HTTP/service历史、channel ACK journal、owner资源恢复 | OPEN_IMPLEMENTATION/QUALIFICATION：公共统一job/queryOperation目前Unsupported；跨进程业务核对／故障矩阵 |
| F24 长期后台／关闭切库被杀 | EXPERIMENTAL有限service<=1h、Windows监督；无限后台不支持 | 每平台生命周期／后台政策，current产品renew/expiry/revoke/join证据 |
| F25 旧三语言共存／未知拒绝 | IMPLEMENTED_BOUNDED固定基线、exact SHA、required feature及独立imports | 第三方/当前host变更持续复验；新版本decoder/router与迁移；现无一般多版本fallback |
| F26 本地/native ABI稳定 | EXPERIMENTAL可信adapter；HostV1指针在本进程生命周期有效 | W05仅codec/local ABI限定资格；恶意native隔离、动态装载/进程/架构/ownership另scope |

## 可执行剩余门槛表

| 门槛／映射 | 现有具体函数／测试入口 | OPEN原因 | 最优下一步与验收 | 授权边界 |
| --- | --- | --- | --- | --- |
| G01 Windows正式channel（F06/08/17/23/24/25） | ChannelBroker::{dispatch,run_invocation,finish_invocation}；channel_executor::{spawn,execute}；Workbench::{prepare_channel,run_channel,channel_status,close_channel}；test/channel_product_native_test.dart、channel_catalog_native_test.dart；runtime channel_controls/faults；windows_channel_executor | IMPLEMENTATION_PRESENT／CURRENT_PRODUCT_EVIDENCE_INSUFFICIENT。discovery正式binding=false，SDK014历史9红未在本候选复验 | 先静态归档旧20case到当前代码/断言的映射，冻结新host/package/UI/source；以原caller数据走真实supervised owner，至少cancel/expiry/catalog revoke/EOF/trap/output bound/maintenance panic/Close/join/Unknown目录UI；通过后另决定descriptor资格，不能先翻available | 本轮只读。编译/合成fixture测试在root既有授权范围；真实protected owner/DB/DPAPI/GUI当前排除，需已有另项明确授权，不能为验收绕过gate |
| G02 网络流源（F03/04/06/14；NET-4/5） | channel::Source/Producer/{push,wait_sent,finish}与三语言channel transport已有；core/src/io.rs::WebSocketConnect Unsupported；network_node_stream_001独立可信原型 | OPEN_IMPLEMENTATION | 从POST SSE独立opt-in源开始：精确endpoint/credential/epoch绑定、UTF8/SSE跨块/多行/截断、慢消费credit、原deadline/cancel/revoke；之后WS；不改变旧SubmitHttp一次整帧语义 | 不运行外部API。root如授权实现先disposable本机合成服务；真实外部账号/写入另批准 |
| G03 完整HTTP/认证（F14/21；NET-1/2/3/7/8） | sdk io::HttpRequest/Response、service Request/Response/resources；sdk_profiles::descriptor的experimental_extensions | 有界七方法/bytes已验；OAuth/account、cookie/signer、trailers/final URL/平台可见性、流body缺公共合同或完整资格 | 分离backend实际support与包声明；独立Account/OAuth profile；headers重复/4xx/5xx/empty/nonUTF8、form/multipart在既有64KiB内可作bytes业务，超限则resource流；新增完整开发者descriptor及负例 | 保存秘密仍host职责；不读真实secret/DPAPI/账号；未知专用认证明确Unsupported |
| G04 文件/附件（F10/11/22） | FileBroker::grant_open_file；mutation::Action；TargetBroker::{replace,replace_controlled}；managed_file_io、mutation_owner/reconciliation；core io Unsupported | read/Create/Delete存在；list/watch/rename/conditional Replace、一般blob/upload公共接口缺失 | 先独立目录/blob transfer profile，能力/选择句柄/祖先边界、累计quota/摘要/租约/关闭；Replace必须找到真正条件原子backend而非metadata-check→rename；当前三语言mutation产品资格单列 | 仅新临时目录和合成文件可按scope执行；真实用户文件及真实删除/替换不得隐式扩权 |
| G05 组合异步依赖（F05/06/12/13/24） | Runner::prepare:227-234拒绝多额外import；dynamic_dependencies::run_graph/Execution::{node,node_body}/RoutedOutput::{validate,validate_liveness}；dependency_graph/dynamic_dependencies | OPEN_IMPLEMENTATION组合profile；同步纯graph不是异步service总线 | 不放宽旧factory：新增combined profile与driver，根deadline/cancel、caller/provider各自撤权、总calls/总bytes/fuel/queue额度原子预留和终态结算；无DB锁等待另域；cycle/reentry/ready后撤权/expiry/trap/join/不重放矩阵 | 在批准slot/实例交集内；禁止新增任意registry/自动后台启动或裸callback权限 |
| G06 changes/cursor（F07/15/16） | runtime::Request七命令；Store::{channel_checkpoint,channel_ack_receipt,commit_channel_ack_guarded}；core/tests/channel_journal.rs、runtime channel_faults::event_cursor_advances... | journal原子ACK已具备；授权内容变更source/公开消费命令缺失 | 新scoped changes profile：事务事件→授权过滤→分页/retention/epoch→ACK/CAS；撤权/重复/缺口/重启重批准；历史cursor不能恢复grant | 先公共合同/合成库；Cloud品牌插件后置，禁止公开SQL/Store路径或自动同步真实库 |
| G07 重启Unknown（F23/24；NET-6） | core/store/io_intent::{claim_io_dispatch_local_authorized,lookup_matching_io_intent}；service durable_route；owner_recovery::{preview,recover}；channel_journal guarded ACK | 有限历史与不重放存在；通用公共IO QueryOperation仍Unsupported，完整跨进程业务核对未闭合 | 逐能力列效果、结果交付、资源退出三个事实；进程退出前/后claim、发送、持久保存、交付，重开旧history、重新批准后只读核对；合成服务记录副作用次数；不更换幂等key来恢复 | 不运行真实DB。故障仅临时合成数据；恢复资源不授予业务重放 |
| G08 声明式UI／真实第三方（F01/16/17/18/25） | sdk ui.rs、UiSession、packages/morrow_plugin_ui；workbench external_transform/external_ui路线content/dependency=false | 基础六节点原件支持；通用第三方内容/dependency路线和富UI缺独立产品资格/能力 | 使用SDK分发候选之外独立开发者workspace，保原包；真实安装→批准→基础UI→host授权内容提交→撤权关闭；富UI独立profile逐控件/fallback/lease验收 | W06模板验证不能替代第三方；不借内置private Workbench协议冒公共支持；GUI现行排除仍生效 |
| G09 多平台（F08/10/14/17/24/25/26） | sdk_profiles platform OS/arch；shared_memory非WindowsUnsupported；shared_memory_web复制；Store::open_private*拒绝；ProtectedStoreSlice仅test构造；channel_binding::require_owner；Android计划 | OPEN_IMPLEMENTATION及OPEN_QUALIFICATION，不能复用Windows/Linux历史PASS | 先按OS×arch×profile列backend/owner/storage/credential/UI/lifecycle；Linux完成严格Store调用点/audit备份恢复/原controller监督接入再启产品；macOS/mobile逐后端；Web主题导入既有小范围保持 | 不切通用Store作保护fallback；不安装组件/获取工具链/改变平台权限或安全设置；设备/其他主机执行须相应授权 |
| G10 冻结身份／第三方与runner（F25/26） | compatibility exact schema/pins；tool/windows/verify_sdk_native_bounded.py committed-tool gate；W05/W06外部负责 | CURRENT_EVIDENCE_INSUFFICIENT，未提交工具身份不满足原runner gate | W05/W06报告独立绑定准确SHA；决定候选范围前保留旧base/transport/schema/pin，新扩展新ID；原runner不得绕gate，等工具身份可合法取得再复验；第三方接入和平台矩阵必须独立结论 | 本轮无commit/push/release授权；不自动提交来满足工具gate，不把分发ZIP当公开Release |

G01、G02/G04/G05/G07、G09按Windows能力与多平台SDK推进；G06/更富UI以完整目标实际需求单独完成。F02/15/19/20具体产品适配随后推进，不把CCswitch/Codex acquisition作为当前门槛。可分层稳定，但不能在未获用户范围确认时把其完整目标缩为旧有界profile。

## 已修复／已有实现，缺产品复验

1. **队列锁等待期间授权/期限复核**已在Resource::check_locked、dispatch取锁后及Send reserve后存在；commit_gate检查原Control/monotonic deadline，cleanup在同锁记录原首因。channel_controls中的cleanup_first_after_atomic_stop、cleanup_first_after_monotonic_expiry与final_public_store_guard等源存在。不能重复旧“锁后未验权”缺陷；本轮未执行这些tests。
2. **failed TaskReport停原source**已在finish_invocation实现；channel_executor分别处理spawn失败、output bound、owner不可用、execute panic、maintenance error/panic，先stop原Control并保留owner/首因。windows_channel_executor诸真实thread测试源存在；不能描述仍未接Stop。
3. **Dart返回grant与异步世代**已在channel_task_session：冻结所有frame后才准入，取回完整identified preparation先保留原key，检查所有ceiling与全部frame再append/run，合法maxRequests1缩窄不拒绝，旧generation不更新新状态；Directory 256wire-byte预检与typed handler gate已在代码。fake-backend tests不等于当前真实catalog产品PASS。
4. **Linux VFS并非全无**：当前owned VFS公开SQLite ABI与process-lifetime注册上下文已存在；stage15修复stage14 named-memory borrower潜在UAF，1024注册槽位不回收、64fd retirement另算；guarded Store slice支持有限card/ACK。公共private factories/完整业务方法/audit owner仍关闭，不能引用最早foundation报告把已实现的低层项列空白。
5. **注册表临时文件发布及测试资源析构**已有当前修复与限定实证：旧目标bytes确认、OS5/32/33最多4次同temp发布、变化/不确定返回CommitUnknown；dependency/HTTPfixture先释放manager/db再删temp，原件回归与零残留有报告。实测1000/1000无重试不能证明当轮实际治愈OS5重试。
6. **网络service出站组合/端点资源选择**已有公开service-http starter与resource目录、私有持久selected route。历史“主应用一律拒绝出站IO／面板没有接入”的早计划后续已推进；当前缺本候选Windows窗口/TLS/凭据生命周期与完整网络能力，不能重复早计划状态。

## 确实未实现或明确Unsupported

公开SSE/WS网络backend；公共授权content change source；公共异步service/dependency组合profile；一般guest大对象／blob上传资源；目录list/watch/rename与可靠条件Replace；OAuth/Account及专用signer/cookie等profile；富UI/native窗口/媒体会话；公共exec/PTY与完整多turn产品授权；一般多版本schema fallback；非Windows映射backend和Linux产品protected owner入口。这里“未实现”限定对应公共/产品层，不否认同名低层prototype/private code存在。

本报告只推荐下一步；未执行其验收。平台、模板、native实际编译与compatibility各自记录，不能替代能力实现、授权生命周期及真实产品复验。

静态输入共53项，前后 SHA256 恒同，完整输入清单及原始报告保存于本地 W07 证据目录。本文为独立只读审核的归档；本轮工具实施、原生运行与后续新来源身份分别见同目录专项报告，不能以本表静态判断替代运行资格。
