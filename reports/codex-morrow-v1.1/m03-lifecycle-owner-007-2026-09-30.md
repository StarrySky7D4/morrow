# M03 Windows 生命周期与 owner 下一批007（2026-09-30）

本轮继续原checkout `C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor`，分支 `codex/m03-stream-revocation-backpressure`，HEAD `04b060ef2a8ae7e7806af7b0dcc772308e9b07e4`。现有未提交修改和历史证据保留；没有Git提交/推送、CI、安装、发布、合并或部署。

## 验收顺序与实际范围

依据 [partial-write003清单](m03-partial-write-003-2026-09-29.md)、[lifecycle004](m03-lifecycle-004-2026-09-30.md)、[M02 owner清单](m02-admission-owner-002-plan-2026-09-29.md)：先真实Windows pipe前缀/原operation/reap，再完整host临界竞争与真正非零短成功completion，继而并发/生命周期、同用户隔离与崩溃owner核对，最后全产品/其他平台。前批006已修生产host到期Stop后过早关闭输入，见 [006修复](m03-expiry-terminal-006-2026-09-30.md)。

当前原生产PIPE_TYPE_BYTE/PIPE_WAIT/overlapped模式下，实测仍只有全成功或pending/取消/断开回收，未取得非零短成功completion。Microsoft的[wait mode说明](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-type-read-and-wait-modes)明确区分阻塞写与NOWAIT byte部分写；[WriteFile](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-writefile)说明取消返回ERROR_OPERATION_ABORTED及buffer生命周期。不能改成NOWAIT、改变nNumberOfBytesToWrite或用mock completion把该门槛标通过；也不据文档断言所有故障环境永远无法发生短成功。此门槛保持未实证，不降级。

本批选择本机可以实证的实际错PID连接拒绝、崩溃后持久拒绝、继承stdio未确认清理及有限并发。它们对应原清单子项，不能替代剩余高优先短成功、完整恢复/恶意插件隔离。

## 实现、生产/fixture区分

本批没有修改生产owner状态机、pipe模式/ACL或SDK合同。新增 `native_session_stream_001/src/bin/morrow-native-isolation-peer.rs` 是同用户真实CreateFile connector；`morrow-native-close-peer.rs` 新增合成descendant-stdio/stdio-holder，保留原006行为。`pipe_observation_tests.rs` 新test验证实际另一个PID连接后driver拒绝、无Connected/Read/Write、gate ordinal0和实际owner join；ChildGuard保证失败清理，try_wait有截止。新 `tool/m03_host_lifecycle_007_check.py`、`m03_lifecycle_process_007.py`、`m03_owner_concurrency_007.py` 保存新候选/独立run、实际PID句柄、环境、原始host日志和seal。

任何process终止只操作本轮直接创建或由本轮受信fixture返回、随后OpenProcess/GetProcessId核验并保留的精确句柄；不按扫描到的任意PID清理。所有场景0 HTTP，没有重发Unknown业务。

## 新候选与实测

[check002](host/m03-lifecycle-007-check-002/receipt.json)13组命令全部exit0，default/feature构建成功，62次通过的定向测试调用（含default/feature重叠，不是62唯一测试）。272项源码执行前后不变；新增跨进程wrong-PID在两个feature组合均实际执行。

[冻结candidate](host/m03-lifecycle-process-007-run-005/candidate/manifest.json) SHA256 `7dae3397abb1d9b15d12bd72aba022bd37f02a271db2604286f64309f1f5c7f7`；default host exe `ffd53a490fbd2cc55c15f6b454d47a4061c38e2c13ee768ccc49628671a96a98`，peer `2fd1264177c065e3fe874b129e2b63d05b8e9c2d2046fbb6980ae1083d1cbb03`。该候选从check002已实际构建产物复制，不继承006测试为007通过。

