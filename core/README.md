# Morrow core — test.8

这是独立于 Flutter 的 Rust 契约实现，当前版本 `0.1.9-test.8`。阶段为 M0／M1／M2 的部分交付，**尚未成为工作台的数据后端**。旧 Flutter 保存路径仍是旧应用唯一权威；新 SQLite 数据库仅由明确指定路径的实验 CLI／可信宿主访问，没有自动迁移或写入旧资料的入口。构建不运行应用、不读取用户资料。

## 已实现

- `schemas/content.proto`：Card、BlobRef、Relation、Preview、Workspace、ViewPlacement、Draft 的候选版本化契约。业务类型与正文不由核心硬编码；身份为不含路径分隔符／冒号的非空 UTF-8 字符串，兼容旧条目 ID。
- `CardRecord` 保留完整 Protobuf 记录，`CardSummary` 仅为只读投影，禁止拿投影覆盖正式记录。已知标题修改保留顶层与嵌套未知字段、未知枚举和未知 oneof 载荷。未经编辑的记录返回原字节；编辑后的编码与原始解码字节分别保留。`original_bytes` 不是历史证据存储服务。
- `schemas/runtime.capnp`：内部重命名请求与摘要布局。请求包含操作 ID、目标卡片 ID 和 UInt64 预期修订；纯编辑提案检查目标和修订并递增修订。编解码本身不执行命令；Store 路径已提供操作 ID 去重。调用者身份不由消息自报，重命名写入通过 HostPolicy.commit_rename 完成授权。
- Protobuf＋LZ4 容器、解压前限额和 SHA-256 损坏检测；不经过 JSON。SHA-256 不提供签名、来源认证或审计封存。
- `morrow-core-check self-check` 运行内存中的协议→纯编辑→容器往返；`verify <file>` 只读验证指定容器。诊断文本不是正式记录。

Workspace／ViewPlacement／Draft 本轮仅有 schema，未实现工作区操作或草稿持久化。BlobRef 保持可移植引用；test.5 原生 Store 在提交中核对暂存原字节、建立当前／历史引用，并提供受保护回收。卡片预览无需插件即可读取；原始附件可从实验 CLI 导出，Flutter 展示与导出入口尚未接入。

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

- Rename 与 ReadSummary 是独立的对象级授权；操作结果查询在 test.8 加入独立授权；创建、导入、附件和搜索尚未扩展成完整的授权命令。
- 读取在查询前与交付投影前检查身份、能力、对象及期限；过期、撤权、排空或旧实例均拒绝。无读取权限时，存在与不存在的目标返回同类 Denied，不先查询内容。授权读取返回基础摘要，不返回正文或附件名。
- 响应为带请求关联 ID、契约摘要的 Cap’n Proto v4：提交回执、摘要或稳定错误枚举。失败响应不夹带私有标题、路径或内部异常文本；CommitUnknown 单独表达，不当成确定未提交。
- 运行期单消息最多 64 KiB；摘要预览文本最多 16 KiB，超限返回 Limit，不把裁剪后的摘要保存回记录。这里的授权是同步独占宿主的线性化检查，数据交付后不能追溯收回已复制的字节。
- Rust 原生集成和 Web Worker 接入使用同一分派器；test.8 原生 Dart 已经实际 DLL 接入分派；原有 morrow_buffer_process／Dart-Wasm 探针仍保留协议往返用途。完整跨进程隔离、授权配置持久化、审计和插件执行待后续阶段。

## test.5 边界与宿主状态

`bridge.rs` 与 [C 头文件](include/morrow_core.h) 提供同一原生／Wasm 缓冲区 ABI。旧 morrow_buffer_process 仍只验证并往返协议；新增的原生 morrow_host_* 管理接口与 morrow_host_dispatch 则接入 HostRuntime。运行期协议已升到 4，test.2–test.7 的旧协议请求被明确拒绝；Protobuf 卡片容器仍为 1。Dart 原生与 Chrome Dart/Wasm 实测见 [客户端说明](../packages/morrow_core_client/README.md)。test.6 通过独立 web.dart／Rust 编解码入口解决普通 Dart JavaScript 的精确整数接入；旧生成绑定主入口仍受该限制。

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
- 当前支持带已暂存附件的卡片创建、标题修改、附件列表替换、结果查询和原件导出；缺失或损坏的载荷被拒绝。关系、草稿、工作区的实际事务待实现。数据库格式已升为 2，不自动打开或迁移 test.4 的格式 1；需要新建实验数据库，不切换旧 Flutter 保存路径。

实验 CLI：

```powershell
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --bin morrow-core-store -- init build/example.db
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --bin morrow-core-store -- create-local build/example.db operation-1 card-1 示例
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --bin morrow-core-store -- rename-local build/example.db operation-2 card-1 1 新标题
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --bin morrow-core-store -- query build/example.db operation-2
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --bin morrow-core-store -- check build/example.db
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
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.8 --bin morrow-core-check -- verify path/to/card.morrow
```

test.5 已导出原生／Wasm ABI 并在 Chrome Worker 中实测协议往返；浏览器存储、共享内存、审计与插件运行仍未实现；原生事务仅在实验 CLI／Rust API 中可用，FFI 仍是协议往返探针。具体协议与库仍需其他平台、性能和主应用接入验证，不因本轮通过而冻结全平台实现。

参考：[prost-reflect 未知字段 API](https://docs.rs/prost-reflect/0.16.5/prost_reflect/struct.DynamicMessage.html#method.unknown_fields)、[LZ4 有界解压 API](https://docs.rs/lz4_flex/0.14.0/lz4_flex/block/fn.decompress_into.html)。本轮兼容性结论以仓库中的演进测试为依据，不把普通 prost 生成类型直接作为无损编辑载体。
