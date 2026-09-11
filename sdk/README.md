# Morrow 第三方插件 SDK 原型

基于 0.1.9-test.10。当前是 **可编译的传输基础接口**，尚未建立正式插件加载器、Wasm guest imports、完整类型化内容／UI API 或稳定 ABI。设计见 [插件 SDK 与 UI](../docs/PLUGIN_SDK_AND_UI.md)。

| 语言 | 入口 | 使用方式 |
| --- | --- | --- |
| C11 | c/include/morrow_plugin_sdk.h | 编译 c/src/morrow_plugin_sdk.c 为静态库；宿主适配器提供只负责交换消息的回调 |
| C++17 | cpp/include/morrow_plugin_sdk.hpp | 链接 C 库；morrow::client 管理响应，STL 不跨 ABI |
| Rust | rust/Cargo.toml | 路径依赖 morrow-plugin-sdk；Client 显式绑定 HostV1 生命周期，不可跨线程共享 |

发送的是预编译 Cap’n Proto 消息，SDK 不使用 JSON，也不直接链接可信核心。当前长度上限 64 KiB；请求在回调前复制，完整输出空间在提交前准备。业务拒绝也会收到正常协议响应，所以 MP_OK 仅表示收到回复。响应仍须解码、检查契约和请求关联。所有传输失败均不自动重试，不能推断没有提交。

上下文／回调指针属于适配器本地，不是可序列化身份或权限。真正执行器必须根据实际通道绑定实例并独立校验权限；不可信代码能绕过 SDK，所以 SDK 校验不能作为安全边界。同步接口当前只用于测试及后端原型，后续接入有界异步调度，不能放在 Flutter 每帧工作中阻塞输入。

```powershell
pwsh -File tool/verify_plugin_sdk.ps1
```

当前脚本在 Windows 验证 C 静态库、C++ 封装、Rust 单元测试及 wasm32-unknown-unknown 编译检查，并用可信测试适配器把 C SDK 接到实际 Rust DLL／SQLite。tests/native_adapter.c 中的宿主打开、授权与缓冲区管理只属于测试宿主，不包含在 guest SDK 中。

产物位于 build/plugin-sdk，静态库为 morrow_plugin_sdk.lib，测试程序为 c_transport.exe／cpp_transport.exe／native_adapter.exe，Rust 产物在 rust 子目录，日志为 verification.log。测试数据库与请求／回复是独立合成资料。当前未验证 C／C++ Wasm 编译、Rust Wasm 实际执行、其他操作系统或插件 UI；测试通过不等于完整插件系统建立。

开发者便捷 API、示例插件与各语言绑定生成将在 M1-05／M3-06 补齐；UI 渲染器在 M6-06 推进。SDK 和应用版本、内部协议、guest ABI 需分别记录兼容关系，当前接口可随测试阶段变更。

## 本轮验证结果

Windows 本机：C11／C++17 严格警告编译通过；Rust fmt、Clippy -D warnings 与 5 项单元测试通过。Rust wasm32-unknown-unknown 仅编译检查通过。

真实链路已验证：C SDK → 可信测试适配器 → Rust DLL → HostRuntime → SQLite。缺权限时收到 Denied；宿主授权后提交修订 2，同命令再次提交返回完全一致的回执；独立 Rust 解码器核对请求关联、错误和修订，关闭后缓冲区为零，SQLite 完整性检查通过。C++ 目前验证包装及 C ABI 传输，Rust SDK 目前验证测试适配器；不据此宣称三种语言的真实插件已运行。
