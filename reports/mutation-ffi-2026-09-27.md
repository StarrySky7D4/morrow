# 文件变更 C／C++ SDK 编解码接口

2026-09-27，`codex/io-safety-refactor`。在上一轮独立 mutation 契约上新增 C ABI、C++17 类型化封装和 Windows 原生验证脚本。SDK 契约仍是实验草案，未冻结；本轮未提交、推送或发布。

## 实现

- `sdk/c/include/morrow_plugin_mutation.h`：版本化请求描述符，encode／validate／schema digest，响应 decode／get／free；八种动作，无宿主调用 helper。
- `sdk/rust/src/mutation_ffi.rs`：复用同一 Rust 编解码，映射稳定本地错误码，限制所有输入 span；先读 8 字节 ABI／大小前缀，通过后才借用完整结构。拒绝无关 span 和标量、非法 UTF-8、越界 offset 和未批准格式。编码失败保持输出缓冲原状并归零长度，响应失败清空合法输出 handle 槽。
- 解码 handle 自有类型化结果和原始响应帧，输入缓冲被覆盖后 view 仍可读取。调用方必须提供存活、对齐、互不重叠的合法内存；这是本地 unsafe C ABI 的契约，不能保证任意地址安全。
- `sdk/cpp/include/morrow_plugin_mutation.hpp`：八类动作工厂，自有字符串／数据及编码前长度检查；响应为只可移动 RAII，含自移动保护。
- `tool/verify_plugin_mutation_sdk.ps1`：离线构建 SDK DLL，C11／C++17 严格编译，运行两种语言的实际测试程序，前后核验既有 transport 原件。
- C 常量／枚举与 schema 自动比对，避免只修某一语言的上限或状态编号。

## 最终验证

| 验证 | 结果 | 证据 |
| --- | --- | --- |
| Rust SDK 全量测试 | 76/76，含 5 项新 FFI 测试 | `build/mutation-ffi-test.log` |
| SDK 全目标 Clippy | `-D warnings` 通过 | `build/mutation-ffi-clippy.log` |
| SDK DLL 构建 | 通过 | `build/mutation-ffi-cdylib.log` |
| C／C++ 原生程序 | 两项编译及实际运行通过，各消费 21 个共享向量，覆盖八种动作 | `build/mutation-ffi-native.log` |
| Rust SDK wasm32 编译检查 | `cargo check --target wasm32-unknown-unknown` 通过 | `build/mutation-ffi-wasm-check.log` |
| 契约／限额／枚举／向量工具 | 10/10 | `build/mutation-ffi-tools.log` |
| SDK 项目锁定 | 12/12 | `build/mutation-ffi-sdk-lock.log` |

新 FFI 回归包含物理上仅 8 字节的短描述符、空指针／对齐、错误 ABI／长度、无关字段、错误回执相关性、失败长度／缓冲／handle、输入帧覆盖后结果稳定。C++ 原生测试验证移动构造、移动赋值、自移动及输入帧覆盖后精确结果帧仍存活。

首次 Clippy 拒绝测试中的手工悬空指针写法，改用标准 dangling pointer 构造；首次 C++ 严格检查拒绝语法上直接自移动，改用别名保留实际自移动路径。均保留 `-D warnings`／`-Werror`，最终通过。没有通过屏蔽警告降低验证要求。

重现命令：

```powershell
./tool/verify_plugin_mutation_sdk.ps1 -Python <本机Python路径>
```

原生程序位于 `build/mutation-sdk-native`。这只是测试工具输出，不是应用发布包。

## 范围与下一步

Rust／C／C++ 已有一致编解码入口；**尚不能让 guest 通过它执行文件操作**。当前没有新 Wasm import、运行时协商、变更 job 租约或原 owner 分派。Wasm 的 cargo check 也不是 C／C++ Wasm 运行证明，Windows 原生测试不外推 Linux／其他平台。

下一阶段按 [SDK 接入计划](../docs/PLUGIN_MUTATION_SDK_PLAN.md) 补显式 managed mutation job、可信审批内容绑定与独立执行许可，在暂停 guest 后直接借用原 owner，禁止等待自身队列。之后验证三语言真实 Wasm Create／Delete、旧原件运行、非空内容故障恢复，再考虑 SDK 冻结。

## 后续纯编解码边界补验（2026-09-27）

运行时 guest 授权接口尚待专项授权，本增量只补现有 C ABI 回归，不新增权限、import 或文件效果。检查未发现需要修改编解码实现的问题。

- 九个合法请求原件逐字节截断：1,848 次拒绝检查。
- 三个合法响应分别截断响应帧与原请求帧：1,176 次拒绝检查，输出句柄必须归零，不能残留调用前的哨兵值。
- 60 KiB 最大分块放在 16 MiB 内容区间末端；验证零容量、1 字节、差 1 字节和精确容量，失败不写输出，成功不破坏两侧各 16 字节哨兵。
- 结果视图从 0 字节到完整结构差 1 字节的所有容量均拒绝，保持真实对齐存储的哨兵不变。

最终纯 SDK 全量测试 **78/78**（FFI 7/7），日志 `build/mutation-ffi-boundary-sdk.log`、`build/mutation-ffi-boundary-test.log`；全目标 Clippy `-D warnings` 及该测试文件 rustfmt 检查通过，日志 `build/mutation-ffi-boundary-clippy.log`。契约同步与共享向量工具 **10/10**。这些结果不替代 Wasm 实际运行、宿主授权或文件效果验收；本增量未重跑 C／C++ 原生程序，因为未修改实现或头文件。
