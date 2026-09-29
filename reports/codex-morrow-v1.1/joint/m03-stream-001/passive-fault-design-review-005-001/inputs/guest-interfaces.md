# fixture003 接口提案（未实现）

本文件是设计契约，不是现有 callable API。所有示例是 proposal-only；不编辑 frozen v3 Schema。host/guest 的现有身份、控制与数据验证继续有效。

## G1：guest spec / 启动

保留既有八个 client args：`--fixture-base URL --evidence-dir NEW_BATCH --max-chunk 1024 --fixture-spec-sha256 SHA256`；host自己的 `--morrow-native-http-v3` 前缀不变。目录仍为插件固定 out root下非reparse新批次；spec摘要绑定原始文件字节。新003二进制仅接受版本2、`mode=passive-observe` 和三个精确scenario；002/harness011继续只接受原版本，不提供兼容默认值。

`scenario-specs.example.json` 中每个对象是一个完整 spec；实际文件仅放其中一个对象。fresh nonce 为本批随机64位小写hex（示例全0不可用于实际运行）。所有字段必填，reject unknown；除scenario/fresh nonce外均固定。没有 release_file、min_pending_samples、revoke_ack_limit_ms、fault时间或延长期限字段。`pipe_prefix_bytes=12` 只在pipe场景非空，其他场景必须null；它不是HTTP批准或host fault授权。

Retry保持关闭、真实Core事件标识与gate保持原义。三个场景均pass-through：不调用hold_consumer/hold_reserved，不使data_read_paused=true，不等待release-consumer.json。由原Protocol/取消/expiry自然关闭业务交付，不设test-only成功路径。

## G2：被动 guest markers

保留原envelope：`identity{session,epoch,child_pid,attempt,operation_id_sha256,host_execution_config_sha256}`、nonce、spec_sha256、ordinal、elapsed_ns、clock_domain。新增固定 `mode=passive-observe`、scenario。身份只能来自验证完成的native admission；事件哈希来自实际数据/事件。外部harness核对nonce/spec/身份/连续ordinal/容量与完整落盘，无效/遗漏不能触发故障或签收。

| marker | 实际接缝及 detail |
| --- | --- |
| `authority_bound` | 已接受Challenge之后：原 remaining_ms、首读起点/推导expiry相对同一guest时钟的offset、后续仍使用该deadline的绑定事实；不读写host批准 |
| `response_head_accepted` | native严格accept_data成功之后；实际kind/seq/gen/status，相关frames index；不是server flush时刻 |
| `core_event_observed` / `core_event_delivered` | 真实Core值被观察/经过原最终门后交付；保留event stream/delta ordinal、kind、bytes、SHA；不持有事件等待测试命令 |
| `parser_progress_observed` | 第一次真实consumption中的parser_yielded_bytes>0；snapshot真实分类offset与nullable对应control credit seq。小前缀在正常8192 B credit门槛前可尚无ACK，仅观测，不为触发提前settle/发credit |
| `data_partial_frame_observed` | sole guest data owner实际read完成、framer.push得到None之后，仅一次：actual read id、transferred、本帧buffered_bytes、declared_payload_bytes、expected_frame_bytes、prefix SHA、read_issue_count；已观察的12 B前缀不能假设是完整BodyChunk |
| `data_end_observed` | 真实zero/error completion：actual id/error/transferred、framer lengths、complete_boundary、io index；不覆盖原分类，不人为暂停Read |
| `cancellation_transition` | 原取消latch同一短锁内取得before/after和真实first reason、stage/control引用；marker enqueue在锁外。若已有首因，仍保留before/after不改变事实 |
| `data_worker_finished` | 实际worker返回Ok/Err及failure stage/index/framer snapshot；与实际join分开。不能因先有aggregate error而省略 |

