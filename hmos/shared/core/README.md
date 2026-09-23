# Morrow core — test.37 存储进度

这是独立于 Flutter 的可信 Rust 核心，当前应用开发版本为 `0.1.9-test.37+42`；核心 crate 的 Cargo 版本仍为 `0.1.9-test.22`。Windows 重构工作台已通过 Rust 宿主使用本核心；Web 实验适配复用同一 Store。Windows test.19 开发版通过审计 Session 接入系统保护密钥与分批封存；构建与测试不读取用户资料。以下旧阶段章节保留历史范围，当前封存与数据库格式以本节为准。

## test.35 内容与纯任务证据同事务

test.35 引入的内容数据库格式为 **7**；当前格式8的物理共享表示见下节。`task_evidence` 保存以原始 Protobuf SHA-256 标识的证据容器；`operation_evidence` 保存操作内有序引用。可信宿主的 `create_content_with_evidence`／`edit_content_with_evidence` 接口仍须通过真实连接的内容权限及最后一次授权检查。Store 在同一事务写入内容、原始证据、引用、可恢复回执的原始 Commit、永久事件索引及待封存队列；没有提交后补写的旁路。旧公开内容接口继续传空集合。

有证据时 Commit 使用 schema 2，无证据时保留 schema 1。每操作最多 16 份，按每一引用对应的原始 PB 与容器之和累计不超过 64 MiB；重复引用也计入。按摘要去重仍核对原始 PB，保留首次容器。幂等重试必须同时匹配原命令与有序摘要，已存原件必须完整；不会用新捕获替换历史证据。授权失效会回滚本次全部写入，提交结果不明仍需查询核对。

`Store::operation_evidence(card_id, operation_id)` 仅供可信宿主读取历史资料，校验操作、卡片及内容类型归属；不存在、其他卡片或非内容操作返回 NotFound，合法无证据操作返回空集合。完整性检查核对每条 Commit 引用、原件及所有额外／缺失关联，拒绝孤立原件。快照在固定源事务内先完成这些校验；证据随库一起保存。

4／5／6 按既有阶段迁移至 7；6→7 先验证旧库，再将建表、版本更新和新格式校验放入单一事务。旧格式不能携带未受支持的证据引用或孤立新表来绕过迁移验证。`evidence-migration-before-commit`／`after-commit` 与内容事务 `after-task-evidence` 为显式测试故障点。

签名封存覆盖包含摘要的原始 Commit，从而关联证据原件；它不证明插件输出正确或最终正文必然由该输出推导。test.35 时默认 UI 尚未自动捕获；test.36 已接入默认工作台 create／apply，见下节。设置分页、版本化宿主投影重建、依赖图证据、全库证据配额和 GC 仍待实现。见[关联设计](../docs/PLUGIN_COMMITTED_EVIDENCE.md)与[test.35 验证](../reports/test.35-committed-evidence.md)。

## test.36 原操作查询与工作台重试

`Store::operation_commit(card_id, operation_id)` 在固定读取事务中按卡片和内容操作类型筛选原始提交，SQL 先检查载荷长度，再解码校验 ID 与全部证据引用。返回 `Option<(Commit, Receipt)>`；不存在、其他卡片或非内容操作为 None，超限／损坏／引用不一致为错误。它只供可信宿主恢复历史命令，不新增 guest 授权，内容库格式仍为 7。

默认 Windows 工作台 create／apply 已从实际 `Pool::record_transform` 获得原件后调用 with_evidence 内容接口。已提交重试核对存储原请求中的用户意图和预期基准修订，复用原 command／evidence，重新取得当前对象 grant 并让核心返回原回执。返回该次操作的历史 Record，不改写当前最新内容，也不刷新撤销期限；新操作仍做修订 CAS。CreateCard 原命令包含完整历史卡；编辑命令保存正文／标题等修改字段，不能据此宣称所有类型的完整历史 Card 均可重建。

这些实现的 test.36 相关回归与实际Windows自检已通过，见[本轮记录](../reports/test.36-workbench-evidence.md)。设置多页、capture→create 因果关联、查询证据、通用宿主投影、依赖图与全局证据配额／GC 不在本次完成范围。

## test.18 日志身份绑定

test.18 引入的数据库格式 6 在独立的 `audit_identity` 表持久绑定日志身份和公钥。即使还没有封存事件，已绑定库也要求宿主提供匹配的外部可信身份；数据库内的公开绑定记录不能自行建立信任。`audit_binding_status` 只读检查状态，不创建或迁移数据库。Windows 显式初始化与恢复入口见 [审计工具](../audit/README.md)；test.19 默认 Windows 工作台已调用，其他平台后端仍待接入。

