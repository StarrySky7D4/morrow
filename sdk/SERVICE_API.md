# 实验入站服务 SDK v1

2026-09-26：C11、C++17、Rust 的有界服务请求解码与关联响应编码，复用既有 `core/schemas/service.capnp`。本接口不纳入 `guest-v1-rc1` 冻结候选。完整 SDK、原生 ABI、全平台支持均未冻结。

## 调用路径

宿主批准包、绑定实际实例、认证远端、确定路由并签发服务与监听授权后，将服务 Request 交给 guest。guest 经 `morrow_task_v1.read_input` 读取一次，构造 Response，经 `morrow_task_v1.complete` 完成一次。服务帧不是普通 Task Invocation，不应交给 `read_task` / `complete_output`，也不应通过 `morrow_io_v1.call` 转发。

| 语言 | 入口 | 生命周期 |
| --- | --- | --- |
| Rust | `service::{Request, Invocation, Reply, Response}`、`wasm::{read_service_request, complete_service_response}` | 请求拥有原始帧与摘要；响应按该请求编码 |
| C | `morrow_plugin_service.h`、`mp_service_request_decode/get/free`、`mp_service_response_encode` | 不透明请求句柄拥有全部视图；输入缓冲可在解码后释放，视图在句柄释放后失效 |
| C++ | `morrow_plugin_service.hpp`、`morrow::service_request` | 不可复制、可移动的 RAII 句柄；`read` / `complete` 提供 Wasm 接线 |

C 的 reply descriptor 仅在同步调用期间借用；输出与输入／descriptor 必须按头文件约定保持有效、对齐且不重叠。编码失败清零输出长度并保留输出缓冲。C/C++ 类型和指针不是 wire 身份或能力。

`principal` 是可信宿主在认证后写入的事实，不能从任意网络字节自行认证。SDK 不提供 Store、监听 socket、发布授权、密钥、端点批准或远端 principal 选择接口。guest 校验是开发便利，宿主仍独立验证所有帧和权限。

## 原帧、限额与失败

响应同时绑定精确 UInt64 `callId` 与完整原始请求帧 SHA-256。语义相同、Cap’n Proto 分段不同的请求也具有不同摘要；不能重编码请求后替代原件。重复头、二进制正文及非 ASCII 头值保持原顺序和字节。

- 帧上限 128 KiB，正文上限 64 KiB；最多 64 个头，累计头预算 16 KiB（包含每项四字节分隔成本），字段与嵌套解码同核心限额。
- 支持 GET、HEAD、POST、PUT、PATCH、DELETE、OPTIONS；目标必须是有界 origin-form，拒绝绝对 URL、非法转义、控制字符等。
- 入站拒绝 Authorization、Cookie 及传输控制头；响应同样禁止伪造 Content-Length、Connection 等传输头。真实节点在送入 guest 前移除宿主负责的敏感头。
- 响应状态为 200–599；204、205、304 不允许正文。错误 schema、尾随字节、畸形输入、错误关联和超限均拒绝。
- 编码成功不证明响应已送达远端；完成错误不推断业务回滚，不自动重发。

本轮只新增同步有界服务 SDK，不新增流式响应、异步续接、内容与出站组合 profile、TLS 配置或持久 Unknown 恢复接口。

## 创建服务插件

```sh
python tool/morrow_plugin.py new build/my-service --language rust --kind service --id org.example.my-service
python tool/morrow_plugin.py pack build/my-service
# --language c / cpp 使用对应模板；打包时可显式指定 --sysroot。
```

生成模板回显二进制正文，声明 `service.echo` handler 和恰好 `http-listen` / `http-publish`。`build.kind = "service"` 与 `[io]` 声明成对出现。核心打包器要求显式 `--service`，固定现有服务 schema 摘要；普通出站 IO 项目不能隐式升级为监听服务。

模板仍为有限 IO profile：30 秒声明上限、单作业、1 MiB 累计预算，不声明 `service-run-v1`。它适合验证短期节点调用，**不等于已有工作台长时服务运行入口所需的完整包配置**。批准、路由、TLS、认证和运行租约须由宿主另行管理，打包不授予能力。重复运行不是恢复旧租约。

## 本地验证

```sh
python tool/verify_plugin_service_sdk.py --sysroot /absolute/path/to/wasi-sysroot
# 首次缓存依赖可显式添加 --allow-network；不使用 Actions/CI。
```

该入口在新目录生成、构建、检查三语言原包，在真实 loopback TCP 节点运行七种方法和权限拒绝，并保存包摘要及日志；缺少工具或原包时失败，不伪造通过。每个 Rust workspace 使用独立目标目录。`scope.json` 明列平台排除项。

另有 `plugin_runtime/tests/sdk_service_codec.rs` 验证核心与 SDK 双向互操作及不同分段原帧关联；其中原生 C/C++ 检查需先将 `sdk/tests/c_service.c`、`sdk/tests/cpp_service.cpp` 链接当前 SDK，再设置 `MORROW_SDK_SERVICE_NATIVE_C/CPP`，显式运行 ignored 用例。完整命令与本次实际结果见 [验证报告](../reports/plugin-service-sdk-2026-09-26.md)。Windows 专属冻结依赖门槛仍须在 Windows 运行。
