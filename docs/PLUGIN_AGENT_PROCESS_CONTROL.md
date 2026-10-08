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

# Codex 会话与进程接口

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

更新：2026-10-06，C12 本地接口增量，未提交或推送；完整 SDK26/G04 和产品资格继续 OPEN。

该扩展将 Wasm 会话逻辑与可信原生执行层对接。正式会话和工具记录仍归原 Core
内容库与 R2 owner；插件不能通过一个路径、进程 ID 或历史记录自行恢复执行权。

| 接口 | 实现位置 | 语义 |
| --- | --- | --- |
| 会话创建、分页历史、writer fence、事件追加、checkpoint、sealed 续接 | `extensions/codex-session-exec-client-r2` | 安全 Rust 客户端与实际 Wasm guest；每个请求仅交换一次 |
| 提议、Claim、事实报告、Inspect | 同上，原 `agent-session-exec-v1-r2` | 审批与原生启动仍由可信宿主执行；Unknown 不重放 |
| 能力发现、输出 Read/Events、stdin、关闭输入、Interrupt、Terminate、PTY resize | `extensions/agent-process-control-v1` | 独立预编译 Cap'n Proto；能力取实际 provider 与宿主批准的交集 |
| 进程接口的类型化消费与 Wasm import | `extensions/codex-process-control-client-v1` | 关联核验、有限预算、单调请求 ID；未知控制后仅允许读事实 |
| 包审查、会话/进程统一运行入口、实时授权 | `extensions/agent-session-process-v1-host` | 完整包摘要绑定双 schema、范围和上限；复用原连接与 owner |
| Windows 原生执行端口 | `companions/morrow-codex/session-exec-windows-v1` | 使用固定上游真实 ExecBackend/ExecProcess；执行资格单独记录 |

运行入口只有一个额外 import：`morrow_agent_session_process_v1.call`。旧工厂拒绝该
入口；新工厂继续拒绝混合 IO、依赖、旧 R2 import、WASI 及未知版本。运行期分别传递
原 canonical R2 或 process-control 帧，不复制一套会话 schema。包声明以版本化
Protobuf/LZ4 保存，声明本身不批准任何能力。

宿主必须先在原 R2 中完成 Propose、可信审批、Claim，再通过原 `execute_claimed`
一次性启动。只有原 `invocation_started` 与完整工具身份匹配，才可以把真实 provider
绑定到一个新的有限进程 handle。绑定保存原执行 Connection/Admission；另行检查插件的
读取批准。新增纯读取 `validate_started_tool` 重用原 R2 执行授权、提议者、期限、会话
epoch 和 owner 检查，不改变原 wire、存储 schema、令牌或执行状态机。

校验期间读取真实时钟，不能把一个旧 `now` 值重复作为“新鲜”检查。控制前、控制后与
编码后均复验；效果发生后失权只能返回 Unknown。Unknown、响应丢失或关联失配禁止客户端
自动重新写入/停止/发送信号。当前控制回执与 handle 只在当前原生进程内有效，重启不恢复。

输出分页携带序号、截断位置、gap、实际退出与实际输出结束。退出不等于 EOF；可信收尾可
在插件撤权或预算耗尽后读取事实。已启动进程只有在退出与 EOF 均被观察、且 provider
和待完成工作不再占用后才返还原生资源及配额；provider 可以提前 Drop 并将资源交给
异步收尾，收尾未确认的资源继续计费。有界 tombstone 保留 handle 的旧身份，禁止复用。

关闭输入与 PTY resize 的协议已定义，但 provider 必须实际支持才能声明能力。当前固定
上游方法缺口不能被一个成功响应代替。Windows 普通合成执行、系统沙箱、受保护内容库、
账户与完整 Codex 应用注入分别验收；通过客户端、Wasm 或逻辑 provider 测试不代表上述资格。

限定测试、原始失败、源码/插件 hash 与未运行项见
[C12 记录](../reports/reconstruction-2026-10-06/codex-sdk-c12.md)。旧 SDK 原件与 R1/R2
封存归档保留，新增接口处于独立实验版本；SDK 全集冻结尚未完成。
