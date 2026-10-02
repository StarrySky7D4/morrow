# Channel v1 SDK

Channel v1 是独立的有界字节流和事件通道协议。Rust、C11 与 C++17 使用相同的固定 Cap’n Proto codec；Wasm import 为 `morrow_channel_v1.call`。现有 guest ABI、task 协议和 IO 协议保持不变。

SDK 提供 Directory、Request、Response、Frame codec、native adapter 和流式摘要。Directory 描述已经由 host 绑定到本次执行的通道；解码不会创建 producer、批准数据源或赋予权限。实际数据、deadline、grant、预算和资源回收由 owning runtime 管理。

## 从 caller prepare 到 typed task input

真实 local caller 显式提供 bytes 或 events，调用 managed channel job 的 prepare。Host 返回现有 channel `Directory` 的编码字节，其中包含 `scope_sha256`、最多 8 个 endpoint，以及每个 endpoint 的 `reference`、`source_epoch`、kind 和预算。

通用插件可以在自己的 manifest 中声明一个接收 Directory 的 handler。建议输入类型标识为 `morrow.channel.directory.v1`；这是一项应用级 typed input 约定，不是新 SDK wire 字段。Caller 按 manifest 构造现有 `TaskInput::Transform`：

| 字段 | 值 |
| --- | --- |
| handler | 插件显式声明的 handler 名称 |
| input_type | handler 声明的类型，例如 `morrow.channel.directory.v1` |
| input | prepare 返回的 canonical Directory 字节，原样传入 |
| output_type | handler 显式声明的业务输出类型 |
| input/output 上限 | handler、旧 task 与 host 三者预算共同限定；旧 value 上限仍为 64 KiB |

Directory 不携带业务模式、插件 handler、任务身份或任意路径。插件依据自身明确的业务合同选择 endpoint，并分别检查 kind、预算和 source epoch。多个 endpoint 的选择策略也属于插件合同；SDK 不把第一个 endpoint 解释为任何隐含的数据源角色。

任务输入只传有界元数据，较大的数据经 Receive/Send 逐帧传输。任务输出仍在旧 64 KiB value 上限内；插件可输出自己的摘要或业务结果。64 字节 `CHV1` 不是所有 channel 插件的统一输出格式。

Caller 的生命周期是 **prepare → append → run once → status → close**：prepare 取得本次 fresh job 的 Directory；append 显式加入真实的有限 bytes/events；run once 在同一个 job 上启动一次执行，并将该 Directory 放入所声明的 typed task input；status 查询实际进度、业务输出与 cleanup proof；close 请求停止并继续核对资源回收。Append 和 run 的预算由 owner 准入。Run 已提交但回复不确定时不再次 run；使用同一个 job key 查询并关闭。

`None` 本身不证明实际 join：没有业务输出不能推断“未执行”，线程 handle 不存在也不能冒充“已经 join”。Owner 正面记录的 `NoProducer` 表示没有成功创建 producer，区别于 `Joined` 的实际 join 证据。`Unknown` 保留源/执行的不确定性，即使 producer 已实际 join，也不变成业务成功。`ClosingUnconfirmed` 或 pending cleanup 则保留尚未确认回收的状态。Private host job 字段名由 host 合同定义；公开 channel wire 仍只使用原 status 和 `resource_reclaimed`，不把这些 host 状态塞进新的 SDK 字段。

## 三语言 Directory 解码

下面的片段假设 task 已经过旧 task codec 解码，输入类型符合 manifest，`selected_index` 由明确的插件策略选定，`call_id` 是本次调用的非零 32 字节唯一身份。Host 仍会独立检查真实绑定和授权。为另一项操作分配新的 call ID，不以重复请求来掩盖不确定结果。

Rust 使用 `morrow_plugin_sdk::channel`。Directory 和 endpoint 拥有自己的数据，可在输入 buffer 释放后继续使用。

```rust
use morrow_plugin_sdk::channel::{Action, Directory, Error, Request};

let directory = Directory::decode(&transform.input)?;
let endpoint = directory.channels.get(selected_index).ok_or(Error::Invalid)?;
let request = Request {
    call_id,
    reference: endpoint.reference,
    source_epoch: endpoint.source_epoch,
    action: Action::Receive {
        last_acked: 0,
        credit_bytes: endpoint.budget.max_frame_bytes,
    },
};
let response = morrow_plugin_sdk::channel::transport::call_wasm(&request)?;
// native 场景使用 unsafe 构造的 channel::transport::Client::call。
```

片段所在函数需要分别处理 codec 与 transport 的错误类型。读取 Frame 后，在真正消费 bytes 之后调用 `Frame::digest()`，使用该 frame 的 sequence、摘要及原始 cursor 构造显式 Ack。Request/Response 验证自动检查 schema、原请求 SHA、call ID、reference、source epoch、sequence 与 Receive credit。

C 包含 `morrow_channel_v1.h`。Directory decode 复制输入；getter 返回 SDK handle 所有的只读视图。先检查每次 codec 调用和索引，再使用 endpoint。

