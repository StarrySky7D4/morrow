# 实验入站服务 SDK v1

2026-09-26：C11、C++17、Rust 的有界服务请求解码与关联响应编码，复用既有 `core/schemas/service.capnp`。本接口不纳入 `guest-v1-rc1` 冻结候选。完整 SDK、原生 ABI、全平台支持均未冻结。

## 调用路径

宿主批准包、绑定实际实例、认证远端、确定路由并签发服务与监听授权后，将服务 Request 交给 guest。guest 经 `morrow_task_v1.read_input` 读取一次，构造 Response，经 `morrow_task_v1.complete` 完成一次。服务帧不是普通 Task Invocation，不应交给 `read_task` / `complete_output`，也不应通过 `morrow_io_v1.call` 转发。

| 语言 | 入口 | 生命周期 |
| --- | --- | --- |
| Rust | `service::{Request, Invocation, Reply, Response}`、`wasm::{read_service_request, complete_service_response}` | 请求拥有原始帧与摘要；响应按该请求编码 |
| C | `morrow_plugin_service.h`、`mp_service_request_decode/get/free/digest`、`mp_service_response_encode` | 不透明请求句柄拥有全部视图；输入缓冲可在解码后释放，视图在句柄释放后失效 |
| C++ | `morrow_plugin_service.hpp`、`morrow::service_request` | 不可复制、可移动的 RAII 句柄；`read` / `complete` 提供 Wasm 接线 |

C 的 reply descriptor 仅在同步调用期间借用；输出与输入／descriptor 必须按头文件约定保持有效、对齐且不重叠。编码失败清零输出长度并保留输出缓冲。C/C++ 类型和指针不是 wire 身份或能力。

`principal` 是可信宿主在认证后写入的事实，不能从任意网络字节自行认证。SDK 不提供 Store、监听 socket、发布授权、密钥、端点批准或远端 principal 选择接口。guest 校验是开发便利，宿主仍独立验证所有帧和权限。

## 原帧、限额与失败

响应同时绑定精确 UInt64 `callId` 与完整原始请求帧 SHA-256。Rust `Request::digest()`、C `mp_service_request_digest`、C++ `service_request.digest()` 返回这份原件摘要；C 输出须至少 32 字节，失败保留原缓冲。语义相同、Cap’n Proto 分段不同的请求也具有不同摘要；不能重编码请求后替代原件。重复头、二进制正文及非 ASCII 头值保持原顺序和字节。

- 帧上限 128 KiB，正文上限 64 KiB；最多 64 个头，累计头预算 16 KiB（包含每项四字节分隔成本），字段与嵌套解码同核心限额。
- 支持 GET、HEAD、POST、PUT、PATCH、DELETE、OPTIONS；目标必须是有界 origin-form，拒绝绝对 URL、非法转义、控制字符等。
- 入站拒绝 Authorization、Cookie 及传输控制头；响应同样禁止伪造 Content-Length、Connection 等传输头。真实节点在送入 guest 前移除宿主负责的敏感头。
- 响应状态为 200–599；204、205、304 不允许正文。错误 schema、尾随字节、畸形输入、错误关联和超限均拒绝。
- 编码成功不证明响应已送达远端；完成错误不推断业务回滚，不自动重发。

SDK 保持同步有界；历史查询、续租、监听与授权由可信宿主管理。流式响应、通用异步续接和完整业务核对仍是后续范围。

## 创建服务插件

```sh
python tool/morrow_plugin.py new build/my-service --language rust --kind service --id org.example.my-service
python tool/morrow_plugin.py pack build/my-service
# --language c / cpp 使用对应模板；打包时可显式指定 --sysroot。
```

生成模板回显二进制正文，声明 `service.echo` handler 和恰好 `http-listen` / `http-publish`。`build.kind = "service"` 与 `[io]` 声明成对出现。核心打包器要求显式 `--service`，固定现有服务 schema 摘要；普通出站 IO 项目不能隐式升级为监听服务。

缺省模板保留短期 IO profile。工作台长时入口需要显式有限运行声明：

```sh
python tool/morrow_plugin.py new build/my-long-service --language rust --kind service --id org.example.long-service --service-run-ms 120000 --service-run-jobs 16 --service-run-bytes 4194304
```

等价配置为 `[service_run]` 的 `duration_ms = 120000`、`max_jobs = 16`、`max_bytes = 4194304`。三个值必须同时提供：期限最多 3600000 ms、累计任务保留最多 1000000 次、累计字节最多 64 MiB。打包加入 `service-run-v1` 和 `service-run-budget-v1`，保留原单作业 1 MiB／30 秒上限；宿主通过 `bind_budgeted_service_run` 另行批准，可进一步收紧，不能用普通 `bind_io` 绕过。

续租针对原 Manager、实例和运行修订执行 CAS，增加累计上限不会重置已消费用量；已撤销、已停止、到期的授权不能复活。历史重放也占用本次准入预算，但不重新执行 guest。重开数据库只保留历史，必须重新签发运行与监听授权；Unknown 不自动重发。

## 出站资源目录

