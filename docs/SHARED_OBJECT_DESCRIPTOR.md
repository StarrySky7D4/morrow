# 共享对象运行期描述契约

`core::shared_object` 提供可移植对象元数据，使用固定预编译 Cap’n Proto schema v1。描述符仅是数据：知道 arena／object／generation 或复制其编码不产生授权，不允许把这些编号持久化后作为访问权限恢复。

## 字段与边界

| 字段 | 约束与含义 |
| --- | --- |
| `arena / object / generation: u64` | 三者均非零，原样保留完整 64 位值；仅用于关联宿主对象身份，不能替代连接绑定与活跃租约。 |
| `length: u64` | 1–16 MiB，`MAX_OBJECT_BYTES` 固定上限。 |
| `sha256: [u8; 32]` | 完整逻辑对象字节的摘要声明。解码仅检查长度和结构，不假称已读取并验证实际对象。 |
| `segments: Vec<Segment>` | 1–64 段；每段 `offset / length: u64`。正长度、从 offset 0 开始、按序连续，完全覆盖对象长度，不重叠、不留空洞、不超界。 |

段偏移描述逻辑对象布局，不是进程指针、文件路径、操作系统句柄或映射地址。每段终点使用 checked_add，整数回绕直接拒绝。

## API

- `Descriptor::validate()`：纯结构验证，不访问文件、对象目录或权限状态。
- `Descriptor::for_bytes(arena, object, generation, bytes)`：先检查字节长度上限和身份，再计算 SHA-256，生成一段覆盖描述；不复制或持有字节，不创建租约。
- `Descriptor::encode()`：先验证，再编码固定 version 与 schema SHA-256。
- `Descriptor::decode(bytes)`：在解析前拒绝超过 16 KiB 的消息；遍历上限 2048 words、嵌套上限 8；使用有界拥有型消息读取，允许未对齐的传输切片；拒绝截断、尾字节、多个拼接消息、未知版本、错误 schema 摘要以及非法字段。
- `schema_digest()`：以仓库已有规则归一化 schema 换行并计算摘要；不依赖运行期 schema 下载。

手写实现无 unsafe。固定 Cap’n Proto 生成器的 schema 元数据代码沿用现有生成模块隔离方式，不将其当作手写对象访问代码。

此模块没有持久化 API，也不修改现有 runtime／task／SDK 消息版本。宿主交付对象时仍必须独立验证调用连接、租约、对象代次、可用范围、撤权状态和实际内容摘要；即使描述符合法，这些验证也不能省略。原生映射、浏览器共享缓冲区、对象销毁和资源预算由承载实现负责，不能从描述契约可编译推断映射已实现。

## 验证范围

10 项 Windows 核心集成测试覆盖完整 64 位身份、最小与最大对象、64 段边界、零身份、空对象／超限对象／过多段、空洞／重叠／乱序／零段／不完整覆盖、整数溢出、版本与摘要漂移、所有前缀截断、尾数据和超限消息，已知 SHA-256 向量和 for_bytes 边界，以及地址偏移 1–7 字节的非对齐传输切片。

```powershell
cargo test --offline --locked --manifest-path core/Cargo.toml --target-dir build/shared-contract --test shared_object
cargo clippy --offline --locked --manifest-path core/Cargo.toml --target-dir build/shared-contract --all-targets -- -D warnings
cargo check --offline --locked --manifest-path core/Cargo.toml --target-dir build/shared-contract --target wasm32-unknown-unknown --lib
```

Wasm 编译只验证可移植编译边界，不证明浏览器内共享内存、跨进程租约或真实对象授权。
