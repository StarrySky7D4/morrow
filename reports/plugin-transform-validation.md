# 三语言纯转换任务验证

日期：2026-09-11；Windows x64，应用基线 0.1.9-test.10。任务契约 v2，guest ABI v2，内容消息 v6，包 schema v1。主 Flutter 数据后端未切换。

## 实际执行

Rust、C11、C++17 的独立 Wasm 模块均重新编译、打包并由原生 Worker 执行。每种语言运行两个 handler（bytes.reverse、bytes.ascii-uppercase），分别处理空输入、含中文及 emoji 的 UTF-8 原字节、0–255 全字节值和 64 KiB 输入，共 24 次纯转换。输入／输出类型均为 bytes；不将字节反转或 ASCII 转换声称为 Unicode 文本算法。

可信测试宿主独立计算预期结果并逐字节核对。每次执行返回 0、host_calls 为 0、response 为空且 output 类型／内容符合预期。宿主时钟回调设置为进入即失败，进一步证明没有调用内容派发。任务排空、连接停止后重新打开 SQLite：种子卡片原内容保留，仅有一条原创建事件，完整性通过。

实际执行只证明这些示例算法及本次输入。运行时 verify_output 核验关联、格式、类型与上限，不证明任意第三方计算正确；输出不是核心提交回执。

## 回归与权限边界

- 核心格式、严格 Clippy、107 项默认测试、7 项故障恢复、自检及原生／wasm32 构建通过。
- 运行库 packages 配置 32 项测试通过；新增 3 项覆盖空／二进制／最大长度、超限、输出类型、旧版本、任务 ID、内容／转换混用，以及真实授权实例尝试越过纯转换限制。
- 恶意模块即使拥有卡片重命名授权，纯转换中的 exchange 仍在进入核心前被拒绝；无提交、无输出、无权威回复。完成后 trap 和预先取消同样不交付输出。
- 14 项 SDK 测试、C／C++ 原生严格编译、独立核心消息向量及 Rust DLL／SQLite 授权链路通过。
- 原有 ABI v1 包与后台任务、三语言各 8 项内容任务、C++ 失败／内存耗尽 trap 回归通过。
- core-web 的实际 wasm32 feature 配置检查通过；这不证明浏览器插件执行、Flutter UI 或其他平台运行。

复现：tool/verify_core.ps1 -Web、tool/verify_plugin_runtime.ps1、tool/verify_plugin_sdk.ps1，以及 cargo check --locked --manifest-path core-web/Cargo.toml --target wasm32-unknown-unknown --target-dir build/core-web-package-check。

日志：build/core-test.10/transform-verification.log、build/plugin-runtime/transform-verification.log、build/plugin-sdk/transform-verification.log。

## 本次产物

包目录：build/plugin-packages/9edeea8dfce14294aaed6081c340bc6e。下表是实际字节 SHA-256，不能代替作者签名；历史报告保留其原构建摘要。

| 包／模块 | 字节 | SHA-256 |
| --- | ---: | --- |
| c-rename.mplugin | 42164 | `3c4fca5578b6619105aa80bc5d2d2d4a845254d32c12277a2dafbc82633b5d3d` |
| c-task.mplugin | 49205 | `b093e89d699688f850204600ce68f5ffd3fc3a4e426a7521ba1805e7ee883bb0` |
| c-transform.mplugin | 43540 | `bb544f9abcbe36fab4530a1c7e6c5d283a53467001271764e429f1e6996cc112` |
| cpp-allocator.mplugin | 44626 | `2bfeb1c324207fecb53388dae4fd97a846cdec3f9f513f8ac2ade3d866cb4618` |
| cpp-rename.mplugin | 43421 | `5349891fe6395c6e4bdf46bfdd27be7599ccd7f446921dda625fcdb0398cc866` |
| cpp-task.mplugin | 49860 | `b7dcefd3a8faf55563a8f060f3cb5ba811eb64228d892d58a60ace721a965bd7` |
| cpp-transform.mplugin | 44354 | `423b7285a62677eaea6f855b6541b362668da8940d79e01a02fdbf4c281d5481` |
| rust-rename.mplugin | 52615 | `b9f978c696ef665e91a90b66a87f6a852e5288f95c8cc120cd83bbfbb7938b8c` |
| rust-task.mplugin | 52463 | `809d6e1f743275870d3c1be44687a4517c6e398ca107684d82d24198928dcfbf` |
| rust-transform.mplugin | 52089 | `eac302d87a29ba3ce187802bbbdd8fd398a9b38506b8d4a1ab6cb20788c0fd6d` |
| morrow_example_transform.wasm | 101216 | `636f8643e3d3fe9721cc45e44395930d7d56b7f4da67c472b3170ff3a25a759b` |
| c_transform.wasm | 77955 | `662ab7933812b8b6130787cb0ddc1ca8f2168f3c380d13f2de5bce8e4eaeac4b` |
| cpp_transform.wasm | 79172 | `1614bfeea153e275a7e87a0ceec068d457ed16b471e04d439f7feeec03bb797d` |

旧实验任务 schema 包会因摘要不匹配而拒绝，需同步 SDK 后重建。未切换主应用资料库，未推送或发布 Release。

## 后续门槛

正式 handler／类型注册、类型化插件错误、修改提案的授权提交、多步骤任务、持久恢复、声明式 UI 及三语言 UI 构造器仍待实现。首期继续仅支持 C／C++／Rust guest；Flutter 宿主负责 UI，无需动态 Dart 插件，暂不提供 TS／JS guest。平台资格分别验收。
