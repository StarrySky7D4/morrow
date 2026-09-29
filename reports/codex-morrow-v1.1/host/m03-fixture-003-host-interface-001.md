# H1/H2 宿主实现接口 001

日期 2026-09-30；接口已选定供主协调对接，实施和验证结果另报。只新增资格入口和只读事件，不改 v3/platform API。自然期限和原因边界见同目录 `m03-fixture-003-host-seams-design-2026-09-30.md`。

## Cargo 与 CLI

`native_session_stream_001` 新增 `qualification-pipe-fault`，`default=[]`。默认构建拒绝下列 CLI 选项和 action。feature 构建未传 plan 仍走普通 driver。旧 LaunchSpec 字段、guest 八参数及普通 FrameWrite 不变。

host `serve` 可附加 `--qualification-pipe-plan JSON`，仅一次，不读取旁路文件或环境变量。JSON 严格拒绝额外字段，完整对象：

```json
{"version":1,"scenario":"pipe-partial-close","nonce":"HEX64","fixture_spec_sha256":"HEX64","target":"first-response-body-chunk","prefix_bytes":12,"close_trigger":"matched-passive-partial-frame-witness"}
```

nonce 必须是非零小写 hex64；摘要为小写 hex64。批准前验证 `--fixture-spec-sha256` guest 参数精确出现一次且值等于 plan；plan 的固定 canonical JSON bytes 经域分离加入 admission execution config，然后继续原持久 authority 绑定。完整 tuple/generation/原deadline由 host 的实际 admission 填充。plan 不在普通 guest wire 中出现。

## 只读输出

现有 host_observation 中每条 observation 增加 `at_ns` 和 `clock_domain="host-authority-monotonic:<session>:<epoch>"`；原 at_us 保留，二者来自同次 admission.created.elapsed。纳秒单位不是硬件精度证明。

- `authority_deadline_bound`: created_offset_ns=0，original_deadline_offset_ns，lifetime_ms，parent_deadline_matches_gate，expiry_not_renewed=true。
- `challenge_deadline_sample`: sample_offset_ns，remaining_ms，original_deadline_offset_ns。原始 Challenge 完整实际发送仍由 control_frame_sent 的 raw_hex 和同域 timestamp 核验。
- `http_cancel_applied` 保留原 code/requested_code/source/progress，增加 effect_fence 的真实原gate状态、关闭ordinal/last_write/originaldeadline；首次持久原因不覆盖。
- `qualification_pipe_plan_bound`: canonical plan_sha256、plan、实际 identity 和 identity_sha256、generation=1、原deadline引用。未提供plan不产生此事件。
- `qualification_pipe_prefix_issued/reaped`: 原frame sequence/SHA/完整长度/body_offset/body_end/原声明payload长度，prefix_sha256/bytes=12、真实OS id/ordinal/requested/pending/completion/error，original_frame_complete=false、tail_reissued=false。
- `qualification_pipe_witness_matched/closed`: 相同binding、witness/reference，原始关闭/取消/reap事实；外部仍须核真实data_worker_joined、child exit/双EOF/Released。

`identity_sha256` 的精确 preimage 为下列按此键序列的紧凑 UTF-8 JSON（无空格/换行）：`session, epoch, child_pid, attempt, operation_id_sha256, host_execution_config_sha256`。数值为JSON整数，摘要字符串小写hex。host输出完整identity及SHA，harness应从真实绑定tuple独立复算。

## 内部见证转交

仅 feature host 的既有受信 operator 输入新增 action；不是 guest Capnp 消息：

```json
{"action":"qualification_close_data","grant_id":"HEX64","witness":{"identity_sha256":"HEX64","nonce":"HEX64","fixture_spec_sha256":"HEX64","marker_sha256":"HEX64","marker_ordinal":1,"read_id":1,"read_issue_count":1,"buffered_bytes":12,"declared_payload_bytes":1360,"expected_frame_bytes":1364,"prefix_sha256":"HEX64"}}
```

保留原1024B操作输入上限；使用紧凑JSON。outer只允许 action/grant_id/witness，nested严格拒绝额外字段。identity摘要代替转发完整tuple以留在原长度预算内。harness先验证完整G2 envelope、scenario/kind、真实IO数组引用与原JSONL行，再转交此投影；marker_sha256 为完整该行字节含末尾LF的SHA，仅为证据引用。host无法独立读取/认证guest文件，operator是受信转交边界，不能把摘要引用宣称为密码学证明guest来源。

host验证 active grant、实际tuple摘要、plan nonce/spec、唯一marker、read_id/count正整数、buffered=12、declared/expected符合目标真实完整帧、prefix SHA匹配、prefix已真实完成reap、原generation1/gate仍开且原deadline未到。错误/重放/目标未命中不触发close。有效见证经独立容量1的typed channel交同一sole owner；操作ACK仅表示owner接受关闭请求，不是资源已关闭证明。不得设置人工HTTP原因、强制first reason或伪造clean。

原frame前12B不含合法可消费HTTP正文；prefix OS issue使用body_end=None，保留reserved真实原body范围，但不推进完整issued/OS-completed/peer offsets。fault命中后禁止新data写入和尾部重发，正常读/真实取消reap继续，关闭当前同一data endpoint。无匹配见证时仍受原期限约束。

## 本地验证范围

先覆盖严格式plan/witness、config改变、default拒绝、未绑定/重复/错误见证、expired/revoked gate、原完整帧不完成；真实同进程pipe验证12B真实Read、12B实际Write completion一次、见证后实际reap/join与无尾部写。默认与feature各自构建，feature定向回归另建证据目录。本轮无HTTP、无旧候选改写、无Git操作。
