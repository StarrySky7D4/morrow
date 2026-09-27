# 文件变更取消控制与同 owner 借用（2026-09-27）

本轮完成 Windows `TargetBroker` 的受控同步入口及宿主分借用前置条件。真实 Create／Delete 的执行路径可接收单操作取消，取时与权限／额度检查由同一个控制器串行执行。**尚未将文件变更命令接入 owner 队列，也未开放公共 SDK／UI。**

工作目录 `build/io-safety-refactor`，分支 `codex/io-safety-refactor`，基线 `b9225f64f6c62584ad7243e30249d8a088bcb155`；本轮为本地未提交增量，不打包、不推送。

## 实现

- 新增可信宿主 `TargetControl::with`：原单调时钟的取样及对应检查／准入在同一临界区执行。取消属于当前操作，必须锁存，不能伪造过期时间或撤销整个实例。旧接口保留无取消的兼容包装；这不使任意旧闭包自动具有跨线程时钟安全性。
- 选择／验证／释放／准备／暂存／创建／删除／Unsupported Replace 均提供 controlled 路径。文件打开、metadata、写入、同步、发布、句柄释放与 Store 事务主体不放入时钟临界区；事务授权回调只在检查期间进入它。失效回收在取时之后、锁外进行。
- 创建在读取暂存、claim guard、每个 64 KiB 写入块、同步及发布前检查。删除在 claim guard 及效果前检查。单个同步 OS 调用不能被取消强制打断。
- `ManagedHostOwner::with_managed_runtime` 默认拒绝且不调用闭包；`Self: Sized` 保持 trait object 兼容。工作台实现从原 owner 分借用原 Manager 与原可变 HostRuntime，不复制 Manager、不另建 Store。测试检查原地址、原 binding 和实际存储写入。

## 取消与历史

| 时点 | 本次调用结果与历史 |
|---|---|
| 选择、准备／暂存事务提交前、执行 claim 前 | `CancelledBeforeDispatch`；本次事务回滚，无新的外部效果。原有 Prepared／暂存可能仍存在 |
| Prepared／暂存已提交而交付被取消 | `CommittedButDeliveryCancelled`；保留计划或内容＋回执，读取原历史确定结果 |
| 已确认 Unknown claim，外部效果尚未完成 | `OutcomeUnknown`；选择不可再次执行，禁止自动重发。创建的可确认临时项按原句柄清理 |
| 外部效果已经完成 | 保存真实 Response＋Observed，再限制交付；取消不能把真实效果改为回滚 |
| 同一已派发选择再次调用且新调用已取消 | 先核验原 owner，再保留 `AlreadyDispatched`／提交不明的 `OutcomeUnknown`，不被派发前取消遮蔽 |

资源关闭不明仍保留原计费并要求重启。失活或已取消命令的维护回收不能依赖成功交付。崩溃残留临时项仍不自动清除，也不根据同名文件猜测归属。

## 验证

| 范围 | 最终结果 | 日志 |
|---|---|---|
| Runtime：lib、file_owner、io_binding、io_execution、io_owner、managed_file_io、owned_service_renewal、owner_commands，packages＋fault-injection | **216 passed／0 failed／6 ignored**；8 个测试摘要 | `build/file-control-runtime-final.log` |
| Runtime 同范围 Clippy | 通过；`-D warnings -A clippy::collapsible_if` 保持原有例外 | `build/file-control-runtime-clippy.log` |
| 实际 Workbench 宿主 `cargo check --lib` | 通过，5 项既有 dead_code 警告 | `build/file-control-host-check.log` |
| 修改文件 rustfmt／git diff --check | 通过 | 终端记录 |

上述测试含本轮新增 17 项：11 项执行控制、4 项准备／目录选择、2 项 owner 分借用。分项数字不重复累加。ignored 中有由父测试运行的崩溃子入口，以及未在本轮单独执行的既有慢测和 Windows Replace 限制原型；不将 6 ignored 全部视为已执行。Core 生产逻辑本轮未改，未重新跑 Core 全量；此前的 737 项证据见上一报告，不能当作本轮重跑。

本轮命令均使用 `--locked --offline`。组合测试选择 `--lib --test file_owner --test io_binding --test io_execution --test managed_file_io --test owner_commands --test owned_service_renewal --test io_owner --no-fail-fast`，Clippy 选择相同目标；不是全部运行时或成品应用验收。

测试覆盖真实 Windows 临时目录的创建／删除、分块取消、claim 前后取消、效果后观察、重复调用、准备／暂存的全部控制取样点，以及三层目录链部分准入的释放。另以独立线程的 `try_lock` 和实际 resource/job/byte 使用量变化确认准入发生于持锁回调内；这不等于实际 worker 控制适配已接通。

独立只读审核发现 spent 分支原先让取消遮蔽已派发事实，已修正并补回归。首次 preparation 定向测试发现测试辅助函数将同一 binding 的时钟从 5 回退到 4，已改为后续调用继续使用 5；没有放宽生产单调时钟检查。

## 剩余接线

1. 在原 owner 有界命令队列中建立实际 ticket＋原共享时钟适配，保持 state→clock→IO 锁顺序。队列取消、停止、容量耗尽、foreign worker、真实 join 及丢回执查询必须实测。
2. 现有内容暂存是完整有界 slice；后续需独立的分块协议与暂存预算，不能把最高 16 MiB 的正文塞进 64 KiB owner 命令。
3. SDK 三语言与 UI 接线必须基于上述队列，不能在 UI 线程直接调用同步文件效果。目录列举、完整 Unknown 业务核对和其他平台资格继续开放。
4. Windows 条件替换仍在 claim 前明确不支持，不用无条件覆盖替代。SDK 未冻结。
