# Codex SDK 接口缺口与基础接口冻结

日期：2026-10-05。开发线 `codex/windows-sdk-convergence-20261005`，基准 `679e16a6a44f8aac13f85da05204d9cd8afb30b8`。R2已修补原六项基础缺口并完成整体核查，以新revision／独立pin冻结session／safe-exec-basic。正式Rust156／Python32、真实Codex消费2及移地客户端编译通过，见[修补与整体核查](session-exec-v1-r2-repair.md)和[新验收](session-exec-v1-r2-validation.json)。R1[历史审计](session-exec-v1-audit.md)、原基线／pin与crate保持；live Core为R2演进，历史身份对R1归档核验。SDK26／G04／生产Codex G0仍OPEN，本轮未提交、推送或Release。

当前R2补齐满额收尾、严格Snapshot回复、可信观察递进／Unknown晚到核对、失效准入回收、独立宿主终结／退休和generation防重放；独占owner、启动metadata／业务行闭合与原子墓碑防止权限／存储漂移。实际ThreadStore跨进程持久消费、专用包／import／native frame路由、固定输入执行器及独立源码分发已有限定资格。实际生产Exec provider／事件、完整应用注入、认证批准链和平台资格仍需完成，扩展执行延期。

## 当前究竟缺少什么

| 范围 | 已有实现 | 仍缺接口或接线 |
|---|---|---|
| M02 NativeSession | `agent_host_v2_capnp` 与native owner已有握手、自身状态、Close、撤权和退出／双EOF观察；早期fixed256候选已撤回 | 生产安装信任、执行域／后代隔离、更多能力准入及平台资格；不得把所有native准入写成空白 |
| M03 HTTP流／M08关联 | `agent_host_v3_http_stream`已有HTTP proposal／独立批准／一次Commit、status、raw重复headers、可信remote、credit／cancel／terminal／RequestClosed；Windows native stream owner已有有限接线 | 公共账户／凭据／TLS及获准目的地的完整策略、认证恢复；Codex native WS协商；产品与平台资格。HTTP status／headers不再是当前缺接口 |
| M04 持久会话／事件 | R2收尾预留、writer／tail CAS、严格Snapshot／checkpoint／gap、分支、原子retire／generation已冻结；实际ThreadStore覆盖持久化／恢复／撤权／归档／物理delete | 完整产品注入、认证批准链、通知／订阅、复杂历史及平台资格；有界合同不提供无限保留 |
| M05 内容 | 原Core已有授权摘要／正文／附件读取、Create／Edit／Rename和操作核对；本轮新增独立Query／ReadRef／ProposeMutation／InspectOperation与真实原HostRuntime适配 | 本轮范围以外的附件与其他卡片动作、复杂workspace查询、内容→endpoint外发交集、持久提案恢复、真实native IPC及package准入、C／C++绑定与产品资格 |
| M06 执行 | R2固定提案／独立批准／单次Claim及回调、退出／EOF递进、Unknown晚到核对／安全retire、准入回收／独占owner已冻结；Linux固定执行器和actual ExecBackend否决边界通过 | 真实生产StartedExecProcess／事件provider、完整Codex注入、认证transport／批准链、OS隔离／Windows资格；PTY／交互输入输出／resize／signal／terminate延期 |
| M07 流视图 | 基础声明式UI、现有projection与changes能力各有独立实现 | Codex `stream-view-v1` Subscribe／Snapshot／DeltaBatch／Terminal／Detach合同及持久subscription／projection与产品生命周期接线 |
| M08 账户认证 | 现有IO具备部分宿主资源／账户路由与fixture凭据注入 | 公共Account／OAuth、凭据来源／权限／代际、刷新与认证挑战、撤销和恢复，不能回退个人认证或把秘密放普通事件 |
| M09 原生包／套件 | Wasm mplugin封装、SDK源码分发与现有Registry存在 | native-package／bundle Inspect／PlanInstall／Stage／CommitSelection、成员与平台验证、独立安装批准／原子选择及恢复；现有Wasm打包不代表native bundle安装已完成 |
| 共享SDK26／G04 | 目录Open／Next／Finish／Cancel及三语言codec已实现；Core已有真实blob staging／读取与事务底座 | 真实目录guest／原owner／产品资格、blob公开transfer→批准→原owner→耐久底座与history／恢复接线；异步组合、长期cursor和完整平台矩阵继续OPEN |

原六项基础缺口已在R2修补并以新证据冻结；生产和完整SDK资格继续独立验收。实际Exec正向进程provider及完整产品接线仍OPEN，否决测试和通用底层能力不代替产品资格。

