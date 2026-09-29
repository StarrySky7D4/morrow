# M-02 下一切片：授权记录与跨宿主会话归属

2026-09-29。用户在原生会话首片限定签收后明确要求“继续”，三个既有会话已重新收到实施任务。此前“不自动下一批”不构成当前阻塞。

## 范围与接续基线

上一片联合 ready-handoff 为 `joint/m02-native-session-001/ready-handoff.json`，SHA256 `263be0729c12ff25ac68c5b37b864d0b4ec44eacf7ee66a80cc6f1a4df2e8418`。其 10 案/64 帧/322 语义检查只覆盖原生控制首片，完整 M-02 仍 partial，G0/P-02/J-00 未通过，产品图 0/2、84 产品场景 not_run。

这次实现最小流程：**宿主维护授权记录 → 固定启动身份的一次性准入 → 多宿主对同一会话位置的独占 → 撤权/关闭/不确定退出后保留正确归属**。同时用真实上一会话帧验证历史重放拒绝。

仍仅提供 read-own-session，不扩大为成功网络、内容持久化或任意命令执行。完整产品审批 UI、正式 CLI 认证和强隔离按真实进度单列，不能因新增测试批准入口就标为完成。

## 分工

| 会话 | 本批写入范围 | 任务 |
|---|---|---|
| 宿主 `01a0e7af-d5c9-7221-a7af-6c89b2ee2f9b` | 实际开发 checkout 的新版本实现及 host 新批次报告 | 复用审批/生命周期/存储能力，宿主权威批准记录、一次性领取与撤销、真实跨宿主 owner；先交最小启动/操作接口 |
| 插件 `01a0e7b0-2b5c-7e61-a9cb-6a296054bfb1` | 独立 morrow-codex 新批次 | 优先复用普通 client001；新增分离的历史帧重放 peer/必要客户端适配，不自行授予权限 |
| 联合 `01a0e7b0-79f4-7193-a84d-de58af037cd0` | 仅 joint/m02-admission-owner-002/ | 方案预审、矩阵、ready 后双宿主与历史帧独立运行，证据分层与门槛判定 |

主协调负责方案/依赖决策、结果审阅与阶段记录，不创建重复会话。

## 关键要求

1. 批准绑定固定产物、配置、协议、身份、能力、实例/授权代次、会话位置和必要操作身份。客户端自报身份或 approved 不能授权；重复领取不能启动第二实例。
2. 首次批准即确定单调期限，延迟启动、重试或重连不能续期；输入变更、撤销、过期必须拒绝。
3. 两个宿主不能各自用私有 HashMap 就声称互斥。必须在声明的本地权限/资料库或 profile 范围内使用真实独占机制；文件存在、PID 相同或可抢占 TTL 不是充分身份/锁证据。
4. ClosingUnconfirmed 保留归属。宿主崩溃不等于子进程退出；不能在锁随宿主消失后盲目启动替代客户端。无法核实则保持拒绝并暴露可核对状态，不盲杀用户进程或清理未知锁。
5. 历史重放使用真实第一次会话接受过的原始帧，经第二个真实会话重放；修改当前 epoch 不是历史重放。角色互换只有实际合同支持才测试，不自造角色协议。
6. 只追踪本批创建的已知 PID/句柄，不扫描全机。异常 peer 有有限 watchdog 且无窗口；主机关停与测试清理分别记录，不把测试清理算产品恢复。

## 验收顺序

- 宿主先交精确可行的实现与最小接口，插件并行准备原帧捕获和分离 peer，联合建立矩阵并提前审查 owner/权限方案。
- 固定新候选后独立运行两个宿主竞争、存活/ClosingUnconfirmed 占位、完整释放后的合法新实例、首次批准后延迟启动、旧授权撤销/重复领取及历史握手重放。
- 宿主崩溃测试仅终止本批创建的宿主，随后独立观察子进程。若只做到安全拒绝，明确不宣称完整自动恢复。
- 真实审批入口与 fixture 授权分开；复用现有权威而非新建与产品无关的第二套批准服务。若产品审批 UI 尚未完成，继续列缺口。
- 独立检查新输入在运行前后保持一致；不机械重跑旧 10 案及 codec 全套，不把新诊断计入原 84 场景通过数。

## 版本与执行边界

旧 kit/源码副本/工具/回执保持原字节；新版本、工具与证据单独目录。现有 Cap'n Proto 协议足够就直接消费；确需扩展由宿主在新版本提供唯一 Schema/生成绑定/摘要，旧 003 和 Capnp kit001 不改，不再引入自定义 JSON 运行期协议。

实际开发路径仍为 `morrow/build/io-safety-refactor`；项目根的 ArkTsUI checkout 不修改。所有测试限隔离临时目录，不读取个人密钥、MCP、真实内容库，不公网/付费调用，不使用 SubagentBridge，不自动提交/推送/发布/关机。

启动状态：三会话已接续，本批尚无实现或验收通过结论。

