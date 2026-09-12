# Windows 宿主私有只读共享映射

**状态：2026-09-12 已完成 Windows 原生模块编译、映射资格测试与严格静态检查。** 源文件为 `src/shared_memory.rs`。经用户明确批准，运行时根采用 `deny(unsafe_code)`，只有该模块通过 `allow(unsafe_code)` 使用原生边界；其他模块继续拒绝 unsafe。此结果不代表跨进程或 Wasm 共享对象协议已经完成。

## 对外能力与范围

`FrozenRegion::copy_from(&[u8]) -> std::io::Result<Self>` 接受 1 字节至 16 MiB 的输入，可信宿主完成一次冻结复制后提供 `bytes() -> &[u8]`、`len()` 和 `is_empty()`。对象不实现 Clone；需要跨线程共享时使用 `Arc<FrozenRegion>`，借用不能比拥有者活得更久。

源缓冲与返回映射无关联，生产者随后修改或释放自己的缓冲，不应改变已冻结内容。本轮测试已确认该性质。没有页面池、写入接口、操作系统句柄导出、可变视图导出或 guest 选择的对象名称。Debug 仅显示逻辑长度。

这是**当前宿主进程内部的匿名映射**。虽然底层为 Windows 文件映射对象，尚无跨进程句柄交付、只读能力句柄分发、子进程退出回收、进程间通知或读者崩溃回收证明。Wasm 线性内存与该地址不是同一块内存，后续适配必须显式复制；不得据此宣传跨 Wasm 零复制。

非 Windows 原生后端代码对有效长度输入明确返回 Unsupported，未使用 Vec 模拟 mmap。没有 Linux/macOS/Android/iOS/Web 的实现或运行验证。

## Windows 创建及发布顺序

1. 验证逻辑长度，在任何系统分配前拒绝空输入或超限。
2. 调用 `CreateFileMappingW(INVALID_HANDLE_VALUE, NULL, PAGE_READWRITE, 0, len, NULL)`，创建匿名、默认非继承的分页文件映射。
3. 创建宿主私有的唯一 FILE_MAP_WRITE 视图，并从借用的源缓冲完整复制。
4. 显式检查 `UnmapViewOfFile` 成功；失败则返回错误，绝不发布读对象。
5. 只有写视图撤销成功后才创建 FILE_MAP_READ 视图，随后返回 FrozenRegion。

分页文件存储、空名称与继承默认值依据 [CreateFileMappingW 官方定义](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-createfilemappingw)。读访问参数与视图限制依据 [MapViewOfFile 官方定义](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-mapviewoffile)。没有请求可执行、写时复制、大页或命名映射访问。

底层映射对象最初允许读写，以便完成可信初始化；保留的映射句柄是宿主内部私有实现细节，并未在内核对象层收窄为只读访问权。因此安全不变量包括“只有本实现可使用该句柄、没有后续写映射 API”；它不是对同进程任意 unsafe 代码的隔离或反篡改保证。

## 静态安全审查

本地 Windows SDK 10.0.26100.0 已核对 `memoryapi.h` 中的 CreateFileMappingW、MapViewOfFile、UnmapViewOfFile，以及 `handleapi.h` 中 CloseHandle 的参数顺序、DWORD/BOOL/SIZE_T/HANDLE 表示和 system 调用约定。只传 NULL 的可选 SECURITY_ATTRIBUTES 使用不透明指针，不读取或构造该系统结构。没有新增 Cargo 依赖。

- 长度先验证为非零且至多 16 MiB，映射大小低 32 位转换与 Rust 切片长度都不发生截断；映射页尾未使用空间不进入 bytes()。
- copy_nonoverlapping 的目标是刚分配的独立写映射，源为有效借用切片，不可能在安全调用中重叠。
- bytes() 的 slice 仅覆盖已经初始化的逻辑长度，并借用 Self；裸地址不通过公共方法返回，调用者从普通切片获得的地址不能安全地用于写入。
- Send/Sync 的理由依赖发布前撤销唯一写视图、之后只有只读视图、且字段私有；不能把这两个 unsafe impl 泛化给可写共享区。
- View 记录原始映射基址；成功撤销后清空 Option，避免正常路径重复撤销。所有出错路径先捕获系统错误，再由局部 RAII 清理。
- FrozenRegion 字段声明让 View 先释放，再 CloseHandle；构造中途失败也由局部变量逆序释放写视图及句柄。取消最后 Arc 时才触发拥有者 Drop。视图撤销与对象句柄关闭的分工见 [UnmapViewOfFile 官方说明](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-unmapviewoffile)。
- Drop 无法返回错误；其 Unmap/Close 返回值未向调用者报告。发布前写视图撤销则显式检查，不能把清理失败当作成功冻结。资源故障、错误注入与实际回收仍需资格验证。

## 已执行测试与证据限制

Windows 上执行了模块测试，测试框架显示 **5 项通过**：包括生产者缓冲修改/释放不改变冻结内容，8 个 Arc 读者跨线程并存及最后拥有者退出，1 字节/页边界/16 MiB/超限检查，以及实际写保护故障资格。其中一条测试是独立子进程入口，正常测试枚举时不重复触发故障；实际有效资格路径为 4 条。全目标全特性 `cargo clippy ... -- -D warnings` 通过。

故障资格创建了无窗口的独立测试子进程，设置该子进程的系统错误弹窗抑制模式，写入准备标记后故意向只读映射执行写操作。父进程使用 10 秒截止，已经实际确认准备标记和 **0xC0000005 访问异常退出码**。该故意违规操作仅用于隔离硬件故障探测，不是合法 Rust 客户端示例，不进入生产逻辑。

非 Windows Unsupported 分支未在其他原生目标编译或执行。系统内存耗尽、映射/撤销/关闭的故障注入以及直接资源计数回收检查尚未完成；Arc 最后拥有者测试证明 Rust 对象租约结束，不应扩大为跨进程退出回收证明。

复现命令（Windows，仓库根）：

```powershell
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --target-dir build/shared-memory --lib shared_memory -- --test-threads=1
cargo clippy --offline --locked --manifest-path plugin_runtime/Cargo.toml --target-dir build/shared-memory --all-targets --all-features -- -D warnings
```

这些测试只覆盖同宿主对象和专用故障进程，不证明跨进程读者协议、真实插件工作流、对象总量/总字节额度、审计关联、持久恢复或任意进程终止回收。总量限制与资源租约管理由上层共享对象服务另行实现。
