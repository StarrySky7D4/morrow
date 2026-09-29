# fixture003 固定接口

新 guest source：`qualification/m03-fixture-003` 0.1.4、`native/m03-fixture-003` 0.4.2。G1 与已接受提案的完整17字段一致，示例文件 SHA-256 `783b30b0d511b6769c9e748fe3a49ee1405591455c4198cc978a530ef7513a49`。示例全零 nonce 禁止用于运行；实际必须 fresh 非零小写 hex64。

本候选只完成本地回归与编译，三场景均未运行。fixture002/harness011/upstream/v3 Schema 没有修改。

## 启动与spec

host加原 `--morrow-native-http-v3` 前缀；client仍八参数：`--fixture-base URL --evidence-dir NEW_BATCH --max-chunk 1024 --fixture-spec-sha256 SHA256`。固定loopback origin、无凭据、原预算/原期限、exact-byte digest和非reparse路径校验保留。

spec全部字段必填，unknown拒绝，显式nullable `pipe_prefix_bytes` 缺失也拒绝：

- version=2，mode=`passive-observe`，scenario为`authority-deadline`/`network-abort`/`pipe-partial-close`，nonce为本批非零hex64。
- request_limit=32768、response_limit=65536、consumer_events=8、credit_limit=16384、pipe_buffer=1024、max_chunk=1024。
- deadline_policy=`retain-original`，events_file=`fixture-events.jsonl`。
- evidence_queue_capacity=128、evidence_record_limit=128、evidence_record_limit_bytes=8192、evidence_file_limit_bytes=1048576。
- pipe_prefix_bytes仅pipe场景为12，另外两个必须null。没有release字段、人工取消时间、续期或HTTP授权。

三种均pass-through，无hold/reserved barrier/release文件读取/DataBound Read暂停；仍由原取消和完整帧边界断连解释逻辑决定实际交付。原控制逻辑准入到blocking stdout OS issue的间隙没有被本候选修成内核顺序保证。

## G2 JSONL

envelope：`identity{session,epoch,child_pid,attempt,operation_id_sha256,host_execution_config_sha256}`、mode、scenario、nonce、spec_sha256、ordinal、elapsed_ns、clock_domain=`guest-fixture-monotonic`、kind、detail。

ordinal是实际成功入队顺序，失败队列项不伪计为已入队。总数/字节/写失败锁存fixture.failure，不作为目标fault。queue使用try_send，所有marker的queue/文件IO均在native/Operation锁外；owned writer逐行写入并flush，停止后排空已有队列，只有真实finished handle才能join。harness逐字节append-only验证并自行计算完整行含LF的SHA；guest不计算该marker引用SHA或认证外部operator转交。

| kind | detail |
| --- | --- |
| authority_bound | initial_remaining_ms、clock_domain、first_read_offset_ns、original_deadline_offset_ns、deadline_recomputed_after_binding=false；仅一次绑定，不重算 |
| response_head_accepted | kind="ResponseHead"、sequence、generation、status、frames_index；真实control接受成功，零基index指向host_control记录，数组溢出不冒指旧项 |
| core_event_observed / core_event_delivered | event={stream_ordinal,delta_ordinal,bytes,sha256,kind}；delivered另有gate_cancelled。OutputTextDelta内容标识来自真实Core值；Completed/Other无内容记录，bytes=0/SHA空字节摘要。marker时读到的gate_cancelled不否定在原门内已合法交付的事件 |
| core_event_suppressed | 同一真实event；producer可有stage="producer"，保留实际gate_cancelled；无人工事件或等待测试命令 |
| parser_progress_observed | consumed_offset、parser_yielded_bytes、drain_discarded_bytes、error_body_consumed_bytes、credit_sequence nullable、io_index nullable、body_frame_index nullable；第一份真实非零parser进度，仅一次，不提前发credit。seq仅关联完全匹配的真实credit，io/body引用最近成功接受BodyChunk的实际completion/host_data数组项 |
| data_partial_frame_observed | read_id、read_issue_count、transferred_bytes、buffered_bytes=12、declared_payload_bytes、expected_frame_bytes、prefix_sha256、prefix_hex、read_fragments、io_index；DataBound已完整接受后真实framer恰12B且未完整，io_index为此次Read os_completion，缺IO引用或碎片链则failure而非伪见证 |
| data_end_observed | read_id、io_index nullable、os_error nullable、transferred_bytes、buffered_bytes、expected_frame_bytes、complete_boundary；只来自实际zero/error read，不把CancelIo意图当作EOF |
| cancellation_transition | observation_ordinal、stage、requested_reason、transition={cancel_gate_before/after,first_cancel_reason_before/after,delivery_paused_before,host_control_closed_gate}；取值在原gate同一锁内，之后锁外输出。observation_ordinal是捕获顺序，JSONL ordinal/elapsed是入队时刻，两者不当作OS/跨进程时间 |
| host_revoke_received / gate_closed | 保留002原合法控制kind/sequence/generation/code与cancel_transition / read_issue_count事实，只作观测，不作为003签收标准 |
| data_worker_finished | 实际data_owner返回的result={ok,error nullable}、first_data_failure nullable；这是owner函数已返回，不代表线程句柄已finished/joined |

