# Windows 生产 owner 显式恢复 010（2026-10-01）

本轮承接生产 supervisor 009。分支 `codex/m03-stream-revocation-backpressure`，HEAD 仍为 `04b060ef2a8ae7e7806af7b0dcc772308e9b07e4`；全部源码/文档保留为未提交工作树。未运行 Actions/CI，未安装服务、自启动或提权，未推送/合并/部署，未改写历史 evidence 或用户旧 Unknown 库。

当前 Windows 默认产品链已实际接入 Flutter → 独立 workbench supervisor → 原 Rust host → Wasmi。这里的 Wasmi guest 是 host 内模块；实验 M03 native guest 的独立进程协议仍另外验收。本轮不是整个 Codex/CC Switch G0、SDK 冻结或 84 项产品验收。

## 生产实现

- `native_pipe_win_001/src/job.rs`/`job_process.rs`：从真实持有的 process HANDLE 查询 creation FILETIME；child 在原 CREATE_SUSPENDED handle、resume 前绑定，不通过 PID 扫描补身份。GetProcessTimes 的 creation/查询权限语义见 [Microsoft 文档](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes)。creation 仅排除原 writer 仍活着，不能补 Job/EOF/worker 证明。
- `workbench_host/src/workbench_supervision.rs`：新 owner JSON v2 绑定 supervisor/host/package SHA、sup/child PID+creation、canonical profile、代次、incarnation 与恢复历史 head。合法 v1 Released 可以新 claim 升到 v2；v1 非终态不补身份、不自动迁移恢复。旧二进制拒绝 v2。正常 Released 原有完整资源/正常退出条件不放宽。
- `owner_recovery.rs`：只读 preview，token 覆盖原 SQLite record 原文/digest、schema、canonical profile、历史 head 与恢复判定。同一 IMMEDIATE 事务重新验证全部条件，原文+digest 双 CAS，归档原始异常记录及管理凭据，写入独立 `Recovered`。历史 head 连锁校验且 v2 缺表拒绝；新代次正常 update 前后也校验历史，历史丢失不得向 UI 报 durable 正常完成。
- 恢复只接受原 v2 Closing、全部原资源 flags/gate revoked、Unknown、明确 normal=false、原 supervisor 已终止/不存在或不同 creation 被复用。查询错误/访问拒绝保守拒绝；PID 消失从来不是资源证明。
- `workbench_supervisor.rs` 复用同一 exe 的临时 `--owner-preview`/`--owner-recover` 管理入口，不另建 Job、guest 或常驻层。仅 Recovered 可供新的 supervisor claim generation+1；direct host/maintenance 仍拒绝。原代次、incarnation、异常 exit、Unknown 及历史不覆盖，不重放任何 attempt。
- Flutter `workbench_owner_management.dart` 校验真实管理进程结果、profile、原产物 SHA、原记录/请求 token。`session_coordinator.dart` 额外验证当前 UI 代次和实际 transport exit，资源恢复不修改原 verified exit Future，不制造 exit0。默认 `main_rust.dart` 的启动失败和运行故障恢复界面均接入只读检查、明确 Unknown 勾选确认和显式恢复；恢复页一直保留直到用户自己 Retry。部分新增提示提供中文/英文，其他 locale 为英文 fallback；全语言人工验收未完成。

## 故障窗口与安全边界

| 窗口 | 当前可验证行为 |
|---|---|
| 正常关闭 | 原 worker/资源完成且正常 proof 持久化才 Released；协议 ACK 另计，产品 Wasmi 路径为 not_applicable |
| host 崩溃、supervisor 活着且完整清理证明落盘 | 原 exit72/sup2 保留；原 sup 结束后可显式 Recovered；新 claim 才新代次，业务结果 Unknown |
| UI 崩溃、supervisor 完成清理并落盘 | 原 gate 立即撤销，资源回收与 owner证明有界；有完整 v2 证明后可显式恢复 |
| sup 在 Pending/Active 或完整证明落盘前崩溃 | 即使 Job kill-on-close、持有 HANDLE 已 signaled，也缺原 IO/job/控制回收证明；原 owner 保守保留，拒绝恢复 |
| sup 落盘完整异常证明后崩溃/重启 | 在身份、同产物、原证明、历史和 CAS 全部匹配时可由同 exe 临时管理入口显式修复；当前实测 host/UI fault 使用原 sup 自行 exit2。精确 kill-between-commit/exit 的专门注入未另验收 |
| 恢复 commit/output 不明 | UI 不报成功、不 claim、不重试修复；重新 preview 读取实际状态。已 Recovered 的记录可关闭窗口后手动重新打开，绝不重放业务 |

本地 SHA 是一致性/CAS 绑定，不证明抵抗可重写同用户 ledger 并重算所有哈希的恶意用户。当前匿名 Job 由 supervisor 单一可信 owner 持有；当其先于完整证明崩溃，PID 消失/超时无法补回已失去的 Job/IO 观测。该窗口需未来明确选择可存活证明来源的架构，当前不增加第二常驻 supervisor、服务、安全身份或权限。保持拒绝是合同要求，不把 exit2 放宽为通过。

