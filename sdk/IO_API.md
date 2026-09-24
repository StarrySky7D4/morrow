# 实验性 IO SDK

2026-09-24：C11、C++17、Rust 增量；协议为现有 IO v1。此接口尚未冻结，独立于 guest-v1-rc1 的内容／任务／UI／依赖兼容基线。源码 SDK 版本仍为 test.50，本轮没有发布新的 SDK 包。

## 能力与权限

| 动作 | SDK | 当前核心／宿主边界 |
| --- | --- | --- |
| Read | 编码、解码、调用及关联响应 | 读取宿主授予的 32 字节不透明文件引用；没有路径打开权限 |
| Finish／Cancel | 编码、解码、调用及关联响应 | 受管资源的结束／取消；不是取消任意网络任务的通用句柄 |
| SubmitHttp | 方法、相对目标、头、正文、端点与凭据引用、操作号、期限 | 对接现有受管 HTTP；不得自选 URL authority 或直接传入秘密 |
| SubmitFileRead／Poll／QueryOperation | 实验 schema 编码形状 | 当前核心解码为 Unsupported，不能依赖其创建任务或恢复操作 |
| Write、目录操作、监听、SSE、WebSocket | 未提供 | 仍需设计、实现及独立验证 |

编码成功不授予权限。Manifest 的 IO 声明、Manager 明确批准、当前实例、包摘要、代次和绑定预算仍由宿主检查；绕过 SDK 的 guest 也必须经过同样的宿主边界。跨实例引用、撤权及失效绑定不能借缓存响应恢复权限。

## 调用生命周期

现有受管载体把 **原始 IO 请求帧** 作为任务输入。它不是普通转换任务的 Invocation；不能把它交给 `read_task` 或 `mp_task_decode`。

1. 从任务输入读取并校验 IO 帧。
2. 调用一次 `morrow_io_v1.call`，保留原请求字节。
3. 校验响应版本、schema 摘要、callId、原请求帧 SHA-256，以及当前动作允许的字段和长度。
4. 检查业务 status。完成任务时交回经验证的原始响应帧，宿主再次验证其来源和可交付性。

实际受管载体可要求调用与宿主输入完全相同。SDK 编码器不表示 guest 可以在一个输入内任意追加文件读取、HTTP 调用或自行开启循环。示例采用一帧一次调用；分块和后续动作由宿主管理。

`Completed` 与 HTTP 成功不是同义词：HTTP 429／500 仍可正常收到状态、头和正文。`Denied`、`NotFound` 等是合法业务结果，不能当作解码错误。传输失败、坏回执或取消不证明远端回滚；SDK 不自动重发。业务 operationId 与本次 callId 分开管理，Unknown 的跨重启核对仍是后续工作。

## 三语言入口

Rust 使用 `morrow_plugin_sdk::io::{Request, Action, Response}`；启用 `wasm-guest` 后提供 `wasm::read_io_request`、`call_io`、`complete_io_response`。只调用原有 SDK 的插件不会因此增加 IO 导入。最小完整示例见 [rust-io](examples/rust-io/src/lib.rs)。

```rust
let request = morrow_plugin_sdk::wasm::read_io_request()?;
let response = morrow_plugin_sdk::wasm::call_io(&request)?;
// response.bytes 是载荷；encoded_frame() 是已验证的完整响应帧。
morrow_plugin_sdk::wasm::complete_io_response(&response)?;
```

上例置于返回 SDK Result 的函数中；导出的 `morrow_run` 仍须转换为 i32。Response 展示字段是自有值；改写这些字段不会改写 `encoded_frame()`，完成边界只提交原始验证帧，不能用修改后的展示值伪造完成结果。

C 包含 `morrow_plugin_io.h`，使用 `mp_io_request_v1`、`mp_io_request_encode`、`mp_io_response_decode/get/free`；Wasm 下提供 `mp_wasm_io_call_frame` 和 `mp_wasm_io_call`。`MP_CODEC_OK` 后仍检查 `view.status`。响应句柄拥有载荷、头及 encoded_frame；视图借用至 free，不能跨释放保留指针。参见 [c-io](examples/c-io/plugin.c)。

C++ 包含 `morrow_plugin_io.hpp`，使用 `morrow::io_request` 和 `morrow::io_response`；后者不可复制、支持移动，负责释放 C 句柄。`view()` 仅在 codec status 成功后调用。Wasm profile 保持无异常、无 RTTI。参见 [cpp-io](examples/cpp-io/plugin.cpp)。

C/C++ 本地 ABI 不传递 STL 对象；调用方仍负责指针、长度、对齐、句柄生命周期及不重叠缓冲的有效性。它是本地库约定，不是隔离恶意本机代码的安全机制。

## 限额

IO 帧最多 128 KiB，载荷最多 64 KiB，引用恰好 32 字节；最多 64 个头、头总长 16 KiB、单个头名 128 字节／值 8 KiB。方法最多 16 字节，相对目标最多 2048 字节，端点及操作号最多 256 字节，凭据引用最多 4 KiB，提交期限最多 30 秒。这是协议／codec 上限；宿主绑定可施加更低限额。

拒绝非相对目标、非法方法／头、调用方注入 Authorization／Cookie／Host 等宿主管理字段、过大消息、尾随字节、未知动作和不关联回执。声明式字段不包含本机地址、句柄、秘密或权限授予。

## 验证与复现

```powershell
pwsh -File tool/verify_plugin_sdk.ps1
pwsh -File tool/verify_plugin_io_sdk.ps1 -Sysroot "实际的/wasi-sysroot-34.0"
```

第二条先核对旧插件原件，再构建 Rust／C／C++ 示例并显式运行 `sdk_io_guest` 的五个测试；每个测试覆盖三种语言，缺任何一个产物都会失败。普通 Cargo 测试将这些依赖编译产物的用例显示为 ignored，不把它们误计为已验证。

本轮完成 Windows 上的真实 FileBroker／Manager 授权、Read／Finish、无批准、跨实例引用和未知引用测试；HTTP 是核心编码的 429 响应穿过真实 Wasm runner，验证头、正文和错关联拒绝，**不是公网请求或完整网络传输验收**。旧 SDK 原件继续保留。资格记录见 [本轮报告](../reports/plugin-io-sdk-2026-09-24.md)。

项目生成器已支持 `new --kind io --language rust|c|cpp`；默认 file-read，需 HTTP 时显式传 `--io-capability http-request`，使用宿主凭据时再加 `--io-capability credential-use`。HTTP 模板声明工作台要求的 `morrow.http.forward.v1`，只声明网络能力不足以获得该入口兼容资格。宿主仍负责批准、端点／凭据和绑定；打包和静态准备不授予权限。完整命令与固定声明预算见 [项目工具](../docs/PLUGIN_PROJECT_TOOLS.md)。完整 IO SDK 冻结仍需网络与文件系统余项、独立第三方接入、异步／恢复协议和跨平台验收。

## 后续真实网络验证（2026-09-24）

在上述 codec／文件验证后，新增 `tool/verify_plugin_io_network.ps1`：三语言 guest 经现有受管 worker 发出真实本地 HTTP，七种方法、重复参数和头、二进制正文、宿主凭据注入、429、发送前拒绝、断线 Unknown 与同操作不重发均通过。关闭并重开数据库后仍能读取 Unknown；这不等于完整跨进程业务核对。范围和复现见 [网络资格报告](../reports/plugin-io-network-sdk-2026-09-24.md)。公网服务商互操作、入站服务 SDK、流式和完整恢复仍开放。
