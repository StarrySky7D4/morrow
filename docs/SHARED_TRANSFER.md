# 父子进程共享读取引导契约

`core::shared_transfer` 是可信宿主与明确指定子进程之间的私有引导消息，不属于通用插件 guest runtime 或 SDK。它不添加内容事务、对象权限或持久化入口。

## 消息与 API

外层使用固定预编译 `shared_transfer.capnp` v1、归一化 schema SHA-256 和显式 Offer／Reply union。两类消息不可互换；不认识的版本、schema 摘要、消息种类会拒绝。`Offer`、`Reply` 均提供 `validate / encode / decode`。

| 消息 | 字段与校验 |
| --- | --- |
| Offer | `transfer: u64`、`remote_handle: u64` 均非零；`descriptor: shared_object::Descriptor` 单独验证，包括对象 1–16 MiB、至多 64 段连续覆盖、checked_add 溢出拒绝。 |
| Reply | `transfer: u64` 非零；descriptor 同样单独验证；`payload: Vec<u8>` 非空、最多 64 KiB、长度必须等于 descriptor.length，实际 SHA-256 必须与 descriptor.sha256 相同；`write_rejected: bool` 记录写入探测观察结果。 |

Descriptor 作为独立契约的二进制消息嵌入，保留自己的版本、schema 摘要、结构和长度上限，不绕过共享描述符验证。

`MAX_FRAME_BYTES` 为 128 KiB，解析前检查总消息长度；Cap’n Proto 遍历上限 16384 words，嵌套上限 8。使用有界拥有型消息读取，允许任意地址对齐的传输切片。拒绝截断、尾字节、拼接消息，以及超限嵌套描述符；Reply 在复制 payload 到结果前检查其字节预算。

Offer 可描述 16 MiB 对象，但本资格回执只复制并返回至多 64 KiB 的完整对象；超出该复制上限的 Offer 需要调用方拒绝此资格读取，不能用截断 payload 冒充完整回执。管道长度前缀和精确读写由承载层处理，不能因 codec 有界而无上限读取输入流。

## 权限与关联边界

`remote_handle` 仅表示已经向本次指定目标进程交付的只读句柄值；不得在父进程或其他进程里解释该数值，不是全局对象 ID，不允许持久化后当作权限恢复。此协议不负责打开、复制、降权或关闭操作系统句柄，也不能证明句柄已按只读权限交付。

独立 Reply 解码只检查结构与所返字节的自洽性，无法证明发送者身份或它对应哪次 Offer。父端必须使用可信连接绑定实际子进程，检查 transfer 与 descriptor 和当前 Offer 精确一致，并继续检查对象活跃租约与撤权状态。合法摘要和自洽字节不能代替这些条件；回执不是内容提交或内容审计回执。

`write_rejected=false` 是合法观察数据，codec 不强迫其变成 true。资格执行者应根据实际探测结果判定通过或失败；布尔声明本身不是防写证明。已复制到子进程的内容无法靠关闭句柄或撤销未来访问收回。

## 当前 Windows 父端生命周期

`SharedObjects::start_reader` 与 `RemoteReader` 提供当前原生承载入口。调用者提供受信任的辅助进程命令；guest 不选择命令。启动前通过真实 HostRuntime、consumer Connection 和活跃租约取得 Mapping，在指定子进程已经创建、准备复制只读句柄之前再次校验该 Mapping。父端持有确切的 Child 对象与 Mapping 保留引用，不根据 PID 重新打开进程，也不按远端数值反向关闭句柄。超过 64 KiB 的本轮资格读取在创建子进程前拒绝。

Offer 写入与 Reply 读取使用独立线程。`close_input()` 可在收到 Reply 之前调用；它释放维持输入管道的信号，写线程完成 Offer 写入与刷新后关闭 stdin，无需等待读取线程先收到回复。这是正常关闭请求，不是进程退出或页面回收证明，也不承诺立即中断已阻塞的管道写入。

`receive()` 在接收前和交付前重新校验真实连接、对象租约及 Mapping 归属；只接受与本次 transfer 和完整 descriptor 相同、且 `write_rejected=true` 的回执，成功原始字节最多交付一次。合法 Reply、输入／输出 EOF、取消请求或等待超时都不会释放父端持有的 Mapping 引用。回执表示读取资格结果，不表示子进程已经放弃映射，更不是内容提交或页面释放确认。

| 操作或观察 | 对进程和 Mapping 的影响 |
| --- | --- |
| `close_input()` | 请求关闭正常协议输入，继续保留 Child 与 Mapping。 |
| `stop()` | 先关闭输入，再请求终止确切 Child；终止调用成功或失败均不自动释放 Mapping。 |
| `poll_exit()` 返回未退出 | 继续持有 Child 与 Mapping。 |
| `poll_exit()` 观察失败 | 原所有权保留，调用者可继续核对同一进程；不推断它已经退出。 |
| `poll_exit()` 确认真实退出 | 不论退出码是否成功，释放该 RemoteReader 持有的 Mapping 引用；其他仍活跃引用继续决定实际资源保留。 |
| RemoteReader 被丢弃 | 请求终止并尝试确认退出；尚未确认时交给后台回收线程继续持有并等待，不能通过丢弃对象绕过资源保留。 |
| 后台回收线程启动失败，或等待退出失败 | 保守保留进程对象与 Mapping，必要时持续到宿主退出；不假定资源可以复用。 |

该实现依赖辅助进程可信、不会把收到的句柄转交给其他进程。它不是针对恶意本机代码的沙箱，不提供任意进程树隔离，也不能收回已经复制到接收方的字节。此入口仍是受控原生资格路径，尚未成为默认工作台数据通道或插件 A→B 调用机制。

以上生命周期说明来自当前实现核对；不将错误处理分支的存在表述为已完成对应故障注入验证。进程观察失败、回收线程启动失败和等待失败等分支的实际资格证据应单独记录。

## 验证

9 项 Windows 合成数据测试覆盖完整 u64 值、Offer 16 MiB／Reply 64 KiB 边界、空值与零身份、实际 payload 哈希和长度、不合法嵌套描述（含整数回绕和尾字节）、消息种类混用、版本与摘要漂移、所有短消息前缀截断、超限帧及地址偏移 1–7 字节的切片。另验证自洽 Reply 不自动证明 Offer 关联。

```powershell
cargo test --offline --locked --manifest-path core/Cargo.toml --target-dir build/transfer-contract --test shared_transfer
cargo clippy --offline --locked --manifest-path core/Cargo.toml --target-dir build/transfer-contract --all-targets -- -D warnings
cargo check --offline --locked --manifest-path core/Cargo.toml --target-dir build/transfer-contract --target wasm32-unknown-unknown --lib
```

这些结果仅证明消息契约与编译边界；实际父子进程句柄权限、映射、存活和关闭行为需要单独的原生资格测试。Wasm 能编译不意味着浏览器支持操作系统句柄。