## 此前内容片段（先于本轮v25冻结）

源码位于 [agent-content-v1](../../extensions/agent-content-v1/README.md)，使用说明见 [内容接口](../../docs/PLUGIN_AGENT_CONTENT.md)。唯一新schema为 `extensions/agent-content-v1/contracts/agent_content.capnp`，version1／revision1，raw SHA256为 `4679e41c9154eea42fc158778babc86bebd85d98bcef903510625dc0a0063d79`。它是独立实验扩展，不增加旧v1 capability bit，不改v2／v3身份，也不声称已被native owner协商。

四项请求、回复与消费校验已经实现；宿主只接受其事先指定的有限对象集合，继续逐项核对原grant及包能力上限。提案路径读取原对象以核验revision／length／whole body digest，不写内容；可信批准绑定完整原帧且不续期；一次执行进入原Core事务授权和CAS。已消耗或Unknown提案禁止再次执行，历史核对不会恢复活授权。

另有显式内存预算的完整读取客户端：按固定引用逐32KiB读取，任意错误即停止，收齐并核验完整正文摘要后才返回VerifiedContent；空正文也须实际授权读取。它不执行保存、上传或外发。

## 内容阶段当时验证与证据

平台为Linux x86_64，Rust1.96.0、Cap'n Proto1.4.0，使用锁定离线依赖和一次性普通SQLite合成库。新扩展最终60个方法PASS：codec29、原HostRuntime／普通Store17、完整读取消费者14，失败／忽略／过滤均0；unit及doc runner各0项不计功。命令、源码清单及workspace日志摘要保存于 [validation](codex-sdk-interface-validation.json)。示例退出0是功能检查，不额外增加测试方法数。

原Core相关四组回归实际24 PASS：content_changes4、content_guard6、guarded_dispatch9、operation_commit5。它们验证原事务授权、revision／operation CAS、晚期交付拒绝与持久原件。旧guest36／transport17个manifest成员重新核验，连同manifest及root pin为冻结57；当前v1 candidate再次核验180个kit文件及12个canonical源码文件，通过且仍qualification_only／SDK OPEN／P-02 NOT_RUN。旧源码、锁文件、schema、kit和归档receipt未修改；原SDK327没有重建或更新，也没有重新运行其全部历史资格。

真实Store测试覆盖：独立host nominations与package ceiling不能替代对象grant；引用身份／revision／总长／摘要失配拒绝；Propose／Approve不写库；一次原CAS与回执可关闭重开核验；撤权／到期／外来host或connection不得执行；Core已经提交但最后授权到期不给确切回执时保持DispatchUnknown，后续新QueryOperation授权只读核对，不自动重放；Query／ReadRef的最外层编码后再到期也不给明文；16份合法大摘要累积超限返回有界Limit。完整客户端另外验证70KiB真实Store三段读取、空正文必须读取、部分拒绝不重试、篡改正文即使元数据一致也须wholeSHA拒绝。

原始失败保留：一次集成编译因并行开发中client模块文件尚未落盘返回101，没有执行测试；随后host17中的两个新夹具把准备后host时间从1倒退到0，15PASS／2FAIL。修正夹具为同一单调时间后，精准两项分别通过、完整17通过，最终整个扩展60通过。没有放宽Core时钟或权限检查。此前提前写入客户端能力及断链证据的文档被自动审批拒绝，待实现、实际验收及验证JSON齐备后再写入。

上述内容层历史记录保留其当时范围；R2另行通过实际ThreadStore跨进程、专用Wasm/native路由和固定输入执行器限定验收。完整P-02产品、Windows owner／ProtectedSession、GUI、账户／TLS、C／C++及SDK26矩阵未验收。Core既有18处warning保持，不称整库lint清洁；既有锁文件保持，新锁仅属于新增独立包。

## 后续顺序

1. 将本独立内容合同接入新native能力准入／IPC和当前包审批，继续补内容外发交集；保持旧v1／v2／v3及consumer review原件。
2. 消费已冻结R2会话合同和实际ThreadStore适配器，接完整产品／认证批准链与平台资格；保持精确退休、防重放及有界历史。
3. 消费已冻结R2安全执行基础合同，接真实生产Exec进程／事件provider与完整Codex注入并验收OS执行域；PTY／交互输入输出／控制延期。另接M07视图、M09安装选择和M08账户。
4. 对最终候选执行完整产品、第三方和平台矩阵，再确定整体SDK稳定冻结身份。本轮局部冻结不关闭完整M05、G0或SDK26。
