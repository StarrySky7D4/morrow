# M03 下一小片：真实 Core 撤权与 OS pending（设计待审）

**本轮仅设计。** 两个独立场景；不实施、不构建、不启动进程或 HTTP。承接原 `m03-stream-001/integration-design-001.md` 的慢消费者/取消行及 `integration-supplement-002.md` 的真实 OS 背压边界；不扩大为队列饱和或 partial-write 故障验收。

**隔离与绑定。** 后续另建、另封存 `qualification/m03-fixture-001`（仅适配层接缝）和 `native/m03-fixture-001`；upstream Core、qualification002、native003、wire/pipe kit 及旧批次不改。Core 场景可复用 host004；pending 场景需要新增宿主观测候选。原六个参数加 `--fixture-spec-sha256 <64hex>` 一对，保持八参数上限。固定 `fixture-spec.json` 放已绑定 evidence-dir，白名单限定 mode、nonce、原限额和固定相对 marker/release 名称；启动验证实际字节 SHA。LaunchSpec 配置绑定目录与 spec SHA；不新增 v3 测试指令或普通插件权限。

marker 由固定 fixture 源码写入，仅作证据；包括 mode/nonce、session/epoch/PID/operation hash/attempt/config hash、阶段序号及本进程单调时刻。文件 create-new、限长，release 一阶段一次、严格同批校验；nonce 防串批，不是权限。harness 结合固定 exe/hash、子进程句柄和 host 原始事件核对，不能仅信任可写文件授予网络/IO。

锁内仅取有界快照/序号；marker 写盘、flush、release 文件读取及任何慢 IO 均在短 state/取消门锁外。证据经独立有界队列交由保存 join 句柄的 writer 落盘，控制路径不等待慢磁盘；满队列/写失败判证据不全并继续取消清理，不默默丢弃后宣称通过。harness 只依赖已完整落盘的同事件 ID/hash 阶段链，不能以别的事件或零输出替代。

| 场景 | 精确接缝与成立条件 |
| --- | --- |
| **A：真实 queued/reserved 事件撤权** | 原 `request_task::consume_core` 取得实际 ModelClient 的 delta A，经现有 `permit.send` 入队后记录同一事件序号/hash/长度；`RequestTask::next_event` 收到 A 后、现有最终 `deliver_if_live` 门前异步 hold，保留原值并记录尚未交付。下一真实 delta B 在 `events.reserve` 成功后、producer 原发送门前 hold 原 event/permit。原 8 槽及 upstream 队列不改，不手工构造 ResponseEvent；此前已交付事件单列。server 发两 delta 后扣住后续字节/EOF。A-held 与 B-reserved 都实证成立才发 host revoke；分别记录持久撤销、guest 收到撤销、同步取消门关闭，harness 再释放 A，实际原门拒绝该 A。B 由取消唤醒后经原发送门拒绝并释放 permit。屏障不持 state/取消门锁，不靠下一次 read 关闭门；记录 read issue/业务交付计数及同事件抑制结果。release 缺失受原 deadline 限制，后台控制与 cleanup 继续。 |
| **B：真实 OS 写 pending 下控制进展** | `data_owner` 处理完真实 DataBound、下一次 `begin_read` 前一次性闩住：framer 空/expected=4、最后 Read 已 poll-reap、`!has_operation(Read)`；由 owner 发 pause marker，携带最后 read ID 和累计 issue 数。之后只禁新 Read，继续真实请求 Write、poll、deadline/cancel；不以 cancel_all 暂停再恢复。host 待 marker 后放响应，在原 16KiB credit、1024 pipe buffer、正文 chunk≤8KiB（编码帧沿用 32KiB payload 加4字节前缀上限）下，必须记录同一 OS write ID/ordinal/offset/len 首次 IO_PENDING，及至少三次真实 IO_INCOMPLETE；无 completion/reissue 偷换操作。不足条件即未触达并停止，不增加 credit/额度或重试。独立控制 revoke 触发取消，owner 直接进入原 cancel/reap，不靠恢复读取解除阻塞。 |

**时序与 ACK。** 新 host 使用 pipe-owner 的同一固定 Instant origin/时钟域（run+owner 实例），记录 issue、每次 poll 前后、取消请求及 reap；ns 的 1e9/s 只是 duration 编码比例，分辨率记未测，不需要 raw QPC 或新增 unsafe。三次 IO_INCOMPLETE 的相邻采样间隔必须至少25ms，必须同一不可复用 operation ID/OVERLAPPED 实例及 issue ordinal；supervisor 收件时刻另列。三次采样不等于取消时仍 pending：owner 收到取消请求、调用 CancelIoEx 前，对同一操作再原始 poll；若已完成，记录竞态并判未触达，不拿此前采样替代。marker 后 read issue 数必须不增。

harness 同一 `monotonic_ns` 域在 revoke 写入前及完整回复行读到时立即取时，解析/调度另记；匹配 operator_result 的 RTT（含传输调度上界）预锁 ≤500ms，不事后放宽。该 ACK 仅表示 host persisted/runtime_applied、application_pending=false，另对应 `http_cancel_applied(19, source1)` 且无持久化错误；不是 guest gate ACK 或 Close ACK。guest gate-closed 独立核对，不相减跨进程 elapsed。取消后写完成/reap 是预期，不要求取消应用后仍 pending。

**收尾与判定。** 保留 native003 Close/aggregate/control-failure 分层；业务失败不变成功、Observed 不倒退、无 Unknown 白名单。追踪同一 OS 操作实际完成/reap、worker 显式 join、RequestClosed、两端退出/双 EOF和持久 Released。新 fixture 保留 server、handler、harness control-reader 及 guest 控制线程的 join 句柄，在既有收尾预算内先观测完成/is_finished 再 join，记录实际 join/仍存活，不把阻塞 join 放进异步控制路径，也不为 join 续期；router-end 不冒充线程 join。等待沿用原单调 TTL、既有限额与有界关闭预算；不等待自身进程退出。未知/超时/未确认回收保留所有权和证据，停止后续场景，不重发 POST 或自动接管。旧007失败及008结论不改；本设计不授权运行，也不升级完整 M03/产品门。