## test.16 核心原子封存

签名契约和验证实现统一位于 `schemas/audit.proto` 与 `src/audit.rs`；独立 `audit/` 工具复用该实现。只有持有宿主独立固定日志身份／公钥的 Store 才能调用封存或打开已封存库，不能用段内公钥建立信任。

- `Store::open_audited` / `open_opfs_audited` 接收可信日志身份，`seal_pending` 验证签名、链连续性和每条原始事件与本库对应关系，将签名原件、永久事件关联、待封存队列确认放在同一事务。返回 true 表示本次封存，false 表示已封存的幂等重试。
- 原始 operations、永久 operation_events 顺序、内容和历史附件关系不会被队列确认删除。容量只计算待封存事件；归档存储与历史保留配额仍需实现。
- 格式 4／5／6／7 经校验后按阶段原子事务迁移至当前格式 8；已签名旧库要求外部可信身份。损坏旧库拒绝迁移；格式 3 及更旧库继续拒绝。格式 8 不应交给旧核心写入。没有自动接入 test.1 旧资料迁移。
- `sealed_segment` 返回核心保留的签名原件。`open_read_only_audited` 供独立验证器读取格式 5／6／7／8，不创建、不迁移数据库。
- 核心验签与索引核对覆盖本地已封存状态，不表示已独立见证。生产签名密钥管理、自动封存调度、完整宿主事实与重放继续推进。test.16 当时的 Windows 应用尚未启用队列确认；test.19 起的默认接入情况见本文开头及审计文档。


## 已实现

- `schemas/content.proto`：Card、BlobRef、Relation、Preview、Workspace、ViewPlacement、Draft 的候选版本化契约。业务类型与正文不由核心硬编码；身份为不含路径分隔符／冒号的非空 UTF-8 字符串，兼容旧条目 ID。
- `CardRecord` 保留完整 Protobuf 记录，`CardSummary` 仅为只读投影，禁止拿投影覆盖正式记录。已知标题修改保留顶层与嵌套未知字段、未知枚举和未知 oneof 载荷。未经编辑的记录返回原字节；编辑后的编码与原始解码字节分别保留。`original_bytes` 不是历史证据存储服务。
- `schemas/runtime.capnp`：内部重命名请求与摘要布局。请求包含操作 ID、目标卡片 ID 和 UInt64 预期修订；纯编辑提案检查目标和修订并递增修订。编解码本身不执行命令；Store 路径已提供操作 ID 去重。调用者身份不由消息自报，重命名写入通过 HostPolicy.commit_rename 完成授权。
- Protobuf＋LZ4 容器、解压前限额和 SHA-256 损坏检测；不经过 JSON。SHA-256 不提供签名、来源认证或审计封存。
- `morrow-core-check self-check` 运行内存中的协议→纯编辑→容器往返；`verify <file>` 只读验证指定容器。诊断文本不是正式记录。

Workspace／ViewPlacement／Draft 在 test.10 具有实际事务和独立修订；既有卡片草稿可单独保存，布局只引用内容。BlobRef 保持可移植引用；test.5 原生 Store 在提交中核对暂存原字节、建立当前／历史引用，并提供受保护回收。卡片预览无需插件即可读取；原始附件可从实验 CLI 导出，Flutter 展示与导出入口尚未接入。

## test.10 工作区、视图与草稿事务

records::Record 使用同一预编译内容契约和有界反射解码，未修改时保留原始 Protobuf 字节，已知字段更新保留未知字段。Workspace／ViewPlacement／Draft 各自新增 UInt64 revision；各类容器分别使用 MORROWW1、MORROWV1、MORROWD1，避免跨类型解释。