最多每个阶段一个marker；Core事件仍受实际队列/固定记录预算限制。framer可捕获短锁外的value snapshot，worker真正Err路径设置仅诊断的sticky first-data-failure，不改变主错误/取消语义。write/overflow failure锁存到fixture.failure；禁止在native/gate锁内file IO或阻塞send。证据失败取消若发生也必须有真实首因且不得签收为目标fault。

## G3：guest result 增量

原 task/frames/io/data_end_observation/close_result/final_control_result/control_end_result/close_observation 与join字段保留，不删失败或改原字段含义。新增（类型契约）：

```text
fixture_observation:
  mode: "passive-observe", scenario: enum, nonce: hex64, spec_sha256: hex64
  evidence_complete: bool, failure: null|string
  enqueued_records/written_records: u64, writer_joined: bool
  observed_stages: [marker kind], runtime_qualified: false, product_accepted: false
authority_observation:
  initial_remaining_ms: u64
  clock_domain: "guest-fixture-monotonic"
  first_read_offset_ns/original_deadline_offset_ns: u64
  deadline_recomputed_after_binding: false
data_worker_observation:
  result: null|{ok: bool, error: null|string}
  first_data_failure: null|{stage: enum, error: string, io_index: null|u64,
    read_id: null|u64, os_error: null|u32, buffered_bytes: u64,
    expected_frame_bytes: u64, prefix_sha256: null|hex64}
  finished: bool, joined: bool, handle_retained: bool
```

stage固定为`framer`/`read_completion`/`protocol_accept`/`write_completion`/`cancel_reap`/`worker_other`。没有原始信息的字段为null，不能从aggregate字符串补造OS error。null result表示未实际返回；joined只能来自真实finished-only JoinHandle.join，不能由result存在/RequestClosed/Released推导。first_data_failure独立保留，即使首因是HostCancelled、aggregate是Unknown。有限marker输出与result引用的index需指向同一实际IO/frames数组。

guest不设置`scenario_reached=true`或整体“passed”。新harness拥有跨域证据汇总；其`qualification_result`与guest原业务结果分别保存。旧A/B `stages_complete`/hold/release阶段不作为新模式字段或标准。

## H1：宿主 authority 只读诊断导出（可能需补字段，无新协议）

主会话需确认现有host ledger/events能否提供下列同一host Instant时钟的数据；已存在的直接引用，不重复加接口：

- admission创建与原Parent.deadline相对同一`host-authority-monotonic`原点的offset、批准原lifetime_ms；Challenge.remaining_ms取样与实际发送offset。
- HTTP claim / SendContext / LiveGuard / write fence引用原deadline的事实；first persisted cancellation的reason/source/provenance与effect fence关闭ordinal。
- 完整control frame sent/raw身份、实际network错误与HTTP EOF/material、实际pipe/network join与child/owner终局。

只导出真实值；不把wall clock转换成跨进程纳秒证明，不要求新Challenge字段，不暴露凭据。场景1需要这些诊断才能证明自然expiry而非普通server关闭。宿主到expiry后不能发的确认就保持缺失。

## H2：宿主测试专用 prefix-then-close（新增内部接口，需要主会话核对）

现有 `pipe_driver::Command::{Read,Write}` 没有这个入口。建议在独立测试候选显式启用，而非默认生产功能；它必须进入候选execution-config绑定。由宿主内部拥有的fixture plan在真实admission后绑定一次：

```text
PipePrefixClosePlan {
  version: 1,
  scenario: "pipe-partial-close",
  identity: validated native identity,
  revocation_generation: 1,
  nonce: fresh hex64,
  fixture_spec_sha256: hex64,
  target: "first-response-body-chunk",
  prefix_bytes: 12,
  close_trigger: "matched-passive-partial-frame-witness",
  original_deadline: reference to existing Parent.deadline
}
```

这不是控制协议frame，也不允许fixture JSON提供任意原始bytes、pipe路径/handle、PID、完整frame或新的authority。identity/generation来自host已验证admission，非信任外部字段；plan与execution-config摘要、nonce/spec一次匹配，不能重新arm。完整真实BodyChunk到达时，host固定实际frame seq/SHA/full长度/body offset/end，发给**既有sole owner**的新typed fault command；fault plan只可替换该一帧的正常发送。

