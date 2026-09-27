# Windows 原句柄删除与持久执行边界

日期：2026-09-27。基线 `b9225f6`，应用版本仍为 `0.1.9-test.56+60`。本轮未提交、推送、发布或构建安装包。

## 本轮实现

1. Core 新增独立 Protobuf＋LZ4 `DeleteOutcome`，绑定 operation、subject、请求摘要、目标引用及预期身份；结果为 Deleted 或非零 Win32 错误的 OsRejected。构造前验证身份/摘要，解码拒绝非规范、重复、未知、超限字段。
2. 专用 `claim_file_delete_local_authorized` 以同事务 CAS 将 Prepared 转为 OutcomeUnknown。泛用 IO claim 继续拒绝文件变更。专用入口核对原始删除计划，拒绝删除计划附带暂存内容/回执；重复 claim 不会再给执行许可。
3. `observe_file_delete_local_authorized` 原子保存响应证据和 Observed，并强校验所有绑定和完整证据摘要；开库、审计、封签和快照检查支持删除的新历史。数据库表结构仍为 v23，沿用既有 IO phase；旧文件计划 reader 会显式拒绝新执行相，不能宣称向旧 reader 兼容。
4. Windows TargetBroker 增加实际删除：校验原选择和实时授权，预留原实例 job/字节，提交 Unknown，再检查原租约和句柄元数据，最后通过原句柄标记删除并确认关闭。没有按旧路径重开或按字符串删除。
5. 选择在 claim 后消费，留下有上限的原 owner/请求摘要记录；最多 128 个当前选择和已消费记录共用预算，失效时 reap。CommitUnknown 也不重用选择。重复操作不能删除后来创建的同名文件。
6. 进程级 EffectGate 在持久 claim 前通过 try_lock 取得；Busy 不产生 claim，可稍后重试。锁覆盖最终授权、Set 和 Close，避免跨 broker 的关闭失败状态竞态，也避免等待或回调重入死锁。
7. 记录真实观察与交付权限分开：文件效果之后撤权，仍保存 Response＋Observed，但拒绝交付。效果或观察提交结果不确定时保留 Unknown，不自动重放。

## 原生边界与剩余限制

新增私有 `file_target/native_windows.rs` 受控 unsafe 边界，只有 Win32 SetFileInformationByHandle/CloseHandle 调用；没有 guest 指针、数值句柄或路径接口。其余目标模块维持 deny unsafe。独立复审发现并修复了关闭失败后重包句柄以及全进程失败状态的并发竞争。

Set 成功后用 into_raw_handle 转移所有权，恰好一次 CloseHandle。关闭失败时不重新包装、不重试未知有效性的数值句柄；保持 Unknown，禁止新变更目标准入，并保留资源计费至进程退出。底层句柄可能滞留至进程退出，此异常需要重启，不伪报资源已回收。真实内核 CloseHandle 失败未强行制造，不把源码审查当成该硬件/驱动异常的实机资格。

Deleted 表示所选目录项的删除经本轮 OS 调用及关闭确认；其他硬链接仍存在，不保证掉电后持久性。OsRejected 只记录 API 报告失败，不推断绝无底层副作用，也不允许自动重放。实现依据：[SetFileInformationByHandle](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle)、[CloseHandle](https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-closehandle)、[Rust IntoRawHandle](https://doc.rust-lang.org/std/os/windows/io/trait.IntoRawHandle.html)。

只读历史能报告 Unknown/Observed，不能自动判断跨重启的实际删除结果；目录/新建/条件替换、完整 Unknown 业务核对、原 owner 队列、公共 SDK/界面和其他平台继续开放。用户库与用户文件未参与测试。

## 验证

- Core 完整故障注入回归：**706 passed / 0 failed / 12 ignored**；82 个测试汇总（含 doc-tests），日志 `build/file-delete-core-full.log`。ignored 均为父测试调用的独立子进程入口。
- Windows runtime 组合：**135 passed / 0 failed / 3 ignored**，包含内部 53、file_owner 10、io_binding 9、io_execution 19、managed_file_io 44；日志 `build/file-delete-runtime-final.log`。三个 ignored 同为父测试调用的子进程入口。
- 完整回归后，最终新增了 outcome 构造前的长度/摘要校验，并给 Windows-only 只读文件清理添加一处局部 lint 注释；随后 Core 编码/删除 Store **8 项**、runtime 真实删除/崩溃 **11 项**定向复验通过，不重复累计到完整回归数。日志 `build/file-delete-core-targeted-final.log`、`build/file-delete-targeted-final.log`。
- Core 四处真实提交崩溃：claim 提交前/后、observe 提交前/后；证明只出现完整 Prepared、Unknown 或 Observed＋响应，不能半提交。runtime 三处真实 OS 执行崩溃：after-claim（原文件仍在、Unknown）、after-effect（文件已删、Unknown）、after-observe（文件已删、Observed）。全部子进程退出码 86，父测试重开原库核验；不会按未知结果再次执行。
- 实际临时文件覆盖只读属性导致的 OsRejected、被选中的硬链接删除而别名仍可读、外来 owner/错误计划拒绝、提交前和提交后撤权、同名后来文件不被旧操作删除、跨 broker 可重入调用 Busy 且未 claim、不丢失效果后的观察。
- Core/runtime Clippy 均通过 `-D warnings -A clippy::collapsible_if`；Windows-only 临时文件 `set_readonly(false)` 的跨平台 lint 有一处带说明的局部例外。日志 `build/file-delete-core-clippy.log`、`build/file-delete-runtime-clippy.log`。新增文件格式检查与 diff --check 通过。
- 一处初始 crash 测试把未产生响应误期望为 None；已按 Store 既有 EvidenceUnavailable 契约修正，未放宽生产行为。OS 效果测试仅使用测试创建的隔离临时文件。

主要命令：

```powershell
cargo test --locked --offline --manifest-path core/Cargo.toml --features fault-injection --no-fail-fast
cargo test --locked --offline --manifest-path plugin_runtime/Cargo.toml --features fault-injection --lib --test managed_file_io --test io_binding --test io_execution --test file_owner --no-fail-fast
```
