# 下一片：真实 Core 撤销与持续 OS 写入 pending（仅设计）

基线为 host004 / plugin003；复用 `REMAINING-MATRIX-PROPOSAL-001.md` 第2优先级两行。008已签收但不重跑，007及此前失败不改。本文件不授权实施、构建或HTTP；OS partial-write故障单列后续。

修订说明（2026-09-29）：已吸收插件与联调意见，锁定A的post-recv/pre-delivery屏障、取消时同op补采样及未触达分支、唯一ID+ordinal全程关联、reader完整行即时ACK打点；Instant仅作相对时序，ns为duration编码比例，ACK仅指host持久并应用。

**候选与绑定。** A可复用host004的真实revoke、来源台账、effect gate、控制帧和清理证据；plugin003没有这两个屏障，需另冻结native fixture及qualification适配层，保留upstream Core和旧候选。B另需host观测候选：004的`data_write_incomplete.at_us`是supervisor收件时刻，缺I/O线程原采样时间。v3及冻结pipe平台接口不改。建议现6个guest参数加`--fixture-spec-sha256 <hash>`，恰好8个；spec在已有配置绑定的evidence目录中，绑定mode、随机nonce、固定相对文件名和原限额，启动核实际字节SHA。marker/release绑定完整会话身份、operation/attempt、config、nonce、单调序号；只开一次指定测试屏障，不能授权IO。新目录拒绝旧记录和路径逃逸。

锁内仅做有界快照/序号；marker经有界非阻塞证据队列交给保留join句柄的writer，写盘/flush/release读取均在state/取消门锁外。队列满或写失败判不合格，继续取消和清理，不阻塞控制、不静默丢证据。

**A：排队/预留后的实际Core事件。** 真实ModelClient解析两个可识别delta。A在原`permit.send`成功后记queued序号/内容hash/长度；`next_event`从真实队列取到同一A后，在现有最终`deliver_if_live`前异步扣住并记held，非目标事件可照常交付。B在原`events.reserve`成功后、现有producer发送门前暂扣原event与permit。server扣住后续字节/EOF，8槽不变。harness见证A queued→held及B reserved后发唯一host revoke，分别核持久撤销、host应用、guest收到撤销及共享取消门关闭；再放行A，实际原门拒绝、`next_event`返回None，A未新增到业务流；B取消唤醒后经原发送门拒绝并释放permit。等待不持state/cancel锁，不阻控制或清理，受原deadline约束；此前已交付事件仍保留。禁止合成Core事件、用零事件或仅通道关闭替代本证据。

**B：完整读边界暂停后的真实pending。** owned data线程处理并接纳真实DataBound后，在下一次`begin_read`之前一次性置hold；发布marker时framer空、下一目标为4字节长度头、`has_operation(Read)==false`，附最后read完成/reap ID及issue计数；后续计数不得增加。继续原请求body的Write、控制、deadline和取消；暂停不调用cancel_all，撤销后不恢复读，直接进入原清理。harness核marker后才放响应正文。固定请求buffer1024（另记实际值）、credit16384、chunk≤8192、queue2、response65536：须同一不可复用write ID/OVERLAPPED实例、issue ordinal/offset/length先返回IO_PENDING，后有3次实际IO_INCOMPLETE采样且相邻≥25ms，没有换op或中途完成。pipe-owner接到取消请求时，对同op再做一次原始poll并记时，仍incomplete才按pending取消；若已完成则保留竞态、按未触达处理，不能拿此前3次替代。不足则停止，不增窗口/伪ACK/重发。

**采样和ACK含义。** 新host以run+pipe-owner实例标识clock_domain，固定一个`Instant` origin；issue、每次相关poll前后、cancel请求和reap都在此域原地采样。保留supervisor收件时间，禁止代替采样或称内核完成时刻。duration编码比例为10^9 ns/s；硬件频率不声明、实际分辨率未测。联调已确认无需raw QPC或新unsafe封装。harness用自身`monotonic_ns`记录revoke写入前至reader读完唯一匹配`operator_result(action=revoke)`完整行的差，解析/调度另记；该含传输/调度的观测上界预锁≤500ms，禁止跨进程elapsed相减。ACK仅指host应用：要求persisted/runtime_applied=true、application_pending=false，并交叉核`http_cancel_applied` source1/reason19、revoke持久/应用事实、无persistence-unconfirmed；guest收帧/gate关闭另证，Close ACK另验。三个pending采样只证明三个采样点；取消时额外poll证明owner处理取消的该采样点，不声称连续监测内核状态或精确kernel送达时刻。

**最小顺序与收尾。** 先静态审屏障/绑定/采样，再经单独授权做无HTTP的本地回归与冻结核hash；随后另授权A一次，审核收口后B一次，各新profile/op/grant且POST最多1。handshake6000及原TTL10000、其他预算不变；deadline/Unknown/首异常即停，不重发。取消逐op记录实际完成bytes/error/reap，之后worker显式join、RequestClosed、进程退出/双EOF、台账Released；未确认则保留owner/资源证据。新harness显式拥有server、handler、控制reader线程及socket，finally停止接收、关闭/唤醒活动连接、逐个join并记录仍存活者；daemon退出/server_close不能冒充join。业务终态、IO Unknown/Observed、控制clean与资源释放分别判定，不外推完整M03或产品门。

采样相邻≥25ms是硬门；线程join须在既有有界收尾预算内先观察完成再join，不把阻塞join放入异步控制路径，不延长TTL/关闭预算，超时保留未确认状态。
