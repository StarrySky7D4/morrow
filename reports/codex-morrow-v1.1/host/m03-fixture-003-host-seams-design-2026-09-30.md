# Fixture003 宿主接缝设计核对（2026-09-30）

## 结论与范围

原批准 TTL 自然到达、HTTP chunked 正文缺少终止 chunk 的断开，可以使用现有 v3 schema、宿主控制和真实 Core/child 路径验收。需要派生新的被动观察 fixture003 和运行 harness；fixture002 的 A/B 屏障及正常 Close 判据不能直接充当这些故障的终局判据。残帧可先尝试原期限内的真实前缀停读；若要求确定性地只写原帧前缀再关闭数据 lane，需另建默认关闭、仅资格二进制可用的 owner hook。

本报告只读核对当前开发源码，新增本文件；没有修改实施源码、构建、测试、启动 HTTP 或重审旧 A/B。引用的插件源码是当前工作区 `companions/morrow-codex` 下的 fixture002 副本，不冒充已实现的 fixture003。candidate-002 和新 A/B 的运行、冻结及联合结论由主协调管理，本报告不替代该审证。

所有运行建议均使用新 profile/op/grant/nonce，最多一次 POST；保留原限额、原批准绝对期限及已有清理预算。资格测试的故障观察完成、生产协议结果、业务结果、资源回收分别记录。观察到预期错误不构成生产协议或业务成功。

## 1. 原 TTL 自然到达的准确路径

| 接缝 | 当前函数及位置 | 行为 |
| --- | --- | --- |
| 原批准起算 | `native_session_stream_001/src/authority.rs:666`，启动分支 | `admission.created + ttl_ms` 成为 Parent 和 EffectGate 的同一绝对期限；不是从 POST、屏障或重试起算。 |
| 网络期限 | `native_session_stream_001/src/http_authority.rs:419`，HTTP claim；`:535`，`LiveGuard::check` | SendContext 使用 Parent 原截止；未撤权而到期返回网络 Timeout。先撤权则返回 Cancelled。 |
| 宿主到期分支 | `native_session_stream_001/src/supervisor.rs:734`、`:902`，`run` | 每次 tick 在尚未 Closing 时检查原期限，到达设置 session `reason=20`。 |
| 首次持久撤权 | `supervisor.rs:280`，`Http::cancel_http`；`http_authority.rs:84`，`Parent::revoke_native`；`authority.rs:355`，`record_revocation` | 原生首次撤权写 parent state3/source2/reason20；已存在的首次原因不覆盖。关闭 effect gate、取消网络/管道、清除待交付正文、generation2；已启动且无 EOF 的网络为 Failed，HTTP error20。 |
| Closing 和 Stop | `supervisor.rs:804`、`:820`、`:825`、`:844` | 下一 loop 进入 Closing，开始 `spec.close_ms` 收尾预算。没有已接收 Close 序号时，未部分写的控制队列被清空，尝试发送 Stop20；正在部分写的控制帧被中断并关闭 stdin，记录 `partial_control_write_cancelled`。 |
| Close ACK | `supervisor.rs:825`、`:897`、`:920` | 只有已接收 Close、session reason25、撤权持久化已证实且 stdin 尚在时才追加匹配 State ACK。ACK pending 时原期限/帧期限/close 期限到达会失败并关闭 stdin。Stop 完整写出也关闭 stdin。自然到期不能承诺新的 Close ACK。 |
| 回收及释放 | `supervisor.rs:532`、`:619`、`:694`、`:854`；`authority.rs:774`，`NativeAuthority::poll` | 分别观察 pipe 实际 finished-only join、网络 lease.finish 的真实 worker_joined、ticket 已释放。RequestClosed 还要求 pipe_joined、网络已 join、network_pending=false，并避开 join 同一 tick。whole owner Released 另须 child.wait 成功、stdout/stderr 两侧 EOF，再由 authority 持久化释放。 |

`Http::cancel_http` 先排 HttpTerminal，但自然到期的 Closing 分支会清空尚未写出的队列并改发 Stop；Stop 完整写出后关闭 stdin，后来产生的 RequestClosed 不保证能送达 guest。`rules.rs:48` 允许剩余时间为 0，所以 Stop20 本身无需改 schema。不能用“准备排队”代替“完整控制帧实际写出”。

若 child 未及时退出，`supervisor.rs:905` 在原 close_ms 到达时请求 kill；之后再次使用 close_ms 观察收尾，仍未证实时进入 ClosingUnconfirmed、保留 owner。kill 请求不是 exit/reap/join 证据。这些清理观察不允许新的 HTTP/data 效果，也不延长原批准期限。

