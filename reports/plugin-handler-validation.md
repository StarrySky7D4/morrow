# 单包处理器注册验证

日期：2026-09-11。Windows x64，应用基线 0.1.9-test.10；任务契约 v2、guest ABI v2、内容消息 v6、包 schema v1。新增必需功能 transform-handlers-v1，主 Flutter 数据后端未切换。

## 实现与真实执行

Protobuf manifest 声明最多 16 项处理器，每项包含名称、输入／输出类型和各自的字节上限。处理器与完整包摘要绑定。声明与必需标记不成对、重复名称、非法类型名和超过全局上限的声明拒绝加载。旧的无声明内容任务包仍可加载，但纯转换任务在执行前拒绝。

统一打包工具 pack-transform 将两个示例处理器写入 Rust／C／C++ 包，inspect 可以读取声明。三种实际 Wasm 模块分别验证未注册名称、错误输入类型和错误输出类型，共 9 次拒绝；结果均为 TaskProtocol、全部 fuel 保留、零宿主调用、无输出或核心回复。随后每种语言在原生 Worker 中执行 8 次纯转换，共 24 次，通过空输入、UTF-8 原字节、全字节值和 64 KiB 数据。可信宿主独立计算并核对结果，关闭后重新打开 SQLite，原内容与事件保持不变。

## 检查范围

- 核心格式、严格 Clippy、109 项默认测试、7 项故障恢复、自检及 wasm32 核心检查通过。
- 包测试现为 11 项，覆盖声明完整性、重复／未知功能、非法名称、16／17 项边界、零长度上限、包内查询、类型匹配和声明变更导致包摘要变化。
- 运行库 packages 配置 33 项测试通过，SDK 14 项回归通过。新执行测试区分执行前拒绝与执行后输出超限：前者保留全部 fuel，后者消耗 fuel 但没有可交付结果。
- 原有实际内容任务、授权／撤权、后台队列、ABI v1 包和 C++ trap 回归通过。
- core-web 的实际 wasm32 配置检查通过；没有运行浏览器插件或其他平台插件，也没有接通 Flutter 插件 UI。

日志：build/core-test.10/handler-verification.log、build/plugin-runtime/handler-verification.log。

复现：tool/verify_core.ps1 -Web；tool/verify_plugin_runtime.ps1；cargo check --locked --manifest-path core-web/Cargo.toml --target wasm32-unknown-unknown --target-dir build/core-web-package-check。

## 本次重新打包产物

包目录：build/plugin-packages/88039e65a1854af490abd57b0a541e89。SHA-256 为实际字节摘要，不是作者签名。此次修改包声明和可信宿主，三语言任务编解码接口未改变。

| 包 | 字节 | SHA-256 |
| --- | ---: | --- |
| c-transform.mplugin | 43616 | `787fb05fa9d9b602c434378bcd2b83d6ed2ec9070305b6d2e57230af305d2be6` |
| cpp-transform.mplugin | 44430 | `a093a21d96ff2f3f23d949610bef1adaf6820ac4e620ff087843013d946554ea` |
| rust-transform.mplugin | 52162 | `bf76cad28bfff6f6499a08e754f6a3bfafc48ce0ed4825f525e3a878189381e5` |

## 仍未完成

当前是单包处理器声明与执行匹配，不是跨包自动选择、启用管理、依赖解析或类型版本协商。声明不证明算法实现正确、不授予权限；纯转换继续禁止内容调用。类型化插件错误、修改提案、持久恢复、声明式 UI 及全平台资格仍需推进。未推送、未发布 Release。
