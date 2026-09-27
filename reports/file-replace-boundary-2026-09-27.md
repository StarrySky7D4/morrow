# 条件替换持久协议与 Windows 平台验收

日期：2026-09-27。开发分支 `codex/io-safety-refactor`，基线 `b9225f64f6c62584ad7243e30249d8a088bcb155`。本地增量，未更改应用版本、未打包／推送／发布。

## 结论与实现

**Core 已补充 Replace 的持久执行／观察协议；当前 Windows 后端明确不支持满足预期对象条件的替换。** 数据库测试验证历史与恢复规则，不构成实际文件替换成功证明。此结论遵循 `PLUGIN_IO_DESIGN.md` 第 4 节的平台不支持规则，没有将 replace 改成无条件覆盖或原地截断。

新增独立 `MROWFER1` Protobuf＋LZ4 `ReplaceOutcome`，绑定 operation、subject、请求摘要、原目标引用、预期对象身份、正文摘要与长度。结果分为 Replaced 和正数 OS 错误；与 Create／Delete 编码互不混用。

`claim_file_replace_local_authorized` 仅接受精确 Prepared 请求，在原内容／审计回执／响应预留齐全后，以单事务记录一次性 Unknown。回执必须早于派发。`observe_file_replace_local_authorized` 核验所有绑定和完整响应材料摘要，将 Response＋Observed 原子保存。重复派发、缺失或错配证据、错误权限、通用派发／响应写入／预留释放旁路均被拒绝；恢复仍不能自动重放。

这两个 Store 方法只供可信宿主记录执行事实，不判断文件系统现场身份、不授予 OS 权限。宿主必须先证实平台具备所需条件语义，才能调用 claim。数据库测试中的 Replaced 是人工构造的宿主观察，只用于验证协议。

Windows `TargetBroker::replace` 先验证原实例、存活授权、精确所选目标和元数据，再返回 `UnsupportedConditionalReplacement`。此拒绝不 claim，不创建临时项、不消耗执行任务／字节预算，不修改 Prepared、暂存材料或原文件；选择仍可由原 owner 释放。不会通过关闭旧句柄、放宽共享或改用路径覆盖来规避限制。

## Windows 实际原型：排除不安全路径

环境：Windows NT 10.0.26200.0，临时目录位于 C: NTFS。原型仅在独立 TempDir 运行，代码仅在测试构建启用，正常回归中显式忽略，需 `--ignored --nocapture` 运行。它们是能力测量／竞争反例，测试返回成功不表示条件替换受支持。

1. 保留父目录 READ 共享与原文件 share=0，写好并同步临时文件，用 `NtSetInformationFile` class 65（FileRenameInformationEx）、flags 3（REPLACE_IF_EXISTS＋POSIX_SEMANTICS）、NULL RootDirectory＋单段叶名尝试发布：返回 `NTSTATUS 0xc0000043`、Win32 **32 / sharing violation**。原文件、原硬链接字节不变，候选临时字节仍在。
2. 仅在反例夹具将原文件共享改为 FILE_SHARE_DELETE，保持父目录不变。先由另一候选替换目标名称，老句柄仍能读取原字节；再按原名称发布本方候选，结果覆盖了中途出现的文件。终端输出 `CAS_COUNTEREXAMPLE_CONFIRMED`。这证明该共享放宽方案不保留预期对象条件。

官方 `FILE_RENAME_INFORMATION`／Ex 只携带源句柄、目标目录／名称及 flags，没有目标预期 fileID 参数。新增 fileID／名称前置检查可以发现既有漂移，但单独的检查后重命名仍有竞争窗口；不能凭它声明原子比较替换。以上实测仅排除本轮候选路径，不宣称所有 Windows API 或所有文件系统永远无法实现条件操作。

原型及日志：`plugin_runtime/src/file_target/native_windows/replacement_qualification.rs`，`build/file-replace-native-qualification.log`。两个原型完成，2 passed；必须结合上述否定结果阅读。

## 验证

| 范围 | 本轮结果 | 日志 |
|---|---|---|
| Core 全量＋fault-injection | 737 passed / 0 failed / 15 ignored；86 个摘要含 doc-tests | `build/file-replace-core-full.log` |
| Runtime lib／file_owner／io_binding／io_execution／managed_file_io | 162 passed / 0 failed / 6 ignored | `build/file-replace-runtime-full.log` |
| Replace Core 定向 | codec 4＋Store 9 通过；1 个子进程入口由父测试调用 | `build/file-replace-core-targeted.log` |
| Windows Unsupported 真实文件专项 | 3 passed / 0 failed | `build/file-replace-runtime-targeted.log` |
| 两个显式原型 | 2 passed，结果是上述失败能力探测与竞争反例 | `build/file-replace-native-qualification.log` |
| Core／Runtime Clippy | 通过，`-D warnings -A clippy::collapsible_if` 保持既有例外 | `build/file-replace-core-clippy.log`、`build/file-replace-runtime-clippy.log` |
| 本轮文件 rustfmt／git diff --check | 通过 | 终端记录 |

分项有重叠，不相加为产品通过率。常规 ignored 包括子进程入口、既有独立慢测及两个显式原型，不将所有 ignored 一概称为已执行。Core 新增测试验证完整请求与结果绑定、不同结果类型不能混用、内容／回执／响应预留缺失拒绝、授权前置及回滚、完整备份重开／封存、claim 和 observe 各自提交前后真实进程退出。运行时拒绝测试验证原文件／硬链接和准备材料保持原样、无执行额度增量、重复调用不派发、foreign owner／错误计划先拒绝。

编译过程曾撞到并发编辑期间尚未建立的 `replace.rs` 模块，因此未执行原型；等待 Core 编译就绪后重跑成功。测试补充时一处 `Ok()` 拼写错误也被编译发现并修正。最终日志以上表为准；这些过程错误不算功能通过或未解决失败。

独立只读复核检查了 Core 绑定／历史闭包／通用旁路和 Windows 无效果拒绝入口，未发现阻断缺陷；复核者未重跑 Cargo，运行结果以主验收日志为准。

## 下一步与未完成范围

- 接线前先为 `TargetBroker` 增加单操作取消和持锁取时核验接口，并解决同 owner 的 Manager／可变 Runtime 分借用。不能伪造过期时间或撤销整个实例来取消一个命令；claim 前取消、claim 后 Unknown、效果后保存历史需要分别验证。
- 让已验证的 Create／Delete 进入原 owner 有界队列：选择、Prepared、内容暂存、执行及历史查询都归原宿主；句柄不得迁往另一个 owner，取消和退出不产生自动重发。
- Replace 必须在 worker 进入 claim 前反馈平台不支持，SDK／UI 保留独立能力与准确错误；已有暂存可取消或保留，不能替用户转成 Create、无条件覆盖或多步可见的改名交换。
- 增量写入协议仍需处理分块长度／摘要、额度、丢回执及跨重启 Unknown。临时项清理需要对象归属证据，不能凭同名或相同内容自动删除。
- 公共 C／C++／Rust SDK、Flutter 操作／审批界面、文件列举、其他平台和完整业务恢复继续开放。SDK 未冻结；本轮没有新增成品包验收。

参考：[FILE_RENAME_INFORMATION](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information)、[FileRenameInformationEx 字段](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-fscc/4217551b-d2c0-42cb-9dc1-69a716cf6d0c)、[FILE_INFORMATION_CLASS](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/ne-wdm-_file_information_class)。
