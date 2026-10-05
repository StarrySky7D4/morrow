# sse-event-v1

独立实验 Rust／C／C++17 SSE payload 库，采用原生 version1／raw SHA256 `bec1e174b7586e46155b60471867900bf91f1fb11849e6761916bfc3ca7c9863`。完整 data／event／id UTF-8、内嵌 NUL 和 retry None／0／MAX 保持；codec 不执行上游文本 parser 的截断、ID忽略或重连策略。

Rust Event／EventRef，自有 C 单一合计有界 storage 与 C++ move-only owner。逻辑字段及 wire 各限65,536 bytes，保持原生默认 allocator；接受的 wire 重新编码可能因真实开销返回 Limit。原生 FFI 内存有效性是调用者义务，错误保持输出不变。

只依赖 capnp／capnpc／sha2，13 registry 依赖与原 SDK lock 相同；可选 c-transport 合并不变原 SDK 的 C transport。包特性仍为原 channel-v1，payload 名称不授予网络、Store、来源或 replay 权限。

Windows 本阶段库6方法、网络100、准备策略11及原生语料／实际三语言客体通过，各计数和边界见 [接口](../../docs/PLUGIN_SSE_EVENT_SDK.md)、[实测](../../reports/reconstruction-2026-10-05/sse-event-sdk.md)与[独立分发](../../docs/PLUGIN_CHANNEL_PAYLOAD_DISTRIBUTION.md)。其它平台／生产批准／完整 SDK 仍 OPEN；测试源码与示例只使用普通合成数据，历史原件未重建。
