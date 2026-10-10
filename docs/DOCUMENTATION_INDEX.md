# Morrow 文档导航与维护范围

<!-- C28-CURRENT-BEGIN -->
## 当前开发检查点（2026-10-10，C28 阶段 21）

严格 checkout 第二轮来源验证与匹配 helper 第三轮离线构建已通过各自限定验收及独立读回；没有运行 helper 或真实沙箱。公开 guest 第三次后继的 metadata、编译与外层均实际退出 0，原始输入、物理集合和完整新 Cargo home 后置守卫通过。新 Wasm 为 425,912 字节，固定机器路径模式的全字节扫描无命中；这只是有界路径检查，不是通用秘密检测证明。

独立静态核对确认新旧 Wasm 的调用接口相同，数据区与堆起点地址变化，宿主须使用动态布局。真实七步会话测试及控制器已完成源码审核，127 包原锁和现有 120 份 registry 依赖已核对；尚未执行宿主编译、Wasmi 实例化或七步行为测试。新二进制尚未接入公开 harness，不能将静态接口核对称为运行资格。

本次仅更新七份脱敏进度文档，产品源码与已推提交 `98ad75dc` 一致，版本保持 `0.1.9-test.58+62`。阶段 18 仍为 Cargo 退出 0、外层退出 1；公开 Git 树完整构建保持 `NOT_RUN_MISSING_SESSION_FIXTURE`。原生 Start 的 Unknown 不重放；owner finish、factory release、cleanup/join、真实断连、生产沙箱及后续 11 项原始库测试仍待验收。`SDK26_G04=OPEN`，`release_eligible=false`，SDK 未冻结。

下一步实际执行已审核的单次七步会话测试，核对新 guest 的父子会话、检查点和输出；其后再推进获得明确授权的 Windows 生命周期复验。会话层与安全执行层验收后暂停准备测试预览，不等待扩展执行层。见[阶段 21 进展记录](../reports/reconstruction-2026-10-10/windows-agent-sdk-c28-stage21.md)、[阶段 20 准备记录](../reports/reconstruction-2026-10-10/windows-agent-sdk-c28-stage20.md)与[阶段 18 编译记录](../reports/reconstruction-2026-10-09/windows-agent-sdk-c28-stage18.md)。下方内容保留历史时点。
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

当前C10检查点（2026-10-05）：本次开发分支更新收录C08–C10。C10已新增独立 `fs-directory-request-v1`、严格单import、包feature及只读discovery，复用原已批准selection和owner，不导出路径、句柄或新授权。Windows新Rust34（codec12/helper4/profile9/owner9）、既有回归191及frame5分别通过；Python33和两个原生C/C++消费者另计。新Rust Wasm仅编译通过，真实新Rust guest、C/C++ Wasm、Workbench产品／GUI、受保护Session与其他平台仍未验收。旧Core IO FileList保持Unsupported，SDK26／G04仍OPEN，无新Release。 详见[接口与实测边界](../reports/reconstruction-2026-10-05/directory-request-sdk.md)。

核对日期：2026-10-05。当前版本、资格和下一阶段以[项目状态](PROJECT_STATUS.md)为准；具体任务见[开发看板](DEVELOPMENT_BOARD.md)、[路线](FUTURE_ROADMAP.md)和[SDK 门槛](../reports/reconstruction-2026-10-05/sdk-next-gates.md)。

本次逐文件盘点覆盖更新前的 1,759 份 Markdown。下表按文档用途划定维护范围；核对记录不增加任何测试通过数。本页与当前状态、同步报告为本次新增入口，不计入盘点前基数。

| 文档用途 | 数量 | 维护方式 |
|---|---:|---|
| 项目自有维护说明 | 141 | 核对接口、当前源码与资格；修正具体过时描述，仍准确的内容保留 |
| 有限夹具／适配器说明 | 3 | 保留各自有限范围，不能外推生产资格 |
| 带版本与日期的架构方案 | 1 | 保留原设计；当前执行顺序放入状态与路线 |
| 历史阶段报告与证据说明 | 506 | 保留原日期、输入身份、失败、过滤及未运行项；当前入口另行更新 |
| companion／upstream 快照与资格说明 | 1,093 | 保留来源、版权与历史身份，不作全量版本替换 |
| 冻结 SDK／原二进制基线说明 | 15 | 原字节保留；新的使用说明与冻结原件分开 |