- 工作区可创建及改名。视图位置可创建，保存顺序、折叠与宽度；工作区和卡片必须存在。多处引用不复制卡片，也不修改卡片修订。
- 草稿关联已有卡片、基准修订、内容类型／格式及不透明正文。保存递增草稿修订，保留原基准，不覆盖正式卡片；正式卡片变更后仍可保存旧基准的草稿。合并、发布、附件保留及无正式卡片的新建草稿尚待实现。
- records::proto::Patch 仅允许对应类型的字段，拒绝空更新、错误类型字段、超限布局及修订溢出。禁止拿只读字段投影覆盖完整记录。
- Store 内同一个 IMMEDIATE 事务提交记录、操作与 outbox；操作 ID 在卡片和三类记录之间共用命名空间，事件预算也共用。同一完整命令重试返回首次结果，不同命令复用 ID 拒绝。查询先按对象类型和 ID 筛选；卡片范围查询不会泄漏同名工作区结果。
- record_transaction.proto 保存固定命令与结果原字节／摘要，容器 MORROWR1；与既有卡片事件共同进入有界 pending 队列。启动检查所有记录、引用、操作／事件对应及最新结果原字节；没有第二条持久化权威。
- test.10 当时的实验库格式 4，拒绝格式 3；当时运行期协议 6 更新了内容契约摘要，但三类记录当前仅走可信本地 API／CLI 和 Web 实验管理入口，尚不是通用插件运行期命令。Web 使用 morrow-test10 命名空间。

CLI 新增 workspace-local、placement-local、layout-local、draft-local、draft-save-local、record-query-local、record-export-local。workspace-local 的预期修订 0 表示创建，查询／导出 kind 为 1 工作区、2 视图位置、3 草稿。草稿正文通过文件按原字节输入；导出在临时文件完整写入并同步后发布，拒绝覆盖目标。完整参数运行 morrow-core-store 查看。删除／跨工作区移动、列表索引、正式授权分派及 UI 接入继续推进。

## test.9 附件分块读取

ReadAttachment 是独立能力，精确限定 card_id 与 attachment_id。读取摘要、重命名或同卡片的另一个附件授权均不能代替；相同原件被多个逻辑附件引用也不共享授权。原生 C／Dart 与 BrowserStore 提供独立的附件授权和撤权管理入口，业务消息不携带权限句柄。

- 请求绑定预期修订、偏移和 1–32768 字节长度；单次返回最多 32 KiB，整个协议消息仍限 64 KiB。查询前与交付前复核授权，每包在同一 SQLite 读事务内解析当前卡片及附件引用。修订变化返回 RevisionConflict；不自动切换到新修订。
- 每个 64 KiB 存储块的摘要以预编译 Chunk Protobuf＋LZ4 写入 blob_chunks，和原始字节一起暂存提交、去重及回收。范围读取最多核验覆盖的两个存储块，避免每包扫描整件文件；原件保持原字节。启动、发布和完整导出仍执行整件校验。
- 每包包含身份、修订、偏移、总长度、整件 SHA-256 和字节。Dart 的 AttachmentTransferVerifier 只维护递增摘要与进度；拒绝混合修订、乱序和元数据变化。调用者须先写私有临时目标，finish 成功后才能发布，失败后不应暴露半成品。
- 撤权阻止后续授权读取，不能收回已经交付的字节。分块和整件摘要用于损坏检测，不能替代签名或审计封存。没有跨包持久读会话，也未实现异步调度或插件隔离。
- test.9 当时以实验库格式 3 增加分块索引，拒绝格式 2；test.10 当前格式和命名空间见上节。

实际验证见 [test.9 记录](../reports/0.1.9-test.9-refactor.md)。

## test.8 原生宿主与结果查询

新增 `bridge/native.rs` 和 C 头文件中的 morrow_host_* 接口。可信原生宿主只能显式打开已经存在的绝对 UTF-8 数据库路径，不导出创建、导入或迁移 API。同一规范路径在本进程拒绝第二个宿主；上限 8 个宿主、每宿主 128 个连接。宿主／连接 ID 不复用，关闭宿主使其全部连接失效。句柄是可信进程内控制对象，不是可交给插件自选的权限凭据。

- 原生时间来自 Rust Instant；管理入口只接受相对 TTL，请求不能提交绝对时间或调用者身份。
- dispatch 在执行前预留一个有界响应槽位，缓冲区满时不执行命令。发生响应丢失仍须按稳定操作 ID 查询，不能因为调用失败就断言回滚。
- QueryOperation 独立于 Rename／ReadSummary 授权。数据库 SQL 在读取结果载荷前按 card_id 与 operation_id 一起筛选，并在交付前再次检查授权。
- 查询响应区分 locallyCommitted 与 absentSnapshot，后者只是本次卡片范围内的快照，不是没有在途提交的证明。回执中的卡片、操作 ID、修订与摘要均经验证。
- 目前同步调用在持锁期间完成存储读写；尚未形成后台 isolate／线程调度、异步取消和背压的完整产品适配。多进程宿主之间的权限同步也未完成。