## 本轮证据

`host/m03-owner-recovery-010-qualification-004/`：9 个本地命令 exit0；分析/生产构建、ledger 10/10、生产 gate 1/1、原真实 worker shutdown 5/5、Flutter UI/session 15/15、真实生产 native 客户端 8/8。最后 8 项含原 009 正常写入/跨打开、host/sup 崩溃、并发库、原 service 实际 loopback listener，以及 3 个新增真实 owner 恢复场景。后者用实际生产 exe 和持有 kernel HANDLE 的 TerminateProcess(72)，没有手工构造资源 proof。

- host 故障：原 child72、sup2，完整 Closing proof →显式 Recovered normal=false Unknown →新 generation2、新 incarnation、独立正常 exit0；Recovered direct host拒绝、旧 preview拒绝。
- sup 故障：原 sup72/child实际回收，原 Active digest unchanged、history0，preview缺证明，repair2并拒绝重新打开。
- 并发：两个实际临时管理进程共用一 preview，仅一个 exit0、另一个 exit2(database locked)；history1，新代次只一个 owner。
- 独立只读复核已验证 5 个实际产物 SHA、三份 JSON、SQLite 原文/digest/receipt/hash链、完整原异常归档和当前代次。相关 WAL 长度0，核验未漏未合并记录。复核修复了历史丢失终态及 UI 恢复页提前退出的问题；未发现新增 P1。
- ledger 合同单测的 proof flags/模拟 PID reuse 是测试 fixture，只能证明判定/CAS/历史合同。真实 OS 清理结论来自上述真实进程及桌面 evidence，不能用单测替代。

`host/m03-owner-recovery-010-desktop-004/`：实际默认 `flutter build windows --release --no-pub` exit0，164.237737 秒；新 source/bundle冻结。默认构建也编译真实 workbench Wasm，但包版本字节仍按旧 SHA 显式保留，没有用新 Wasm覆盖同版本旧包。

`desktop-004/run/`：正常启动产生真实 workbench_frame/revealed_frame、三持有 HANDLE 正常退出；真实 UI kill 后旧资源 proof/Unknown 保留、自动 reopen拒绝，随后同 Release supervisor显式恢复、原 raw history逐字归档、direct host仍拒绝、新generation2实际 Flutter两帧及正常三进程退出均通过。此项不是实际鼠标点击 recovery 确认对话框验收；按钮/确认/缺证明/非自动Retry由 widget测试覆盖。

当前追加的 Release host/sup 故障及 native WM_CLOSE、完整 host 198项回归待读取完成回执后更新；不会把未完成状态写为通过。

保留失败/未确认：ledger001原 canonical profile mismatch(6/9)，修复后 ledger002为9/9；qualification001/002为新测试分析错误，003为两处测试控件查找失败(13pass/2fail)，004全部上述focused通过。桌面001连接中断无receipt，002/003恢复前冻结配置断言拒绝且没有实际构建；004核实唯一差异为 Flutter生成 FLUTTER_TARGET 的路径分隔符，并冻结旧/新原文，所有其他输入相同。全部失败未覆盖。

## 真实 G0 剩余分类

依据 `joint/g0-checklist.json` 的 C05–C09、原计划 `03_契约交接与验收.md` G0定义/84矩阵及 `joint/review-round6-2026-09-28.md`；历史 coverage.csv 不能当作最新009/010无证据，也不能用 focused局部结果改成全部通过。

- 本机后续仍可执行：C05/C06实际固定 Codex/CC Switch native/Wasm产品 target、feature/lock/patch闭包；C07/C08同真实产物的成功 IPC/backend、完整 bypass和M04 writer/M06exec生命周期；QX/KX坏包/缺依赖/旧dist负测；真实系统选择器和产品鼠标交互。现 companion build-contract 的两产品manifest/lock仍为空、portable slice未实现；既有P02 24/109 qualification是已编译原源码探针，不是双产品graph。Workbench默认Flutter/Wasmi成功不能替代此门槛。SDK仍experimental。
- 其他平台/设备：macOS/Linux、Android、iOS/HMOS及Web独立平台资格；成功非零 OS短写 completion仍缺 qualifying观察，pending/cancel/error/qualification partial不替代；真正断电/存储耐久需故障设施或硬件，本轮无实际关机。
- 架构/权限决定：恶意同用户隔离的安全身份/ACL；无原证明 sup崩溃后的可信证明来源；真实账号/登录刷新/公网模型端点与费用范围；安装/签名/系统权限/真实断电须对应授权。此次独立sup许可不含这些权限。

单一ZIP备份 delivery-receipt 与独立Python packager交付仍按原交接范围保留，不因本轮构建/本地证据宣称上传或交付完成，不重复上传。