owner保持原effect fence：issue前同一gate检查未撤权/未expiry，真实OS issue id/ordinal正常记录。故障命令保存完整原帧及12 B prefix，记录其不同含义，不调用把12 B视为完整原帧的普通FrameWrite成功路径。不得放宽FrameWrite.issue/complete的原帧长度约束；不得发原完整frame的WriteCompleted/data_frame_sent，不推进未发完整HTTP body为OS-completed，也不把body_end当作已发送。记录保留reserved原正文和prefix物理字节，issued/body-completed事实依据真实作用区间。

实际prefix write必须完成并reap一次；若pending/短成功/错误，保留真实结果，在原期限内按单操作规则处理，不能为目标12 B重新发送。新harness从G2证据取到有效部分framer snapshot，严格匹配身份、nonce、spec、prefix长度/SHA/expected长度后，经host内部有界消息把`CloseDataAfterWitness`交给这个owner。owner等待期间正常轮询/cancel/reap并受原deadline约束，不持gate/native锁等待；harness读marker文件也在native/authority锁外。

`CloseDataAfterWitness`只关闭已绑定session的同一data endpoint；先处理当前操作真实完成/取消/reap，再实际disconnect/close。没有原期限内匹配witness则`unreached`，按原取消/expiry清理；绝不延时补发。control lane不被测试命令伪clean关闭，也不强制其first reason；随后所有真实pipe/control故障与持久撤权按现有流程记录。

新增host证据必须包括：plan binding摘要/one-shot状态、原完整frame与prefix摘要/长度、真实seq/body范围、OS requested/completed bytes/error/id/ordinal、matched guest witness引用、实际data close、原帧complete=false、余下未发字节、原gate/expiry、真实cancel/reap/join。故障计划命中不等于已观察到残帧；它是故障意图，guestwitness与EOF/failure才是结果。

本接口方案只证明故意截断；它没有把prefix command的12 B成功completion包装成“原完整帧短成功completion”。完整WriteFile的非零短成功需要另一个真实OS故障案例，不在003提案范围内。

## H3：新 harness 的服务器开关（无需host协议修改）

新harness使用独立目录/版本，不覆盖harness011：

| scenario | 服务器开关 | 自动触发/观察条件 |
| --- | --- | --- |
| authority-deadline | `unfinished-length-body-until-original-expiry` | head/实际prefix后保持连接；以host自然expiry事件为后续server收尾依据 |
| network-abort | `short-length-body-close-on-consumed-witness` | 原期限内真实Core delivered delta + 真实consumption parser_yielded_bytes>0，关闭已接受socket；不强求门槛前CreditState，记录actual方法 |
| pipe-partial-close | `unfinished-length-body` + H2 | 正常网络body供真正BodyChunk；harness仅把匹配部分framer witness转交soleowner关闭data；不向guest发release |

三种均只1 POST，Content-Length8192且实际前缀<1024，无Completed/[DONE]，server只接受指定Responses路由。独立server/handler thread实际join、accept/request计数、发送/关闭事件保存。固定单场景计划，无自动fallback或重试；不得把old A/B“通过”直接复用为新case。

## 接口核对项

1. H1现有receipt是否已有各时域原deadline与首次持久原因；缺字段时由宿主负责最小诊断接口。
2. H2是否可作为独立测试候选typed sole-owner命令，保持普通FrameWrite和effect fence不变；physical prefix vs原body进度的记录方式是否准确。
3. 新harness能否将合法G2 witness只交给本批host owner并受原deadline截断，保留control的真实竞争路径。
4. G3独立data失败/worker结果能否只增003诊断且不改变原首因、credit或终局语义。
5. 每case的`unreached`/故障观察完整/清理未确认标准分别固定；先固定候选和标准，再考虑单POST运行。
