# M03 首片联合接口预审001

2026-09-29。仅只读提案/定点源码审查及新增本目录文件；没有编译、运行候选、HTTP请求或M02复测。主协调已另外授权host新stream层及plugin独立Core入口准备；本文不替代主协调授权，也不签收尚未冻结的wire/runtime。各项实际运行均为not_run。

输入优先级：host IMPLEMENTATION-PROPOSAL-001.md；plugin integration-design-001.md加integration-supplement-001.md，后者覆盖原数值、Drop与审批顺序草案。精确输入摘要见review-inputs-001.json。source-audit是生产方的只读来源声明，本次仅独立核对所列审查文件，不声明全Core图或其全部来源已核验。

## 可继续落实的接口方向

1. 真实ModelClient构造prepared请求，经受限Prepare/RequestChunk仅提案，可信operator核对收到的原始请求并明确批准，随后单次Commit。批准必须绑定提案、原session/epoch/operation、精确endpoint及预算；提案等待、HTTP批准、ACK及drain均消耗首次native期限。网络前拒绝路径要有server零连接/零POST证据。
2. sealed native网络授权复用原Core io_intent/io_evidence，禁止伪造ManagedInstance；native ledger仍只承担批准/owner。独立网络授权不由read-own-session派生。两个库的不确定提交保持保守状态，不宣称跨库原子性。
3. network_node增量出口保留同源策略；已有client.rs:295-307在bytes_stream后累加Vec，现状不能满足首字节早于EOF。新lease跨head/body/取消持有期限、许可和worker，版本化提取必须提供原代码到新实现的差异清单。
4. operator stdin、guest控制stdio、认证数据pipe分别有唯一writer。独立控制路径、真正OS pending写与取消资格，不能以队列名、线程数量或biased select代替。
5. plugin补充的ParserDetached→RequestTask裁决→原期限内bounded drain方向成立。冻结parser在responses.rs:626-634将Completed送入事件队列后即return，因此parser先drop而Core稍后见Completed是实际代码允许的顺序。

## 合同冻结前必须具体化的意见

### R1：取消ACK、effect fence与在途效果分开

给出至少四个独立事实：取消被控制域接受及generation；禁止新dispatch/read/数据写许可的栅栏生效；之前已发起的HTTP/OS写还在途；worker/channel实际关闭。ACK要说明它对应前两者中的哪一个，不能从ACK推导已在途POST/OS缓冲字节被撤回。禁止持有数据库事务或正文锁等待网络/pipe完成。

已有原子claim仅保证唯一Prepared→Unknown，不自动证明异步网络send与revoke线性化。实现需写清ticket交付、首次网络poll、撤权接收三者的串行化点和竞态结果；竞争输了但已经Unknown时保留Unknown，不改成CancelledBeforeDispatch。取消之后允许既有OS请求完成的范围必须标注，禁止把“无新许可”等同“服务端此后绝无字节”。

### R2：证明真的OS写挂起，额度设置需能达到它

16KiB credit、两片queue和≤64KiB响应可能先耗尽应用额度而始终不触及OS pending写。冻结pipe缓冲请求值、chunk/frame实际尺寸、有限initial credit、停读屏障及写入API观察点。必须保存真实pending操作及其取消/完成结果；async任务Pending本身也可能只是等待应用锁或credit。若预算内无法触达，明确not_reached并向协调提出独立有界fixture配置，不静默扩大生产额度。

控制ACK≤500ms是该fixture上的观测阈值。分别记录控制命令发出/接收/ACK、pending write状态、worker退出及pipe闭合；跨进程用屏障因果，不能直接比较各自Instant原点。partial frame取消后关闭data，不在残帧后写控制或重新发送整帧。

### R3：offset和credit的归属必须只有一种解释

合同明确network_received、完整frame_write_completed、adapter_received、parser_yielded或drain_discarded、ACK及cancel generation；名字可另定，但不能统称delivered。绝对ACK单调且不超合法接收边界，重复ACK不重复扩credit。ParserDetached时可能还有已yield但未ACK或parser缓存内的尾字节，必须保证由同一lease接管记账、无丢失/重复计数。drain只消费原响应，不产生新的模型事件或HTTP发送。

### R4：模型终态、完整HTTP观察与owner释放保持正交

不能把Terminal写成一个互斥枚举后抹掉其他维度。至少能表达“Core Completed但drain断流且durable Unknown”、“parser错误但HTTP已完整Observed”、“已收到取消但worker未退出/owner未释放”。若HTTP已EOF且完整响应材料已合法持久化，之后的消费者取消/数据交付失败不能回写Unknown；尚未成功持久化则不能凭EOF声称Observed。原Core存储io_intent.rs:528起为Observed消费预留，需按既有状态转换实施。

成功屏障至少两组：服务端首delta→真实Core OutputTextDelta→才允许尾部/EOF；以及parser发Completed/drop→Core见Completed→drain→服务器EOF。后一组须让用户取消能抢先于排队Completed，并包含drain超限/到期。Core terminal的supervisor不能等待UI队列腾空后才处理取消。

### R5：修正一个源代码事实，避免验收断言失真

原plugin design“response.failed/incomplete按上游产生错误”概括过宽。冻结responses.rs:418-451显示：response.incomplete且incomplete_details.reason等于interrupted，并有合法response结构时，返回真实Completed且end_turn=Some(false)；其他reason才返回错误。请保留上游语义，分别建立interrupted与其他reason的预期。typed Completed并不等于完整模型成功。此发现只来自源码，不是实际M03运行结果。

### R6：单POST覆盖全层且输入完整性可追溯

采用补充中的retry=0、无auth recovery、禁WS/next-session重试、host唯一claim、库no_retry/no_redirect。保存Core prepared→host提案/批准→Core request evidence→服务端实际body的摘要关联。对header区分批准的语义多值集合与HTTP库合法加入/排序的线缆表现，不要求无法保证的全局header线缆次序。实际400/302/401/426/429/503与收完POST后断线都查server计数；重复Commit/新连接不增计数。新批准/operation不得被用于隐式重试旧Unknown；首次切片只允许同一业务task单次发送。

## 执行入口与继承限制

双方对齐以上语义后，以host唯一schema/生成绑定/限额manifest冻结，再分别交源码、lock、实际编译图及exe身份，才可开展联合真实执行。源提案仅支持接口审查，不支持当前“实现已正确”结论。

M02-002已封存，所有旧候选与验收文件继续冻结。其known-PID/非全进程树、诊断EOF延迟及句柄继承隔离未验、崩溃拒绝接管不等于恢复等边界原样保留，M03不是这些缺口的修复声明。M03首片仍仅loopback合成数据，无账户、公网、真实配置/内容库；不代表全产品或旧UI运行兼容。

M02 partial；G0/P02/J00 blocked；G1未通过；产品图0/2；原84项not_run。新矩阵只用于本片诊断，不改变原门槛。
