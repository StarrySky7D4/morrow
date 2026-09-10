# Morrow core — test.2

这是独立于 Flutter 的 Rust 契约实现，当前版本 `0.1.9-test.2`。阶段为 M0／M1 的部分交付，**尚未成为工作台的数据后端**。旧 Flutter 保存路径仍是旧应用唯一权威；本核心没有数据库、自动迁移或写入旧资料的入口。构建不运行应用、不读取用户资料。

## 已实现

- `schemas/content.proto`：Card、BlobRef、Relation、Preview、Workspace、ViewPlacement、Draft 的候选版本化契约。业务类型与正文不由核心硬编码；身份为不含路径分隔符／冒号的非空 UTF-8 字符串，兼容旧条目 ID。
- `CardRecord` 保留完整 Protobuf 记录，`CardSummary` 仅为只读投影，禁止拿投影覆盖正式记录。已知标题修改保留顶层与嵌套未知字段、未知枚举和未知 oneof 载荷。未经编辑的记录返回原字节；编辑后的编码与原始解码字节分别保留。`original_bytes` 不是历史证据存储服务。
- `schemas/runtime.capnp`：内部重命名请求与摘要布局。请求包含操作 ID、目标卡片 ID 和 UInt64 预期修订；纯编辑提案检查目标和修订并递增修订。操作 ID 尚不提供去重保证。调用者身份不由消息自报，授权与持久化必须由后续宿主管理。
- Protobuf＋LZ4 容器、解压前限额和 SHA-256 损坏检测；不经过 JSON。SHA-256 不提供签名、来源认证或审计封存。
- `morrow-core-check self-check` 运行内存中的协议→纯编辑→容器往返；`verify <file>` 只读验证指定容器。诊断文本不是正式记录。

Workspace／ViewPlacement／Draft 本轮仅有 schema，未实现工作区操作或草稿持久化。BlobRef 只描述引用，未验证文件存在性、原子发布或回收。卡片预览无需插件即可读取，但 Flutter 展示及附件导出尚未接入。

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
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.2 --bin morrow-core-check -- verify path/to/card.morrow
```

Wasm 目标的 `cargo check --lib` 仅证明编译检查通过，不证明已导出 Wasm API、接入浏览器 Worker、实现 Dart 互操作或浏览器存储。原生 FFI、共享内存、权限、提交事务、审计和插件执行仍未实现。具体协议与库仍需后续跨边界验证，不因本轮通过而冻结全平台实现。

参考：[prost-reflect 未知字段 API](https://docs.rs/prost-reflect/0.16.5/prost_reflect/struct.DynamicMessage.html#method.unknown_fields)、[LZ4 有界解压 API](https://docs.rs/lz4_flex/0.14.0/lz4_flex/block/fn.decompress_into.html)。本轮兼容性结论以仓库中的演进测试为依据，不把普通 prost 生成类型直接作为无损编辑载体。
