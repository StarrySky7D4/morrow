# 宿主选中文件的有界固定读取

2026-09-26：原生运行时新增 `FileBroker::grant_open_file`。Linux 本地及三语言既有 guest 模块已验证；后续已接入 [工作台原生后台文件任务](PLUGIN_FILE_TASKS.md)，私有协议和独立 Dart 客户端已补齐；文件选择器／任务页面、Windows 实机和其他平台适配仍未验收。本接口不开放目录或文件写入，也不改变 IO wire/schema。

## 授权与所有权

平台适配器先通过合法的用户选择获得已打开的 `std::fs::File`，再把句柄的所有权交给 broker。调用同时要求原 `Manager`、`HostRuntime`、`ManagedInstance` 和有 `FileRead` 的 `IoBinding`。文件路径、包声明、`FileList` 类别或另一实例的绑定均不能替代这些条件。

此入口不按路径打开文件，不解析 symlink/junction，也不提供根目录解析保证。平台适配器负责实际选中对象和句柄来源，独占其游标，不与其他读取者共享偏移。入口定位到句柄的零偏移，始终读取该对象；即使原路径后来被替换，也不会重新按路径打开。所有返回路径都关闭传入句柄。

这是同步宿主接口。适配器应在原 owner 的执行线程调度，不能堵塞 Flutter UI，也不能为了调度而重新打开另一个 Store。普通文件的 OS read/seek/metadata 仍可能阻塞，检查点无法强制中断系统调用；本轮不提供异步暂停、强制超时或跨进程隔离保证。

## 容量与固定字节

1. 先核对当前 FileRead 和实例绑定，再检查句柄元数据；只接受普通文件。
2. 原始长度必须不超过调用者的 `max_bytes` 和 broker 的 256 MiB 驻留上限；`max_bytes = 0` 仅允许空文件。
3. 分配和读取前，以原实例共享账本预留一个 job、一个 resource、`length + 1` 个累计字节。额外的一字节用于 EOF 探测。包的单作业／累计额度和共享资源上限仍可更小。
4. 有界分配后从零读取，每次至多 64 KiB，块前后检查原租约、撤权、单调时钟和期限。提前 EOF、多出字节或最终长度不符都返回 `SourceChanged`。
5. 计算实际固定字节的 SHA-256，再做最终授权检查；仅此后发布不透明 reference、长度与摘要。job 释放，resource 留到 Finish／Cancel／reap／broker 丢弃。

失败、取消和超时均不发布半成品引用；预留后失败会释放并发槽，但累计字节不退回。准入前拒绝不分配数据缓冲、不 seek/read、不扣额度。现有 `grant_file(Vec<u8>)` 仍按传入字节数收费，不增加 EOF 探测费用；两条入口共享资源容量、序号和发布实现。

这里固定的是**实际读到的字节**。源文件可在复制期间发生等长修改，两次元数据检查无法证明原子快照；返回摘要可能对应混合时刻的字节。该语义已用真实文件改写测试固定。需要原子源快照的适配器必须另外提供符合平台语义的快照／锁。

## guest 交付与失败

guest 仍只使用原来的 Read／Finish／Cancel。每次交付沿原 FileBroker 检查关联请求、原实例、当前授权、期限和共享预算；摘要和引用的存在不恢复已撤销权限。清理不退回累计费用。

`CaptureError` 区分 `Admission(Denied/Expired/Clock/Limit)`、`NotRegularFile`、`SourceChanged`、`Io(ErrorKind)`、`Allocation`。OS 错误不携带路径或原始错误文本。不重试失败的整个捕获；被 OS 中断且未读到字节的单次 read 可在重新检查授权后继续。

字节只驻留当前 broker 内存，长度／摘要不是持久证据记录，重启不会恢复引用。导入附件仍须走现有内容 API，不把读到字节视为已经保存卡片。

## 本地复验

具备仓库宿主工具链及依赖缓存后，在仓库根目录执行：

```sh
cargo test --locked --offline --manifest-path plugin_runtime/Cargo.toml \
  --features packages --test managed_file_io
python tool/plugin_transport_baseline.py verify
MORROW_RUST_IO_WASM="$PWD/sdk/compat/transport-v1-rc1/rust-io.wasm" \
MORROW_SDK_IO_GUEST_C="$PWD/sdk/compat/transport-v1-rc1/c-io.wasm" \
MORROW_SDK_IO_GUEST_CPP="$PWD/sdk/compat/transport-v1-rc1/cpp-io.wasm" \
cargo test --locked --offline --manifest-path plugin_runtime/Cargo.toml \
  --features packages --test sdk_io_guest -- --ignored
python tool/plugin_transport_baseline.py verify
```

最后五项测试直接使用保留的三语言 Wasm 字节，并在测试内创建明确声明 FileRead 的临时测试包。它们没有改写冻结文件，但**不表示原来的 HTTP 包获得文件权限**，也不计为原包文件读取验收。目录／设备拒绝专项仅在 Unix 运行；Windows 需独立运行实际文件与共享句柄测试。

完整结果与下一门槛见 [开发记录](../reports/plugin-selected-file-2026-09-26.md)。

2026-09-26 协议接线补验修正：内容摘要改用原始字节 SHA-256，不再使用 schema 文本规范化函数；二进制引用 secret 的派生也作同样修正。先前历史报告的“实际摘要通过”不能替代这次独立二进制校验。详见 [协议报告](../reports/plugin-file-wire-2026-09-26.md)。
