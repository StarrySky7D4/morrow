# 原 owner 后台文件任务

2026-09-27 [文件创建／删除界面与独立执行会话](../reports/mutation-execution-ui-2026-09-27.md)：Windows IO 设置已接选择、范围预览、准备、独立执行确认及退出／恢复交接；九语言补齐。修复停止与迟到回执门控、ACK 身份校验，以及真实宿主保留历史提交导致后续操作禁用的问题。执行会话 19/19、Flutter 组合 43/43（含非空三块内容的真实原生与真实控件链路）、本地化 4＋7 通过，严格分析无诊断。系统选择器由测试注入路径；未做完整应用构建或人工验收。下一步为版本化 guest 扩展、三语言 SDK／Wasm 和非空中间故障点；SDK 未冻结，未提交或推送。

2026-09-27 [真实故障后的恢复界面联调](../reports/mutation-crash-widget-2026-09-27.md)：真实时钟 Flutter 控件＋Windows 宿主完成 Create／Delete 各三阶段故障及普通宿主对照，7/7 通过；实际按钮完成发现、选择、离页返回、退出确认及独立核对，Unknown／Observed 文案分离，最终等 ACK 与宿主 close。既有面板及竞态回归 12/12，严格分析无诊断。新增三语言文件变更 SDK 接入计划；正式变更编辑／审批、guest SDK、非空内容中间故障点及其他平台继续开放，SDK 未冻结，未提交或推送。

2026-09-27 [真实宿主故障退出与恢复](../reports/mutation-crash-recovery-2026-09-27.md)：Create／Delete 各三个阶段的真实进程退出及普通构建对照共 7 项通过；新宿主重开临时受保护库，发现原计划并两次独立只读核对，Unknown 不重放。修复发送尚未完成时 EOF 导致未处理 Future 错误的竞态；确定性红绿回归和传输／关闭组合 38 项通过，严格分析无诊断。仍待真实崩溃后的 UI 按钮整链、正式变更编辑流程与 guest SDK；不代表断电或外部强杀验收，SDK 未冻结，未提交或推送。

2026-09-27 [文件变更恢复界面](../reports/mutation-recovery-ui-2026-09-27.md)：Windows IO 设置已接只读发现、显式翻页／续扫、所选计划核对及退出／修复／确认，新增有界展示模型保留页面外状态。九语言 30 条文案与资源同步；Dart 组合 66、真实宿主与既有设置组合 27、i18n 4＋7 项通过，新增 widget 11 项通过（含 320×640 窄屏），严格分析无诊断。后续为强制崩溃整链、正式文件变更编辑流程及 guest SDK；SDK 未冻结，未提交或推送。

2026-09-27 [同宿主断点续扫](../reports/mutation-checkpoint-2026-09-27.md)：Core／runtime／私有协议／Dart 已接通有界 checkpoint，可在旧任务退出与 ack 后以当前授权显式开启新任务续扫。位置限定同一存活 Store 与范围，宿主只保留最近 64 份，仍受共用 512 个启动身份限制，不跨进程持久化。Core 19、runtime 26、Dart 62、Flutter／真实宿主 27 项通过；宿主全量首次 171 通过、1 服务短期限失败，单独复跑通过，限制并发全量 172 项通过。下一步为恢复 UI 与真实崩溃整链；SDK 未冻结，未提交或推送。

2026-09-27 [独立恢复会话与真实进程恢复](../reports/mutation-recovery-session-2026-09-27.md)：Dart 会话稳定持有有界发现页／核对结果，补齐精确提交重取、丢回执、停止意图、实际退出／确认及失败准入放弃。真实 Windows 新进程重开临时库验证原计划一致、Observed／Prepared 分别核对及丢页后的明确重扫。Dart 组合 61、Flutter／真实宿主 27、打包工具 16 项通过，0 失败（Dart 1 既有文件夹具跳过），严格分析无诊断。下一步为恢复 UI、预算耗尽续扫与真实崩溃整链；SDK 未冻结，未提交或推送。

