# Windows 文件创建的受控目录选择与持久准备

日期：2026-09-27。开发线 `codex/io-safety-refactor`，基线 `b9225f6`，应用版本仍为 `0.1.9-test.56+60`。本轮未提交、推送、发布或构建安装包。

## 已完成

- `TargetBroker::select_create` 接收可信宿主选择的根目录、经过原字符串校验的相对路径与 Create 审批范围。返回会话内不透明引用；引用不能跨 broker、释放或重启恢复授权。
- 根目录通过 Win32 打开；其下每个父目录通过 `NtCreateFile` 的 RootDirectory 原句柄逐段打开，不拼接绝对路径、不 canonicalize、不按原路径重开。只打开已有目录；每个返回句柄都检查目录类型并拒绝重解析点。
- 保留根与全部父目录句柄，只允许 READ 共享，阻止普通写/删除访问改变被保留目录。真实测试证明保留时不能改名根目录与中间目录，release 后恢复。此约束不锁定叶子文件内容。
- 每个目录独立占用原实例一个资源槽；整个链的资源在任何 OS open 前预留。已有资源上限最多为 8，不扩大授权。配额失败、缺父目录、类型不符、打开后撤权均释放全部句柄和预留；broker 的 128 个保留槽同时计入目录链、原文件选择和已消费引用。
- 请求必须精确匹配原引用、相对路径、subject、包摘要、审批摘要、Create 动作，且无 expected_identity。先按实际引用识别原选择、核验 owner，再检查计划动作；改变动作不能改走其他选择的权限。
- Create 复用既有 Prepared/内容暂存事务，保留确切计划、内容及审计回执。准备与暂存均检查原租约，提交后撤权继续沿用拒绝交付语义；没有第二套持久化逻辑。
- 本轮不打开、探测或修改最终叶子。因此已有文件即便被排他打开，准备与暂存仍可进行且不改变该文件；目标是否可创建必须由后续执行边界判断，Prepared 不是写入成功或写权限承诺。

## 验证与证据

- Windows 创建目标专项 **10 passed / 0 failed / 0 ignored**：中文多级目录、原内容与 LiveStaging 回执、现有叶子不触碰、缺失/非目录父段、配额先于 OS 打开、外来 owner/错计划拒绝、根与中间目录固定、撤权/到期回收、打开最后父段后失活、重选不能复活旧计划、真实 root/intermediate junction 拒绝。日志 `build/file-create-target-tests.log`。
- 最终 runtime 组合 **145 passed / 0 failed / 3 ignored**：内部 53、file_owner 10、io_binding 9、io_execution 19、managed_file_io 54。包含此前删除的真实效果、并发门闩和崩溃子进程回归；三个 ignored 为父测试调用的子进程入口。日志 `build/file-create-selection-runtime-final.log`。专项计数已包含在组合中，不重复相加。
- Core 既有计划/内容/回执 Store 定向 **28 passed / 0 failed**（12＋9＋7）。覆盖 Create/Replace 内容经重开与快照读取、旧入口拒绝文件变更派发、原件/镜像/审计绑定及容量失败。日志 `build/file-create-core-targeted.log`。本轮未更改 Core，也未重跑完整 Core 套件。
- runtime Clippy `-D warnings -A clippy::collapsible_if` 通过，沿用既有已记录 lint 例外；未为本轮新增抑制。日志 `build/file-create-selection-clippy.log`。具体文件 rustfmt 检查、Git diff --check 通过。
- 初次专项 junction 夹具因 cmd 引号编码失败，修正为只接受隔离临时路径的受检 raw_arg，并关闭 AutoRun、延迟展开与可见窗口；之后真实 junction 测试通过，没有把创建夹具失败视为生产拒绝的证明。
- 独立只读复审未发现本轮选择/准备/暂存中的可复现越权；复审提示的 DIRECTORY＋OPEN_REPARSE_POINT 组合已在本机普通目录与 junction 上实测。其他文件系统、目录 symlink、驱动异常及其他平台未由这组测试覆盖。

主要命令：

```powershell
cargo test --locked --offline --manifest-path plugin_runtime/Cargo.toml --features fault-injection --lib --test managed_file_io --test io_binding --test io_execution --test file_owner --no-fail-fast
cargo test --locked --offline --manifest-path core/Cargo.toml --features fault-injection --test file_mutation_store --test file_content_store --test file_content_receipt_store --no-fail-fast
cargo clippy --locked --offline --manifest-path plugin_runtime/Cargo.toml --features fault-injection --lib --test managed_file_io --test io_binding --test io_execution --test file_owner -- -D warnings -A clippy::collapsible_if
```

## 边界及后续

绝对根路径上级仍由可信选择器/Win32 解析；逐段防重解析只覆盖选中根及其下的相对父目录。后续产品入口必须让审批绑定实际打开的根目录对象，不能把界面早先显示过的路径文字当成防竞态证明。选择引用只证明当前保留对象，没有永久文件 ID、跨重启授权或本地物理磁盘承诺。

本轮尚未实现实际 Create 派发、临时文件写入/flush/不覆盖发布、崩溃 Unknown 核对或条件替换。通用 IO 派发仍拒绝文件变更，专用删除入口也只接受 Delete；不会把本轮 Prepared 当成可执行授权。完整文件 IO、原 owner 队列、公共 C/C++/Rust SDK、Flutter 界面和 SDK 冻结仍开放。

下一步先为 Create 增加专用一次性 Unknown claim 和确切内容/回执核验，再用保留父句柄实现不覆盖发布；临时文件同样属于外部效果，不能在 claim 前创建。需要单独验证目标已存在、部分写入、flush/发布失败、失权与进程崩溃，不按 Unknown 自动重放。

原生结构和 RootDirectory/同步打开语义依据微软 [NtCreateFile](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile)、[OBJECT_ATTRIBUTES](https://learn.microsoft.com/en-us/windows/win32/api/ntdef/ns-ntdef-_object_attributes) 与 [IO_STATUS_BLOCK](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/ns-wdm-_io_status_block)。文档依据不替代上述实测范围。