### 需要保留的期限来源竞争

guest 本地 Admission 期限是 `first_read_started + Challenge.remaining_ms`（`companions/morrow-codex/native/m03-fixture-002/src/admission.rs:26`），不是与 host 共享的时钟原点。`workers.rs:229`、`Session::wait`（`session.rs:1758`）及 Core Operation 原期限都可能首先触发 Deadline。guest cancel writer 可发送 HttpCancel，host `Http::handle_control` 在 `supervisor.rs:377` 固定请求 reason19。若这个持久化先发生，后来 reason20 必须保持首次原因19，而不能回写成20。

因此“验证宿主自然 TTL reason20”必须从原始日志证实：没有 operator revoke/stop；最先的持久撤权实际为 source2/reason20；此前没有获处理的 guest HttpCancel19、pipe error24 或 Core 结束引起的 Close25。若另一来源抢先，这一批是竞争分支证据，不能通过修改期望或自动重跑取得 reason20。报告同时列出 session_close_reason 与 `http_cancel_applied` 的有效原因，两者可能因首次原因规则不同。

### guest 的现有终局含义

- `Session::accept_control`（`session.rs:1323`、`:1334`）将 Stop20 映射为 BridgeError::Deadline，并保留首次 CancelReason；若此前已由合法 generation2 Progress 关闭门，首次原因可已是 HostCancelled。`request_task.rs:406` 的 `cancelled` 只有首次原因 Deadline 才映射 CoreTerminal::Expired；否则为 Cancelled。不可在观察层重写它。
- `Shared::finish_control`（`session.rs:1609`）即使观察到完整帧边界 EOF，只要没有 Close ACK，也写 sticky `control_read/Unknown`。`close_observation`（`:1851`）要求 Close 已写且已 ACK 才给 control_protocol_clean=true，所以 Stop20→EOF 的完整受控终止在当前实现中仍可能 protocol_clean=false。这是明确的生产协议边界，不能清除 sticky 错误来取得“干净”。
- `Session::cleanup`（`:2083`）先对已 finished 的 data handle 实际 join，工作 Err 仍返回 CleanupUnconfirmed；`cleanup_progress`（`:1272`）还要求真实 RequestClosed、匹配消费结算及无 control_failure。没有 RequestClosed 或 EOF 先结束控制，可返回 CleanupUnconfirmed。真实 host 已回收不等于 guest 已得到完整清理证明。
- Operation cleanup 的 2000ms（`qualification/m03-fixture-002/src/network.rs:426`、`limits.rs:9`）与 main 的一个共享 500ms Close/control/evidence 观察预算（`native/m03-fixture-002/src/main.rs:156` 起）保持不变。新 observer 应保留 task、close、control_end 和独立 join 结果，避免早返回后丢失报告；不得续期来伪补 ACK。

## 2. chunked HTTP 缺终止 chunk 的断开

### 测试服务接缝

由受信 harness 创建独占 loopback 服务，返回 200、`Content-Type: text/event-stream`、`Transfer-Encoding: chunked`。先发送一个完整合法 HTTP chunk，载荷为有效但没有 response.completed 的 SSE 前缀；记录请求原字节摘要、响应原字节和发送区间。等待 guest 实际消费证据与 host 已验证的消费 CreditState 对齐，再关闭该连接，明确不发送 `0\r\n\r\n`。不能使用普通 close-delimited 正文，因为那种 EOF 可能是合法结束。

单个合法 chunk 本身可有完整 `\r\n` 尾部；“缺 HTTP chunked 终止标记”已足以构成传输失败，不要求伪造 Capnp 残帧或假 ACK。服务关闭须记录真实 socket/handler 收尾；没有达到发送和消费前置条件便记 target_not_reached，不自动重试。

### 当前宿主路径

`network_node_stream_001/src/stream.rs:336` 的真实 reqwest `body.next()` 若返回 Some(Err)，在 `:341` 成为 Error::Transport；只有 None 才设置 http_eof=true。`supervisor.rs:615` 调 `network_code`（`:709`），Transport 为26，进入 `Http::cancel_http(26)`。预期有效首次原因是 parent state3/source2/reason26，network Failed、intent Unknown、无 HTTP EOF/response material；网络工作错误与稍后的真实 worker join 分别保留。

