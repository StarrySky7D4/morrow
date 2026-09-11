# Morrow 第三方插件 SDK 原型

基于 0.1.9-test.10，运行期协议 v6，实验本地 ABI v1。当前提供 **传输接口与四类内容命令的类型化编解码**，已增加 C／C++／Rust Wasm guest 适配原型，但尚未建立正式插件加载器、完整内容／UI API 或稳定 ABI。设计见 [插件 SDK 与 UI](../docs/PLUGIN_SDK_AND_UI.md)。

| 语言 | 入口 | 使用方式 |
| --- | --- | --- |
| C11 | c/include/morrow_plugin_sdk.h、morrow_plugin_codec.h | C 传输静态库＋Rust 实现的独立编解码动态库；宿主适配器提供消息交换回调 |
| C++17 | cpp/include/morrow_plugin_sdk.hpp、morrow_plugin_codec.hpp | 链接上述两库；request 拥有字符串，decoded_reply 自动释放响应，STL 不跨 ABI |
| Rust | rust/Cargo.toml | 路径依赖 morrow-plugin-sdk；protocol::Request／Action／Reply 与 Client 配合使用 |

SDK 不链接可信核心，不提供创建宿主、授予权限、直接打开 Store 或选择插件身份的入口。C／C++ 编解码复用同一 Rust 实现，因此构建编解码库需要 Rust 工具链；当前没有单独的纯 C 协议实现。Rust 运行期依赖锁定的 capnp，构建使用 capnpc、sha2 和 Cap’n Proto 编译器。当前在 Windows x64 验证，尚未分发各平台预编译包。

## 内容接口

| 命令 | 类型化输入 | 成功响应 |
| --- | --- | --- |
| 重命名卡片 | 请求／操作 ID、卡片 ID、预期修订、标题 | 已提交回执 |
| 读取摘要 | 请求 ID、卡片 ID | 类型、格式版本、修订、标题和预览 |
| 查询操作结果 | 请求 ID、卡片 ID、被查询操作 ID | 当前快照不存在／本地已提交回执 |
| 读取附件片段 | 请求 ID、卡片／附件 ID、预期修订、偏移和长度 | 有界字节、总长度和完整附件摘要 |

修订和偏移保持无损 UInt64。SDK 校验版本与两份 Schema 摘要、请求 ID、响应类型、目标，以及适用的操作 ID、修订和偏移。附件响应仅是片段；必须核对完整长度与 SHA-256 后才可作为完整附件发布。当前 SDK 尚未封装整文件装配与摘要验证。

消息上限 64 KiB，附件片段上限 32 KiB。请求在回调前复制，完整输出空间在提交前准备。业务拒绝也属于正常协议响应：MP_OK／MP_CODEC_OK 不代表获准或提交成功，应检查 Reply／mp_reply_view.kind。查询的“快照不存在”不能证明没有正在执行的操作。

典型调用顺序：构造类型化请求 → encode → Client／mp_exchange → 用原请求 decode_reply／mp_reply_decode → 检查结果。传输或响应校验失败不自动重试，也不能推断回滚；需要按固定操作 ID 查询权威结果。请求 ID 的产生与跨重试保存仍由调用方负责。

C 返回不透明 mp_reply；mp_reply_get 的视图由该响应拥有，释放后不能继续使用。C++ decoded_reply 不可复制，可移动；移动赋值会释放此前响应。Rust 返回自有值。调用示例见 [C 实际核心测试](tests/native_adapter.c)、[C++ 类型化用例](tests/cpp_codec.cpp)、[Rust 协议用例](rust/tests/codec.rs)。C 测试中的宿主打开／授权／缓冲区管理只属于可信测试适配器，不属于 guest SDK。

上下文／回调指针只在适配器本地使用，不是可序列化身份或权限。执行器必须依据实际通道绑定实例并独立校验权限；不可信代码能够绕过 SDK，因此 SDK 校验不是安全边界。同步接口用于测试及后端原型，后续接入有界异步调度，不能阻塞 Flutter 输入线程。

## 构建与验证

需要 Rust、Cap’n Proto 编译器、Python 3、PowerShell 7、Clang／LLVM 以及 wasm32-unknown-unknown 目标。脚本允许用 -Python、-Clang、-ClangXX、-Ar 指定路径。

```powershell
pwsh -File tool/verify_plugin_sdk.ps1
```

固定契约随 Rust SDK 保存，可脱离宿主源码独立构建。修改核心 Schema 后运行 tool/sync_plugin_sdk_contracts.py 更新快照；验证脚本以 --check 防止契约漂移。十份二进制样本由 core/examples/sdk_codec_vectors.rs 独立生成并与提交样本比对，不允许只靠 SDK 自身编码／解码互证。

产物位于 build/plugin-sdk：morrow_plugin_sdk.lib 为 C 传输库；rust/debug/morrow_plugin_sdk.dll 与 .dll.lib 为编解码动态库及导入库；C／C++ 测试程序位于上层。测试数据库和请求／回复全部采用独立合成资料。本轮日志保存在 build/plugin-sdk/verification.log。

## 本轮验证结果

Windows 本机 C11／C++17 严格警告编译、Rust fmt、Clippy -D warnings 与 **15 项 Rust 测试**通过。C++ 用例通过 C ABI 检查四种请求与六种响应、UInt64 最大值、响应所有权、移动语义及非法输入；Rust 检查错误契约、请求关联、截断／超限和嵌套回执。上述原生测试之外，三语言 Wasm 实际执行结果见下一节和执行后端说明。

真实链路已验证：**C 类型化 SDK → 可信测试适配器 → Rust DLL → HostRuntime → SQLite**。C 自行编码请求并解码真实回复；缺权限时收到 Denied，授权后提交修订 2，重复提交返回完全一致的回执。独立核心解码器再次检查请求和响应，关闭后核心缓冲区为零，SQLite 完整性检查通过。响应句柄释放路径已执行，未进行专门的内存泄漏检测。

