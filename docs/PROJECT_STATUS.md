# Morrow 当前开发状态

<!-- C28-CURRENT-BEGIN -->
## 当前开发检查点（2026-10-09，C28 H011 构建与验证准备）

后续 H011 隔离源码候选完成锁定、离线 Windows 依赖核验及 release 验证程序编译，实际退出 0。构建图 774 个可达包、19 条唯一路由已核对；4,299 个固定输入无漂移，原锁 837 个外部依赖记录保持。程序已保存并核对完整哈希，尚未执行；公开相对路径副本的完整构建仍 NOT_RUN，验证入口当前三个 Wasm 嵌入目标缺失，需补齐可复现输入准备。

控制包两个有限静态独审通过，十个 PowerShell 文件实际 5.1 纯语法解析通过；包仍硬禁用，真实目标恢复与执行未验收。此前 Native 11 项纯诊断与投影器 1,987 项合成检查保留原范围；底层 observer 8、runner 3、默认 backend 1 项仍 NOT_RUN，独立库离线核验受固定测试依赖缺失阻塞。

历史 Start 保持 Unknown/BackendError，不自动重放；原 owner 的正常退出、EOF、ACK、cleanup/join 与资源债务仍 pending。SDK26/G04、Agent 会话层及安全执行层验收仍 OPEN，SDK 未冻结，release_eligible=false。

下一步补齐底层库资格、完成控制包实际绑定及 Windows 生命周期复验。达到 Agent 会话层和安全执行层验收后暂停并准备预览，扩展执行层不是暂停前提。本次仅同步现有开发分支，应用版本不变，无新 Release。见 [最新构建与范围](../reports/reconstruction-2026-10-09/windows-agent-sdk-c28-h011-build.md)；[逐次诊断证据](../reports/reconstruction-2026-10-09/windows-agent-sdk-c28-lower-start.md)保留。下方为历史时点。
<!-- C28-CURRENT-END -->

<!-- C16-CURRENT-BEGIN -->
2026-10-06 C16 本地实验性候选：新增可信 Rust Agent start/submit/poll/cancel/recover/acknowledge
入口，移动完整原 owner，沿用同 Core/Store/runtime/连接；每 import 维护原 owner。
借用执行与 scheduler 回收有界；真实 join 后仅同步唯一 Runtime 所有权可回收，别名保留 16 有界债务。
普通无沙箱、非 ProtectedSession 的封存 Rust Wasm＋实际 Windows worker 耦合 3 项通过；各组收据见报告，
不作为生产 GUI、真实受保护库/DPAPI 或生产沙箱资格。旧 SDK327/57、合同、Linux 与 C15 工件守卫通过。
接口仍 experimental，SDK26/G04 OPEN；未提交、推送、Release 或 CI，不代表正式 SDK 冻结。
见 [原生 owner 接口](PLUGIN_AGENT_NATIVE_OWNER.md) 与 [C16 报告](../reports/reconstruction-2026-10-06/codex-sdk-c16.md)；下方 C15 及更早内容保留其历史范围。
<!-- C16-CURRENT-END -->

<!-- C15-CURRENT-BEGIN -->
2026-10-06 C15本地候选：工作台完整会话插件管理已接独立admin协议/owner与设置界面，
借原Manager/持久catalog，保留原busy/lost/恢复门禁和顶层native管道，不改旧Core/IO。
admin10、owner18、旧host18/14/13/1回归分别通过；Dart18（含实际Rust五对向量互验
和原管道Busy路由）通过。新库strict Clippy、Dart fatal-infos/限定格式和Windows宿主check
通过；不是新OS执行/真实桌面/ProtectedSession资格。327/57/旧合同/Linux/封存工件保持。
实际native port借原runtime/worker、生产GUI、认证/sandbox、交互控制及全SDK26/G04仍OPEN。
没有commit/push/Release/CI。详见 [C15报告](../reports/reconstruction-2026-10-06/codex-sdk-c15.md)；下方C14及更早内容保留其历史范围。
<!-- C15-CURRENT-END -->

