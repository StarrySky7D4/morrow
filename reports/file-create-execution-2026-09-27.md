# Windows 文件创建执行与原子观察验收

日期：2026-09-27。基线 `codex/io-safety-refactor` / `b9225f64f6c62584ad7243e30249d8a088bcb155`；本轮为本地未提交增量，应用版本未变，未打包、推送或发布。

## 本轮结果

底层 `TargetBroker::create` 已从既有目录链选择、Prepared 与内容暂存，接通专用一次性 Unknown claim、临时文件写入、同步、不覆盖发布、原句柄关闭及 Response＋Observed 原子保存。实际 Windows 测试通过，不等于公共插件 SDK 或工作台创建功能已交付。

- Create 使用独立 `MROWFEC1` 有界 Protobuf＋LZ4 结果，绑定 operation、subject、精确请求摘要、原目标引用、内容摘要与长度。成功和正数 OS 错误均是历史结果，不提供重试许可；Delete 原有编码保持。
- 内容和回执、响应预留、原实例权限及预算先核验。临时文件额外占一个资源，执行额度包含请求、正文及响应上限；所有保留目录也分别计费。临时创建必须等到 Unknown 提交得到确认。
- 使用保留父目录的相对 `NtCreateFile(FILE_CREATE)`，不覆盖同名临时项，不使用 `FILE_DELETE_ON_CLOSE`。按 64 KiB 分块写入，逐块及同步／发布前核对权限，`sync_all` 后才发布。
- 同目录发布使用 `NtSetInformationFile(FileRenameInformation)`、NULL RootDirectory、单段 leaf、ReplaceIfExists=false。保留目录仍只共享 READ；没有放宽 WRITE／DELETE 共享。真实嵌套目录验证目标落在所选父目录，不依赖进程当前目录。
- 发布前失败仅尝试删除本次独占创建的临时对象。清理或关闭不确定时保留 Unknown；关闭失败不重试数值句柄，保持资源计费并阻止后续效果至重启。名称碰撞不清理原占用项。
- 实际发布／关闭结果即使随后撤权也保存到历史，交付仍需原权限有效；不因交付失败删除已发布文件或重复发送。
- `observe_file_create_local_authorized` 原子写入结果与 Observed，校验原计划、暂存内容、回执和完整材料摘要；历史读取拒绝缺失或不匹配的结果。通用文件响应写入和派发旁路保持关闭。

## 实测与检查

| 范围 | 结果 | 日志 |
|---|---|---|
| Core 全量，含 fault-injection | 724 passed / 0 failed / 14 ignored；84 个结果摘要（含 doc-tests） | `build/file-create-execution-core-full.log` |
| Runtime lib、file_owner、io_binding、io_execution、managed_file_io | 158 passed / 0 failed / 4 ignored | `build/file-create-execution-runtime-full.log` |
| 最终 Windows Create 专项，含新增临时名碰撞 | 14 passed / 0 failed / 1 ignored | `build/file-create-runtime-final.log` |
| Core 定向 Clippy；Runtime 相关 Clippy | 通过；`-D warnings -A clippy::collapsible_if` 沿用已有例外 | `build/file-create-execution-core-clippy.log`、`build/file-create-execution-runtime-clippy.log` |
| 修改文件 rustfmt、git diff --check | 通过；已有换行提示不算失败 | 本轮终端记录 |

专项与组合测试有重叠，不累加为 172 项。最终专项比组合增加一个临时名碰撞测试，覆盖普通文件、目录及硬链接；其余 13 项复验。相关唯一测试合计 159，但并非一次 159 项完整运行。ignored 包括父测试显式启动的子进程入口，也有既有独立慢测入口，不将其统一宣称已执行。

Create 专项覆盖嵌套中文路径、空内容、已有最终文件／目录不覆盖、缺内容／临时资源、错误 owner／计划、提交前拒绝、提交后失权、发布前失权清理、发布或观察后撤权、同操作防重放、broker 丢失及跨 broker 并发门。七个真实进程退出点：claim、temp、write、flush、publish、effect、observe 之后。发布前保持 Unknown，可能留临时文件；发布后未观察保持 Unknown；观察提交后保留完整 Response＋Observed。Core 另验观察事务前／后进程崩溃、错误绑定、授权回滚、内容／回执缺失、重开、封存及完整备份。

首次补充硬链接碰撞夹具失败于 Windows error 32：夹具尝试在已限制 WRITE 共享的父目录内新建硬链接。修正为先创建占位文件，再向同 TempDir 的未固定目录创建外部别名；生产共享限制未改变。修正后的三类碰撞均验证占用项及别名字节不变。首次失败日志保留在 `build/file-create-temp-collision.log`，最终结果以上表最终专项为准。

独立只读代码复核未发现可复现的本轮阻断，核对了授权／资源顺序、原生 ABI、不覆盖发布、关闭不确定处理与观察事务；复核者未重跑测试，实测证据以上述主验收日志为准。

## 边界与后续

1. 仅在本机 Windows 普通文件系统验证。未新增其他平台、网络盘、所有重解析类型、真实磁盘写满／设备掉线、失败 CloseHandle 或突然断电资格；`sync_all`＋发布成功不承诺断电后名称持久性。失败关闭处理经过代码审查，未通过人为破坏句柄来冒充真实失败测试。
2. 进程崩溃可能留下临时文件或已发布但未保存观察的文件；现阶段只允许核对，不能按名称自动删除、按内容相同猜测归属或自动重放。跨重启对象身份证据与受控清理仍待实现。
3. 内容 codec 上限 16 MiB 不是执行承诺：执行需同时计入命令和响应，实际可接受内容受原实例字节、任务及资源预算约束，最大正文可能低于 16 MiB。深目录同样受资源上限约束。
4. 根目录上级路径仍属于可信选择器的审批边界；UI 接线必须将审批关联到实际打开的根，不重新使用历史路径作为授权。
5. 条件替换、变更任务接入原 owner 队列、公共 C／C++／Rust SDK、Flutter 操作与审批界面、完整 Unknown 核对及平台资格仍开放；SDK 未冻结。

下一步先设计并验证条件替换的原对象身份、同目录发布和失败恢复边界；同时收敛变更 owner 命令契约，再接 SDK／UI，避免把同步磁盘调用放入 UI 线程。

参考：微软 [FILE_RENAME_INFORMATION](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information)、[NtSetInformationFile](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntsetinformationfile)、[FILE_DISPOSITION_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_disposition_info)。