`service_resources::Directory::from_headers`（Rust）、`mp_service_request_resources`（C）、`service_request.resources()`（C++）解析宿主注入的可选目录。C 成功返回空句柄、C++ `present()==false` 表示未注入；重复或非法目录明确失败。独立资源句柄拥有所有端点、方法和引用视图，可在原请求释放后继续读取，资源句柄释放后视图失效。

目录最多 8 个端点／4096 字节原帧，保留 scope 摘要、端点及凭据引用、允许方法和各项限额。只接受规范编码；它不包含凭据值，也不产生 IO 授权。

自定义服务可在 `[io]` 中显式加入 `http-request`（需要凭据时另加 `credential-use`），并设置 `service_resources = true`。对应核心选项 `--service-resources` 只在服务且有 HTTP 出站声明时接受。回显模板不会因此自动转发；插件需实现业务调用，宿主仍独立批准并校验每次 IO。

## 入站服务调用受管 HTTP

三语言可选 starter 将两个公共 SDK 接在一起，无需链接宿主核心：

```sh
python tool/morrow_plugin.py new build/my-forward-service --language rust --kind service --service-http --id org.example.my-forward --service-run-ms 120000 --service-run-jobs 64 --service-run-bytes 4194304
python tool/morrow_plugin.py pack build/my-forward-service
```

`--service-http` 仅适用于 `--kind service`，选择 `service.http.forward` handler，显式声明四项能力 `http-listen`、`http-publish`、`http-request`、`credential-use` 及资源目录。`[io].max_resources = 4` 为监听、发布、一个端点和在途 HTTP 分别预留槽位；它是声明上限，不是实时授权。自定义项目可显式设置 1–8，对应核心 `--io-resources N`；未设置时仍为 2，不会因组合能力自动放宽。当前仍为单作业、单请求有界执行。

starter 只接受 POST，要求恰好一个宿主选择的端点且允许 POST，把二进制正文发送到该端点的 `/`。调用者的路径、查询、认证、Cookie、其他头均不转发。凭据只传 opaque 引用，由宿主在实际 HTTP 发送时注入；目录本身不携带秘密。HTTP 完成后返回上游状态与正文，不复制上游响应头。

宿主必须使用持久路由 `durable_route`（工作台配置服务使用对应持久流程），保存原请求并保持 namespace。出站操作 ID 是 `service-http-` 加精确原服务帧摘要；每次 guest 执行只调用一次 HTTP，无自动重试。外层服务幂等键冲突不会产生新业务执行。重开必须重新批准原包及相同资源策略。目录/凭据策略改变应改变 scope，旧记录不能因此重新执行。

| 结果 | 服务状态与正文 |
| --- | --- |
| 不是 POST | `405 post-required` |
| 目录缺失或不是恰好一个端点 | `503 one-endpoint-required` |
| POST 不在目录允许方法中 | `403 method-denied` |
| 正文超出端点输入限额 | `413 request-too-large` |
| IO 返回 OutcomeUnknown | `409 outcome-unknown` |
| IO 返回 Conflict | `409 operation-conflict` |
| IO 返回 Denied / Revoked | `403 outbound-denied` |
| IO 返回 Expired / Quota | `504 outbound-expired` / `429 outbound-quota` |
| 其他 IO 状态 | `502 outbound-unavailable` |

这些是 guest 收到有效 IO 回执时的映射。停止、撤权或宿主完成失败也可能直接终止 guest，由宿主持久历史入口返回 Unknown，不能保证经过上述正文。外层已观察到 `409 outcome-unknown` 响应也不意味着内层 HTTP 结果已核对。应用须保持未知状态并使用提供者证据核对；更换幂等键不是恢复手段。流式与业务级事务恢复仍未实现。

## 本地验证

```sh
python tool/verify_plugin_service_sdk.py --sysroot /absolute/path/to/wasi-sysroot --native
# 三语言服务 → 受管 HTTP，包含等待期间本地写入、停止/撤权及历史重开。
python tool/verify_plugin_service_sdk.py --sysroot /absolute/path/to/wasi-sysroot --native --service-http
# 首次缓存依赖可显式添加 --allow-network；不使用 Actions/CI。
```

该入口在新目录生成、构建、检查三语言原包，在真实 loopback TCP 节点运行七种方法、权限拒绝、续租、耗尽、撤权和重开核对，并保存包摘要及日志；缺少工具或原包时失败，不伪造通过。每个 Rust workspace 使用独立目标目录；`--build-root` 可复用编译缓存，输出证据目录仍必须新建。`--native` 当前强制 Linux C/C++ 原生验证。`scope.json` 明列平台排除项。

另有 `plugin_runtime/tests/sdk_service_codec.rs` 验证核心与 SDK 双向互操作及不同分段原帧关联；其中原生 C/C++ 检查需先将 `sdk/tests/c_service.c`、`sdk/tests/cpp_service.cpp` 链接当前 SDK，再设置 `MORROW_SDK_SERVICE_NATIVE_C/CPP`，显式运行 ignored 用例。完整命令与本次实际结果见 [验证报告](../reports/plugin-service-sdk-2026-09-26.md)。Windows 专属冻结依赖门槛仍须在 Windows 运行。