<!-- C14-CURRENT-BEGIN -->
2026-10-06 C14本地候选：完整Agent wrapper持久catalog/approval接口已实现，原Manager撤销seam
和同原connection/freshadmission接线保持。新catalog18、Manager新3与其原6、原回归11/1/6、
旧managed/route/schema14/13/1分别通过；实际Windows新1通过（helper1过滤，sandbox=None）。
大包装不受原快照512KiB限制，static审批不恢复livegrant；保存故障/Unknown即时撤旧Core，
旧效果不重放。327/57/旧合同/封存工件保持；Workbench新协议/GUI、ProtectedSession/native
owner、认证transport、sandbox、交互控制/其他平台及SDK26/G04仍OPEN。未commit/push/Release。
详见 [C14报告](../reports/reconstruction-2026-10-06/codex-sdk-c14.md)；下方C13及更早文字为各日期历史，不替代本条实际范围。
<!-- C14-CURRENT-END -->


<!-- C13-CURRENT-BEGIN -->
当前 C13（2026-10-06，本地未提交）：会话/进程新入口已接原 Catalog/Registry/Manager，
共用原实例限额、Control 与连接；基础包与完整 wrapper 仍独立批准，预算取交集。
新增 managed host 14、Manager 6、HostIdentity 3、真实 Windows managed Wasm 2 分别通过；
原 host 1+13、Manager 11+1、strict 6、process 18+5 回归另计，重复不累计。
修正提交前取消、effect 后 Unknown 与错配收尾；新 host strict Clippy/限定格式通过。
旧 327 SDK／57 冻结输入、wire/schema 与封存工件保持；新候选不继承旧冻结 pin。
完整 wrapper 持久审批、Workbench/ProtectedSession owner、sandbox/认证/其他平台仍
OPEN/NOT_RUN；SDK26/G04 未冻结。见 [C13 接线与实际边界](../reports/reconstruction-2026-10-06/codex-sdk-c13.md)。
下方 C12 及更早检查点保留历史结果与当时身份；本轮现状以 C13 为准。
<!-- C13-CURRENT-END -->

<!-- C12-CURRENT-BEGIN -->
当前 C12 接口增量（2026-10-06，本地未提交）：已补会话 Rust/Wasm 客户端、类型化进程控制、
严格 single-import 组合运行入口和原 R2 执行权实时复验。process 合同 23、R2 客户端 10、
旧入口真实 Wasm 6、process 客户端 10、组合 host 14、运行入口 6、定向 runtime 回归 21、
原 R2 回归 106 分别通过，重复方法不累计。Windows 原生实际执行 7 项、纯注册表 3 项、真实 proposal Wasm→Windows→process Wasm 耦合 3 项分别通过；封存 Wasm 未重建，Unknown 不重放。
327 SDK／57 冻结输入及旧封存归档保持；新候选不继承旧冻结 pin。
原生 close-input／PTY resize 尚 Unsupported；完整 SDK26／G04、产品 GUI、受保护内容库、
OS sandbox、认证 transport 和其他平台仍 OPEN／NOT_RUN。详见 [C12 接口与限定证据](../reports/reconstruction-2026-10-06/codex-sdk-c12.md)。

下方 C11 及更早日期的文字保留当时的身份、结果与未运行范围；本轮现状以 C12 为准。
<!-- C12-CURRENT-END -->

当前C11检查点（2026-10-06，本地未提交）：Windows R2接口106通过；原Linux R2归档已用新外层校验器通过封存核验，旧pin不变。真实新Rust/C/C++目录Wasm在原managed owner上12方法通过；原件与目录回归70、只读映射7分别通过。四Python组99通过/3 FIFO跳过，新构建门禁15通过。SDK327/冻结57保持；完整SDK26/G04、产品批准链、ProtectedSession、picker、真实Windows执行器与其他平台仍OPEN/NOT_RUN。详见[C11限定资格](../reports/reconstruction-2026-10-06/windows-sdk-c11.md)。

