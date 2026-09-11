# C／C++／Rust SDK Wasm 执行记录

日期：2026-09-11。基于 0.1.9-test.10；消息协议 v6、实验原生／Wasm ABI 各为 v1。应用版本和数据格式未更改，没有发布 Release。

## 已验证的实现

C 使用 C 传输和类型化编解码接口；C++ 使用现有 C++17 request／client／decoded_reply 包装；Rust 使用同一固定契约的 SDK。每种示例均独立编译成 Wasm 模块，并在 Windows x64 的 Wasmi 1.1.0 后端运行。请求与核心独立编码的消息逐字节一致，实例／权限／时钟由可信适配器绑定。

C/C++ 编解码静态库由 Rust 实现。C malloc／calloc／realloc／free、C++ new/delete 及对齐分配，与 Rust 编解码共用一个 guest 分配器。libc++／libc++abi 的致命诊断在该 profile 中直接 trap；运行模块只保留 morrow_v1.exchange，纯错误样例没有任何宿主导入。后端仍拒绝 WASI 文件、时钟及其他额外接口。

## 证据范围

- 12 项执行后端测试、14 项 SDK 测试，格式与 Clippy -D warnings 通过；C11／C++17 严格警告构建通过。
- Rust、C、C++ 和 C++ 分配器样例分别运行授权拒绝、授权成功、重复提交、其他连接拒绝、撤权、取消和停止后拒绝。每个样例使用独立合成数据库，重开后核对修订 2、操作回执及创建／修改共两条事件。
- C++ 分配器样例在 Rust 编解码工作期间保留 32 个长字符串及对齐对象，检查数据未损坏、calloc 清零、realloc 保留数据和 256 字节对齐。
- C++ 无效回复访问和实际 128 MiB 分配失败都在 guest 内终止，宿主提交次数为零。OOM 样例的内存传给独立编译的函数，避免分配被优化消除。
- 既有三类首次提交后故障测试仍通过：取消、trap、fuel 耗尽均不撤销已提交结果。此项使用故障模块验证共同后端，不能代替真实进程被强杀、宿主阻塞或断电测试。

工具链：Rust 1.95.0，Clang／LLVM 22.1.0，官方 WASI SDK 34 的 wasm32-wasip1/noeh 头文件与静态库。下载资产为 wasi-sysroot-34.0.tar.gz（119,283,116 字节），完整 SHA-256 为 `9d813544eeebe38b7b8f2244ed591de46b6db812c6dd1a257ff9f0d2a905a2be`；完成核对后才解压和编译。

## 复现

```powershell
pwsh -File tool/prepare_plugin_c_wasm.ps1
pwsh -File tool/verify_plugin_runtime.ps1
pwsh -File tool/verify_plugin_sdk.ps1
```

允许用 -Python 指定解释器；C/C++ 构建允许 -Sysroot、-Clang、-ClangXX 及 -DebugSymbols。正式输出默认移除调试符号。模块导入可通过 plugin_runtime 的 inspect 示例只读检查；是否允许执行仍由 Runner 校验。

本轮执行日志位于 build/plugin-runtime/verification.log；原生 SDK 日志位于 build/plugin-sdk/verification.log。最终去除调试符号的五个 C/C++ 模块已实际重跑链路／错误路径，之后的目标平台防护修改没有改变模块字节。以下是当前本地产物，不承诺跨编译器版本的位级重现。

| 模块 | 字节数 | SHA-256 |
| --- | ---: | --- |
| morrow_example_rename.wasm | 78,312 | `dcf60939ef118578c870dd691b36787e167c52cf50d089c7b977c37fd7d3fd3e` |
| c_rename.wasm | 73,792 | `85abc6da058aeb373039e1ed3bf656d7a1a532c2601b0603f6df6e1e8f703b01` |
| cpp_abort.wasm | 122 | `a910bf8ff380e7a8dc2d11c3f8c5d5ee2f98cf4b60af88facf2f92e47f498d8e` |
| cpp_allocator.wasm | 77,421 | `43ac17186ccb70b357ec888f69ac34d0b745d71bc967a10278ea3d43a405a8d8` |
| cpp_oom.wasm | 51,637 | `b0c34ae62dfd26a88e4d140ac21464282108a11626ea77115a2b2cb8b4f9d9da` |
| cpp_rename.wasm | 75,619 | `2f298760d3b410a2d5fcec11b516b84462b0c50e4a85d7910c25e4370ebf2a38` |

Rust 模块在 build/plugin-guest/wasm32-unknown-unknown/release，其余在 build/plugin-c-guest。

## 尚未完成

这轮证明三语言 SDK 能通过同一受限后端提交内容，不等于正式插件系统完成。C++ profile 暂无异常和 RTTI；不承诺完整 WASI／全部标准库能力。包契约与安装、签名／依赖锁定、长驻实例、异步任务、期限与阻塞调用终止、mmap、审计封存、UI 与其余平台仍须验收。主路线见 [未来任务](../docs/FUTURE_ROADMAP.md)，开发者用法见 [SDK 说明](../sdk/README.md)。
