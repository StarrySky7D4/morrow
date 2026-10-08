<!-- C16-CURRENT-BEGIN -->
2026-10-06 C16 本地实验性候选：新增可信 Rust Agent start/submit/poll/cancel/recover/acknowledge
入口，移动完整原 owner，沿用同 Core/Store/runtime/连接；每 import 维护原 owner。
借用执行与 scheduler 回收有界；真实 join 后仅同步唯一 Runtime 所有权可回收，别名保留 16 有界债务。
普通无沙箱、非 ProtectedSession 的封存 Rust Wasm＋实际 Windows worker 耦合 3 项通过；各组收据见报告，
不作为生产 GUI、真实受保护库/DPAPI 或生产沙箱资格。旧 SDK327/57、合同、Linux 与 C15 工件守卫通过。
接口仍 experimental，SDK26/G04 OPEN；未提交、推送、Release 或 CI，不代表正式 SDK 冻结。
见 [原生 owner 接口](PLUGIN_AGENT_NATIVE_OWNER.md) 与 [C16 报告](../reports/reconstruction-2026-10-06/codex-sdk-c16.md)；下方 C15 及更早内容保留其历史范围。
<!-- C16-CURRENT-END -->

<!-- C15-ADMIN-ENTRY -->
当前 C15 管理入口：`extensions/agent-catalog-admin-v1` 使用独立私有 Cap’n Proto profile，
`extensions/agent-catalog-owner-v1` 借原 Manager 与 C14 Catalog，工作台原管道接新设置界面。
review/install/baseSelect/baseEnable/wrapperSelect/approve/wrapperEnable/remove/page 为独立决定。
ID 与双 revision 只核当前 owner；Unknown 不自动重试，查询不会恢复写权限。没有运行按钮。
native 执行器借原 ProtectedSession/runtime/worker、真正桌面验收及全 SDK26/G04 仍 OPEN。
当前证据见 [C15 报告](../reports/reconstruction-2026-10-06/codex-sdk-c15.md)。
<!-- /C15-ADMIN-ENTRY -->

# Agent 完整包装与持久审批接口

2026-10-06，C14 本地候选，`morrow-agent-session-process-v1-host::catalog`。
本接口针对完整 `MROWASP1` 包；基础 `MORROWP1` selection、内容库与实时授权继续归原宿主管理。
实现、限定测试和产品接入是不同门槛；当前资格以本轮报告和原始收据为准。

## 宿主调用流程

1. `Catalog::inspect(bytes)` 返回完整包装 SHA、基础包 SHA、两个 schema 摘要，以及声明的会话、进程、session 和执行域。检查不会保存或批准。
2. `Catalog::install(bytes, expected_sha, manager, Revisions)` 保存不可变归档并登记目录。它不选择或启用原基础包，不授予会话或进程权限。
3. 通过原 `Manager` 明确选择基础包；传入实际的 manager revision，再用 `Catalog::select` 选择完整包装。初始状态未批准且未启用。
4. `Catalog::approve` 保存声明以内的能力、session 范围和相同 execution domain。批准后仍需显式启用；原基础包也必须已启用。
5. `Catalog::connect` 核对两个当前 revision、完整包装和原选择，借原 `HostRuntime`／`SessionExecHost` 建立新的 managed admission。返回 `CatalogManagedPackage` 复用同一个原 `Arc<Connection>`。
6. `run`、`register_process` 继续执行原 Manager 与 R2 的实时校验。停用、撤销、更改审批、catalog 销毁或存储失效后，旧实例不能恢复。
7. `owned_handles`／`close` 保留可信收尾入口。实际退出、EOF、耐久终态与 `finish` 仍由原 owner 确认；清理请求不等于资源已释放。

分页目录采用完整包装 SHA 的游标和精确 catalog revision，一次最多 16 项。`Revisions` 的两个数字各自对应原 Manager 和新包装目录，不可互换。当前目录变更采用整体版本失效：发布新快照会停止该目录已发出的全部实例，之后须重新建立实时授权。

## 持久化与失效

`WrapperCatalogStorage` 是可信独占存储接口。native `NativeCatalogStorage` 复用原 `SqliteRegistryStorage` 的独占租约、同步策略和紧凑快照发布；完整包装归档保存在同根目录下按完整 SHA 寻址的不可变文件中。原 SQLite package cache 接收真实基础包，不能用新包装冒充旧包。

新的审批快照采用独立版本的 canonical Protobuf＋LZ4 容器，不修改旧 registry envelope 或 schema。快照记录静态能力天花板，原 live Connection、Admission、Control、permit、Claim、进程 handle 和 Unknown 效果均不序列化。

完整包装不塞入原 512 KiB 快照，也不降低合法包装的归档大小上限。目录数量、分页和审批快照有明确上限；原 Manager 的 128 个实例配额仍共享。不可变归档写入与审批快照不是跨存储原子事务，失败留下的归档不产生授权。

审批决策先停止同基础包的原 Control，再发布完整快照。原实例表无法区分这一步的 profile，因此撤销接口保守停止同包的普通实例。保存失败不会复活它们；不确定提交需重开、核验实际磁盘状态并重新建立实时授权，不自动重放。

## 产品接入仍须完成

当前旧工作台 `PluginInspect`／`PluginImport`／`PluginApprove` 不能表示完整包装审批。正式接入需要独立版本协议与界面，完整展示双 schema、两个摘要、声明和批准范围；不能把 session／process 权限挤进旧 Core／IO 字段。

工作台应将包装目录随原 `WorkbenchState`、Storage 和 Manager 一起持有，遵循原 busy／lost／恢复与写入门禁。执行时借用原 ProtectedSession 的 runtime，并衔接实际 native owner／工作线程；不能另开内容库或复制 owner。

该接口本身不证明认证 Codex transport、生产 OS sandbox、托管网络、PTY／resize／interrupt 或其他平台。SDK26／G04 保持 OPEN；参见 [本轮证据](../reports/reconstruction-2026-10-06/codex-sdk-c14.md)。