2026-09-27 [原计划发现协议与 Dart 客户端](../reports/mutation-discovery-wire-2026-09-27.md)：有界持久发现已接私有启动／翻页／领取／关闭协议和类型化客户端，补齐请求去重、混合字段拒绝与发现任务隔离。宿主全量 171、Dart 组合 46、Flutter 受控传输 26 项通过，0 失败（Dart 1 项既有文件夹具跳过）；7 份真实 Mutation 回帧已验证。下一步为独立恢复会话、丢回执处理、预算耗尽续扫和真实跨进程恢复；SDK 未冻结，未提交或推送。

2026-09-26：原生 `IoWorker` 的类型化文件命令与 Workbench Rust 文件任务已接通。Linux 的真实 Store／三语言既有 Wasm 模块和测试用 Workbench 所有权路径已验证。后续已接私有进程协议及独立 Dart 客户端；系统选择器、Flutter 页面与 Windows 实机仍未验收。

## 一个 owner、一条现有队列

平台适配器提供已经获得合法选择授权的 `std::fs::File`，不传 guest 路径。`capture_file` 接收句柄并返回 `FileSession` 和 `FileCommandHandle`；准入线程不读取文件元数据、不 seek/read，拒绝时会丢弃传入句柄。原 owner 已移动到现有 `IoWorker` 线程；prepare、实际捕获、guest 执行、断开与维护均在该线程完成。

命令复用原 owner lane：最多八个排队／运行／未领取回执，持有完整回复上限的队列预留。新的原生接口不提供任意闭包或 guest 授权入口：

| 接口 | 结果与约束 |
| --- | --- |
| `capture_file(file, handler, max_bytes, secret)` | 显式已选句柄、声明内处理器、可信宿主新随机 secret；成功返回固定字节元数据 |
| `read_file_chunk(session, offset, limit)` | 精确文件会话，单块最多 64 KiB；零 limit 使用上限，offset 不自动推进 |
| `finish_file(session)` | 原 guest 执行 Finish；不论 guest 正常完成或失败，此文件资源随后退休 |
| handle `poll/read/cancel` | 非阻塞、一次领取；停止、到期或失去授权后抑制未领取字节 |

`FileSession` 只在原 executor 上有效，没有反序列化恢复入口。文件字节由执行线程局部资源表持有，退出／panic 会丢弃；调用端只持会话身份与有界回执，不携带 FileBroker 或 Store。捕获回执被取消且没有成功领取时，执行线程回收对应资源；资源回收不能恢复累计额度。已领取的文件须显式 Finish 或停止 worker，丢弃一个已消费回执不等于关闭文件。

捕获复用 [单文件合同](PLUGIN_SELECTED_FILE.md) 的有界读取、类型检查、实际字节摘要及逐块撤权检查。额外取消检查在捕获边界拒绝继续；单次 OS 系统调用和单次 guest 执行仍不能被命令队列强制打断。后台线程化不等于异步 guest 续接。

## 两层费用与交付

宿主 `JobLimits` 与原实例 IO 预算同时有效：

- 宿主账本在命令准入时保守预留：捕获为调用者的 `max_bytes + 1`；Read／Finish 为两份 IO 最大帧，即 256 KiB。受单作业与累计上限约束，与既有 guest 作业共享 `worker.bytes()` 累计账本。因而宿主 ceiling 必须留出读取和结束余量。
- 原实例账本仍由 FileBroker 按实际固定长度加 EOF 探测字节及原请求／响应帧计费。读取作业与持有文件资源沿原 Manager、绑定和共享额度验证；宿主预留不替代实例批准。
- 取消、失败和丢弃已经准入的命令均不退累计费用；未准入的 Limit／Busy／外来会话不占队列槽。

工作台每个文件任务只允许一个待处理／未领取回执，低于底层八项队列上限。块回执使用 `Zeroizing<Vec<u8>>`，队列取消或丢弃时擦除其拥有的块缓冲；这不承诺整个文件 spool、guest 内存或调用者额外副本都已擦除。

共享时钟的采样和对应授权／计费检查在同一个短临界区内执行。文件 read/seek/metadata、摘要计算和 guest 运行不占用这把时钟锁；状态查询不会因为锁跨外部 IO 而一直等待。底层同步阻塞仍需等待实际返回，不能用一个“已取消”状态冒充线程退出。

## Workbench 原生接入