## 方案审查与实施对齐

主协调读取了现有 `core/schemas/service_authority.proto`、`core/src/store/service_authority.rs`、`core/src/store/service_authority_lock.rs` 与 `core/src/plugin_package/registry/native.rs`。现有 Authentication/Publication 记录及 kind=1/2 表约束是网络服务专用，不可伪装成原生启动许可；锁、Store 身份、事务和撤销机制可按其原始边界复用。

宿主已确认以下实施方案，当前为待代码/运行核验的设计，并非验收结果：

- 新宿主目录 `native_session_owner_002`，旧 `native_session` 保持冻结；继续 Capnp kit001/major2 revision1，普通 client001 可复用。
- 新原生领域持久记录使用 `native_admission.proto`、生成 Prost 绑定及带独立 magic 的 Protobuf+LZ4 封装，不复用网络服务记录类型，不以 JSON 持久化权威状态。
- 同一可信本地 profile 绑定一个 slot，使用 Core 持久库身份及同一 LOCALAPPDATA 锁命名域，直接复用 `Store::pin_service_authority`。本片是 profile 范围排他，不是任意 profile 或全机全局独占。
- `init` 仅初始化空 profile；`serve` 只产生提案，不自动批准或启动。独立可信控制入口提供 approve/inspect/claim/revoke/stop；guest 不持有该入口。
- approve 创建同一不可反序列化 LiveApproval 并开始首次单调期限；claim 不重建 Admission、不续期，只有原存活 issuer 可使用批准记录关联的 liveauthority。
- claim 拟先取得真实 owner pin，再以 IMMEDIATE 事务把批准标为 Consumed 并写入 LaunchPending；确认提交后才启动进程，随后登记实际身份。持久未 Released 记录在宿主崩溃后仍拒绝接替，不提供 clear/steal/TTL 抢占接口。

已交实现方与联合重点复核：原生 ledger 的具体存储位置和唯一权威边界；批准/撤权/claim 的原子顺序及未知提交；profile 初始化竞争、别名与稳定锁身份；plugin/role/operation 等额外绑定必须进入权威批准身份而非仅日志。Core 与新 ledger 不得维护两份可独立推进的同义批准状态。

首批暂只提供宿主嵌入的受控批准 API，不冒充已接生产 UI 或正式 CLI 认证；崩溃后拒绝也不等于完整恢复。上述未完成项仍是后续必须推进的原计划要求，不能因保守拒绝测试通过而从目标删除。

## 最终限定签收

主协调已读取联合正式报告，并现场核对以下三个文件摘要一致：

- `joint/m02-admission-owner-002/ready-handoff.json`：`820f1c7065bc276f472f07dbf0a165eb379cdde82ee1276efea6cdca5e774ad7`。
- `joint/m02-admission-owner-002/review-2026-09-29.md`：`a39179f6a10def9bcb948d3dfee58a6a0abd48dcefe0f89249ecf17bc430ffd4`。
- `joint/m02-admission-owner-002/evidence-manifest.json`：`6530368ef3ddb89d81165873e2ac85ee4bdaa2df3a3465ef4a3d5ff68c83884b`。

联合独立完成 11 组实际场景、501 项语义检查、59 帧重新解码、20 条批准配置摘要重算。接受范围包含 17 个实际 serve 宿主、10 个直接子进程和 1 个 holder 的已知 PID 句柄证据。2,233 项运行输入前后不变，2,033 项旧基线保持不变；联合未独立重编宿主或客户端，主协调也没有重复运行这些场景。

已覆盖一次性批准与首次期限、固定上下文篡改拒绝、双宿主竞争、外部撤权应用、真实历史 Hello/Query 重放、ClosingUnconfirmed 占位、完整释放后的新批准、宿主崩溃拒绝接替、启动前产物变化、profile 复制拒绝，以及旧 issuer 批准不能恢复领取权。冻结前 Revoked 状态读取与完整批准比较问题已修复并复核。

首次崩溃审查把 OS 进程退出与诊断管道 EOF 合并等待而失败，原证据保留并不计通过；新脚本只补未完成场景。补跑确认旧 child 在 host 退出后仍活，新 host 拒绝接替；旧 child 自行退出后也不自动清空持久 owner。这是安全拒绝，不是完成崩溃恢复。诊断管道 EOF 晚于 host 退出的具体句柄来源尚未独立取证，继承句柄封闭性仍缺资格。

M-02 整体仍 partial；生产审批/CLI、安装身份、防换包、完整恢复、精细并发窗口、在途部分写入与控制饱和、OS 隔离和其他平台仍未完成。完整 G0/P-02/J-00 未通过，G1 未通过，产品图 0/2，84 产品场景 not_run。本片成功限 read-own-session，不代表已成功执行网络或内容写入。

用户已要求持续推进；下一步按 `m03-stream-001-plan-2026-09-29.md` 对齐真实增量网络接口。上述缺口继续列入剩余范围，旧候选冻结，不自动提交、推送或发布。