实际 Dart DLL 探针见 [客户端说明](../packages/morrow_core_client/README.md)，测试记录见 [test.8](../reports/0.1.9-test.8-refactor.md)。

## test.7 宿主分派与摘要读取

`dispatch::HostRuntime` 持有唯一 Store 和 HostPolicy；可信传输持有由宿主创建的 Connection，请求不能自报实例、授权、时钟或数据库路径。每次请求先复制到有界缓冲区，再解码、授权并执行。连接失效后不能复用；另一宿主或连接不能借用其能力。第一方本地初始化入口 `store_local[_mut]` 只供宿主管理，不可映射为插件命令。

- Rename 与 ReadSummary 是独立的对象级授权；操作结果查询在 test.8 加入独立授权；test.9 附件读取按卡片与附件单独授权；创建、导入、附件写入和搜索尚未扩展成完整的授权命令。
- 读取在查询前与交付投影前检查身份、能力、对象及期限；过期、撤权、排空或旧实例均拒绝。无读取权限时，存在与不存在的目标返回同类 Denied，不先查询内容。授权读取返回基础摘要，不返回正文或附件名。
- 响应为带请求关联 ID、契约摘要的 Cap’n Proto v6：提交回执、摘要或稳定错误枚举。失败响应不夹带私有标题、路径或内部异常文本；CommitUnknown 单独表达，不当成确定未提交。
- 运行期单消息最多 64 KiB；摘要预览文本最多 16 KiB，超限返回 Limit，不把裁剪后的摘要保存回记录。这里的授权是同步独占宿主的线性化检查，数据交付后不能追溯收回已复制的字节。
- Rust 原生集成和 Web Worker 接入使用同一分派器；test.8 原生 Dart 已经实际 DLL 接入分派；原有 morrow_buffer_process／Dart-Wasm 探针仍保留协议往返用途。完整跨进程隔离、授权配置持久化、审计和插件执行待后续阶段。

## test.5 边界与宿主状态

`bridge.rs` 与 [C 头文件](include/morrow_core.h) 提供同一原生／Wasm 缓冲区 ABI。旧 morrow_buffer_process 仍只验证并往返协议；新增的原生 morrow_host_* 管理接口与 morrow_host_dispatch 则接入 HostRuntime。运行期协议已升到 6，test.2–test.9 的旧协议请求被明确拒绝；Protobuf 卡片容器仍为 1。Dart 原生与 Chrome Dart/Wasm 实测见 [客户端说明](../packages/morrow_core_client/README.md)。test.6 通过独立 web.dart／Rust 编解码入口解决普通 Dart JavaScript 的精确整数接入；旧生成绑定主入口仍受该限制。

`lifecycle.rs` 提供纯内存 HostPolicy：宿主分配不可自报的实例身份与代次、对象级重命名授权和到期时刻；接单固定请求，完成前再次核对身份／授权／期限。正常停止不接新任务、等待已有任务，达到排空期限则撤权；安全停止先撤权后取消。宿主须用单调时钟调用 expire_drains，迟到完成也会检查期限。停止／退休后旧实例、授权与任务不可复用。

CommitState 区分未提交、结果待核对、本地已提交、封存、见证和查证未提交。超时不会改成未提交，撤权也不回滚已提交内容。状态枚举本身没有封存证明、外部见证或生产授权 UI。test.5 新增的 HostPolicy.commit_rename 在独占宿主期间，将固定请求、提交前的新时钟授权核对与实际 SQLite 事务串联；依赖、共享对象、运行后端与取消外部效果仍待 M3。

## test.5 原生事务存储

