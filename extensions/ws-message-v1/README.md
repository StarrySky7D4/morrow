# Typed WebSocket message codec

`morrow-ws-message-v1` encodes and decodes the exact existing `network_node_stream_001` message envelope. It provides owned Rust messages, caller-owned C results, and a C++17 owner. It can be used with the original opaque `channel-v1` transport. `ws-message-v1` identifies this codec; it does not create a new guest import, required package feature, source grant, socket, retry policy, or production route.

The raw schema is `contracts/ws_message.capnp`, version 1, SHA-256 `2d2f3b1913060bd410d3bc608362c01d28cf7d0e3c1cdc82a5293abcaa696e6d`. The build checks these exact raw bytes before generating Rust bindings. The standalone library links Capnp and contains no Core, runtime, or network library. The optional `c-transport` feature combines the original C transport exports with this codec in one Rust archive for C/C++ guests.

## Rust

```rust
use morrow_ws_message_v1::{Message, MessageKind, MessageRef};

let outbound = MessageRef {
    kind: MessageKind::Text,
    payload: "Hello, 雪".as_bytes(),
    close_code: None,
};
let wire = outbound.encode()?;
let incoming = Message::decode(&wire)?;
assert_eq!(incoming.kind, MessageKind::Text);
assert_eq!(incoming.payload, outbound.payload);
```

`Message` owns its payload. `MessageRef` borrows an outbound payload only during encoding. Both validate semantic fields; encoding also validates the actual serialized size. Their `Debug` output shows kind, length and close code, without payload contents.

## C

Include `morrow_ws_message_v1.h` and link the built `morrow_ws_message_v1` archive. `mws_message_v1` contains its own bounded payload storage and has no retained pointer or destructor requirement. Allocate this approximately 64 KiB object on a suitable heap or with an adequate stack budget.

```c
mws_message_v1 *message = malloc(sizeof(*message));
uint8_t *wire = malloc(MWS_V1_MAX_ENVELOPE_BYTES);
uint32_t length = 0;
if (!message || !wire) {
    free(wire);
    free(message);
    return 1; /* allocation failure, inside the caller's function */
}
uint32_t status = mws_message_v1_set(MWS_V1_TEXT,
    (const uint8_t *)"hello", 5, 0, 0, message, sizeof(*message));
if (status == MWS_V1_OK)
    status = mws_message_v1_encode(message, sizeof(*message),
        wire, MWS_V1_MAX_ENVELOPE_BYTES, &length);
if (status == MWS_V1_OK) {
    /* Send exactly length bytes over the existing channel transport. */
}
free(wire);
free(message);
```

All inputs must be valid and readable for the documented counts, and output storage must be valid and writable. Struct and length pointers must have their normal C alignment. Pointer validity is a native caller obligation, not a sandbox guarantee. Null outbound payload is accepted only at length zero. Inputs may overlap outputs because input is consumed before writing. Encoding rejects overlap between its written wire prefix and output length. Every codec error leaves result storage, wire output and output length unchanged. Decode/set success zeroes unused owned payload storage; encoding preserves unused wire capacity. Payload length is explicit, without NUL termination.

## C++17

```cpp
#include "morrow_ws_message_v1.hpp"
using morrow::ws_message_v1::kind;
using morrow::ws_message_v1::message;
message outbound;
auto status = outbound.assign(kind::Close,
    reinterpret_cast<const uint8_t *>("bye"), 3, uint16_t{1000});
std::vector<uint8_t> wire;
if (status == MWS_V1_OK) status = outbound.encode(wire);
message incoming;
if (status == MWS_V1_OK)
    status = incoming.decode(wire.data(), static_cast<uint32_t>(wire.size()));
// After successful decode, incoming owns the payload independently of wire.
```

The wrapper is move-only and heap owns the C result. A failed decode/assign preserves its previous result; a failed encode preserves the caller's vector. Views remain valid until successful reassignment, moving or destruction. Moved-from methods return `MWS_V1_INVALID`; their view is an empty sentinel. Allocation follows ordinary C++ library behavior.

## Bounds and wire compatibility

Text requires valid UTF-8. Binary accepts arbitrary bytes. Ping and Pong allow at most 125 payload bytes. Close allows at most 123 UTF-8 reason bytes; a nonempty reason requires a code. Allowed close codes match native tungstenite 0.29: 1000–1003, 1007–1013 and 3000–4999. Other message kinds cannot carry a close code.

The serialized envelope cap is 65,536 bytes, with a 16,384-word traversal bound and nesting limit 8. Decode accepts native-compatible multi-segment, far-pointer and compatible expanded struct shapes, while rejecting trailing bytes, reachable capabilities, malformed fields, wrong version/digest and inconsistent close-code flags. Encoding uses the original default Capnp allocator, including real segment overhead. A valid decoded envelope near the cap may fail re-encoding with `Limit`; semantic validity does not guarantee that the default allocator's serialized representation fits.

## Build and evidence scope

Build using the extension's own lockfile and exact pinned dependencies. The Windows qualification used Rust 1.95.0, native `x86_64-pc-windows-msvc`, Capnp 1.4.0, Release, locked/offline cache and a dedicated external target. All 13 registry dependency name/version/source/checksum tuples match the original frozen SDK lock.

Fresh library evidence covers six unique native methods, strict Clippy and owned-file formatting. Independent native corpus, C/C++ consumers and actual compiled guest/source runs have separate evidence receipts. Windows synthetic/keyless proof does not establish Linux, external TLS/service, account, GUI, CI, release, or production route qualification. This source extension does not upgrade the frozen global SDK exports. C05 separately qualified the 90-file WS/SSE source profile and new external guest artifacts; see [distribution usage and boundaries](../../docs/PLUGIN_CHANNEL_PAYLOAD_DISTRIBUTION.md). Its byte inventory and runtime evidence remain separate from production approval or full SDK freeze.