这个 HTTP 错误本身不设置外层 session `reason`。`run` 的 tick（`supervisor.rs:915`）将已撤权 owner 标为 Revoked；随后 guest 真实 zero-window 消费结算和 Close 可使 session reason25→Closing。正常匹配 ACK 在原期限未到达且控制未损坏时可完成。`record_revocation` 保留早先26，不能被 Close25、随后退出24覆盖。不能把网络26直接写成“宿主已进入 Closing”，也不能凭 Close ACK 声称传输成功。

guest 接受合法 generation2 Progress 时先 `cancel()`（`session.rs:1343`），再因错误 progress 记录 Unknown（`:1481` 起）；首次取消原因可能 HostCancelled，Core 可能 Cancelled 或此前已保留的其他终局。新 observer 原样记录，不以 error26 强行要求 CoreFailed。如果服务发送了 Core Completed，那么现有 `request_task.rs:389` 起会保留 Completed，再单独记录 drain 错误；本最低矩阵主动不发送 Completed，避免混淆该切片。

全帧边界的 data EOF 可按现有500ms且不超过原批准截止的 disconnect reconciliation（`session.rs:1117`、`workers.rs:537` 起）；部分帧或其他错误仍是 Protocol/NativeFailure，不能因最终网络26解释而抹掉。

## 3. fixture003 最小改动建议（不改 v3 schema）

建议在新目录派生 fixture003，保留 fixture002 原件。新资格 spec 增加明确 passive-fault 模式及 case 标识，继续以现有 `--fixture-spec-sha256` 参数绑定完整 spec；该 digest 已在 LaunchSpec args 及 execution_config 中，不增加普通 guest 控制消息或权限。严格拒绝未知 case、跨批 marker、重复 ordinal、摘要不匹配和超过固定限额的参数。

1. `qualification/.../fixture.rs:24` 的 Mode/`:50` 的 validate 增加被动模式；`request_task.rs:308` 只在 CoreRevoke 生成屏障事件 ID，因此被动模式无需持有真实 delta/permit。保持原 Operation 最后交付门不变。
2. `workers.rs:584` 的 DataPending 停读只能在明确的残帧/背压测试中启用；一般自然到期和截断 HTTP 模式正常读/解析/消费。新的 stages_complete 由实际已绑定启动、错误观察和终局记录决定，不能套用 `fixture.rs:419` 原 A/B 屏障要求。
3. guest reporter 在所有错误路径保存 `task.core_terminal/audit/cleanup`、`close_result`、`final_control_result`、`control_end_result`、sticky_control_failure、完整控制帧/IO 观测、data/control/evidence 各工作结果及真实 finished/join/handle-retained。必要时仅增加只读 worker-observation 方法，不改变 fatal、首因或协议 clean 计算。
4. 考虑宿主原 close_ms 可请求 kill，应先把真实当前错误/状态刷新为有序 checkpoint，再在现有清理预算中写最终 join 记录；checkpoint 不代替最终证据。若 kill 前最终报告未完成，要明确 unconfirmed/missing，不以 checkpoint 声称已 join。
5. harness 将 fault_target_observed、native_protocol_result、business_terminal、resource_cleanup 分开。例如期限负例可证实 Stop20/截止禁止新效果/真实 Released，同时仍输出 control_protocol_clean=false、CleanupUnconfirmed。产品 clean-close 资格保持未通过；后续修复是独立工作。

## 4. 残帧中断和仅资格 owner hook

### 优先不加 host hook 的接缝

新 guest 模式可在 DataBound 完整消费后正常执行请求，并在首个 BodyChunk 的真实 Read 收到合法4字节长度和部分载荷后停读，保留 Framer.bytes/expected、Read ID/实际完成字节及无在途操作证据。停止发生在 guest 的 sole owned-I/O thread，不交给控制线程操作 pipe。若剩余 host 原期限小于既有500ms部分帧期限，让原期限真正关闭该在途写；不得调长部分帧期限或人为更新 authority。

当前 `workers.rs:508` 在 cancel/closed 与 partial Framer 同时存在时返回 `close with partial data frame`；`:516` 的500ms部分帧期限也可能先发生。这两者必须分类记录。若只产生完整原帧，或者部分帧超时首先触发，不能宣称命中了自然 TTL 残帧分支。guest 接受前缀不是 OS 返回过短成功 completion；0B/995回收不能虚构非零确认前缀。

### 若需确定性 host→guest 截断，建议的最小接口

仅作为设计候选，尚未实施：

