# Windows 宿主私有只读共享映射

**状态：2026-09-12 已完成 Windows 原生模块编译、映射资格测试与严格静态检查。** 源文件为 `src/shared_memory.rs`。经用户明确批准，运行时根采用 `deny(unsafe_code)`，只有该模块通过 `allow(unsafe_code)` 使用原生边界；其他模块继续拒绝 unsafe。test.27 另已验证向实际 Child 交付限定只读句柄及接收复制；这不代表完整跨进程协议或 Wasm 共享对象协议已经完成。

## 对外能力与范围

`FrozenRegion::copy_from(&[u8]) -> std::io::Result<Self>` 接受 1 字节至 16 MiB 的输入，可信宿主完成一次冻结复制后提供 `bytes() -> &[u8]`、`len()` 和 `is_empty()`。对象不实现 Clone；需要跨线程共享时使用 `Arc<FrozenRegion>`，借用不能比拥有者活得更久。

源缓冲与返回映射无关联，生产者随后修改或释放自己的缓冲，不应改变已冻结内容。本轮测试已确认该性质。没有页面池、写入接口、源操作系统句柄导出、裸地址导出、可变视图导出或 guest 选择的对象名称。test.27 的可信交付方法仅返回目标进程内的只读句柄值。Debug 仅显示逻辑长度。

冻结区域本身是**宿主匿名映射**。test.27 增加向实际 Child 的受限只读句柄交付，以及目标进程复制读取，已用真实子进程测试。本模块不实现可信 Offer/Ack 管道、审计绑定、任务协作或生产进程生命周期协调；调用方必须补齐。Wasm 线性内存与映射地址不是同一块内存，仍需显式复制，不是跨 Wasm 零复制。

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

## test.27：最小权限 Child 交付与安全接收复制

新增同步接口：

```rust
FrozenRegion::duplicate_readonly(&std::process::Child) -> std::io::Result<u64>
copy_received_readonly(raw: u64, length: usize) -> std::io::Result<(Vec<u8>, bool)>
```

两条交付路径均限制为 1..=65536 字节。FrozenRegion 本地冻结上限仍为 16 MiB，超出 64 KiB 的对象不能通过本接口交付。没有通过 PID 重开目标进程的代码：Child 的实际进程句柄在借用期间直接作为目标，无法用 guest 自选 PID 获得额外进程权限。

源进程使用 DuplicateHandle 请求 `SECTION_MAP_READ = 4`、inherit=false、options=0。不使用 DUPLICATE_SAME_ACCESS 或 DUPLICATE_CLOSE_SOURCE，也不关闭原映射句柄。返回值仅在目标进程有效；发送方不能把它包入当前进程 RAII，更不能之后按数字远程关闭，因为目标可能已经关闭并复用该值。[DuplicateHandle 的目标句柄及标志语义](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-duplicatehandle)

**DuplicateHandle 成功便已改变目标进程状态。** 即使 Offer 通知随后失败，调用方也必须保留对象/进程跟踪直到确认实际子进程退出，不能将通知失败当作没有交付。该底层方法不拥有 Child，也不承诺租约已释放或自动终止子进程；test.27 上层 RemoteReader 已持有真实 Child 与 Mapping，只有观察到退出才释放；见 [父子进程契约](../docs/SHARED_TRANSFER.md)。

接收方只借用原 raw，不消费或关闭它：先在当前进程复制一个私有 READ 句柄，然后对原 raw 探测 FILE_MAP_WRITE。探测成功意味着原句柄可写，立即撤销该探测视图并拒绝接收；只有失败码恰为 ERROR_ACCESS_DENIED 才继续，其他失败均返回错误。

随后仅映射私有 READ 副本并使用 `ReadProcessMemory(GetCurrentProcess(), ...)` 复制到新 Vec，要求实际复制长度完整。返回 bool 成功时恒为 true，表示原 raw 在探测时拒绝 WRITE，**不证明任意来源映射全局不可变、没有其他写者或通过了身份/内容校验**。上层必须验证可信 Offer 的长度、身份和摘要；已复制 Vec 可以作为验证对象。[ReadProcessMemory 的内存可读性与复制结果语义](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-readprocessmemory)

### 新增 unsafe 审查

- 本地 SDK 10.0.26100.0 复核 DuplicateHandle、GetCurrentProcess、ReadProcessMemory 的 system ABI、参数顺序、BOOL/DWORD/SIZE_T/HANDLE 与输出指针。没有新增依赖，所有 FFI 仍在唯一获批的 shared_memory 模块。
- 子进程句柄来自借用的真实 Child；当前进程使用不可拥有的系统伪句柄，二者都不被本模块关闭。目标数字不会进入本地 Mapping RAII。
- 接收输入数值检查为空、伪句柄和当前机器位宽；其后完全交由内核验证，不把数字转成 Rust 借用。
- 私有 DuplicateHandle 副本独立拥有，即使后续探测/映射/复制失败也由 RAII 关闭；源 raw 从未包装为拥有者。
- 不给可能被外部写者修改的接收映射建立 Rust `&[u8]`。ReadProcessMemory 只写入当前 Rust 独占 Vec，故不会向调用者暴露共享引用的不可变承诺；本接口不读取任意其它进程。
- 原 raw 的借用协议要求调用者在该调用期间保持它的身份稳定。即使错误调用者关闭/复用数字，内核失败及已复制结果仍必须由上层处理；bool 不提供跨时间的权限保证。

### test.27 实际验证

Windows 模块测试现为 **9 项通过**，其中 2 项是独立子进程入口，实际资格路径 7 条；全目标、全特性严格 Clippy 通过。新增路径包括：

- 真实无窗口 Child，通过测试 stdin 交付目标句柄值，读取 1、4097、65536 字节；父缓冲随后变化不影响映射结果。
- 子进程确认原目标句柄不可继承、FILE_MAP_WRITE 被拒，并连续读取两次，证明接收函数未消费原 raw。父对象一直持有到子进程真实退出。
- 原可写映射句柄拒绝接收，原 source 随后仍可正常建立只读视图。
- 无效句柄与长度、超过 64 KiB 的交付、已经真实退出的 Child 均拒绝。
- test.26 的实际写只读页访问异常、源缓冲隔离、Arc 生命周期和 16 MiB 本地边界继续通过。

```powershell
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --target-dir build/transfer-memory --lib shared_memory -- --test-threads=1
cargo clippy --offline --locked --manifest-path plugin_runtime/Cargo.toml --target-dir build/transfer-memory --all-targets --all-features -- -D warnings
```

这些是底层进程交付资格测试，不是生产协作管道或 Wasm helper 的端到端证据。没有完成全平台、内核资源量回收测量、恶意本机进程隔离、进程权限沙箱或任意系统资源失败注入。