C09已完成限定Windows Release／locked／offline资格，见 [C09阶段说明](../reports/reconstruction-2026-10-05/directory-selection-owner.md)。可信宿主 `capture_directory_under(anchor, relative, limits)`只保留并核验原opened anchor到relative leaf的raw UTF-16句柄链，复用原worker FileList、原时钟、取消和预算；root加N个分量共享原8资源，32段语法上限不是可用深度。新selection_path8＋directory_selection12、C08 factory14、原owner九组115和原件42分别当前实际PASS；原件42为base9／dependency3／region7／reader主9／shared14，reader raw10含child helper1不加方法，region保留84过滤。17个credited测试进程合191 meaningful方法（raw192含child1），zero-match失败进程保留且不计功；这些数字不能作为SDK冻结。Workbench第二次Release x86_64 `--locked --offline --lib` check通过，首次缺offline asn1-rs0.7.2的exit101保留；只是编译检查，ProtectedSession／GUI／picker以上provenance和non-Windows产品执行NOT_RUN。C09 network100和Clippy明确NOT_RUN，不继承C08历史通过或lint结果。这不证明picker时刻、anchor以上来源或传入anchor的sharing策略，不增加guest FileList、目录guest或公共UI，blob耐久后端仍缺。SDK26／G04仍OPEN，公开FileList及conditional Replace仍Unsupported。C08/C09历史报告保留当时状态；本次开发分支更新收录C08–C10，无新Release。

9 份多语言 README 共用当前源码检查点与已发布下载的边界。SDK327／frozen57 以及原封存交接包不改；C08已新增原worker fresh-secret入口并完成限定Windows复验，见 [C08实测](../reports/reconstruction-2026-10-05/directory-secret-factory.md)；这仍不把限定Windows PASS计为完整SDK冻结。源码与公开下载的身份见[文档同步记录](../reports/reconstruction-2026-10-05/documentation-sync.md)。

## 项目自有维护说明

以下相对路径是本次盘点的维护入口。列出一份文档表示已纳入状态核对，内容仍准确时无需修改；具体阶段执行结果仍以其原报告为准。

