# 文件变更 guest 扩展草案

2026-09-27 [Guest 审批期限与收尾修复](../reports/guest-mutation-expiry-2026-09-27.md)：首次 Start 前按单调时间计时，到期后禁止新 Prepare/Chunk/Commit/Execute 和提交重试，保留已有回执核对及 Stop→实际退出→必要 Repair/ACK。弹窗到期禁用确认，确认返回后重新观察原任务；撤权／换库精确关闭旧弹窗，不误关其他对话框。九语言提示同步。最终会话 18、Widget 9、真实原生 4，共 **31/31** 通过，包含两次真实 30 秒到期后的独立 Discovery/Reconcile。修复尚未纳入上一轮 Windows ZIP；下一步为系统选择器与人工审批验收、新应用产物及其他平台资格。SDK 未冻结，未提交／推送／发布。

这是独立实验性 Cap’n Proto 契约，现有独立 Wasm import 和受控原 owner 接入；SDK 尚未冻结。`core/schemas/mutation.capnp` 为权威源，`sdk/rust/contracts/mutation.capnp` 为独立 SDK 镜像；既有 IO v1／transport 原件不变。当前只覆盖 Create／Delete，不提供 Replace 降级替代。

## 请求与相关性

最新 [16 MiB 内容事务资格](../reports/guest-mutation-content-crash-2026-09-27.md) **20/20** 通过：四个 Core 提交点、三语言 Guest、普通宿主对照及 Rust 恢复 Widget；全程不调用 Execute。新宿主重复核对保持原 Prepared，独立只读 Store 进一步区分正文／回执回滚与完整持久化。此结果不代表断电、完整应用／选择器、人工审批或其他平台资格；SDK 未冻结。后续条目保留此前各阶段边界。

最新 [16 MiB 文件效果资格](../reports/guest-mutation-max-crash-2026-09-27.md) 已完成 **52/52**：三语言原生会话及 Rust 恢复 Widget 使用真实最大正文和原期限。历史读取现在在普通声明正文上限之外预留最多 12,448 字节计划／结果空间，完整读取照常收费、累计预算保持原声明，不能挪给额外正文或执行操作；低累计额度仍保留原小记录路径。四个内容事务提交点的最大正文／界面资格及其他平台仍开放，SDK 未冻结。

2026-09-27 的 [真实故障恢复资格](../reports/guest-mutation-crash-ui-2026-09-27.md) 已覆盖三语言非空 Create／Delete、新普通宿主原计划核对、Rust 恢复界面与普通宿主对照，共 **32/32**。恢复入口可显式选择 Guest 工作流；不会自动启动发现或重放执行。最大 16 MiB 的完整故障组合与平台资格仍开放。

扩展预算包的历史恢复使用独立 `bind_mutation_history`，不批准扩展执行额度，也不执行 Wasm。仍受当前包摘要、修订、启用状态、已批准单项文件能力及有界只读预算约束；正文／累计沿用普通声明，仅历史内部读取具有上述元数据空间。只允许原计划发现／分页／关闭和独立核对，不能签发目标资源、通用 IO 作业或变更执行许可。普通 `bind_io` 对预算扩展包的拒绝保持不变，原接入背景见 [历史绑定报告](../reports/guest-mutation-history-binding-2026-09-27.md)。

工作台到可信 Dart 客户端的独立私有协议现已接入，见 [私有协议与客户端报告](../reports/guest-mutation-private-wire-2026-09-27.md)。Start／Submit／Status／Read／CancelCommand 不嵌入插件业务请求。结果分别表达 Owner 辅助回执、原始 Core guest 响应、调度／执行失败；Core 响应不被重编码为原生执行结果。读取需要原命令回执，后续 Core 动作持有 Prepare 返回的租约 reference；目标选择 reference 不可代替它。产品独立 guest 会话、实际预算和两次确认面板已接入，见 [界面与三语言验收](../reports/guest-mutation-execution-ui-2026-09-27.md)；预算扩展包跨重启只读恢复已有本机资格，系统选择器人工验收仍开放。

