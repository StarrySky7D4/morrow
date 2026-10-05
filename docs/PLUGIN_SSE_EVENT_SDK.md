# SSE event 类型化 SDK

`extensions/sse-event-v1` 是独立的实验 payload 库，提供 Rust、自有 C 数据和 C++17 owner。原 channel 的 Receive／ACK、包特性及授权方式保持不变；解码事件不会创建网络源、准许 URL、恢复授权或决定业务成功。接口实现与本阶段实测见 [Windows 记录](../reports/reconstruction-2026-10-05/sse-event-sdk.md)。

## 契约与数据

采用原 `network_node_stream_001/schemas/sse_event.capnp` 的原始字节，version 1，raw SHA256 `bec1e174b7586e46155b60471867900bf91f1fb11849e6761916bfc3ca7c9863`。字段为完整 UTF-8 的 data、event、id，以及 retry 的 None／0／u64 MAX。内嵌 NUL 可以存在于 payload；上游 SSE 文本解析器对 id 中 NUL 的忽略、默认事件名或 retry 持续状态不属于这个 codec。

三个文本字段的合计字节数最多 65,536；序列化 wire 独立限制为 65,536，traversal 16,384 words／nesting 8。先检查合计和 UTF-8，再复制自有字段。保留原生默认 allocator：有效字段或可接受的非规范 wire 重新编码时，仍可能因真实开销超过 wire 上限而返回 Limit；不截断或改 allocator。实际来源、channel 和包预算可以比 codec 上限更小。

`sse-event-v1` 是 codec 身份，不是新的 required_feature、import 或网络能力。示例仍只声明 `transform-handlers-v1` 与 `channel-v1`，宿主另行批准精确来源并以原 Store 取得一次 dispatch claim。

## Rust

依赖单独库 `morrow-sse-event-v1`；不依赖 Core/runtime/network。`Event` 拥有三个 String，`EventRef` 借用三个 str；`encode` 与 `Event::decode` 处理原 envelope。

```rust
use morrow_sse_event_v1::{Event, EventRef};
let wire = EventRef {
    data: "第一行\n第二行", event: "delta", id: "", retry: Some(0),
}.encode()?;
let event = Event::decode(&wire)?;
assert_eq!(event.retry, Some(0));
```

Debug 只给出长度和 retry 是否存在，不输出字段正文。实际 ACK 使用原 frame sequence／frame SHA／cursor；重新编码的 payload 不能代替原 frame 身份。

## C 与 C++17

包含 `morrow_sse_event_v1.h`。`mse_event_v1` 使用单一 65,536-byte storage、三组 offset／length、has_retry／reserved／retry，当前结构大小 65,576、对齐 8。decode／set 复制全部输入；有效范围可重叠，逻辑合计仍计三份长度。成功清零未用 storage，不保留输入指针。

`mse_event_v1_decode`、`mse_event_v1_set`、`mse_event_v1_encode`、`mse_event_v1_schema_digest` 返回 OK／Invalid／Contract／Limit／Utf8／Buffer。所有 codec 错误保持输出与长度不变；编码先生成完整 wire，写入的 prefix 不能和 out_length 重叠。原生调用者仍必须提供有效、对齐且足够长的内存；这些 API 不提供指针沙箱。

`morrow_sse_event_v1.hpp` 的 `morrow::sse_event_v1::event` 是 move-only owner；视图借用 owner，成功赋值／decode、move 或销毁后应重新取得视图。移出对象的操作拒绝，view 返回空 sentinel；编码失败保留原 vector。分配遵循普通 C++ 库语义。

## 开发与执行

新示例位于 `guests/rust`、`guests/c`、`guests/cpp`。库可提供 rlib／staticlib；`c-transport` 将不变 SDK 的 C transport 与本 codec 合入一个 archive。Windows 已验证 Rust `wasm32-unknown-unknown` 和 Clang WASI 编译路线；其它平台需要独立验收。

仓库内准备新示例使用 `tool/prepare_sse_event_guests.py`，强制新外部输出、独立 target、原锁的离线依赖、精确 compiler／sysroot／packer。它依赖仓库的原生 schema 和可信包工具，不是独立源码包中的可用 CLI。项目外接入见 [源码分发](PLUGIN_CHANNEL_PAYLOAD_DISTRIBUTION.md)。

本阶段实际三语言示例消费五个完整事件，包括空 ID 重置、retry 0／MAX、NUL id 行忽略及 `[DONE]` 之后的事件。EOF／id／retry／`[DONE]` 不授予 reconnect、resume 或 replay；所有消费 ACK 和线程 join 完成后，外部业务仍 OutcomeUnknown。生产 owner／GUI／账户／TLS／其它平台及完整 SDK 冻结保持 OPEN。
