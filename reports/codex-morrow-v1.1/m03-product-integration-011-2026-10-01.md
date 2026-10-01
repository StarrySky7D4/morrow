# Windows 产品监督接入 011（2026-10-01）

本轮完成默认 Flutter 工作台 → 独立可信 supervisor → 原 Rust 业务 host/内置 Wasmi guest 的 Windows 启动、控制、故障保留、显式资源恢复与正常退出链。分支仍为 `codex/m03-stream-revocation-backpressure`，HEAD 仍为 `04b060ef2a8ae7e7806af7b0dcc772308e9b07e4`；本轮源码/文档未提交。既有未提交工作、原 Unknown run 与历史密封证据保留。未触发 Actions/CI，未推送、合并、部署、安装服务、自启动或提权。

## 生产实现与测试工具的界线

| 范围 | 本轮变化 |
|---|---|
| 生产 Rust `workbench_host/src/workbench_supervisor.rs` | host 的 Data 写失败不再 `?` 提前返回；立即 disconnect/关闭业务 gate，转入原 Closing、stdin 回收、原 child exit、读任务 join/双 EOF/Job 0 和持久证明流程。保留异常 exit 2、Unknown，不伪造正常关闭。 |
| 生产 Rust `workbench_host/src/owner_recovery.rs` | 只读再预览对完整历史验证通过且当前三产物 SHA 一致的 Recovered，输出 `resource_recovery_confirmed` 与归档原 digest；普通 Released、损坏历史、产物漂移不允许借此清除 UI owner。 |
| 生产 Flutter `lib/main_rust.dart`、`session_coordinator.dart`、`workbench_owner_management.dart`、`workbench_recovery.dart` | 未确认请求保留原 token/digest/身份，回读绑定 UI 代次、profile、incarnation、真实 transport exit 与历史原文；显示缺证明、仍存活、过期预览、产物漂移、提交/输出未确认诊断。恢复后原失败与 Unknown 仍在，必须用户手动 Retry 才 claim 新 generation。 |
| 测试 fixture/工具 | `test/workbench_owner_recovery_native_test.dart` 增加原 held HANDLE creation/exit/live-owner 验收；新 process/auditor/G0 graph 工具和测试，qualifier 加只读原文审计及仅 analyzer 使用的私有缓存。外置 Running/Data fixture 和 WM_CLOSE runner 不进入生产 host。 |

复用 009/010 的 Job、ControllerLease、原 worker 和持久 owner 核心，没有新增第二常驻监督器。管理恢复复用同一个 supervisor exe 的短时入口；不重放 guest/旧 attempt，不直接删除数据库解锁。010 的 v2 JSON 身份、原 creation FILETIME、完整资源证明与原文/receipt/history 双 CAS 约束继续保持；sup 在原证明写入前崩溃仍无法证明释放，必须保守拒绝。

## 统一候选和实测

1515 个源文件的当前字节与资格候选一致，默认 Release 用同一源清单，50 个 bundle 文件冻结；全 host 库测试的 682 个相关源 pins 也与该候选完全一致，独立复核零差异。

- Debug candidate manifest：`f507b6d15902c945772f149ecececa6b777e0fefc16fa4a00f8b73e96ce324b6`。
- 默认 Release candidate manifest：`c77c7bf1a526033b521a65ea55277bbaee81d536554034bf854b7de03e2c6969`。
- 既有同版本 plugin 包保持 SHA `6c7d349c61f2aed0f4edded243865321e9ef21d3f3343d3df1c5c8020c9373db`。不将新 Wasm 暗中替换原同版本包。