- [audit/README.md](../audit/README.md)
- [companions/README.md](../companions/README.md)
- [contracts/experimental/agent_host_v1/README.md](../contracts/experimental/agent_host_v1/README.md)
- [contracts/experimental/agent_host_v2/README.md](../contracts/experimental/agent_host_v2/README.md)
- [contracts/experimental/agent_host_v2_capnp/README.md](../contracts/experimental/agent_host_v2_capnp/README.md)
- [contracts/experimental/agent_host_v3_http_stream/README.md](../contracts/experimental/agent_host_v3_http_stream/README.md)
- [core-web/README.md](../core-web/README.md)
- [core/README.md](../core/README.md)
- [demos/plugin_stage_windows/README.md](../demos/plugin_stage_windows/README.md)
- [docs/ANDROID.md](ANDROID.md)
- [docs/ANDROID_PLUGIN_RUNTIME.md](ANDROID_PLUGIN_RUNTIME.md)
- [docs/ARCHITECTURE_BASELINE.md](ARCHITECTURE_BASELINE.md)
- [docs/BUILD_INVENTORY.md](BUILD_INVENTORY.md)
- [docs/BUILD_RECEIPTS.md](BUILD_RECEIPTS.md)
- [docs/DEVELOPMENT_BOARD.md](DEVELOPMENT_BOARD.md)
- [docs/DEVELOPMENT_BRANCH_STATUS.md](DEVELOPMENT_BRANCH_STATUS.md)
- [docs/EDITOR_DRAFT_STAGING_PLAN.md](EDITOR_DRAFT_STAGING_PLAN.md)
- [docs/EDITOR_HANDOFF_PROPOSAL_WIRE_PLAN.md](EDITOR_HANDOFF_PROPOSAL_WIRE_PLAN.md)
- [docs/EDITOR_UI_PERSISTENCE_PLAN.md](EDITOR_UI_PERSISTENCE_PLAN.md)
- [docs/FUTURE_ROADMAP.md](FUTURE_ROADMAP.md)
- [docs/I18N_PREVIEW.md](I18N_PREVIEW.md)
- [docs/IO_INTENT_RECORDS.md](IO_INTENT_RECORDS.md)
- [docs/IO_PROTECTED_EVIDENCE.md](IO_PROTECTED_EVIDENCE.md)
- [docs/LANGUAGES_AND_FONTS.md](LANGUAGES_AND_FONTS.md)
- [docs/MIGRATE_LIBRARY.md](MIGRATE_LIBRARY.md)
- [docs/PLUGIN_API_NODE.md](PLUGIN_API_NODE.md)
- [docs/PLUGIN_APP_HTTP_TASKS.md](PLUGIN_APP_HTTP_TASKS.md)
- [docs/PLUGIN_APP_IO_TASKS.md](PLUGIN_APP_IO_TASKS.md)
- [docs/PLUGIN_APPLICATION_MANAGEMENT.md](PLUGIN_APPLICATION_MANAGEMENT.md)
- [docs/PLUGIN_ARCHIVE_RETENTION.md](PLUGIN_ARCHIVE_RETENTION.md)
- [docs/PLUGIN_BATCH_EVIDENCE.md](PLUGIN_BATCH_EVIDENCE.md)
- [docs/PLUGIN_CAPTURE_PROVENANCE.md](PLUGIN_CAPTURE_PROVENANCE.md)
- [docs/PLUGIN_CHANGES_METADATA_SDK.md](PLUGIN_CHANGES_METADATA_SDK.md)
- [docs/PLUGIN_CHANGES_METADATA_V1.md](PLUGIN_CHANGES_METADATA_V1.md)
- [docs/PLUGIN_CHANNEL_PAYLOAD_DISTRIBUTION.md](PLUGIN_CHANNEL_PAYLOAD_DISTRIBUTION.md)
- [docs/PLUGIN_CHANNEL_V1.md](PLUGIN_CHANNEL_V1.md)
- [docs/PLUGIN_COMMITTED_EVIDENCE.md](PLUGIN_COMMITTED_EVIDENCE.md)
- [docs/PLUGIN_CONTENT_PROJECTION.md](PLUGIN_CONTENT_PROJECTION.md)
- [docs/PLUGIN_DEPENDENCIES.md](PLUGIN_DEPENDENCIES.md)
- [docs/PLUGIN_DEPENDENCY_GRAPH.md](PLUGIN_DEPENDENCY_GRAPH.md)
- [docs/PLUGIN_DEPENDENCY_LOCKS.md](PLUGIN_DEPENDENCY_LOCKS.md)
- [docs/PLUGIN_DIRECTORY_BLOB_SDK.md](PLUGIN_DIRECTORY_BLOB_SDK.md)
- [docs/PLUGIN_DYNAMIC_DEPENDENCIES.md](PLUGIN_DYNAMIC_DEPENDENCIES.md)
- [docs/PLUGIN_ENDPOINT_MANAGEMENT.md](PLUGIN_ENDPOINT_MANAGEMENT.md)
- [docs/PLUGIN_FILE_TASKS.md](PLUGIN_FILE_TASKS.md)
- [docs/PLUGIN_HTTP_SUBMISSION.md](PLUGIN_HTTP_SUBMISSION.md)
- [docs/PLUGIN_INSTANCE_POOL.md](PLUGIN_INSTANCE_POOL.md)
- [docs/PLUGIN_IO_DESIGN.md](PLUGIN_IO_DESIGN.md)
- [docs/PLUGIN_IO_EXECUTION.md](PLUGIN_IO_EXECUTION.md)
- [docs/PLUGIN_IO_JOBS.md](PLUGIN_IO_JOBS.md)
- [docs/PLUGIN_IO_MANAGEMENT.md](PLUGIN_IO_MANAGEMENT.md)
- [docs/PLUGIN_IO_OWNERSHIP.md](PLUGIN_IO_OWNERSHIP.md)
- [docs/PLUGIN_MANAGED_HTTP.md](PLUGIN_MANAGED_HTTP.md)
- [docs/PLUGIN_MANAGED_SERVICE.md](PLUGIN_MANAGED_SERVICE.md)
- [docs/PLUGIN_MANAGER.md](PLUGIN_MANAGER.md)
- [docs/PLUGIN_MUTATION_GUEST.md](PLUGIN_MUTATION_GUEST.md)
- [docs/PLUGIN_MUTATION_SDK_PLAN.md](PLUGIN_MUTATION_SDK_PLAN.md)
- [docs/PLUGIN_MUTATION_WIRE.md](PLUGIN_MUTATION_WIRE.md)
- [docs/PLUGIN_NETWORK_API.md](PLUGIN_NETWORK_API.md)
- [docs/PLUGIN_OPERATION_HISTORY.md](PLUGIN_OPERATION_HISTORY.md)
- [docs/PLUGIN_OUTBOUND_AUTHORITY.md](PLUGIN_OUTBOUND_AUTHORITY.md)
- [docs/PLUGIN_PACKAGE.md](PLUGIN_PACKAGE.md)
- [docs/PLUGIN_PROJECT_TOOLS.md](PLUGIN_PROJECT_TOOLS.md)
- [docs/PLUGIN_READ_ARCHIVE.md](PLUGIN_READ_ARCHIVE.md)
- [docs/PLUGIN_READ_JOURNAL.md](PLUGIN_READ_JOURNAL.md)
- [docs/PLUGIN_REGISTRY.md](PLUGIN_REGISTRY.md)
- [docs/PLUGIN_SDK_AND_UI.md](PLUGIN_SDK_AND_UI.md)
- [docs/PLUGIN_SDK_CAPABILITY_MATRIX.md](PLUGIN_SDK_CAPABILITY_MATRIX.md)
- [docs/PLUGIN_SDK_COMPATIBILITY.md](PLUGIN_SDK_COMPATIBILITY.md)
- [docs/PLUGIN_SDK_DIAGNOSTICS.md](PLUGIN_SDK_DIAGNOSTICS.md)
- [docs/PLUGIN_SDK_DISCOVERY.md](PLUGIN_SDK_DISCOVERY.md)
- [docs/PLUGIN_SDK_EXTENSION_CONTRACT.md](PLUGIN_SDK_EXTENSION_CONTRACT.md)
- [docs/PLUGIN_SDK_PREFLIGHT.md](PLUGIN_SDK_PREFLIGHT.md)
- [docs/PLUGIN_SEGMENTED_OWNER_FRAMES.md](PLUGIN_SEGMENTED_OWNER_FRAMES.md)
- [docs/PLUGIN_SELECTED_FILE.md](PLUGIN_SELECTED_FILE.md)
- [docs/PLUGIN_SERVICE_AUTHORITY.md](PLUGIN_SERVICE_AUTHORITY.md)
- [docs/PLUGIN_SERVICE_CONTENT.md](PLUGIN_SERVICE_CONTENT.md)
- [docs/PLUGIN_SERVICE_HISTORY.md](PLUGIN_SERVICE_HISTORY.md)
- [docs/PLUGIN_SERVICE_MANAGEMENT.md](PLUGIN_SERVICE_MANAGEMENT.md)
- [docs/PLUGIN_SERVICE_RECOVERY.md](PLUGIN_SERVICE_RECOVERY.md)
- [docs/PLUGIN_SERVICE_RESOURCES.md](PLUGIN_SERVICE_RESOURCES.md)
- [docs/PLUGIN_SERVICE_RUNTIME_PLAN.md](PLUGIN_SERVICE_RUNTIME_PLAN.md)
- [docs/PLUGIN_SERVICE_TLS.md](PLUGIN_SERVICE_TLS.md)
- [docs/PLUGIN_SHARED_EVIDENCE_STORAGE.md](PLUGIN_SHARED_EVIDENCE_STORAGE.md)
- [docs/PLUGIN_SSE_CHANNEL.md](PLUGIN_SSE_CHANNEL.md)
- [docs/PLUGIN_SSE_EVENT_SDK.md](PLUGIN_SSE_EVENT_SDK.md)
- [docs/PLUGIN_SUPERVISION_G0_REMAINING_011.md](PLUGIN_SUPERVISION_G0_REMAINING_011.md)
- [docs/PLUGIN_SUSPENDABLE_IO_PLAN.md](PLUGIN_SUSPENDABLE_IO_PLAN.md)
- [docs/PLUGIN_SYSTEM_STATUS.md](PLUGIN_SYSTEM_STATUS.md)
- [docs/PLUGIN_TASK_EVIDENCE.md](PLUGIN_TASK_EVIDENCE.md)
- [docs/PLUGIN_TASK_PROTOCOL.md](PLUGIN_TASK_PROTOCOL.md)
- [docs/PLUGIN_TASKS.md](PLUGIN_TASKS.md)
- [docs/PLUGIN_UI_PROTOCOL.md](PLUGIN_UI_PROTOCOL.md)
- [docs/PLUGIN_UI_RENDERER.md](PLUGIN_UI_RENDERER.md)
- [docs/PLUGIN_UI_SDK.md](PLUGIN_UI_SDK.md)
- [docs/PLUGIN_WEBSOCKET_CHANNEL.md](PLUGIN_WEBSOCKET_CHANNEL.md)
- [docs/PLUGIN_WS_MESSAGE_SDK.md](PLUGIN_WS_MESSAGE_SDK.md)
- [docs/QUERY_CAPTURE_DESIGN.md](QUERY_CAPTURE_DESIGN.md)
- [docs/readme/README.de.md](readme/README.de.md)
- [docs/readme/README.en.md](readme/README.en.md)
- [docs/readme/README.es.md](readme/README.es.md)
- [docs/readme/README.fr.md](readme/README.fr.md)
- [docs/readme/README.ja.md](readme/README.ja.md)
- [docs/readme/README.ko.md](readme/README.ko.md)
- [docs/readme/README.pt.md](readme/README.pt.md)
- [docs/readme/README.ru.md](readme/README.ru.md)
- [docs/RENAMING.md](RENAMING.md)
- [docs/RICH_CAPTURE.md](RICH_CAPTURE.md)
- [docs/ROADMAP_UPDATE_2026-09-15.md](ROADMAP_UPDATE_2026-09-15.md)
- [docs/SHARED_OBJECT_DESCRIPTOR.md](SHARED_OBJECT_DESCRIPTOR.md)
- [docs/SHARED_OBJECTS.md](SHARED_OBJECTS.md)
- [docs/SHARED_TRANSFER.md](SHARED_TRANSFER.md)
- [docs/UI_DESIGN_PRINCIPLES.md](UI_DESIGN_PRINCIPLES.md)
- [docs/WEB_DEPLOYMENT.md](WEB_DEPLOYMENT.md)
- [docs/WEB_PARITY.md](WEB_PARITY.md)
- [docs/WORKBENCH_ID_MIGRATION.md](WORKBENCH_ID_MIGRATION.md)
- [ds.md](../ds.md)
- [extensions/blob-transfer-v1/README.md](../extensions/blob-transfer-v1/README.md)
- [extensions/changes-metadata-v1/README.md](../extensions/changes-metadata-v1/README.md)
- [extensions/fs-directory-v1/README.md](../extensions/fs-directory-v1/README.md)
- [extensions/sse-event-v1/README.md](../extensions/sse-event-v1/README.md)
- [extensions/ws-message-v1/README.md](../extensions/ws-message-v1/README.md)
- [native_pipe_win_001/README.md](../native_pipe_win_001/README.md)
- [native_session/README.md](../native_session/README.md)
- [native_session_owner_002/README.md](../native_session_owner_002/README.md)
- [native_session_stream_001/README.md](../native_session_stream_001/README.md)
- [network_node/README.md](../network_node/README.md)
- [network_node_stream_001/README.md](../network_node_stream_001/README.md)
- [packages/morrow_core_client/README.md](../packages/morrow_core_client/README.md)
- [packages/morrow_i18n/README.md](../packages/morrow_i18n/README.md)
- [packages/morrow_plugin_ui/ONLINE.md](../packages/morrow_plugin_ui/ONLINE.md)
- [packages/morrow_plugin_ui/README.md](../packages/morrow_plugin_ui/README.md)
- [plugin_runtime/INLINE_UI.md](../plugin_runtime/INLINE_UI.md)
- [plugin_runtime/README.md](../plugin_runtime/README.md)
- [plugin_runtime/SHARED_MEMORY.md](../plugin_runtime/SHARED_MEMORY.md)
- [plugins/http_forward/README.md](../plugins/http_forward/README.md)
- [plugins/mid_autumn/artwork/PROMPT.md](../plugins/mid_autumn/artwork/PROMPT.md)
- [plugins/mid_autumn/README.md](../plugins/mid_autumn/README.md)
- [README.md](../README.md)
- [reports/reconstruction-2026-10-05/sdk-next-gates.md](../reports/reconstruction-2026-10-05/sdk-next-gates.md)
- [tool/windows/README.md](../tool/windows/README.md)

## 有限夹具与原架构入口

- [Morrow_Technical_Roadmap_v1_2026-09-15.md](../Morrow_Technical_Roadmap_v1_2026-09-15.md)
- [portable/cc-switch-pure/README.md](../portable/cc-switch-pure/README.md)
- [test/fixtures/README.md](../test/fixtures/README.md)
- [workbench_host/tests/fixtures/service-outbound/README.md](../workbench_host/tests/fixtures/service-outbound/README.md)

历史记录保留当时的“下一项／未提交／未推送”和结果；新检查点通过当前状态与分支提交记录说明。新增公开文档不包含私有备份标识、凭据、原始操作日志或实机用户目录。