G2的core消费/部分帧见证不得推断完整BodyChunk、HTTP EOF、semantic completion、host ledger来源、reap/join或整个session释放。外部跨域证据另外验证。

`read_fragments`为按实际完成顺序的 `{io_index,read_id,transferred_bytes,bytes_hex}` 数组，每项来自真实成功Read的done.bytes，io_index指向该os_completion。自上个完整frame边界累计，合计严格12B、最多12项；prefix_hex为这12B的24位小写hex。超过12B仍未完整时立即丢弃诊断链，每份完整frame decode后重置；没有重包为单次12B，没有保存任意完整body。harness应按IO数量/id/顺序和fragment bytes独立重放，比较prefix_hex、SHA与host原完整帧前12B；原JSONL行含LF摘要仅是引用，不能替代字节重放。

## G3 result

原task/frames/io/aggregate/Close/final/sticky control/zero-credit/RequestClosed与join字段保留。`fixture_observation`改为新mode/scenario、evidence_complete/failure、enqueued_records/written_records、observed_stages/read_issue_count/writer_joined、runtime_qualified=false、product_accepted=false；无旧A/B stages_complete/hold/release标准。guest不设置scenario_reached或整体passed。

新增authority_observation与authority_bound detail相同；尚未绑定则null。新增：

```text
data_worker_observation = {
  result: null|{ok: bool, error: null|string},
  first_data_failure: null|{
    stage: framer|read_completion|protocol_accept|write_completion|cancel_reap|worker_other,
    error: string, io_index: null|u64, read_id: null|u64, os_error: null|u32,
    buffered_bytes: null|u64, expected_frame_bytes: null|u64, prefix_sha256: null|hex64
  },
  finished: bool, joined: bool, handle_retained: bool
}
```

早期open/handle失败无framer信息，长度为null而非伪0；原worker Err与独立首次data failure保留，即使aggregate已Unknown/首因HostCancelled。finished来自实际JoinHandle.is_finished或已真实join，joined只来自真实join；结果存在和RequestClosed/Released不能代替它。合成tests的IO/reference值不是运行期OS证明。

原清理2000ms上限及最终Close/control/evidence一次绝对500ms不变。003修正一处确定失败等待：已经锁存真实control_failure时，在等待缺失RequestClosed之前返回CleanupUnconfirmed；仍先真实data join，不确认欠缺的资源/credit，不把失败改Ok。控制仍活且无失败时保留原等待。

guest继续以原请求/控制失败退出（负例通常非零）；exe exit0不是目标故障签收标准。host reason20或26不强制重写guest first reason；本地Deadline可能先发HttpCancel导致host先持久19，外部应按真实因果判unreached。没有EOF/Observed/material或clean ACK豁免。