后续已增加 [C／C++／Rust Wasm 实际执行验证](../plugin_runtime/README.md)：三种语言独立编译的 SDK 示例在 Windows Wasmi 后端运行并接入核心。本节原生测试本身不证明 Wasm 执行；其他系统、其他后端和插件 UI 仍未验证。通用记录命令、完整包管理、多实例持久任务与执行后端在 M1-05／M3-06 补齐，UI 渲染器在 M6-06 推进。应用版本、消息协议与 ABI 分别管理兼容性；本轮不修改应用版本或发布 Release。

## Rust Wasm 示例

启用 rust/Cargo.toml 的 wasm-guest feature，使用 wasm::host() 构造固定导入的 HostV1，再调用现有 Client 和类型化协议 API。可编译样例位于 examples/rust-rename；当前固定操作 ID 仅用于合成去重测试，正式插件必须从宿主管理的任务契约取得并保存操作身份。

```powershell
cargo build --locked --manifest-path sdk/examples/rust-rename/Cargo.toml --target wasm32-unknown-unknown --release --target-dir build/plugin-guest
pwsh -File tool/verify_plugin_runtime.ps1
```

Wasm guest ABI v1 与原生本地回调 ABI v1 分别管理，两者不共享指针或身份；运行消息仍为固定 Cap’n Proto v6。取消／执行错误不自动重试，已提交状态由核心查询确定。

## C／C++ Wasm 示例

C 示例使用 morrow_plugin_wasm.h 的 mp_wasm_host()、既有 mp_exchange 和类型化编解码；C++ 示例继续使用 std::string／std::vector、morrow::request／client／decoded_reply，不需改写成 C 接口。示例分别位于 examples/c-rename 和 examples/cpp-rename。该 profile 暂不支持 C++ 异常和 RTTI；禁用异常时，SDK 错误访问及标准库致命错误会在 guest 内 trap，宿主随后按操作 ID 核对结果。

```powershell
pwsh -File tool/prepare_plugin_c_wasm.ps1
pwsh -File tool/build_plugin_c_wasm.ps1
pwsh -File tool/verify_plugin_runtime.ps1
```

准备脚本将固定的官方 WASI SDK 34 sysroot 下载到 build/tools，并验证 SHA-256。采用现有 LLVM 22 的 Clang，显式选择 wasm32-wasip1/noeh 的头文件和库；WASI 只作为标准库构建来源，最终模块不获得 WASI 导入。C++ 标准库的终止诊断钩子改为 guest 内 trap，权限边界不随库链接扩大。依据：[官方工具链](https://github.com/WebAssembly/wasi-sdk)、[无异常标准库说明](https://github.com/WebAssembly/wasi-sdk/blob/wasi-sdk-34/CppExceptions.md)。

Rust SDK 的 wasm-c feature 可构建为静态编解码库；构建脚本将其与 C／C++ 源码、固定消息导入和 guest 运行支持一起链接。malloc／calloc／realloc／free 和对齐分配统一交给同一 guest 内的 Rust 分配器，不能混用两套堆。C++ 标准分配失败在此 profile 中终止 guest；C malloc 失败返回 NULL。分配器指针仅在该模块内部使用，不是跨进程句柄或宿主能力。

最终 C／C++ 示例位于 build/plugin-c-guest，默认去除调试符号；构建时使用 -DebugSymbols 可保留符号。构建产生裸模块；统一的实验打包工具另行封装 manifest 并支持不可变安装，见 [插件包开发流程](../docs/PLUGIN_PACKAGE.md)。作者签名、启用／更新管理与 UI 仍未完成。文件、网络、时钟、线程等接口仍须通过后续明确的宿主能力提供；不能把部分标准库成功运行宣称为完整 WASI 或全 C++ 标准库支持。

## 动态任务 SDK（guest ABI v2）

已提供 C 的 morrow_plugin_task.h、C++ 的 morrow_plugin_task.hpp 与 Rust task／wasm API，接收宿主提供的任务、读取类型化命令并构造关联完成消息。实际例子位于 examples/c-task、cpp-task、rust-task。完整说明见 [任务契约](../docs/PLUGIN_TASK_PROTOCOL.md)，结果见 [三语言动态任务验证](../reports/plugin-task-contract-validation.md)。

内容 profile 校验固定命令与实际核心回复，支持四类内容接口。任务契约 v2 还提供纯转换输入／输出：Rust transform／complete_output、C get_transform／output、C++ task::transform／output；示例见 examples/rust-transform、c-transform、cpp-transform。三语言各 8 次真实转换与新产物摘要见 [转换验证](../reports/plugin-transform-validation.md)。旧任务 schema 摘要的包须重建。跨包 handler 选择、类型协商、修改提案、UI 动作和持久恢复仍需扩展。不要求第三方编写 Dart，暂不提供 TS／JS guest。

纯转换示例现在必须用 `pack-transform` 声明处理器名称、输入／输出类型及各自上限，见 [统一打包入口](../docs/PLUGIN_PACKAGE.md#纯转换处理器声明)。声明由宿主验证，不需要给三语言 guest 导出授予权限或核心注册函数。仅改用旧 pack-task 打包不能绕过注册检查。

任务契约 v3 提供 Rust complete_failure、C mp_task_fail、C++ task::fail，错误含固定代码及最多 1024 字节纯文本。bytes.require-ascii 示例贯通三语言业务失败返回；须区分完整执行与业务成功。见 [结果协议](../docs/PLUGIN_TASK_PROTOCOL.md)。旧任务 schema 的包需同步 SDK 后重建。