以下带日期检查点保留当时身份和未运行范围；C11后续结果以上述报告为准。

Codex基础接口R2修补（2026-10-05，Linux云端）：原六项缺口已修补，session／safe-exec-basic以新revision及独立pin冻结。正式Rust156、Python32、真实Codex消费2、旧内容回归60及移地独立客户端离线编译分别通过；573输入前后一致，617成员归档核验通过。R1原件保持，live Core已演进。实际生产Exec进程／事件provider、完整应用注入、认证transport、Windows／OS／GUI及SDK26仍OPEN，扩展执行延期。详见[修补与整体核查](../reports/reconstruction-2026-10-05/session-exec-v1-r2-repair.md)和[当前合同](PLUGIN_AGENT_SESSION_EXEC.md)。本轮提交收录修补源码及R1／R2冻结归档；提交身份以开发分支历史为准，未创建Release。

此前Codex内容片段（本会话前一阶段，2026-10-05）：独立 `extensions/agent-content-v1` 实现有界Query／ReadRef／ProposeMutation／InspectOperation，复用原HostRuntime对象授权、CAS和持久操作记录；完整读取客户端收齐并验证SHA256后返回VerifiedContent。当时60项及原Core24项通过，示例运行成功；本轮v25后内容60项再次回归通过。M05其余接线、扩展执行、view、Account／OAuth、native bundle与产品／平台仍OPEN；不新增旧native协商能力、不关闭SDK26。详见[接口缺口与补齐](../reports/reconstruction-2026-10-05/codex-sdk-interface-gaps.md)和[内容接口](PLUGIN_AGENT_CONTENT.md)。

Codex候选冻结门禁推进（2026-10-05，Linux云端）：新增宿主侧 `tool/verify_codex_sdk_candidate.py`，将consumer review、180个kit文件与当前canonical全部12个源码文件、原始schema摘要及major/revision绑定。宿主校验补齐19对向量与1项版本拒绝、生成identity、完整证据清单及默认不启用fake的命令检查。当前Python门禁40与兼容／契约同步22分别通过；canonical Rust qualification14、默认feature库check及目录静态准入9实际通过。候选仍是 `qualification_only`，完整SDK26／G04及Codex生产插件资格保持OPEN；本轮没有运行P-02、Windows owner、三语言真实目录guest或产品GUI。详见[Codex候选门禁与证据](../reports/reconstruction-2026-10-05/codex-sdk-freeze-gate.md)。

当前C10检查点（2026-10-05）：本次开发分支更新收录C08–C10。C10已新增独立 `fs-directory-request-v1`、严格单import、包feature及只读discovery，复用原已批准selection和owner，不导出路径、句柄或新授权。Windows新Rust34（codec12/helper4/profile9/owner9）、既有回归191及frame5分别通过；Python33和两个原生C/C++消费者另计。新Rust Wasm仅编译通过，真实新Rust guest、C/C++ Wasm、Workbench产品／GUI、受保护Session与其他平台仍未验收。旧Core IO FileList保持Unsupported，SDK26／G04仍OPEN，无新Release。 详见[接口与实测边界](../reports/reconstruction-2026-10-05/directory-request-sdk.md)。

更新：2026-10-06。本文是当前状态入口；[开发看板](DEVELOPMENT_BOARD.md)保存任务，[SDK 门槛](../reports/reconstruction-2026-10-05/sdk-next-gates.md)保存验收顺序。带日期的报告、冻结夹具和历史源码快照保留各自身份，不因文档同步获得新的测试资格。

## 版本与开发线

