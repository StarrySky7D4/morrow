# Windows 文件变更目标绑定与持久准备接线

日期：2026-09-27。开发基线 `b9225f6`；应用版本仍为 `0.1.9-test.56+60`。本轮未提交、推送或发布。

## 已实现

- 新增 Windows `TargetBroker`，由可信宿主选择已存在的文件，保留实际 `File` 与原实例 `IoResourceLease`。只接受替换/删除动作；选择本身不创建、截断或删除文件，不返回原生句柄给插件。
- 完整指定访问位、排他共享与最终分量不跟随 reparse 的打开标志；读取已打开句柄的元数据，拒绝非普通文件/最终重解析点。检查原始路径拼写，拒绝相对路径、设备命名空间、UNC、ADS、点/父分量和非法名称；不以归一化后的路径分段代替检查。
- 目标引用由宿主会话秘密、进程内 broker 身份及序号产生。选择期 expected_identity 绑定目标引用和元数据；它不是稳定文件 ID、内容摘要、跨重启身份或系统 CAS。
- 校验计划时绑定原 manager/host/instance、包摘要、主体、审批摘要、动作、目标引用和 expected_identity，拒绝 guest-relative path。元数据读取前后复查活性。释放/reap/Drop 关闭句柄、归还资源预算；历史记录不能恢复句柄。
- 新增 `prepare_request` 与 `stage_content`，将选中文件的实时授权接到已有 Core 原子计划/审计暂存事务。使用原实例共享的 job/累计字节预算，重复调用仍计入流量；Core 另行计入持久化额度。
- 所有 Store 授权检查使用原 binding 的实时活性。提交前拒绝返回 Admission，明确提交成功但返回前失权返回 CommittedButDeliveryDenied；Store CommitUnknown 保留独立持久化错误。三者不能混同为“没有写入”。

## 边界

这是实际句柄选择及准备接线，尚不是文件效果执行。create/目录授权、条件替换/删除、dispatch claim、Unknown 核对、owner 队列与公共 SDK/界面入口继续开放。

目标路径来自可信宿主选择；最终分量不跟随 reparse 不等于父目录无重解析。父目录可改名/替换，本模块不保存路径供后续效果使用，也不把此检查宣称为受控目录遍历。磁盘盘符可能由重定向器承载，不是本地卷证明。后续效果不能通过释放原句柄后按旧路径重开来延续当前授权。

排他共享会暂时阻止其他应用新开读/写/删句柄，必须由 owner 及时调用 reap/释放。未来后台挂接不得让 UI 长期占用这些句柄。当前 API 只对 Windows 编译开放，不宣称其他平台资格。

实现依据：[Rust OpenOptionsExt](https://doc.rust-lang.org/stable/std/os/windows/fs/trait.OpenOptionsExt.html)、[Microsoft CreateFile](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea)。实际访问限制以本机临时文件测试为准。

## 验证

- 本轮 Windows 运行时内部测试 **52 项通过**；文件任务/授权/执行组合 **70 项通过**（file_owner 10、io_binding 9、io_execution 18、managed_file_io 33），共 **122 passed / 0 failed / 0 ignored**。新增 mutation_target 10 项已包含在组合结果中，不重复计数。
- 命令：`cargo test --locked --offline --manifest-path plugin_runtime/Cargo.toml --features packages --lib`；`cargo test --locked --offline --manifest-path plugin_runtime/Cargo.toml --features packages --test managed_file_io --test io_binding --test io_execution --test file_owner --no-fail-fast`。
- 日志：`build/file-target-runtime-lib.log`、`build/file-target-regression.log`。真实临时文件覆盖排他读/写/删/改名限制、release/Drop/reap 后恢复、symlink 拒绝、原 owner/scope/计划绑定、配额共用、失败释放、Core 精确持久原件/回执、准确重试、错误载荷、提交前撤权回滚和提交后交付拒绝。通用文件 dispatch 仍明确返回 UnsupportedVersion。
- `cargo clippy --locked --offline --manifest-path plugin_runtime/Cargo.toml --features packages --lib --test managed_file_io -- -D warnings -A clippy::collapsible_if` 通过，日志 `build/file-target-clippy.log`。保留现有 lint 类别例外；新增文件 rustfmt 和 `git diff --check` 通过。
- 初轮测试的一处失败来自 Windows sharing violation 在 Rust 中映射为 Uncategorized，而非测试预期的 PermissionDenied。已改为核验实际 Win32 错误码 32；最终测试通过。新增夹具一个缺失 mut 的编译错误已修复。
- 独立只读审查指出提交后交付拒绝错误歧义，已用专门错误类型修正并实测两侧持久化结果。没有新增 unsafe、外部依赖或放宽既有 unsafe 边界。
- 未运行其他平台、完整工作台 UI 或安装包构建，不计为文件变更功能已在产品中开放；上一轮 Core 698 项结果是独立历史验证，不叠加成本轮测试数。