`start_file(StartOptions, File, handler, max_bytes)` 只接受 FileRead 能力集合、当前选中包摘要／Registry 修订和声明的处理器。它复用普通 IO 的连接、原 owner 移交和准入失败清理，但文件会话不会在启动后立即 drain。

正常调用顺序：

1. `start_file` 获得 `TaskKey`；`poll_io` 观察，`read_file_result` 领取 Captured。
2. `request_file_chunk(key, offset, limit)`，随后 `read_file_result` 领取一个块。待领取时拒绝重复准入；无隐式重读或 offset 递增。
3. `finish_file(key)`，读取 Finished 后请求停止。命令终态错误也请求停止；不自动重捕获文件。
4. `poll_io` 等实际 join，再按原规则 `repair_io`／`acknowledge_io`。`cancel_io`、关闭及 EOF 沿现有停止路径回收。

在 owner 尚未归还时，依赖内容库的普通调用仍 Busy。已保存卡片、原 Manager、池和存储保护守卫随完整 `WorkbenchState` 移动，没有第二份 Store。收到 Finished 只表示结束命令完成，不代表维护和断开均成功。

Linux Workbench 的受保护存储打开仍明确拒绝。本轮仅在 `cfg(test)` 下构造真实 Store 供接口验证，维护仍按原实现失败，并验证 `RecoveryRequired`、修复失败和禁止确认；没有增加明文生产后备或把该结果当作 Windows 审计／DPAPI 资格。

## 复验

```sh
cargo test --locked --offline --manifest-path plugin_runtime/Cargo.toml \
  --features packages --test file_owner --test managed_file_io
cargo test --locked --offline --manifest-path workbench_host/Cargo.toml \
  --lib io_tasks::file::tests
python tool/plugin_transport_baseline.py verify
```

文件专项读取仓库保留的三语言 Wasm，测试内建立 FileRead 声明包；旧 HTTP 原包没有被增权或改写。原生接线详见 [owner 报告](../reports/plugin-file-owner-2026-09-26.md)，最新私有协议、Dart 验证与限制见 [协议报告](../reports/plugin-file-wire-2026-09-26.md)。


## 私有文件任务协议与 Dart（2026-09-26 后续）

`host.capnp` 追加 `fileStart / fileChunk / fileFinish / fileRead`，沿现有 ioStatus / ioPoll / ioCancel / ioRepair / ioAcknowledge 控制所有权；属于最外层调度消息，不能放进原 owner 的嵌套业务命令。宿主与 Dart 绑定及私有 digest 同步生成；guest IO 协议和冻结 Wasm 不变。请求／响应沿用 128 KiB 帧上限，单块最多 64 KiB。

`FileStart` 显式绑定随机非零 32 字节 submission、包 ID／摘要、注册表修订、声明 handler、可信宿主选中文件路径、捕获 ceiling 和最多 30 秒期限。一个宿主会话最多保留 512 个文件尝试身份，准入过程中消耗的身份不重用；任务忙时不能替换原身份。丢失启动回执只允许用 ioStatus 核对，不得自动再启动、重新打开路径或恢复旧引用。HTTP／文件状态使用当前共同的提交身份，避免文件任务带出陈旧 HTTP 身份。

`start_file` 仍接受已打开的句柄。新增 `start_selected_file`／`capture_selected_path` 仅用于可信私有 UI 适配：接受绝对、无 NUL、UTF-8 最多 4096 字节的路径，原 worker 在实时 FileRead／原实例／取消检查后才执行 open，再按普通文件捕获规则读取。错误只携带 OS 错误类别。路径授权来自可信 UI；本接口不能证明路径一定由系统选择器产生，也不提供选中时刻对象、symlink/junction 根约束或原子快照保证。对象身份从实际 open 起建立。同步 open 也可能阻塞；停止回执不能代替实际 join。

`fileRead` 每次返回一种结果：Pending、Captured(length + 原始字节 SHA-256)、Chunk(offset + bytes + eof)、Finished。不暴露内部文件 grant 引用。读取仍一次消费，错误响应不带部分结果；Rust 中间结果帧的文件字节在序列化后擦除，Dart 在复制为不可变自有模型后擦除其私有响应帧。这不是所有内存副本的全局擦除承诺。

