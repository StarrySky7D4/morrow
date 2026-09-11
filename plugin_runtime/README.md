# 可替换 Wasm 执行后端原型

基于 0.1.9-test.10，消息协议仍为 v6；新增实验 Wasm guest ABI v1。本轮在 Windows x64 上运行了使用 SDK 的实际 C／C++／Rust Wasm 模块，并通过可信适配器接入 HostRuntime 与 SQLite。应用版本、数据库格式和 Flutter 工作台未切换。

## 执行与权限边界

插件编译为独立 wasm32-unknown-unknown 模块，导出 memory 和 morrow_run() -> i32。当前任务入口无参数，仅用于固定输入原型；后续任务输入、结果、异步调度和持久实例仍需建立版本化契约。模块不含 start 段，准备阶段不执行插件代码。

唯一允许的导入为 morrow_v1.exchange(input_offset, input_length, output_offset, output_capacity) -> i32。四个参数均为 Wasm i32；成功返回正响应长度，传输失败返回 -1，边界违规直接终止执行。输出容量必须为 65536 字节，输入为 1–65536 字节，两个区域必须有效且不重叠。正长度只代表收到协议消息，权限拒绝与提交结果须由 SDK 解码判断。

后端先验证完整输出空间，再复制输入，之后调用宿主；写回时重新取得线性内存视图。guest 不能指定宿主路径、连接、实例、权限或时钟。可信调用方通过闭包绑定实际连接，核心仍执行逐次授权。默认运行库只依赖执行器和 Wasm 解析器；原生可选 packages feature 增加核心适配，绑定包摘要、能力上限和连接。低层 Runner 的核心依赖仍仅用于测试。后端不拥有数据库或授权配置。

Runner 可复用已校验模块，每次 run 创建独立 Store，结束后释放该次线性内存。当前不维护服务实例、共享内存租约或插件间调用图。取消标志只影响执行／响应交付，不能代替核心撤权，也不能删除已提交内容。

## 可复现的实验限额

| 项目 | 默认值／固定上限 |
| --- | --- |
| Wasm 文件 | 至多 4 MiB，二进制解析与验证，不加载预编译宿主机器码 |
| 指令 fuel | 每次 2000 万；调用方可设 1–1 亿 |
| 线性内存 | 默认 16 MiB；允许 64 KiB–64 MiB；至多一份内存 |
| 表 | 至多一张，最多 4096 元素 |
| 实例 | 每个 Store 至多一个 |
| 宿主调用 | 默认至多 16 次；可信调用方可设 0–1024 |
| 消息 | 64 KiB，必须在提交前通过输出范围检查 |

静态模块检查使用 EnforcedLimits::strict；以上是原型参数，不是性能结论或发行参数。缺省关闭 Wasmi 的 WAT 文本解析，仅测试依赖解析 WAT；不提供 WASI、文件、网络或自定义身份导入。由内存增长限额触发的拒绝配置为 trap。

Wasmi 1.1.0 的 fuel 和 StoreLimits 用于验证可替换解释后端，锁定版本不代表它是最新版本或全平台最终选型。依据见 [固定版本 API](https://docs.rs/wasmi/1.1.0/wasmi/)、[资源限额](https://docs.rs/wasmi/1.1.0/wasmi/struct.StoreLimitsBuilder.html)。Wasmtime／Pulley、浏览器 Worker 与平台分发限制继续分别验证。

**取消不等于回滚。** 取消在进入执行、每次宿主调用前后及退出时检查。纯计算死循环由有限 fuel 终止，取消标志不保证立即打断它。fuel 不能终止正在阻塞的宿主数据库／系统调用，也不覆盖编译阶段耗时；已增加原生有界线程队列及期限，见 [后台任务](../docs/PLUGIN_TASKS.md)。生产执行器仍需多实例调度、进程／浏览器 Worker 终止与撤权协调。可信闭包必须遵守有界响应、禁止重入及禁止 panic 的约定。解释器隔离测试不等于对所有恶意模块、宿主依赖或六平台的完整安全验收。

无论 guest 返回错误码、trap、fuel 耗尽或取消，宿主调用都可能已经提交，必须查询固定操作 ID 的权威结果。guest 返回的 20 等示例状态值不构成提交证明；验收还独立重开 SQLite 检查操作回执和内容／事件关联。

## 验证

```powershell
pwsh -File tool/prepare_plugin_c_wasm.ps1
pwsh -File tool/verify_plugin_runtime.ps1
```

需要 Rust、Cap’n Proto 编译器、Python 3、PowerShell 7、LLVM 22 Clang 及 wasm32-unknown-unknown 目标；C/C++ 标准库由准备脚本获取。实际插件来自 [Rust 示例](../sdk/examples/rust-rename/src/lib.rs)、[C 示例](../sdk/examples/c-rename/plugin.c) 和 [C++ 示例](../sdk/examples/cpp-rename/plugin.cpp)，都使用类型化 SDK。每次请求还与核心独立编码的消息逐字节对照。

本轮通过：

- 格式检查、Clippy -D warnings；运行库默认 12 项测试，启用 packages 共 24 项测试，SDK 14 项回归测试。
- 无效模块／导入／入口、禁止 start、初始内存超限、运行期内存增长／越界、死循环、宿主调用洪泛、缓冲区预检、取消和无效回复。
- 实际 C／C++／Rust SDK Wasm 模块：无权限拒绝、授权提交、重复请求、跨连接拒绝、撤权与停止后拒绝，以及取消后的结果核对。
- 三类首次提交后故障：取消、trap、fuel 耗尽；重开库均保留修订 2 和两条原子事件（初始创建与本次修改）。

- C++ 字符串／向量与 Rust 编解码共同分配期间保留数据、calloc 清零、realloc 保留数据及 256 字节对齐分配通过。
- C++ 访问无效回复与 128 MiB 分配失败均为 guest 内 trap、零宿主提交；OOM 样例将存储传给独立编译函数，避免被编译器消除分配。

产物和 SHA-256 见 [三语言运行记录](../reports/plugin-sdk-wasm-three-languages.md)。日志：build/plugin-runtime/verification.log；最终去除调试符号的模块已重新执行同一链路与错误路径。编译工具链或源码变化后应重新生成摘要。

当前只证明 Windows 上该解释后端与三语言示例的执行结果。已增加原生实验包校验、不可变安装与绑定执行，见 [插件包说明](../docs/PLUGIN_PACKAGE.md)。启用／更新状态、签名／依赖锁定、长驻服务、异步任务、mmap、审计封存、UI 对接及其余平台仍待完成。实验 ABI 尚未锚定；未发布插件包或 Release。

原生 Worker 已通过四个实际包的后台任务／排空执行，并覆盖队列满、取消隔离、排空期限、旧连接拒绝和宿主线程故障。结果见 [验证记录](../reports/plugin-worker-validation.md)。版本化任务输入和 Flutter 主界面接入尚未完成。