| 范围与证据目录（下列均在 `host/`） | 真实结果与限制 |
|---|---|
| `m03-owner-recovery-011-qualification-002` | 11 条命令 exit 0；analyze 无问题，ledger 11、gate 1、原 worker shutdown 5、UI/session 24、实际 native 9、auditor 12 均通过；4 库原文/receipt/hash 链及 held PID+creation 独立审计通过。属于 focused qualification。 |
| `m03-owner-recovery-011-host-full-lib-001` | 完整 `workbench_host --lib` 199/199，0 failed/ignored/filtered，403.324 秒；源一致。使用三个冻结既有真实 Rust guest Wasm，不将这项说成重新资格化 guest 源码、全 UI 或 G0。 |
| `m03-owner-recovery-011-desktop-001` | 默认 `flutter build windows --release --no-pub` exit 0，142.927743 秒，源前后不变。 |
| `m03-owner-recovery-011-desktop-faults-001` | 同一新 Release 的 normal、host、supervisor、UI 四案通过；actual Flutter frames、live-owner 拒绝、host 双 manager CAS [0,2]、原文归档、stale token/direct host 拒绝均核验。该目录整体保留窗口测试失败，不改写为矩阵全部通过。 |
| `m03-owner-recovery-011-desktop-early-001` | Active 原身份观测后 0.0355994 秒注入真实 host 故障；原完整 Closing proof/Unknown、资源恢复/CAS 通过。Active 观测本身不等同已安装 reader，更早 bootstrap 未证明窗口仍可能保守拒绝。 |
| `m03-owner-recovery-011-desktop-window-002` | 第 5 次命中 full-proof commit→exit：原 held supervisor 仍 live+suspended 时 preview 拒绝、recover 2 且原文不变；随后仅终止该原 handle 才恢复。前 4 次 not_observed 保留，0.9 秒暂停界限未放宽。该目录只选窗口案，其余四案明确 not_run。 |
| `m03-owner-recovery-011-running-data-new-debug-001` / `old-debug-001` | 在 actual Running 后以合法 128KiB controller-wire fixture、真实 4096 quota pipe、32772 pending bytes、持续 heartbeat 及原 host 故障作对照。旧候选 Broken pipe/os109 并留下 Active、全 proof false；新候选取得完整 Closing proof、sup2、Unknown 并原文恢复。属于真实 OS 转发加 controller fixture；私有 OVERLAPPED/精确 WriteFile 错误分支未直接采样，不冒充实际 Flutter或成功非零短写完成。 |
| `m03-owner-recovery-011-wmclose-001` | 无 startup-check 自动退出，实际向本轮 held UI 的 Flutter runner 主 HWND 发 WM_CLOSE；0.1269869 秒内 UI/sup/host 全 exit0，durable Released/normaltrue/all7proof/history0。真实 visibility=false，仅覆盖产品正常关闭 hook，未声称鼠标或可见桌面交互。55 密封文件独立审计通过。 |
| `m03-shared-supervisor-core-011-check-001` / `run-001` | 15/15 检查、16/16 actual control/Job/Recovery cases；54 held 记录完成，无 cleanup error。实际 normal/expiry-normal ACK，no-close 保持协议失败；HTTP=0，无重放。与产品清单交集261源完全相同，另18项不在产品清单，不能说279全部属于产品候选。 |
| `m03-owner-recovery-011-handoff-001/native-repreview` | 1/1 actual Manager/CLI 再预览与 SessionCoordinator 集成：原 host72/sup2→真实恢复→回读→原 verified Future/closeError 仍失败→手动 Retry 唯一 generation2→正常0。输出不确定采用“UI丢弃真实成功响应”fixture，未注入真实 stdout 损坏/commit失败。 |

独立代理分别复核生产源码、资格候选1515源/5产物与实际日志、完整 host 库源一致性、Release 35 个 immutable SQLite 快照/身份/CAS/历史链、共享核心和 WM_CLOSE。当前限定范围未发现新增 P1；这些复核不扩大成完整协议、跨平台、安全或全产品验收。协议 ACK、资源 proof、持久 owner、业务 Unknown 和 UI exit 始终分列；异常 sup2不是业务成功或 ACK成功。

## 保留的失败与未观察