```rust
// Cargo feature qualification-pipe-fault，default=[]。
#[cfg(feature = "qualification-pipe-fault")]
pub(crate) struct BoundFrameCut {
    // 受信批准 tuple + 固定 fault spec digest；禁止 guest 指令选择。
    // 唯一指定 BodyChunk ordinal，1 <= cut_after_wire_bytes < encoded.len()。
}
#[cfg(feature = "qualification-pipe-fault")]
pub(crate) fn spawn_for_qualification(/* 原 locator/pid/gate */, cut: BoundFrameCut)
    -> Result<Driver>;
```

该入口放在 `pipe_driver.rs:163` 的 Driver 构造接缝附近；supervisor 仅经显式资格构造传入，普通 `Driver::spawn` 不接收计划。新增资格 binary 使用 `required-features`，生产构建无此分支、无环境变量后门、无新增生产控制消息。不能仅凭 build feature 自动截断所有会话。

绑定须先明确：host 测试计划 digest、资格 host exe digest、guest exe/spec digest、执行配置和唯一 tuple 在 trusted harness 冻结记录中一起固定。若复用现有 execution_config，宿主在 spawn 前核对已批准 args 中 fixture spec digest 与 BoundFrameCut 所绑定 digest；不能仅哈希一个未进入批准绑定的旁路文件。失败、重复触发、wrong tuple/ordinal/digest、deadline 已到、目标帧不合限额均拒绝。磁盘验证在进入 effect gate 前完成，不在 gate 内等待文件、屏障或控制线程。

故障只作用于一帧：保留原编码长度、原长度前缀和 body_end，仅对其选定字节前缀发起真实 OS Write；记录实际 requested/ID/issueordinal，真实回收后取消并关闭此 owner 的剩余操作。继续沿用真实 effect gate、cancel/reap/finished-only join。不能注入任意原始帧、伪造 Completed、ACK 或 RequestClosed。

`FrameWrite::issue`（`write_state.rs:49`）要求 requested 等于原帧剩余长度，因此这种**故意缩短 OS 请求**不能混入生产逐帧模型：需显式独立的资格 fault 分支记录 original_frame_len 与 actual_requested_len，始终不发原帧的 WriteCompleted，不推进原 body_end 的 os_completed_offset；所有 Reaped/错误仍来自实际 OS。即使短请求成功，这也只是协议残帧故障注入，不是“OS 对完整请求返回了短成功 completion”。真正短成功 completion 仍须独立真实证据，不在本报告中补授资格。

可证伪条件至少包括：guest 没有真实部分 Framer、原帧被报整帧完成、前缀字节/Read与Write ID无法绑定、出现第二次尾部写入、cut触发多于一次、effect gate 后新写入、目标未达到、未知回收、live handle被报joined、缺少子进程退出/双EOF、或错误被清除以取得clean。任何一项成立，该残帧测试不通过。

## 5. 最低真实运行矩阵

每项单独固定新候选/新批次、一次 POST、无自动重试；根据实际可达到的原期限设置前置条件，不增大 buffer/credit/request budget。下表是计划，不是本轮运行结果。

| 案例 | 受信故障与前置条件 | 必须原始证据与判定 |
| --- | --- | --- |
| D0：原期限等待响应头 | 请求已真实 POST；服务不返回头，不发撤权/stop；让原 TTL 到达 | parent 首次source2/reason20、无先行 HttpCancel19；网络 await-head 真实失败和回收；Stop20实际写出或明确部分控制中断分支；无 fresh dispatch/data；guest Deadline/其他首次原因原样；协议/cleanup错误与host/child Released分别核实。 |
| D1：原期限等待正文 | 正常200 chunked头和合法未Completed前缀；实际消费已匹配确认；服务保持连接且无终止chunk | 与D0同一原批准deadline；对已发/已消费前缀不回滚、不伪EOF；网络和data各实际reap/join；如果guest自然deadline先持久19，记竞争分支，reason20目标未达到。 |
| H0：chunked缺终止chunk | 正常200、合法完整HTTP chunk/SSE前缀、真实消费ACK；在原期限前服务关闭且不发0chunk | 真实body Transport→有效首次source2/reason26；Unknown/Failed/noHTTP EOF/material；同tuple zero结算/Close/ACK及进程释放，或者原样记录控制/清理失败。与正常close-delimited EOF及超时20分开。 |
| P0：数据残帧 | 优先guest真实前缀停读+原期限；需要确定性断开时使用另冻资格cut binary | host原长度/实际请求/ID与guest Framer残留对应；Protocol保留、无业务解析假完成、无原帧WriteCompleted/尾部重发；工作Err与实际join分列；host/childReleased真实核验。该项不等于短成功OS completion。 |