每个请求包含固定版本和 schema SHA-256、非零 callId、32 字节且不能全零的选择租约 reference、同样约束的命令 submission、操作 ID 和 1–30000 ms 的请求期限。操作 ID 为不超过 256 UTF-8 字节的非空字符串，拒绝 Unicode 控制字符及 `/`、`\`、`:`。租约须由原 owner 在明确批准后签发，不能由插件编解码器自行创建权限。

响应回显 callId、reference、submission、操作 ID 和动作类型，接收方逐项校验。每个 mutation job 只允许一次与宿主输入一致的 import，完成帧必须等于宿主实际回帧；不定义隐式 Accepted／Pending，也不自动重试。原 owner 用有界账本将 submission 绑定到规范化语义、原始期限及结果；重取只允许改变交付 callId，仍检查当前授权。编解码器本身不签发权限。

| 动作 | 数据与成功结果 |
| --- | --- |
| PrepareCreate | 长度不超过 16 MiB 与非零 SHA-256；空内容必须为 SHA-256(empty)。结果 Prepared，无效果，无已传字节 |
| PrepareDelete | 无内容字段；结果 Prepared，无效果 |
| Chunk | 1–60 KiB，offset 加长度不得溢出或超过 16 MiB；结果 Prepared，已传字节必须等于该块末尾，尚未持久提交 |
| Commit | 结果 Prepared 且 durableContent 为 true；具体总长度与批准哈希仍由原 owner 检查 |
| Execute | Completed 时必须 Observed，OS 结果为成功或拒绝；不确定时 OutcomeUnknown，无伪造成功 |
| Query | 返回原操作的 Absent／Prepared／Unknown／Observed／CancelledBeforeDispatch；没有记录不证明此前未产生效果 |
| CancelPlan | 只表示派发前取消；不可作为已发生效果的回滚 |
| Release | 无阶段／效果／内容元数据；不等同于 worker 已退出或已 ACK |

一般失败状态不得携带成功阶段、效果或内容元数据。Query 的 Completed 表示查询成功，仍可查询到 Unknown；它不代表文件操作成功。Observed＋OsRejected 表示保存了系统拒绝结果，也不代表文件操作成功。

请求与响应帧最多 128 KiB，解码限制遍历预算和嵌套深度并拒绝尾随消息、非法枚举及能力指针。内容复制前校验长度和偏移。允许合法且有界的 Cap’n Proto 分段布局，不要求整帧具有唯一字节编码；尾随检查针对声明消息之后的多余字节。后续提交去重必须绑定已验证语义参数或规范命令编码，不能因分段布局不同将同一命令重新执行，也不能把 delivery callId 当成业务操作身份。Codec 只能验证单帧；块连续性、累计预算、实际持久化与权限撤销均是后续 owner 的强制职责。

## 共享验证与后续接线

[sdk/vectors/mutation-v1](../sdk/vectors/mutation-v1/README.md) 保存由独立 Cap’n Proto 命令行编码器生成的正反二进制向量，包含 Unicode 控制字符、越界、错误空哈希、尾随数据与结果矛盾。Core 与 Rust SDK 分别解码这些原件，工具校验 schema 和每个向量的摘要。

C／C++ 编解码 ABI 与原生向量验证已完成；原 owner 的可信审批凭证、独立执行许可及 opt-in 强制检查见 [授权检查报告](../reports/mutation-guest-approval-2026-09-27.md)。独立 import、包协商、显式 mutation job 与原 owner 暂停／恢复分派现已接入。不得将此扩展直接送入旧 IO v1 的 exchange 入口。详见 [实施计划及接入约束](PLUGIN_MUTATION_SDK_PLAN.md)。

## C／C++ 编解码边界

C 的 `morrow_plugin_mutation.h` 提供版本化描述符、请求 encode／validate、schema 摘要与响应 decode／get／free。描述符先置零；只有 Create 使用 content_length／content_sha256，只有 Chunk 使用 offset／bytes，其他动作不得携带这些字段。未使用的 span 必须为 `{NULL, 0}`，不能仅将长度清零而留下旧指针。

描述符先检查 8 字节版本／长度前缀，再访问完整结构。调用方仍须提供真实存活、按要求对齐且不重叠的内存；本地 C ABI 不是用于接收任意地址的安全沙箱。失败时合法输出槽中的长度归零、handle 为空，编码目标缓冲不写入部分结果。

成功解码的 handle 独立持有结果和原响应帧；输入请求／响应缓冲可随后释放，view 中借用字段仅在 handle 存活时有效。handle 必须释放一次，`free(NULL)` 可用。C++ 的 `morrow_plugin_mutation.hpp` 提供八种动作的自有数据工厂及只可移动的响应 RAII；移动赋值释放原结果，从移出对象再取 view 失败，自移动保留有效结果。

原生编解码接口不授予文件权限或重试效果。Windows 原生验证入口为 `tool/verify_plugin_mutation_sdk.ps1`；它构建 SDK DLL，严格编译并运行 C／C++ 测试，使用同一 21 个二进制向量。

## 显式 Wasm 接入

包声明 `mutation-v1`、guest ABI 2、`io-v1`、精确 `mutation_schema_sha256`，并声明 FileCreate／FileDelete 能力。协商不等于授权。固定 import 为 `morrow_mutation_v1.call`，签名 `(i32, i32, i32, i32) -> i32`；请求和响应内存必须分离，响应容量为协议最大 128 KiB。普通任务、旧 IO／服务任务以及未显式启用的原 owner 继续拒绝此入口。

可信宿主通过 `IoWorker::submit_mutation_guest_frame` 选择 `MutationGuestJobMode::Stage(lease)` 或 `Execute(permit)`。Stage 支持准备、分块、提交、查询、取消计划和释放，拒绝 Execute；Execute 支持执行、查询和释放，拒绝继续修改内容。宿主在准备任务返回后通过原 owner 队列签发精确计划的一次性执行许可；Wasm 不会在暂停 import 内等待同一队列审批。

Rust 提供 `mutation::wasm_call_frame`／`Request::wasm_call`、C 提供 `mp_wasm_mutation_call_frame`／`mp_wasm_mutation_call`，C++ 提供 `mutation_response::call_frame`／`call`。所有 helper 校验输入、检查回帧相关性并保留原始已验证响应，传输失败不会自动重试。三个示例位于 `sdk/examples/{rust,c,cpp}-mutation`。

运行时逐次预留请求与最大响应字节，并收取实际选择能力的额度。16 MiB 是协议的单内容上限；分块帧、响应预留、输入、准备和执行审批同样消耗额度。旧 IO 的单次 16 MiB／累计 64 MiB 限额保留，不能容纳最大内容的完整 guest 流程。

大内容包须额外声明 `mutation-budget-v1` 与 Manifest 字段 20 `MutationBudget`，单次／累计声明上限分别为 32／256 MiB，仅允许 Create／Delete，不混入依赖或服务 profile。可信宿主使用 `Manager::bind_budgeted_mutation` 批准不超过声明的实际额度，并使用 `JobLimits::mutation` 启动原 worker；普通 `bind_io` 拒绝新 profile。每个实例只能绑定一次，Release 或丢弃绑定不退还累计额度。包声明不构成授权，目标选择、精确计划审批与一次性执行许可仍分别强制执行。

签发 guest 租约前，`MutationBudgetEstimate::for_plan` 对标准无重试流程计算两层账本、单次准入及 submission 上界；不够则在 Prepare 前拒绝。预检是快照，不预留整个流程，也不保证之后的并发用量、权限或时间。实际动作继续逐次检查。请求期限与任务／撤权取消关联，重复提交不能延长原有 30 秒期限；期限不能强制中断已经进入的同步 OS 调用，迟到效果需查询原历史。

可重复验证入口为 `tool/verify_plugin_mutation_wasm.ps1`。它编译三语言 Wasm，严格确认六个测试均已注册，再通过 Release 原受管 owner 运行普通与完整 16 MiB 临时目录用例。正文采用高熵内容，核对实际文件摘要、Observed、Release、预检上界与余量拒绝；仅编译成功、零匹配测试或旧原件哈希一致均不等于执行验收。

Windows 三语言实际执行与范围见 [Wasm 接入报告](../reports/mutation-guest-wasm-2026-09-27.md)。

显式预算 profile 的 Windows Release 六项验证已通过，16 MiB 流程实测约 1.87–2.50 秒。Debug 首轮为 5/6，Rust 单独重现 Deadline；没有放宽期限或改变内容来消除失败。这是限定环境的运行证据，不是任意磁盘或负载下的延迟保证。详见 [预算与最大内容报告](../reports/mutation-budget-2026-09-27.md)；SDK 未冻结。

非空四块正文的恢复资格见 [guest 恢复报告](../reports/mutation-guest-recovery-2026-09-27.md)：三语言实际 SDK 的丢回执流程 3/3、内容事务与 native Create 退出案例 36/36，以及普通构建控制 3/3 通过。入口为 `tool/verify_plugin_mutation_recovery.ps1`。它验证原 owner 的历史回取、独立 Store 恢复和 Core claim 防重放；不代表断电、完整 16 MiB 故障组合、产品恢复界面或其他平台已验收。

## Workbench 任务入口

`start_selected_guest_mutation` 显式接收批准预算，`submit_guest_mutation` 的 Prepare／Execute 分别绑定规范计划摘要。内部 Issue／Authorize 回执只由各自阶段读取，成功领取后才提交一条真实 Wasm 帧。`GuestMutationReply` 区分可信 Owner 辅助结果与 guest Frame，不额外自动查询或回退到原生效果；旧 native 命令入口拒绝 guest 任务。Query／HostQuery 不会将尝试执行后的 Absent／Prepared 当成已核对完成；Release 请求停止后仍须真实 join 和 ACK。

目录可显示 mutation 声明和预算上限，但不代表已批准额度或已完成 Flutter guest 操作接线。Windows Release 三语言实际 SDK 的 Workbench 流程 3/3 通过，入口为 `tool/verify_workbench_mutation_guest.ps1`；其验收范围和剩余私有协议／UI／跨重启恢复工作见 [Workbench 接入报告](../reports/workbench-mutation-guest-2026-09-27.md)。
