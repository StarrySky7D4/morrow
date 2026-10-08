# C14 完整包装持久审批与限定 Windows 资格

2026-10-06，本地增量，基线 `e83cdf1d3506b001d07ffceaaa9f98b688bcc150`，tree `abcdee6e582ce18954118430e514f1d0805ce095`。
没有 commit、push、main、tag、Release、CI 或封存 Wasm 重建。

## 实际实现

新增 `catalog::Catalog`／`WrapperCatalogStorage`／`NativeCatalogStorage` 和
`CatalogManagedPackage`，完整 `MROWASP1` review、安装、选择、批准、启停、移除选择、
分页与新的实时 admission 均有明确接口。批准绑定整个 wrapper SHA、基础 SHA、双 schema、
会话与进程能力子集、有限 session 和执行域。原 Manager 的基础选择不能替代完整批准。

目录快照为独立 canonical Protobuf＋LZ4，复用原 exclusive/full-sync SQLite adapter；
完整合法大包存独立 SHA 文件，不塞入原 512 KiB 快照，不放宽旧表或旧上限。
原 Core Store／HostRuntime／R2 issuer 不新增；重连复用原 Manager 创建的同一 Arc connection。
原 `Manager::revoke_agent_session_process` 先核 exact base/revision，再停止同包 Control；
不发布或增加基础 registry revision，保守涵盖同包普通实例。目录变更整体失效已发实例。

失效、Drop、存储故障及原 Manager 的安装／读取 Unknown 立即撤销原 Core 并取消 guest。
旧批准、旧进程 handle 与 Unknown 不会在 reopen 后恢复或重放。保存失败不复活旧实例，
不确定提交需重开核实际静态状态；快照 revision0拒绝，原空白 None 状态仍可初始化。
native 总配额同时计入孤档、旧版本和遗留临时文件；部分归档不授予权限、不自动删除。
基础 registry／wrapper files／审批快照没有跨存储原子事务，不作此保证。

## 本轮实际运行，分别计数

| 范围 | 实际结果 |
| --- | --- |
| 新持久审批 catalog | 18 PASS；含真实 SQLite 重开、>512 KiB 合法完整包装、双revision、撤权、保存失败、Unknown、损坏和封存 Rust session Wasm；逻辑 process provider 不算原生执行 |
| Manager revoke seam | 3 新 PASS；原 C13 的6另计；原 Manager11、storage1、strict profile6回归通过 |
| 原 managed bridge/route/schema | 14／13／1 回归通过，不增加新方法数 |
| 真实 Windows catalog → Wasm → native进程 | 1 PASS；helper1明确过滤；普通合成SQLite、sandbox=None；完整审批保存/drop/reopen后freshadmission，再proposal→Claim→实际start→processWasm读写/终止→实际exit/EOF/耐久facts/finish回收 |
| 新 host Clippy | all-targets `-D warnings` exit0；原依赖 feature 的19 dead-code warnings保留，不称全仓无警告 |
| 格式 | 7个owned文件限定check exit0，nativefixture独立格式收据另留；不格式化原源码 |

每次最终资格均核测试前后输入一致、真实原件 Wasm hash相同、日志hash与退出码匹配。
本轮目录18项中真实 Rust Wasm被明确固定；真实Windows场景另外计数，不与C13旧场景累计。
准确输入、二进制和日志SHA、命令与各方法名称见 [validation](codex-sdk-c14-validation.json)。
独立静态审查修补四条具体分支后通过；32条拟议矩阵保留NOT_RUN_BY_REVIEWER，未凭18项全关。

## 初次失败与范围

host首轮17/18，唯一失败是重连fixture将issuer时钟6倒退为1；原执行器正确拒绝。
只把该重连测试采样改为6，生产时钟/授权检查未放宽，最终18＋14＋13＋1全部通过。
native首次metadata在受限离线Git缓存失败101；核准的同固定缓存重试通过，版本/source/
checksum不变，仅新host对原tempfile的正常dependency edge新增。host lock字节不变。
native build-only期间该host测试调整，前后sourcechanged保留；最终固定当前输入真实运行通过。

这不是正式工作台安装链完成：旧PluginInspect/Import/Approve仍原合同，不能显示新权限。
后续须新私有协议/GUI，目录随原WorkbenchState与Manager持有，借原ProtectedSession runtime
及native owner/worker，遵守busy/lost/恢复门禁。认证transport、OS sandbox/managed network、
缺失native交互控制、其他平台、完整blob/文件系统/恢复与SDK26/G04仍OPEN/NOT_RUN。
实际清理需exit+EOF和原owner finish；目录close只发可信收尾，不证明真实资源已释放。

327 SDK、57冻结输入、旧wire/schema/Linux与C12/C13封存工件保持。新候选不继承旧冻结pin。
详见 [接口流程](../../docs/PLUGIN_AGENT_CATALOG.md)。本轮源码须经当前前像与审查摘要门禁后
写回原开发树；可恢复交接保存完整未提交增量、前像、原始失败和固定输入成功收据。


## 公开发布路径说明

链接的 validation 是保留原测试结果的公开路径派生记录，不能称原字节或当前源码新资格；原始完整 SHA、派生 SHA 和替换边界见 [公开历史记录说明](PUBLIC_VALIDATION_PROVENANCE.md)。原始失败和 NOT_RUN 范围不变。