011 qualification001 的 11 commands 实测完成，但 freeze 后 C 工具时序补丁造成 source_unchanged=false；原证据保留，最终以 source稳定的002验收。旧010 的 KeyError fixture、早期 host 生产缺陷、旧 verifier 创建旁车，以及011 faults001/window001 的窗口失准均未覆盖或重封。新的封存 SQLite audit 使用 immutable=1，活库仍事务读 WAL。Windows generated config 相对010只有已记录的 target路径分隔变化，属于 Flutter临时生成文件，未改业务源。

WM_CLOSE 的首次可见窗口筛选失败、后续真实 hidden 主 HWND 成功两份都保留。纯 unit 里的合成 proof/PID reuse 与 UI 丢响应 fixture 只证明对应合同；没有将它们代替真实资源观察。全部测试使用新隔离库，未修复、清空或重放用户原 Unknown 库。

## 剩余合同与精确下一步

[G0实际产品图与未完成清单](../../docs/PLUGIN_SUPERVISION_G0_REMAINING_011.md)及[g0-followup-011](g0-followup-011-2026-10-01.md)保留 C05/C06/C07/C08 blocked、native/Wasm 产品图0/2。实际离线 metadata返回101：Codex缺固定 crossterm Git对象，CC Switch 首次 vendor缺锁定anyhow1.0.102。后续在新私有vendor离线补入SHA匹配的anyhow1.0.102与arboard3.6.1；两次真实metadata继续返回101，准确推进到缺async-stream0.3.6，本机缓存/vendor均无它。准备端到端178.87秒（含额外步骤；两份本地receipt elapsed合计163.442秒），无build/test/network，原CC Switch1322源和原vendor53620文件前后恒同；hardlink权限拒绝的首回执保留，成功准备采用普通私有副本。不以qualification probe或补缓存替代真实产品图。

最小下一步是新隔离目录取得原锁async-stream0.3.6 archive（checksum `0b5a71a6f37880a80d1d7f19efd781e4b5de42c88f0722cc13bcb6cc2cfe8476`）；Codex先取得固定crossterm Git rev `efa177859fd9623d57b9fe7ae9bf491ae1ac6ec4`，随后固定h3/所需精确registry集合。下一步需按原锁补足真实产品的精确 Git/registry来源，使用新隔离目录，保持原manifest/lock/vendor/用户缓存不变，然后真实 target resolution/build；CC Switch portable slice 的 JSON ABI、产品调用点和安全能力边界仍需落实。完整 writer/exec/network/auth 生命周期、QX/KX、系统选择器/实际鼠标等本机合同仍不能由本轮focused checks宣称完成。

跨平台/设备、成功非零 OS短写完成、真实断电/存储耐久、恶意同用户隔离及真实账号/权限仍未验收。缺原证明的 supervisor crash恢复需要能幸存的可信资源见证设计，现有匿名单owner Job不能凭 PID消失补齐；不擅自新增服务、安全身份或权限。SDK未冻结，全G0/84矩阵未通过。

独立 Python packager 原ZIP与当前Git跟踪树已在005恢复回执比对：3文件在树且tracked，原ZIP保留；当时62个tool unit tests使用compiler/tool mocks通过，不作为真实Rust SDK或远端交付结论。本轮没有推送。现已核对 `morrow-backups/20260930-m03-paused/delivery-receipt.json`：记录首次399547863字节ZIP relay超时、Drive写入前失败，不能声称已交付。原本地ZIP实际SHA仍与archive-receipt及sidecar一致：`530b13d45adf3b311bf1edba10ac9eb49495ad4aebcb4a1fe29281f15a8507cd`。本轮未复核当前远端最新状态、未重复上传；回执和本地核对留在handoff的backup-receipt-audit。

新索引：[evidence-index.json](host/m03-owner-recovery-011-handoff-001/evidence-index.json)。此报告记录生产集成完成边界；继续工作必须用新 candidate/run，保留本轮与旧历史。
