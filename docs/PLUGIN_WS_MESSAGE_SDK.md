# WebSocket 类型化消息 SDK

`extensions/ws-message-v1` 提供独立实验库：Rust 类型化编解码、C 有界自有消息接口及 C++17 所有权包装。它解释原 channel 的消息字节，不创建 socket、grant 或新的 guest import。完整 SDK 仍未冻结。

## 固定契约

使用既有 `network_node_stream_001/schemas/ws_message.capnp` 的原始字节副本，version 1，SHA256 为 `2d2f3b1913060bd410d3bc608362c01d28cf7d0e3c1cdc82a5293abcaa696e6d`。编译和 guest 准备工具均核对该摘要；改变 schema 时必须定义新的契约身份，不能重写旧 pin。

| 内容 | 当前接口与界限 |
|---|---|
| 消息种类 | Text、Binary、Ping、Pong、Close |
| 文本 | Text 与 Close reason 严格 UTF-8；Binary 与控制 payload 保留原字节，包含零字节 |
| 控制帧 | Ping／Pong payload 最多 125 字节；Close reason 最多 123 字节 |
| Close | 无 code 时 reason 必须为空；允许 code 为 1000–1003、1007–1013、3000–4999，与固定原生后端一致 |
| envelope | 序列化消息总长度最多 65536 字节；旧 channel／批准预算可进一步收紧 |
| 读取 | traversal 最多 16384 words、nesting 8；支持合法多段／far pointer，拒绝畸形指针、尾随数据及可达 capability |
| 输出 | Rust 解码持有自身 payload；C 自有固定缓冲区；C++ 为只可移动的所有权对象 |

payload 长度与 envelope 总长度分别检查。合法消息可能刚好以紧凑布局落在 wire 上限内，但用原默认 Cap’n Proto allocator 重新编码时因 segment overhead 返回 Limit。库不截断 payload，也不悄悄切换编码策略来掩盖此结果。

## Rust

crate 为 `morrow-ws-message-v1`，原库与编译依赖按精确版本锁定；不依赖 Core、Store、runtime 或网络库。`MessageRef` 借用待发送 payload，`Message::decode` 返回拥有 payload 的 `Message`。

```rust
use morrow_ws_message_v1::{Message, MessageKind, MessageRef};

let outgoing = MessageRef {
    kind: MessageKind::Text,
    payload: "你好".as_bytes(),
    close_code: None,
};
let wire = outgoing.encode()?;
let incoming = Message::decode(&wire)?;
assert_eq!(incoming.as_ref(), outgoing);
```

将 `wire` 交给原 channel `Action::Send`，或对 `Receive` 返回的完整 `Frame.bytes` 解码。调用者仍须按原 frame 身份、digest、sequence 与 cursor 提交精确 ACK。解码本身不证明 ACK 已提交、socket 已 flush、producer 已 join 或远端业务成功。

`Message` 的 Debug 仅显示 kind、payload 字节数与 close code，避免把实际消息正文写入诊断。

## C 与 C++

C 头文件为 `include/morrow_ws_message_v1.h`。`mws_message_v1_set` 构造自有消息；`decode` 从原 wire 复制 payload；`encode` 使用调用者提供的可写缓冲区并返回实际长度。`schema_digest` 返回固定摘要。

C 消息的 payload 使用显式长度，不依赖 NUL；空 payload 可传 null／length 0。调用者必须提供有效且满足对齐、长度和读写要求的本地内存。接口不保留指针，成功后 payload 不借用原输入。所有 codec 错误保持输出和编码长度不变；encode 的字节输出与长度输出不能互相重叠。

C++ 头文件为 `include/morrow_ws_message_v1.hpp`，使用 `morrow::ws_message_v1::message`、`kind`、`assign`、`decode`、`view` 和 `encode`。对象不可复制，可以移动；移走后的 encode／decode／assign 返回 Invalid。编码失败保留已有 vector。view 随对象重新赋值、移动或销毁而失效；分配失败遵循所用 C++ 标准库的规则，不冒充 codec 状态。

C 与 C++ 使用同一 Rust C ABI，实现只维护一套 wire／语义规则。`c-transport` feature 将新 codec 与原 SDK C transport 导出合并为单个 archive，供新 Wasm guest 链接；不要同时链接两份 Rust runtime。C++ STL、Rust 默认 ABI、函数指针或自有结构体布局不跨 guest／进程边界。

## 新 guest 与宿主接入

新 Rust、C、C++ 示例在 `guests/`，仅使用原 `channel-v1` import／包特性。`ws-message-v1` 是 payload codec 身份，不是新增 required feature 或网络权限。旧冻结 base／dependency／provider 与历史网络 guest 都不重建。

`tool/prepare_ws_message_guests.py` 在明确的源码目录外生成新 Wasm 与 package，拒绝已有输出、目标重叠、契约或锁不匹配。Windows 分离的 LLVM／WASI 安装须同时给出 `--clang`、`--clangxx`、`--sysroot`；工具不下载依赖或自动换工具链。输出、Rust target、C target 必须各自独立。构建前后核对完整选定 sysroot 文件集、编译器、linker、packer、源文件与原 SDK 摘要；遇到重解析点或内容漂移即拒绝。

可信宿主通过既有 `managed_ws::WsSource` 明确批准精确目标、原实例／包／selection、当前 live owner、预算与原 deadline，再在原 Store 赢得严格 claim 后启动一次源。原 Receive／ACK／Send 规则、撤权、累计计费和真实资源退出证明保持原样。普通 guest 不能凭声明主动请求任意 URL，旧 IO `SubmitWebSocketConnect` 仍为 Unsupported。

真实测试与未覆盖范围见 [C04 Windows 验证](../reports/reconstruction-2026-10-05/ws-message-sdk.md)，后续门槛见 [SDK 实施顺序](../reports/reconstruction-2026-10-05/sdk-next-gates.md)。本阶段不宣称生产 GUI／批准链、账户／TLS／公开 API、普通用户 token、其他平台或完整 SDK 资格。