| 项目 | 当前身份与边界 |
|---|---|
| 本次源码检查点 | `codex/windows-sdk-convergence-20261005`，承接云端 `codex/cloud-sdk-convergence-20261004` 的 `468ef2e912ac74e5f97f0016a8729b7d5c1f5399`，已发布检查点 `772466177fe589cee53bc633e69f411c34610104`保存C02–C07 Windows修正及文档；C08新增在该基线上本地完成限定资格，C09相对目录选择接线已完成限定Windows资格；本次开发分支更新收录C08–C10，提交身份以分支历史为准 |
| 应用源码版本 | `0.1.9-test.58+62`；SDK/crate/schema 的独立版本保持，不把应用版本当协议版本 |
| 已发布 Windows 下载 | [test.56 测试预览版](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.56)；本次同步不产生新安装包或 Release |
| 原 Windows 资格分支 | `codex/windows-sdk-qualification-20261003` 保留 `20669f671152972470340a65eac3458dd2f61b4d`，与本次检查点分开 |
| 原始 SDK / 冻结夹具 | 327 个 SDK 输入与 57 个冻结输入字节保持；这是基线兼容证据，完整 SDK26 仍 OPEN |
| 许可与数据兼容 | 第一方代码、SDK 和文档为 AGPL-3.0-only；test.1 是旧数据类型最后兼容测试版，迁移前保存原库及其保护文件 |

## 最新限定验证

下表保留已封存C07的Windows x64 Release、离线锁定结果；历史数字不代替C08复验。C08本轮实际factory14／69filtered、原owner115／九组、原件42／五组、network100／十一组各自通过；42保留region14与reader1过滤。三文件fmt与strict库Rustc通过；整库Clippy仍exit101／10旧诊断／owned0，见 [阶段说明](../reports/reconstruction-2026-10-05/directory-secret-factory.md)。

| 范围 | 结果 | 计数限制 |
|---|---|---|
| C07 原 owner / 目录生命周期 | 115 个方法通过，9 组 | 新取消 7、目录 owner 14、原 native 16 已包含在 115 内 |
| 原始 SDK 定向回归 | 42 个方法通过，5 组 | 保留 frozen-region 的 14 项过滤及 reader 的 1 项过滤；child helpers 不另加方法 |
| 网络回归 | 100 个方法通过，11 组 | 失败、忽略、过滤均为 0；不与 C04/C05 的历史重跑相加 |
| 当前限定格式 / 编译 | 5 个文件的格式检查通过；严格 library Rustc 通过 | `io_jobs.rs` 的 17 个既有格式差异仍保留，不称整库格式清洁 |
| 整库 Clippy | 失败，exit 101 | 10 处既有诊断、0 处本轮 owned 诊断；整库 lint 仍开放 |
| 本地交接包 | 两个基线的实际恢复得到相同源码树，全部 3,180 成员校验通过 | 包是源码增量及证据交接，不是可安装产品或 Release |

详见 [Windows 复验](../reports/reconstruction-2026-10-05/windows-sdk-revalidation.md)、[目录 owner 实测](../reports/reconstruction-2026-10-05/directory-owner-sdk.md)、[目录与 blob](PLUGIN_DIRECTORY_BLOB_SDK.md)及[本次文档同步](../reports/reconstruction-2026-10-05/documentation-sync.md)。C07 封存树 `dacac682a9e341a4931508021a1c815929f42dea` 是恢复树，不是提交；本次文档更新后的提交身份以分支历史为准。

## 已推进的 SDK 能力

- changes 元数据具备独立 discovery、严格消费者和有限内容更新来源；能力发现不授予执行权限。
- 独立 Rust / C / C++17 WebSocket 消息、SSE 事件库及源码分发已完成限定资格；数据解码不产生网络授权。
- 独立目录观察、blob 分段校验库已完成有限编解码与跨语言一致性验证。blob 的 `VerifiedBytes` 不等于 Store 的耐久提交。
- Windows 目录捕获、分页、结束、取消、未读交付及 idle 清理接入原 `IoWorker`，保持原 Manager、HostRuntime、IoBinding、时钟和预算。
- 目录查询、编码、取消谓词及真实资源 drop 留在短时钟检查之外；额度在 root / broker / lease 真正释放后归还，累计费用不退。Unknown 不自动重放。