```c
mp_channel_directory *directory = NULL;
mp_channel_directory_view view = {0};
mp_channel_response *response = NULL;
mp_channel_request_v1 request = {0};
uint32_t result = mp_channel_directory_decode(input, input_length, &directory);
if (result != MP_CODEC_OK) goto done;
result = mp_channel_directory_get(directory, &view, sizeof(view));
if (result != MP_CODEC_OK) goto done;
if (selected_index >= view.channel_count) {
    result = MP_CODEC_INVALID;
    goto done;
}
const mp_channel_endpoint_view *endpoint = &view.channels[selected_index];
request.abi_version = MP_CHANNEL_VERSION;
request.struct_size = sizeof(request);
request.kind = MP_CHANNEL_RECEIVE;
request.call_id.data = call_id; request.call_id.length = 32;
request.reference = endpoint->reference;
request.source_epoch = endpoint->source_epoch;
request.sequence = 0; /* 最后一个已确认的 ACK sequence */
request.credit_bytes = endpoint->budget.max_frame_bytes;
result = mp_wasm_channel_call(&request, &response);
/* native 场景使用 mp_channel_call(&host, &request, &response)。 */
done:
mp_channel_directory_free(directory);
/* response 单独拥有数据；读取并检查 status 后释放。 */
mp_channel_response_free(response);
```

native descriptor 必须按其 C 类型对齐，至少提供 8 字节可读 prefix。兼容的 ABI/version 与 struct_size 表示完整 descriptor 可读。输入 spans 在调用期间保持有效；adapter 在提交前复制请求，并提前预留完整输出容量。Directory views 在 `mp_channel_directory_free` 后失效；Response views 在 `mp_channel_response_free` 后失效。Ack 引用 response 的 frame SHA/cursor 时，保留原 response，直到 Ack 编码和提交完成。

C++ 包含 `morrow_channel_v1.hpp`。`directory` 和 `response` 为 move-only RAII owner。视图继续借用其 owner；请求的 array/vector 字段拥有数据。

```cpp
using namespace morrow::channel_v1;
auto metadata = directory::decode(input_bytes);
mp_channel_directory_view view{};
if (metadata.view(view) != MP_CODEC_OK || selected_index >= view.channel_count)
    return failure;
const auto& endpoint = view.channels[selected_index];
request operation;
operation.kind = MP_CHANNEL_RECEIVE;
operation.call_id = call_id;
std::copy_n(endpoint.reference.data, 32, operation.reference.begin());
std::copy_n(endpoint.source_epoch.data, 32, operation.source_epoch.begin());
operation.sequence = 0;
operation.credit_bytes = endpoint.budget.max_frame_bytes;
auto reply = response::call(operation);
mp_channel_response_view result{};
if (reply.view(result) != MP_CODEC_OK) return failure;
// 检查 result.status；native 使用 response::call(host, operation)。
```

native callable 通过 `local_adapter<Callable>` 构造 host；C++ 异常在 noexcept callback 内转换为 transport failure。Context 是调用方管理的本地指针，永不进入 IPC 字段。Guest 不保留 host 地址、OS handle 或直接 OS 权限。

## 操作、关联与不确定结果

| 操作 | 请求字段 | 返回语义 |
| --- | --- | --- |
| Receive | last_acked、credit_bytes | 最多一个未 ACK Frame；Idle 表示此次没有 Frame |
| Ack | sequence、frame_sha256、cursor | Acked 记录该 cursor 的收据及 credit 释放 |
| Send | sequence、bytes | Accepted 仅表示接纳发送，不证明 peer 已观察 |
| Query | reference、source_epoch | 显式查看当前状态，不重放业务操作 |
| Close | reference、source_epoch | 关闭请求；实际回收须看 resource_reclaimed |

每项操作都有 call ID、reference、source epoch。Response 必须关联原 Request SHA 和上述字段；SDK 拒绝错误版本、schema digest、非 canonical 编码、越界数据及尾部字节。响应对象的公开字段不赋予任何新权限。

单 wire frame 上限 128 KiB；payload 上限 64 KiB；reference、call ID、source epoch、SHA 为 32 字节；cursor 上限 256 字节。Endpoint budget 可进一步收紧 frame、byte、message、request 和 duration 上限。SDK 不自动重试 Send、Ack、Receive 或业务操作。

Transport failure、坏回复、Unknown 和 ClosingUnconfirmed 都需要应用显式处理。它们不证明回滚，也不授权重放。调用方可按自己的预算发新的 Query 来核对状态。明确的 Idle 或 Limit 也保留为状态，不被 SDK 静默转换为成功。

Cursor Ack 不是业务成功或资源回收。`resource_reclaimed` 只报告 owning runtime 的实际资源回收；即使 Unknown 同时报告该字段为 true，也不能推出先前 Send 或消费成功。Host job 的 run/status/close 生命周期由 host caller API 管理，独立于 guest 输出中的业务状态。Deadline、撤权和 close 后，runtime 负责停止真实 producer 并回收资源。

## 已有 channel.exercise 示例的精确边界