`NativeFileTaskClient` 提供类型化入口并接入 RustWorkbench，复用原传输和调度隔离。请求写入前验证范围，保留 UInt64 为 BigInt；解码拒绝混合结果字段、畸形摘要、超大块和偏移溢出。调用端仍需消费 Captured 后再请求块，对照期望 offset／length／hash 整理最终内容；此客户端不自动循环或重试。

跨层验证发现旧文件 SHA-256 曾误用面向 schema 的文本规范化函数。现在文件内容摘要与文件引用的二进制熵 seed 均使用原始字节 SHA-256；回归覆盖非法 UTF-8、CRLF 和会被有损文本转换合并的两组 secret。引用只存活于原进程，不涉及持久引用迁移。

后续已新增 `FileTaskSession` 与原生 `FileTaskManager` 源码接线，见 [会话报告](../reports/plugin-file-session-2026-09-26.md)。会话按 Captured → 精确 offset 分块 → 最终 SHA-256 → Finished 消费，最多保留 4096 字节预览及五条摘要历史，不重放一次领取。已知 task／submission 不允许替换；丢启动可只读恢复，丢命令或消费保持 Unknown；取消使迟到字节失效，修复／确认仍以实际退出为门槛。状态按 backend 保留，离开页面不重启原任务。

原生能力声明通过后才显示系统路径选择器；只列启用、可用且已声明／批准 FileRead 的插件，提交绑定原目录修订。HTTP 面板在文件尝试未解决时暂停自己的领取和控制。下一步是完整 Flutter 分析／widget 与 Windows 选中、取消、丢回执、修复、实际退出验收，以及完整本地化。Linux 测试 Store 仍不能代表生产受保护存储。此前 Flutter 启动自动审批因间接云元数据访问拒绝；本轮未重试该路径，独立 Dart 验证不记为 Flutter 窗口验收。


## Windows 合并验收与复现（2026-09-27）

