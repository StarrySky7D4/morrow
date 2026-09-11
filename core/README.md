# Morrow core — test.4

这是独立于 Flutter 的 Rust 契约实现，当前版本 `0.1.9-test.4`。阶段为 M0／M1／M2 的部分交付，**尚未成为工作台的数据后端**。旧 Flutter 保存路径仍是旧应用唯一权威；新 SQLite 数据库仅由明确指定路径的实验 CLI／可信宿主访问，没有自动迁移或写入旧资料的入口。构建不运行应用、不读取用户资料。

## 已实现

- `schemas/content.proto`：Card、BlobRef、Relation、Preview、Workspace、ViewPlacement、Draft 的候选版本化契约。业务类型与正文不由核心硬编码；身份为不含路径分隔符／冒号的非空 UTF-8 字符串，兼容旧条目 ID。
- `CardRecord` 保留完整 Protobuf 记录，`CardSummary` 仅为只读投影，禁止拿投影覆盖正式记录。已知标题修改保留顶层与嵌套未知字段、未知枚举和未知 oneof 载荷。未经编辑的记录返回原字节；编辑后的编码与原始解码字节分别保留。`original_bytes` 不是历史证据存储服务。
- `schemas/runtime.capnp`：内部重命名请求与摘要布局。请求包含操作 ID、目标卡片 ID 和 UInt64 预期修订；纯编辑提案检查目标和修订并递增修订。编解码本身不执行命令；Store 路径已提供操作 ID 去重。调用者身份不由消息自报，重命名写入通过 HostPolicy.commit_rename 完成授权。
- Protobuf＋LZ4 容器、解压前限额和 SHA-256 损坏检测；不经过 JSON。SHA-256 不提供签名、来源认证或审计封存。
- `morrow-core-check self-check` 运行内存中的协议→纯编辑→容器往返；`verify <file>` 只读验证指定容器。诊断文本不是正式记录。

Workspace／ViewPlacement／Draft 本轮仅有 schema，未实现工作区操作或草稿持久化。BlobRef 只描述引用，未验证文件存在性、原子发布或回收。卡片预览无需插件即可读取，但 Flutter 展示及附件导出尚未接入。

## test.4 边界与宿主状态

`bridge.rs` 与 [C 头文件](include/morrow_core.h) 提供同一原生／Wasm 缓冲区 ABI。它只验证固定副本的版本与摘要并往返内部请求，尚不分派到持久化或权限执行。运行期协议已升到 2，test.2 的旧协议请求被明确拒绝；Protobuf 卡片容器仍为 1。Dart 原生与 Chrome Dart/Wasm 实测见 [客户端说明](../packages/morrow_core_client/README.md)。普通 Dart JavaScript 编译仍有精确整数限制。

`lifecycle.rs` 提供纯内存 HostPolicy：宿主分配不可自报的实例身份与代次、对象级重命名授权和到期时刻；接单固定请求，完成前再次核对身份／授权／期限。正常停止不接新任务、等待已有任务，达到排空期限则撤权；安全停止先撤权后取消。宿主须用单调时钟调用 expire_drains，迟到完成也会检查期限。停止／退休后旧实例、授权与任务不可复用。

CommitState 区分未提交、结果待核对、本地已提交、封存、见证和查证未提交。超时不会改成未提交，撤权也不回滚已提交内容。状态枚举本身没有封存证明、外部见证或生产授权 UI。test.4 新增的 HostPolicy.commit_rename 在独占宿主期间，将固定请求、提交前的新时钟授权核对与实际 SQLite 事务串联；依赖、共享对象、运行后端与取消外部效果仍待 M3。

## test.4 原生事务存储

`store.rs` 使用固定 rusqlite 0.40.2／bundled SQLite，WAL＋synchronous=FULL＋BEGIN IMMEDIATE；写锁冲突立即报告存储错误，由宿主查询或重试。卡片、操作结果及待封存事件在同一事务中写入。SQLite 页／索引由引擎管理，业务内容保持 Protobuf＋LZ4。[SQLite 同步设置](https://sqlite.org/pragma.html#pragma_synchronous)、[事务行为](https://docs.rs/rusqlite/0.40.2/rusqlite/enum.TransactionBehavior.html)。

- 新 `transaction.proto` 和 `MORROWT1` 容器保存固定 Protobuf 命令、摘要、结果修订及稳定事件 ID；运行期 Cap’n Proto 字节不作为业务持久化格式。操作 ID／事件 ID 在单数据库内唯一，宿主负责跨调用方作用域分配。
- 同操作 ID、同完整命令返回首次结果，即使内容已被后续操作修改；相同 ID 携带不同命令则拒绝。返回原始修订，不用当前卡片冒充首次结果。
- 提交成功才返回 LocallyCommitted。COMMIT 返回错误时报告 CommitUnknown；查询 Absent 只是当前快照，不能断言并发中的操作永远不会提交。当前失败操作不另建耐久“未执行”证明。
- 重命名持有宿主的独占借用，事务内及 COMMIT 前重新核对授权；最后一次核对是授权线性化点。排空期限或授权在准备过程中到期会回滚。最终磁盘同步期间到期不追溯撤销已线性化的提交。多个独立进程／宿主的权限同步仍待实现。
- 默认待封存队列上限 1024 条／64 MiB；先检查容量再写入，事件插入失败同时回滚内容与操作。M4 封存机制建立前没有清空／确认队列入口，达到上限会拒绝新操作。
- 打开数据库检查归属、版本、SQLite 完整性、卡片摘要与操作／事件对应关系。未来版本和无关数据库被拒绝，`open_existing` 不创建缺失文件。这是损坏检测，不是防御已控制宿主和数据库的攻击者。
- 当前仅支持无附件卡片创建、标题修改、结果查询和容器导出；含 BlobRef 的新记录被拒绝，直至 M2-03 建立附件事务。关系、草稿、工作区的实际事务待实现。默认写入只接触明确传入的新数据库，不切换旧 Flutter 保存路径。

实验 CLI：

```powershell
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.4 --bin morrow-core-store -- init build/example.db
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.4 --bin morrow-core-store -- create-local build/example.db operation-1 card-1 示例
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.4 --bin morrow-core-store -- rename-local build/example.db operation-2 card-1 1 新标题
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.4 --bin morrow-core-store -- query build/example.db operation-2
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.4 --bin morrow-core-store -- check build/example.db
```

CLI 的 local 操作是可信本地操作者入口，不能直接暴露给插件；lookup/card 同样需要传输层的读取授权。`fault-injection` 只用于子进程恢复测试，默认构建不响应故障注入环境变量。进程直接退出覆盖提交前后边界，不等于断电或全平台验收。

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
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.4 --bin morrow-core-check -- verify path/to/card.morrow
```

test.4 已导出原生／Wasm ABI 并在 Chrome Worker 中实测协议往返；浏览器存储、共享内存、审计与插件运行仍未实现；原生事务仅在实验 CLI／Rust API 中可用，FFI 仍是协议往返探针。具体协议与库仍需其他平台、性能和主应用接入验证，不因本轮通过而冻结全平台实现。

参考：[prost-reflect 未知字段 API](https://docs.rs/prost-reflect/0.16.5/prost_reflect/struct.DynamicMessage.html#method.unknown_fields)、[LZ4 有界解压 API](https://docs.rs/lz4_flex/0.14.0/lz4_flex/block/fn.decompress_into.html)。本轮兼容性结论以仓库中的演进测试为依据，不把普通 prost 生成类型直接作为无损编辑载体。