[Rust](examples/rust-channel/src/lib.rs)、[C](examples/c-channel/plugin.c) 和 [C++](examples/cpp-channel/plugin.cpp) 的已构建示例声明同一个专用合同：handler `channel.exercise`，input/output type 均为 `bytes`，输入最多且恰为 65 字节，输出恰为 64 字节。

| 示例输入 | 格式 |
| --- | --- |
| mode | 1 字节：0 为 byte-stream consumer，1 为 synthetic producer，2 为 events subscription |
| reference | 接下来的 32 字节 |
| source_epoch | 最后的 32 字节 |

`CHV1` 示例摘要包含 mode、原始 status、resource_reclaimed、frame 数量、byte 数量和 payload 串的 SHA-256。Consumer/subscription 最多处理 32 帧；producer 示例发送 5 个 32768 字节的确定性样例帧。该 producer 模式有自己的测试数据生成规则，不能替代 caller 显式提供生产数据。

在演示该已声明 handler 时，caller 可以显式选择相容的单一 endpoint，再按以上专用格式构造 65B 输入。该适配必须同时确认 handler、类型、长度、kind 和显式 mode。它不是 SDK 的自动行为，不能应用到其他 handler。通用 Directory-input 插件使用自己的 manifest 声明及 typed input，直接解码 prepare 返回的 Directory，不经过这个示例格式。

新增业务插件或 Directory-input 示例应使用新的 source/project/package 身份。已经用于 SDK014 资格的原始包、snapshot、摘要和失败记录保留原样；文档和应用级 input 选择不更改 `channel.capnp` 或其 SHA。

## 独立 Directory consumer 示例

新的 [Rust](examples/rust-channel-directory/src/lib.rs)、[C](examples/c-channel-directory/plugin.c) 和 [C++](examples/cpp-channel-directory/plugin.cpp) 声明 `channel.directory.consume`。输入类型为 `morrow.channel.directory.v1`，输入就是 canonical Directory 字节，上限 65536 字节；输出类型为 `bytes`，上限 64 字节。这些示例不接受旧 65B 格式。

示例自己的 endpoint 策略明确要求 Directory 恰好含一个通道；byte stream 输出 mode 0，events 输出 mode 2。每帧先消费 bytes、更新完整 payload 串的 SHA，再 ACK 原 sequence/frame SHA/cursor。调用身份由示例的 domain、fresh scope/ref/epoch 及本次 counter 的 SHA 组成；无数据生成或业务重放。示例最多消费 `min(32, max_messages, (max_requests - 1) / 2)` 帧，并为一次 Close 留出 request 预算。

这组示例显式采用 64B `CHV1` 摘要合同。计数和 SHA 描述本地实际消费的数据；ACK 失败或 Unknown 会保留原 status。达到示例的帧数边界、遇到 Idle 或关闭通道都不能单独证明 caller 的全部业务输入已被消费，caller 必须结合自己的预期 byte/message 数、摘要和 host status 核对结果。新示例的 package 构建及真实执行需在新的 candidate 中单独验证；源码存在不等于已经完成新资格。

## 验证与当前范围

相关来源为 `sdk/rust/src/channel.rs`、`sdk/rust/src/channel/transport.rs`、`sdk/rust/src/channel_ffi.rs`、`sdk/c/include/morrow_channel_v1.h` 和 `sdk/cpp/include/morrow_channel_v1.hpp`。已有黄金与 native tests 覆盖 schema/correlation、Directory 所有权、alias、完整输出容量、C++ 异常隔离及真实 Windows guard page 下的短 descriptor 拒绝。

本接口只消费 caller 显式提供且已由 host 绑定的本地 bytes/events。插件类型和 Directory 不引入网络、云、路径发现或额外数据源权限。实际生产 caller 的 prepare/run/status/close 实现和验证由 host/runtime 的对应合同与证据记录说明。

## 有界 reusable transport（2026-10-02 重建）

`channel::transport::WasmClient` 在 `wasm-guest` 下持有一个不超过 `MAX_WIRE_BYTES` 的输出 buffer，可在同一次受限 guest 执行内显式重复调用 `call(&request)`。每次仍只提交一次，并返回拥有自身数据的 Response；重用 buffer 不授予、续期或恢复通道，也不自动重试。旧 `call_wasm` 保留 one-shot 行为。

Native `Client` 仅在首个有效请求时分配输出，保留构造与非法请求的错误优先级。输入编码和其 SHA 在 callback 前固定，输入／输出不 alias；native callback 可能写满整个 capacity，所以所有成功和错误路径均清空完整 scratch。受信 Wasm import 只写返回长度对应的 prefix，只有该路径可在 owned decode 后清空 prefix。公开 `Response::validate_for` 仍先验证响应，再按原短路顺序验证 identity，最后计算原 request digest。

新生成 channel 项目保留标准执行预算 20M fuel／16MiB／16 calls；Rust channel 新项目采用 opt-level3。旧 guest、示例原包、schema/ABI/version/pin 不覆盖。此次 native SDK 91 项测试及 wasm-guest/wasm-c 编译是局部新证据，真实 Wasmi import/fuel 与 Windows 产品资格单独验证。