C09已完成限定Windows Release／locked／offline资格，见 [C09阶段说明](../reports/reconstruction-2026-10-05/directory-selection-owner.md)。可信宿主 `capture_directory_under(anchor, relative, limits)`只保留并核验原opened anchor到relative leaf的raw UTF-16句柄链，复用原worker FileList、原时钟、取消和预算；root加N个分量共享原8资源，32段语法上限不是可用深度。新selection_path8＋directory_selection12、C08 factory14、原owner九组115和原件42分别当前实际PASS；原件42为base9／dependency3／region7／reader主9／shared14，reader raw10含child helper1不加方法，region保留84过滤。17个credited测试进程合191 meaningful方法（raw192含child1），zero-match失败进程保留且不计功；这些数字不能作为SDK冻结。Workbench第二次Release x86_64 `--locked --offline --lib` check通过，首次缺offline asn1-rs0.7.2的exit101保留；只是编译检查，ProtectedSession／GUI／picker以上provenance和non-Windows产品执行NOT_RUN。C09 network100和Clippy明确NOT_RUN，不继承C08历史通过或lint结果。这不证明picker时刻、anchor以上来源或传入anchor的sharing策略，不增加guest FileList、目录guest或公共UI，blob耐久后端仍缺。SDK26／G04仍OPEN，公开FileList及conditional Replace仍Unsupported。C08/C09历史报告保留当时状态；本次开发分支更新收录C08–C10，无新Release。

## 下一阶段与冻结门槛

| 顺序 | 工作 | 当前状态 |
|---|---|---|
| C08 | 原工作线程生成目录会话的新鲜随机秘密；原身份／FileList与生成前后clock／取消复核，所持缓冲Zeroizing | 新 `capture_directory_fresh(File, CaptureLimits)`已实现；Windows复验 `PASS（Windows限定）`，不据C07计数推导通过；旧显式secret接口及caller-copy责任保持 |
| C09 | 可信opened anchor到leaf的相对raw UTF-16目录链；原worker FileList预算、全部持有祖先检查及Workbench宿主入口 | 限定Windows新20＋factory14＋原owner115＋原件42分别通过；Workbench仅check通过，产品NOT_RUN |
| G04 后续 | anchor以上／native picker时刻来源、Workbench产品执行、C10独立目录profile已实现，真实三语言guest与产品资格仍待验证、blob耐久后端 | OPEN；bare File仅证明对象，相对链不证明anchor以上来源，公开FileList仍Unsupported |
| 文件完整能力 | blob 耐久 backend/history、upload、watch、rename及恢复矩阵 | OPEN；conditional Replace 仍 Unsupported，不退化为无条件覆盖 |
| SDK26 其余门槛 | 异步组合、长期 changes/cursor、完整账户与网络恢复、第三方安装批准、平台矩阵及生产 UI | OPEN，按完整 26 项要求审计；不缩小为当前已通过的子集 |

生产 protected owner、真实 session、StorageIoWorker、普通用户 token、GUI、账户/TLS/公开服务与其他平台仍需对最终候选分别验收。本轮普通合成 Store 与临时目录的 PASS 不能替代这些资格。Linux 云端证据保持其来源，不能当作 Windows 或全平台通过。

## 阅读与证据规则

默认 README 为中文，另有八种语言入口。[文档导航](DOCUMENTATION_INDEX.md)列出维护入口与历史／冻结范围。状态、已实现接口、实际执行、生产资格及发布状态分别记录；历史报告中的“下一项”“未提交”“未推送”按当时日期理解。原始失败、过滤、Unknown 与未运行项保留，不用后来的通过覆盖。当前开发仍是重构测试线，满足完整门槛后才考虑 SDK 冻结与 0.2.0。
