# H1/H2 实现字段补充 002

2026-09-30。延续接口001的feature、CLI、operator输入和1024B上限；此文明确已实现的摘要前像、嵌套输出及原因边界。

`plan_sha256` 的前像为按固定键序 `version,scenario,nonce,fixture_spec_sha256,target,prefix_bytes,close_trigger` 序列化的紧凑UTF-8 JSON，无空格/换行。输出JSON对象本身可能按键排序，独立复算须按该固定序列重建。`identity_sha256` 仍按001规定的六键序列重建。plan先域分离加入admission.config，再接原authority绑定；不会改变首次created或期限。

`qualification_pipe_plan_bound.detail` 包含 `plan,plan_sha256,identity,identity_sha256,generation=1,deadline_renewed=false`。后续管道事件的 `detail.binding` 包含相同对象。

`qualification_pipe_prefix_issued/reaped` 的 `detail.frame` 包含：

- `original_frame_raw_hex`：实际完整编码原BodyChunk字节，仅供证据独立复算。
- `raw_frame_fully_sent=false,original_frame_complete=false,tail_reissued=false`。
- `original_frame_sha256,original_frame_bytes,declared_payload_bytes,sequence,body_offset,body_end,prefix_bytes=12,prefix_sha256`。

完整帧长度由真实编码决定，001示例1364不是固定常量。本地测试的完整帧1348B、body1024B也必须按真实4B length+8B payload前缀核验。原body的reserved范围仍保留，prefix实际OS issue的body_end=None；不推进issued/OS-completed/peer消费offset，不发原BodyChunk的WriteCompleted或data_frame_sent。

管道事件共用 `detail.clock_domain,before_ns,after_ns,duration_unit="ns",resolution="not_measured"`。该clock由sole owner线程持有，不与host Shared或guest的Instant做绝对比较。真实write ID为 `detail.operation_id`，effect ordinal为 `issue_ordinal`；issue的 `detail.outcome.requested/pending/id` 和reap的 `bytes/error/id` 为实际平台返回。测试专用 `qualification_pipe_read_issued` 用 `outcome.read_operation_id/requested/pending` 记录真实read，用于证明关闭时的ReadFile取消回收，不能把它当成prefix write。

`qualification_pipe_witness_matched.detail.gate_sample` 来自owner在原gate锁内的同一个Instant样本：

```text
clock_domain
sample_offset_ns
original_deadline_offset_ns
remaining_ns
gate_revoked=false
original_deadline_reached=false
gate_deadline_matches_original=true
```

owner拒绝gate被替换/续期、已撤销/过期、prefix未真正reap、错误/重复witness；接受只设置data owner的真实取消请求，不修改HostAuthority首次持久原因。`sample_offset_ns+remaining_ns==original_deadline_offset_ns`，仅在该owner clock内核对。`detail.before_ns/after_ns` 是事件形成时刻，Shared observation的at_ns是消费/发布时刻，均不可替代gate检查的sample。

`detail.close_source` 为 `matched-passive-partial-frame-witness` 或实际抢先的原gate关闭、owner取消、pipe错误路径。`qualification_pipe_closed` 在真实取消/reap且drop当前同一endpoint后产生，`outcome` 含 `data_endpoint_closed,all_operations_reaped,matched_witness,original_frame_complete=false,platform_io_error,owner_thread_join_proven_here=false`。最终join仍由已有data_worker_joined核验。

请求主动关闭endpoint本身不制造OS错误，也不主动指定24/19/20等原因。`platform_io_error` 仅保留实际I/O错误；原帧截断事实始终独立保留。guest的PartialFrame和sticky失败、宿主首次持久原因、RequestClosed/Close/control EOF/实际释放须在后续真实联合运行分别验证，不从接缝ACK或本地pipe join推导。