| 子项 | 原始证据与结论 | 不授予的能力 |
|---|---|---|
| same-user错PID | 实际另一进程成功open，随后driver PID mismatch拒绝，无业务IO，owner join；不是DACL拒绝 | 恶意同用户隔离、无DoS |
| crash-owner | [run005](host/m03-lifecycle-process-007-run-005/result.json)：强制host退出；本次child紧接已退出。原profile重开后owner仍retained、旧grant无live拒绝、新approve后claim仍拒绝automatic crash takeover | 崩溃恢复、旧批准复活、进程消失即可Released |
| descendant-stdio | 同run005：direct child实际退出、后代仍存活、stdout/stderr EOF均false，ClosingUnconfirmed/retained。只在测试清理后代、实际EOF到达后Released；原拒绝/kill等错误仍使host2 | host自身可靠进程树containment、把之后清理升级为最初期限内成功 |
| 多会话 | [并发run001](host/m03-owner-concurrency-007-run-001/result.json)：4个实际child在同一时点均alive；随后各host0、到期终态协议完整、双EOF/ownerReleased/no HTTPworker | 大规模压力、完整Core/HTTP并发或故障穷尽 |
| 同profile竞争 | 同并发run：第二trusted host可approve，但claim实际StorageBusy，未启动第二child | 任意profile全局独占、身份隔离 |

run005与并发run使用真实 `C:\Users\Administrator\AppData\Local` 锁namespace。没有将本机账号权限/security设置改成测试值。

[最终离线核验](host/m03-lifecycle-007-final-review-001/verification.json)：run005 seal301文件、并发seal42文件、全部candidate源码/exe与13组log hash一致；无重放。[独立只读意见](host/m03-lifecycle-007-final-review-001/independent-review.md)确认真实同钟4句柄alive、StorageBusy拒绝、原始owner/EOF及哈希，无新阻断。

## 保留失败与复核修正

process run001因未创建profile目录退出；run002因cwd不空被生产校验拒绝；run003因harness误期待wire事件kind字段，等到原期限实际结束后失败。均保留原failed。run004在私有LOCALAPPDATA内两项通过，但独立复核指出违反真实用户namespace资格，不提升范围。run005修正真实namespace重新运行；wrongPID测试失败清理/有界等待也在check002落实。不得用最后通过覆盖这些失败。

## 安全边界及待用户选择的最小方案

现状只适用于可信/合作式同用户实验插件。DACL允许当前用户+SYSTEM，随机locator、actual PID和wire身份不能抵抗同用户注入/句柄委托，也不能阻止抢先连接DoS。没有Job进程树containment；kill_on_drop不等于host异常退出后的回收协议；path/image race仍明确开放。本批没有安装或实现新增常驻组件。HDESK历史方案不是本项目授权。

用户主对话已在supervisor与显式管理恢复之间征询选择，当前等待确认：

- **独立可信supervisor（建议的可靠恢复方向）**：最小职责是持有原child/Job、control/data/stdio和worker生命周期的真实句柄；在任何插件执行前绑定唯一profile/owner generation、nonce、产物与授权摘要；处理host死亡时立即撤权，完成有界cancel/reap/join和进程树退出，形成不可由guest伪造的终态记录。恢复方经认证且原子验证该记录才允许新generation；原Unknown业务保持不可自动重试。需要设计组件身份、启动/生命周期、认证IPC、Job/suspended-launch原子边界及安装/部署约束，不能直接把已有普通host变成service或安装任务。
- **保留fail-closed并设计显式管理恢复**：旧retained owner/Unknown记录永久保留为未确认，显式管理员审查、停止并隔离旧实例；没有原句柄和EOF证明时不得伪称Released，也不能清owner或TTL抢占。若另开全新profile/authority代际，须显式授权并保持旧记录不可复用，定义数据/业务补偿关系；这不是同一owner的自动恢复。

选择后最低验收条件：真实host/supervisor崩溃在LaunchPending、spawn绑定、已发HTTP、pendingIO、ACK写入等窗口逐一注入；同profile双恢复者竞争；PID复用/坏generation/篡改或重放终态记录拒绝；真实旧进程树全部退出、双EOF、全部OSoperation reap/workerjoin、durable提交后才能新owner；缺任一证明保留retained；未知commit不能产生第二child/第二HTTP。故障结果与恢复动作分开封存。可靠恢复架构选择不能同时被当成恶意同用户隔离授权；后者还需独立OS身份/受限token或容器及认证IPC的实质决策。

## 完成/未完成界线

已完成：006生产到期终态缺口、新007真实安全拒绝/继承stdio子项、四会话重叠/同profile竞争、独立复核及文档同步。当前本机无环境/权限故障阻塞。

未完成：真正OS短成功completion；完整崩溃恢复（等待用户架构选择）；恶意同用户隔离/完整进程树containment；大规模压力/全部故障窗口；原产品G0/84项与SDK冻结；M04持久writer、M06执行边界不能从本批推成完成。Linux/macOS/Android等平台缺本轮执行环境，准确为未运行。所有这些项目继续开放；没有把本批限定通过变成完整M03签收。