九语界面、面板状态、Windows 原生服务 codec 及独立 Dart→宿主文件捕获验收见 [合并记录](../reports/drive-merge-acceptance-2026-09-27.md)。下列脚本从仓库固定的 Rust IO Wasm 临时构建 FileRead 专项包，使用全新临时库；不会安装到用户库。先构建当前 Windows 宿主并准备兼容的内置工作台包，再执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tool/verify_file_task_windows.ps1 -WorkbenchPackage C:\path\to\workbench.morrowplugin -HostExecutable build/workbench-host/release/morrow-workbench-host.exe
```

该验收包括捕获后删除源文件、分块读取与 SHA-256、Finished/退出/确认及撤权后的拒绝。系统文件选择器由 widget 注入选择结果验证取消和状态保持；这不等于原生对话框人工操作验收，也不补齐目录/写入权限能力。


## 生命周期复验补充（2026-09-27）

[本轮记录](../reports/file-task-lifecycle-2026-09-27.md) 补充了真实取消后 owner 回收、丢启动回执后只读恢复且不重发，以及文件/HTTP/服务面板共享任务时的控制互斥。修复完成回执与实际退出同次到达的中断误报；完整字节验证、收到 Finished 和维护成功仍是分开的条件。选择器人工交互及真实磁盘故障资格保持开放。


## 2026-09-27 文件变更持久准备

2026-09-27 [文件变更原子准备](../reports/file-mutation-preparation-2026-09-27.md)：新增有界路径与独立变更计划契约、同事务 Prepared/原件/容量预留、开库与快照闭包、取消恢复；通用 IO 入口禁止绕过文件派发门槛。Windows 核心组合回归 119 项通过（含真实进程崩溃），详见报告。实际写入、内容暂存、现场句柄授权和 Unknown 核对尚未接通，完整文件 IO 与 SDK 冻结继续开放。

该接口只保存可信宿主提交的历史计划，不恢复授权、不打开目标、不触发写入。普通只读文件任务维持现有 owner/会话生命周期；准备、取消与之后的实际执行不得混为同一种成功回执。

## 2026-09-27 内容暂存与数据库 v22

2026-09-27 [文件内容持久暂存](../reports/file-content-staging-2026-09-27.md)：新增有界内容原件、v22 迁移、Prepared 阶段双授权暂存/读取、共享额度及取消留存；完整 Core 故障注入回归 685 项通过，0 失败。实际文件效果、执行就绪承诺和 SDK/界面接线尚未开放。

## 2026-09-27 v23 暂存回执

2026-09-27 [暂存回执与审计迁移](../reports/file-content-receipt-2026-09-27.md)：新增 v23 暂存回执、同事务 kind 6 审计事件、来源/序号校验与正反闭包，内容或回执单边丢失可检测；完整 Core 故障注入回归 698 项通过、0 失败，11 个子进程入口由父测试调用。实际文件效果仍关闭，未重新构建安装包。


## 2026-09-27 Windows 文件目标绑定

2026-09-27 [Windows 文件目标与准备接线](../reports/file-target-binding-2026-09-27.md)：保留可信选中文件的原句柄与实例租约，将计划/内容暂存接到实时授权；明确提交前拒绝与提交后交付拒绝。Windows 运行时组合 122 项通过、0 失败。实际创建/替换/删除、owner 队列与 SDK/UI 接线仍开放，未重新构建安装包。

目标身份只在保留原句柄的选择期有效，不是跨重启文件身份或执行许可。提交后交付拒绝必须读取原历史/回执；不能视作事务回滚。未来效果必须在原对象/受控目录项上完成条件操作，不允许释放句柄后按旧路径重开并继承当前授权。


## 2026-09-27 Windows 原句柄删除

2026-09-27 [Windows 原句柄删除](../reports/file-delete-execution-2026-09-27.md)：底层 FileDelete 已接通持久 Unknown claim、原句柄删除、Response＋Observed 原子保存及一次性选择；实际文件/硬链接/只读拒绝/并发门/真实崩溃已验证。Core 完整 706 项、runtime 组合 135 项通过，0 失败；最终定向复验另见报告。创建/条件替换、完整 Unknown 业务核对、owner 队列与公共 SDK/UI 尚未完成，未构建安装包。

已消费的选择和历史审批摘要不能恢复执行许可。原生关闭失败时不重新包装或重试数值句柄，保持 Unknown、保留资源计费并拒绝新目标准入至进程重启。结果记录和交付授权分离；错误回执也不允许自动重发。


## 2026-09-27 创建目标目录链与暂存

2026-09-27 [Windows 创建目标目录链](../reports/file-create-target-binding-2026-09-27.md)：原目录句柄逐段打开并拒绝重解析点、固定全部祖先、全链预算先行，Create 已接 Prepared/审计内容暂存；不触碰最终文件。Windows runtime 组合 145 项、Core 定向 28 项通过，0 失败。实际创建/不覆盖发布/Unknown 核对、条件替换与 SDK/UI 继续开放，未构建安装包。

当前底层选择保留可信根及所有相对父目录，每句柄一个原实例资源，配额整体预留后才访问 OS；打开后逐句柄验证并拒绝 reparse。真实目录/junction/改名约束、失活回收及重选隔离已验证。根的上级路径仍属于可信选择器边界。Create 的实际外部效果继续关闭，下一步补专用 Unknown claim、原内容/回执核验和不覆盖发布，再接原 owner 队列与公共 SDK。


## 2026-09-27 创建持久执行边界

2026-09-27 [文件创建持久执行边界](../reports/file-create-claim-2026-09-27.md)：Create 增加原计划/内容/回执齐备后的严格一次性 Unknown claim，历史拒绝证据双失和回执晚于派发；补堵通用文件 Response 写入与响应预留释放旁路。Core 完整 715 项、runtime 组合 145 项通过，0 失败。实际临时文件写入、不覆盖发布与 Create 观察尚未接通，完整 IO/SDK/UI 继续开放；未打包或推送。

底层Store专用claim只接受v23完整暂存与回执，内容缺失或响应容量已被释放时不派发；Unknown经重开仍只能核对、不得自动重放。文件Response将由专用观察事务原子保存，公开通用入口不再接受文件变更响应。实际Create执行需在确认claim后使用原目录链创建临时文件，并补写入/flush/不覆盖发布/关闭与崩溃核对。


## 2026-09-27 Windows 创建执行增量

2026-09-27 [Windows 文件创建执行](../reports/file-create-execution-2026-09-27.md)：接通 Unknown claim 后的原目录临时写入、同步、不覆盖发布和 Response＋Observed 原子保存；真实 Windows、名称碰撞、撤权及七处进程崩溃已验证。Core 全量 724、runtime 组合 158、最终创建专项 14 项通过，0 失败（分项重叠）。条件替换、完整 Unknown 核对、owner 队列与 SDK/UI 仍开放；未打包或推送。

创建必须在原目录链／租约内执行，temp 和正文预算在 OS 操作之前准入，Unknown 提交确认后才能创建临时项。结果保存与结果交付分离；崩溃残留禁止自动清理或重放。仅底层同步入口已验证，尚未进入原 owner 任务队列／三语言公开 SDK／Flutter 页面；接线不得在 UI 线程直接执行同步文件写入。后续先完成条件替换的原对象身份与发布语义，再统一变更任务和受控核对。


## 2026-09-27 条件替换平台边界与变更队列计划

Replace 的 Core 持久协议与 Windows 原型见 [条件替换验收](../reports/file-replace-boundary-2026-09-27.md)。Windows 当前明确返回 `UnsupportedConditionalReplacement`，不跨越 Unknown；不以放宽句柄共享、关闭重开、路径覆盖或原地截断来降级。此范围不同于已在 Windows 验证的底层 Create／Delete。

| 能力 | 当前底层状态 | 下一步接线 |
|---|---|---|
| 选中文件读取 | 已有原 owner 队列、私有协议与界面链路 | 继续保留真实退出／丢回执核对 |
| 不覆盖创建 | Windows 原目录链＋一次性执行＋原子观察，原 owner 队列与分块已验 | Workbench 原生任务已接；继续私有协议、SDK／UI |
| 选定文件删除 | Windows 原句柄＋一次性执行＋原子观察，原 owner 队列已验 | Workbench 原生任务已接；继续私有协议、SDK／UI |
| 条件替换 | Core 有持久协议；Windows 当前明确不支持 | 在 claim 前返回平台错误，不影响其他操作 |
| 目录列举 | 尚无完整公共执行链路 | 单独实现有界目录引用、分页与游标语义 |

操作取消／时钟适配及实际 owner ticket＋共享时钟接线现已完成，见下方最新队列记录。不得用 ticket 取消去伪造过期时间，也不能取消一个命令就撤销整个实例。创建的读内容、claim guard、分块写入、同步及发布前都需受同一操作取消控制；删除需在 claim 前／效果前检查。claim 前取消不产生外部效果，claim 后取消保留 Unknown；已完成效果仍保存历史并按当前权限限制交付。

后台适配应沿用 `FileClock::with` 的取时与核验串行约束，不能仅获取 `now` 后释放时钟锁，再在其他线程推进的旧时间上核验。工作台 `manager` 与可变 `runtime` 必须来自同一个 owner，采用受控分借用接口，不克隆另一个宿主或改成独立任务系统。所有命令仍在现有容量上限及一次领取语义下执行。

后续测试必须覆盖：队列饱和不丢预算，foreign worker 不执行，claim 前取消、写入中取消、效果后取消、丢回执只查历史、准备／暂存持久但执行未发生、停止后真实 join 才释放句柄，以及 Unsupported 不写 Unknown。公共三语言协议和 UI 在这些条件验收后接入。


## 2026-09-27 变更控制前置验收完成

2026-09-27 [文件变更取消与同 owner 接口](../reports/file-mutation-control-2026-09-27.md)：完成单操作取消、持锁取时／准入及原 Manager／Runtime 分借用；修复取消遮蔽已派发历史的问题。Windows runtime 相关回归 216 项通过、0 失败，Clippy 与实际宿主编译通过。下一步为原 owner 队列、分块正文及 SDK/UI 接线；条件替换仍明确不支持，未打包或推送。

`TargetControl` 已使选择、准备、暂存、执行与交付采用同操作取消语义，并保留旧同步接口。三层目录链的部分准入取消不会残留新增租约，跨实例即使取消仍先拒绝身份；派发之后新取消不抹去旧事实。`ManagedHostOwner` 新分借用默认关闭，工作台显式提供原 Manager／可变 Runtime。

**此控制前置阶段的下一项原 owner 命令接线，已由下方队列增量完成；SDK 尚未冻结。** 先接选择、准备、取消与历史查询的原 owner 身份，再以有界分块暂存接入 Create／Delete。测试需覆盖实际 ticket 与共享时钟锁顺序、队列满、foreign worker、进程退出与真实 join；本轮控制器测试不能替代这些证据。条件替换继续在 claim 前明确 Unsupported。


## 2026-09-27 原 owner 变更队列验收

2026-09-27 [文件变更原 owner 队列](../reports/file-mutation-owner-2026-09-27.md)：Windows 创建／删除接入原后台队列，补齐 60 KiB 分块、取消、一次领取、历史查询与资源释放；Core 先匹配请求再核验正文，终态查询避免重复正文核验。Core 全量 739、runtime 相关 228 项通过，0 失败；Clippy 与实际宿主编译检查通过。下一步接 Workbench 协议／任务模型、三语言 SDK 与 UI，持久取消／跨重启核对仍开放；条件替换继续明确不支持，未打包或推送。

`MutationSession` 仅对原 worker 有效；目标、Prepared、分块、持久提交、执行、查询及 release 均复用原 owner。单命令取消不等于持久计划取消，本地 release 不撤销计划。丢执行回执后只能核对历史，不能重发；跨重启不恢复原句柄授权。

接下来先定义 Workbench 任务状态、选择授权和一次领取边界，再扩充私有协议与 Dart 客户端；同时明确持久取消与分块重置的显式操作。三语言公共 SDK 必须另行完成版本契约和真实调用测试，不能把当前可信宿主 API 直接视作 guest 能力。


## 2026-09-27：持久计划取消接线

2026-09-27 [文件计划显式持久取消](../reports/file-plan-cancel-2026-09-27.md)：原 owner 队列新增 cancel_mutation_plan，精确 Prepared 可持久取消，保留证据并清除未提交缓冲；已派发结果不可改写，丢回执只读核对。runtime 组合 232 项通过，最后预算修正后专项 16 项通过，0 失败（分项重叠）；Clippy 与宿主编译检查通过。SDK／工作台界面接线、分块恢复和跨重启核对继续开放，未打包或推送。

当前已区分命令取消、持久计划取消和本地资源释放。持久取消保留历史原件，释放原事务预留并禁止再次执行；不会自动关闭仍保留的选择句柄，也不会改写 Unknown／Observed。下一步将这些独立结果接入工作台任务状态与 SDK，不把取消命令成功等同于线程已退出。


## 2026-09-27：Workbench 原生变更任务

2026-09-27 [Workbench 原生文件变更任务](../reports/workbench-mutation-task-2026-09-27.md)：创建／分块／执行／查询／持久取消接入原工作台任务，逐命令身份隔离迟到回执，保留选择元数据并区分待核对与终态；真实 Windows 受保护存储验证通过。Workbench lib 全量 147 项，最终状态修正后专项 4 项通过（重叠）；Clippy 仍有 12 条既有警告。下一步接私有协议、Dart 与 UI／guest SDK；未打包、推送。

逐命令 ID 绑定 TaskKey；Unknown 只允许查询／取消计划／释放，明确终态不再当成待核对。取消整任务仍须等待真实 join。选择元数据只在原会话恢复，不跨重启或赋予新权限。私有消息的重试去重和三语言 guest SDK 尚未接入，不能把原生函数直接视作公开协议。


## 2026-09-27：私有变更调度与生成绑定

2026-09-27 [文件变更私有协议与跨语言绑定](../reports/mutation-wire-2026-09-27.md)：追加启动／提交／状态／领取／命令取消，有界去重阻止重复派发，明确 Core 阶段与 OS 结果投影；Dart 绑定由标准工具生成。Workbench 全量 152、Dart 31 项通过，0 失败（Dart 1 项既有夹具跳过）；Clippy 仍有 12 条既有警告。下一步优先补 worker 退出后的重新授权历史／效果核对，再接类型化客户端与 UI；SDK 未冻结，未推送。

[协议规范](PLUGIN_MUTATION_WIRE.md) 固定了逐命令身份、失败码、一次领取、历史与 OS 效果投影。下一项恢复必须处理原 worker 已退出的路径；不能只增加运行中 Query 测试后宣称跨退出／跨重启 Unknown 闭环完成。


## 原工作线程退出后的独立核对（2026-09-27）

Windows Rust Workbench 提供 `start_mutation_reconciliation(StartOptions, RequestRecord)`：原任务实际 join、必要清理恢复及 ack 后，以当前包审批重新启动只读后台任务，沿用已归还的原内容库。它没有目标路径／现场会话，不能执行、暂存或取消原计划。完整原计划精确匹配后返回历史阶段，Observed 可携带真实 OS 结果；Unknown 保持不确定，缺失历史不代表没有执行。领取成功或失败后自动请求停止，仍须实际 join 才能 ack。

新任务状态 Reconcile=8、回复 Reconciled=10。此阶段仅 Rust 原生启动；后续私有协议与 Dart 客户端进展见下方增量，UI、原计划持久发现及跨进程恢复继续开放。测试证据与边界见 [核对报告](../reports/mutation-reconciliation-2026-09-27.md)。


## 核对协议与类型化客户端（2026-09-27）

2026-09-27 [文件变更核对协议与类型化客户端](../reports/mutation-client-2026-09-27.md)：只读核对已接入私有启动协议，Dart 类型化客户端覆盖变更命令与核对，RustWorkbench 原生接线及敏感发送缓冲清理完成。宿主全量 163、Dart 39、Flutter 受控传输 23 项通过，0 失败（Dart 1 项既有夹具跳过）；Dart 分析无问题，宿主 Clippy 保留 12 条既有警告。下一步为 Rust 侧计划构造、持久计划发现、可恢复会话／UI 与跨进程恢复；SDK 未冻结，未提交或推送。

核对启动不携带目标路径，只携带完整原计划与当前审批身份。类型化客户端每次调用只发送一次，不自动重试或重放效果；Core 计划／结果仍由 Rust 解析。共享 IO 生命周期负责停止、实际 join、repair 和 ack。客户端接线不能替代独立会话、持久计划恢复或界面验收，条件替换仍明确 Unsupported。


## 原 owner 计划构造（2026-09-27）

2026-09-27 [宿主文件变更计划构造](../reports/mutation-plan-builder-2026-09-27.md)：原 owner 从保留的选择构造规范草稿，私有协议及 Dart 类型化入口已接通；不写 Store、不执行文件效果，显式 Prepare 前允许修改。宿主全量 165、runtime 专项 23、Dart 组合 42、Flutter 受控传输 23 项通过（Dart 1 既有夹具跳过）；最终 Dart JSON 分析无诊断。下一步为持久计划发现、独立恢复会话／UI 与跨进程恢复；SDK 未冻结，未提交或推送。

现可通过 `submitBuildPlan` 提交操作 ID／内容长度／摘要，由 Rust 选择句柄补齐其余身份并返回 Core 原件。Planned 是可修改草稿，不能当作 Prepared、可执行许可或跨重启恢复记录。完整协议见 [变更调度规范](PLUGIN_MUTATION_WIRE.md)。


## 原计划持久发现与原生任务（2026-09-27）

2026-09-27 [持久原计划发现与原生任务](../reports/mutation-plan-discovery-2026-09-27.md)：Core 有界键集扫描、受保护原件限额解码及 Store 绑定游标已接入 runtime／Workbench 原生任务；发现与完整核对、文件执行隔离。Core 全量 744 通过／15 ignored，runtime 25、宿主全量 167 通过，0 失败；最终专项另见报告。私有发现协议、Dart／恢复 UI、跨进程闭环及预算耗尽续扫仍开放，SDK 未冻结，未提交或推送。

发现只交付原计划，不交付历史阶段／效果，也不证明暂存正文完整。每页最多 8 个候选，空页仍可能有后续；原游标错误或丢失回执后不能继续。重新授权的完整核对才检查记录与正文／结果闭包；重新选择不能恢复旧计划的执行权。当前只有原生入口，原私有协议在领取前拒绝发现页。