若H0在服务关闭前先触发TTL、人工撤权或坏帧，则reason26目标未达到，不可把其他原因白名单成26。D0/D1不以operator ACK作为TTL证据；不同clock域不直接相减。P0需要分别识别控制先到时的partial-close和data EOF先到时的partial-read错误，单次交错不能声称覆盖二者。

通用终局：原生账本原始只读解码核对 parent/HTTP/owner bindings、首次原因、sendbudget1、原 lifetime、Unknown与无材料；实际 child.wait、双EOF、pipe/network/evidence/controller线程join及ticket释放都要核实。guest无法证明自己退出，最终Released须由host和外部观察核验。业务/协议错误本身不可因资源已回收改成成功。

## 6. 本次只读输入摘要

下列是设计读取的源码快照 SHA-256，供后续识别漂移；不是新的可执行候选 manifest，也不继承旧运行资格。

| 路径 | SHA-256 |
| --- | --- |
| `native_session_stream_001/src/supervisor.rs` | `66ab9c38b83fb8d5b8b62afc32cc1032c93704b0a88f8219538dbb3af6924ad0` |
| `native_session_stream_001/src/authority.rs` | `09ae9f1ca088d2c470f2dff4aa90e2b304849237752b290eacb11602f67a6971` |
| `native_session_stream_001/src/http_authority.rs` | `6b2bc0f8eb1bad59d1d7d79b59ed71b01fa97db2aebdc2489e53429ff3f533dc` |
| `native_session_stream_001/src/pipe_driver.rs` | `341a1cbdb200003a2ed6ce5958f39b625aeaf09cfdff3d899637db80219189ca` |
| `native_session_stream_001/src/write_state.rs` | `661bcf7a07d760c9bd9a50a375fa7123392bf345639becef5d7e1b48bfbaace4` |
| `native_session_stream_001/src/lib.rs` | `475dee1ba32ed6befe06fd38cc035feb20e3eb7053209c7abf0c6a099e9af930` |
| `native_session_stream_001/Cargo.toml` | `88cdf680d02dcdc7c18ae8af75f99aefd6672923bacab4cbce3d48a9b5e03ad6` |
| `network_node_stream_001/src/stream.rs` | `2ccc6ac200871aca66f3634854a18d4dc62da038ad5a7b931901caf6c040a083` |
| `contracts/experimental/agent_host_v3_http_stream/src/rules.rs` | `a71f74b06e39d254025e92a2e6099780953090b41f2ecac3a35930683a0a7794` |
| `contracts/experimental/agent_host_v3_http_stream/native_http.capnp` | `8da8f1455200696d8884949f415aae4edd70cee6b583889bba7906c7730cc864` |
| `companions/morrow-codex/native/m03-fixture-002/src/main.rs` | `c86ab95aad618db4ed124289815b27f0ef14367910c909cc61246bda664082f3` |
| `companions/morrow-codex/native/m03-fixture-002/src/session.rs` | `0db7e61184227d558efe7ebc28809ecb13870062545a08e35d0b349d7a906c96` |
| `companions/morrow-codex/native/m03-fixture-002/src/workers.rs` | `f4d67053ca8e8f0aaf53095517419130ad3212ee1a38aa6572715e6ae2248d14` |
| `companions/morrow-codex/native/m03-fixture-002/src/admission.rs` | `b477a424c839364f95fe85725df3f52c87b0c1f6fe27b610cd57f2aede486e40` |
| `companions/morrow-codex/qualification/m03-fixture-002/src/fixture.rs` | `81068e8383fef97efedc1f527152b32848258032a76651d30bfacf0bd971baf9` |
| `companions/morrow-codex/qualification/m03-fixture-002/src/request_task.rs` | `d6a15eea3fe2e1c9a91f072c0609caa17bf273004dba95f122c2a0bc3f28bbf8` |
| `companions/morrow-codex/qualification/m03-fixture-002/src/network.rs` | `f741e3f29be39d373fcfdbf5a362dd72b477302009c8bd773f6113f734cb7f05` |
| `companions/morrow-codex/qualification/m03-fixture-002/src/limits.rs` | `aa6ae58e61fb7c62038fd6d7c5c1eec1b6af0998d6e4711bfd8342f98376f5e9` |