`store.rs` 使用固定 rusqlite 0.40.2／bundled SQLite，WAL＋synchronous=FULL＋BEGIN IMMEDIATE；写锁冲突立即报告存储错误，由宿主查询或重试。卡片、操作结果及待封存事件在同一事务中写入。SQLite 页／索引由引擎管理，业务内容保持 Protobuf＋LZ4。[SQLite 同步设置](https://sqlite.org/pragma.html#pragma_synchronous)、[事务行为](https://docs.rs/rusqlite/0.40.2/rusqlite/enum.TransactionBehavior.html)。

- 新 `transaction.proto` 和 `MORROWT1` 容器保存固定 Protobuf 命令、摘要、结果修订及稳定事件 ID；运行期 Cap’n Proto 字节不作为业务持久化格式。操作 ID／事件 ID 在单数据库内唯一，宿主负责跨调用方作用域分配。
- 同操作 ID、同完整命令返回首次结果，即使内容已被后续操作修改；相同 ID 携带不同命令则拒绝。返回原始修订，不用当前卡片冒充首次结果。
- 提交成功才返回 LocallyCommitted。COMMIT 返回错误时报告 CommitUnknown；查询 Absent 只是当前快照，不能断言并发中的操作永远不会提交。当前失败操作不另建耐久“未执行”证明。
- 重命名持有宿主的独占借用，事务内及 COMMIT 前重新核对授权；最后一次核对是授权线性化点。排空期限或授权在准备过程中到期会回滚。最终磁盘同步期间到期不追溯撤销已线性化的提交。多个独立进程／宿主的权限同步仍待实现。
- 默认待封存队列上限 1024 条／64 MiB；先检查容量再写入，事件插入失败同时回滚内容与操作。M4 封存机制建立前没有清空／确认队列入口，达到上限会拒绝新操作。
- 打开数据库检查归属、版本、SQLite 完整性、卡片摘要与操作／事件对应关系。未来版本和无关数据库被拒绝，`open_existing` 不创建缺失文件。这是损坏检测，不是防御已控制宿主和数据库的攻击者。
- 当前支持带已暂存附件的卡片创建、标题修改、附件列表替换、结果查询和原件导出；缺失或损坏的载荷被拒绝。关系、草稿发布／附件保留与完整工作区生命周期待实现。数据库格式已升为 4，不自动打开或迁移 test.4–test.9 的格式 1／2／3；需要新建实验数据库，不切换旧 Flutter 保存路径。

实验 CLI：

```powershell
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.10 --bin morrow-core-store -- init build/example.db
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.10 --bin morrow-core-store -- create-local build/example.db operation-1 card-1 示例
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.10 --bin morrow-core-store -- rename-local build/example.db operation-2 card-1 1 新标题
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.10 --bin morrow-core-store -- query build/example.db operation-2
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.10 --bin morrow-core-store -- check build/example.db
```

CLI 的 local 操作是可信本地操作者入口，不能直接暴露给插件；lookup/card 同样需要传输层的读取授权。`fault-injection` 只用于子进程恢复测试，默认构建不响应故障注入环境变量。进程直接退出覆盖提交前后边界，不等于断电或全平台验收。

## test.5 附件与回收

原始附件按 64 KiB 块写入 SQLite BLOB，字节不转码、不压缩；元数据、退休时刻、保留记录和宿主时钟使用 attachment.proto＋LZ4。暂存、校验与完成标记是同一事务，半成品不会在中断后变成可引用对象。长度变化、读取失败、摘要不符或 SQLite 容量耗尽都会回滚。相同内容重新暂存会核对已有原件并返回既有 ID。

卡片／当前附件引用／操作结果／事件／事件附件保留在同一内容事务提交。发布前在事务内验证原始字节；去重操作不会重复创建引用。移除当前附件仍保留历史事件所需原件。修改已有附件已知字段或附件元数据退休时刻时，未知字段继续保留。

- stage_blob 返回的就绪对象持续保留，列出它不等于可以回收；显式 retire_blob_local 或最后一个可释放保留记录的释放，才开始退休等待。
- 回收至少等待 60 秒，并在同一写事务重新检查当前卡片、历史事件、撤销／快照／证据保留；每批最多 16 个。证据及历史事件保留当前没有普通释放入口，等待 M4 的封存规则。
- Undo／Snapshot／Evidence 是保留原语，不表示完整撤销、快照或证据系统已经实现。宿主维护持久时间下界，时钟倒退拒绝回收；等待时间不能代替引用检查。
- 导出使用数据库读快照；并发回收后，已开始的读取仍能完成。CLI 先写同目录临时文件，摘要核对并同步后才发布目标，拒绝覆盖已有文件。通用 Write 接口的调用者也必须等成功后才发布输出。
- 当前上限：单件 200 MiB、数据库原始附件总量 2 GiB／2048 个。数据库和 WAL 的实际磁盘用量可能更高。这些是原型资源上限，不是性能结论。
- 打开时完整扫描原始附件并检查摘要、外键、当前与历史引用；未完成大规模启动性能优化。SQLite 原生路径已验证；test.6 增加命名 OPFS VFS，共用 Store 事务，采用独占 DELETE journal／FULL 配置。Web 当前只开放 4 MiB 的附件复制适配，详情见 [Web 核心](../core-web/README.md)。

CLI 新增 stage-file-local、list-blobs-local、create-attachment-local、clear-attachments-local、export-attachment-local、retire-blob-local 和 collect-retired-local。完整参数可运行 morrow-core-store 查看。所有 local 入口只供可信宿主／本地操作者，尚未接到插件的逐次授权与传输分派。

设计依据：[SQLite 增量 BLOB I/O](https://www.sqlite.org/c3ref/blob_open.html)、[WAL 隔离语义](https://www.sqlite.org/isolation.html)。实际并发与中断结论以 [test.5 记录](../reports/0.1.9-test.5-refactor.md) 的本机测试为准，不作为断电、原生 mmap 或插件隔离证据。

## 容器与资源边界

容器布局为：`MORROWC1`（8 字节）、容器版本 u16 LE、未压缩长度 u32 LE、压缩长度 u32 LE、原始 Protobuf SHA-256（32 字节）、LZ4 block。版本为 1；只承载 `morrow.content.v1.Card`，不能把其他消息塞入这个容器充当正式记录。后续记录种类会采用明确的类型分派和版本演进。

单记录最多 8 MiB，解压前验证长度和总输入大小；反射解码前按固定 descriptor 限制 8192 个字段、每个已知重复字段 1024 项和 16 层已知消息。未知 bytes 不递归解析；废弃的 proto2 group 不在此 proto3 契约支持范围内。内部请求最多 64 KiB，遍历预算 8192 words、嵌套限额 16，拒绝尾随消息、未知协议版本和不支持的操作。标题最多 16 KiB，身份最多 256 字节。这些是原型安全上限，尚未成为性能承诺。

## 生成与复现

依赖精确版本与 Cargo.lock 纳入版本控制。构建期生成 Rust 绑定和固定 Protobuf descriptor，运行期不接受外部 descriptor／schema 注入。测试演进契约位于 `tests/schemas/future.proto`，只被测试二进制引用。

本轮工具：Rust 1.95.0、Cap’n Proto compiler 1.4.0（须在 PATH 中）、capnp 0.24.1／capnpc 0.24.0、prost／prost-build 0.14.4、prost-reflect 0.16.5、lz4_flex 0.14.0、protoc-bin-vendored 3.2.0。后者提供构建主机的 protoc，不依赖全局 protoc。首次构建需要获取锁定依赖。

```powershell
pwsh -File tool/verify_core.ps1
# 安装目标后检查 Web 核心；此命令不会自动安装工具链：
rustup target add wasm32-unknown-unknown
pwsh -File tool/verify_core.ps1 -Web
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.10 --bin morrow-core-check -- verify path/to/card.morrow
```

当前原生 Dart FFI 与 Chrome Worker 已接入同一持久化分派，并提供受授权的附件分块读取；共享内存、审计与插件运行仍未实现，工作台保存路径未切换。具体协议与库仍需其他平台、性能和主应用接入验证，不因本轮通过而冻结全平台实现。

参考：[prost-reflect 未知字段 API](https://docs.rs/prost-reflect/0.16.5/prost_reflect/struct.DynamicMessage.html#method.unknown_fields)、[LZ4 有界解压 API](https://docs.rs/lz4_flex/0.14.0/lz4_flex/block/fn.decompress_into.html)。本轮兼容性结论以仓库中的演进测试为依据，不把普通 prost 生成类型直接作为无损编辑载体。


## 原生一致性快照（test.22）

`Store::snapshot_to` 固定源读取事务；test.35 起在该事务内先核对包含任务证据引用的完整性，再通过 SQLite backup API 有界复制到新的暂存文件，不直接复制正在运行的数据库文件。页面数量与大小检查调用方的字节上限；现有目标拒绝，锁竞争返回错误，不无限重试。失败的暂存文件不得发布。Windows 审计适配随后核验快照完整性并封装为 PB＋LZ4 分块备份；该方法本身不是对用户的备份文件格式，也不复制库外文件。

## test.37 共享证据存储

格式8已接入证据原容器的32KiB物理分块共享；原容器、原始PB、Commit及签名保持不变，逻辑证据配额不放宽。格式7原件在同一迁移事务转换，旧格式只读保持可用。本轮相关回归、真实工作台重放与Windows自检已通过，见[共享存储设计](../docs/PLUGIN_SHARED_EVIDENCE_STORAGE.md)。设置批量存证、全局配额与GC继续推进。
