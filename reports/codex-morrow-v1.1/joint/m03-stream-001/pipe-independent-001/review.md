# Windows pipe-kit-001 限定独立复核

2026-09-29。固定4项实质平台测试各独立执行一次，4/4通过；helper只作为测试内部端点，不计第五项功能。没有发现本轮固定用例中的实际失败，不等于native16KiB信用/HTTP/Core全链或强隔离通过。

## 输入与真实执行

handoff SHA256 `a7949c64c87319a77376a75d2535d7bf1b4bc8ad9fc9ca63c6b73b0304063600`；manifest `d6eee6bdb2f933d59ebeddb10a550359204cb024fe09e58f98182739bbd1787b`；kit/result.json `3e9dbe7efb7a75f704fbfb8ecf85dd966b088a808f3c8cc90d03864ee84287dd`。平台测试exe `platform-a95421a369e34f9f.exe` SHA256 `c51ba2c7de38ba636a78560d7277ceeff879dd428626195c4fe9ac3249a40467`。

复制固定exe到本新joint目录，独立cwd/profile/tmp，清空非必要环境，各精确测试名分别启动。外层仅持有4个本批Popen进程句柄，记录image文件摘要、创建时间、PID和真实退出；没有枚举其他进程。4次均exit0，无超时、强停或失败重试。

23个直接相关source/kit/生产target artifact身份运行前后相同。源码+lock前后记录、真实生产compiler-artifact JSON中的测试目标/源码路径、原target/kit/运行副本摘要形成关联。本次直接运行固定二进制，没有再编译。生产方2292个旧冻结输入不变是其报告，本轮没有将其全部重新散列；不把23说成2292独立验证。

## 四项实际平台证据

| 固定用例 | 本次结果 |
| --- | --- |
| connect_pending_cancel_is_actually_reaped_and_first_instance_is_exclusive | 顶层PID19892，exit0。实际kernel DACL与当前身份+SYSTEM的期望descriptor相等，protected=true、2 allow ACE；实际handle不继承；相同locator第二次first-instance创建失败。buffer回读in/out各1024、单实例；connect实际pending，取消后完成995且slot清空，新connect被拒。 |
| actual_child_pid_duplex_bytes_and_client_disconnect_are_observed | 顶层PID21300，exit0。固定测试以其Child handle id对比GetNamedPipeClientProcessId，helper报告PID12008；实际hello/reply双向字节一致。helper正常退出后server pending read报109，固定测试读helper stdout到EOF。 |
| stopped_reader_causes_sustained_os_write_pending_while_control_and_cancellation_work | 顶层PID4132，exit0。helper报告PID16900并停在尚未issue ReadFile的PEER_CONNECTED屏障；8KiB write id2初始ERROR_IO_PENDING，poll三次均IO_INCOMPLETE，两次间隔各25ms。中途stdio CONTROL_ACK返回；随后read id3也pending，read/write均完成995并清slot，取消后新write拒绝。实际buffer各1024、flags9、max_instances1。 |
| server_disconnect_completes_client_read_without_false_success | 顶层PID1144，exit0。helper明确READ_ISSUED后server endpoint关闭，client read结果109且0字节；helper正常退出。该测试未输出helper PID，也未单独断言其stdout EOF，不能借前两项补造这项事实。 |

这些是固定生产方源码用例的独立重跑及断言结果。外层没有另持helper句柄/自行重做DACL或pipe API采样；helper PID、双向数据、OS错误、pending采样来自所审固定exe实际调用。源代码审查证实poll只有GetOverlappedResult ERROR_IO_INCOMPLETE才返回None，因此不是以应用队列/任务Pending冒充OS状态。

停读用例整进程约80.06ms，仅用于描述本次观测；测试没有记录CONTROL命令及ACK各自时间戳，所以不授予ACK≤500ms时限资格。两次25ms间隔是显式sleep目标，非实时调度保证。没有证明native16KiB应用window/所有信用额与平台背压已集成。

## 源码所有权及修正审查

- Operation拥有独立event、Box<UnsafeCell<OVERLAPPED>>及Box<[UnsafeCell<u8>]>；只在完成事实后读取普通字节。Pipe为单owner Send而非Sync，移动不会移动OS正在引用的堆存储；没有向消费者暴露裸handle/OVERLAPPED。
- Begin将立即完成和ERROR_IO_PENDING分开，has_operation只表示未回收slot。CancelIoEx成功或ERROR_NOT_FOUND都不能替代完成；GetOverlappedResult意外错误不take slot。Drop若仍不确定保留原ManuallyDrop handle及操作分配，避免用可能失败的clone换掉原handle。
- create明确protected current SID+SYSTEM DACL、first-instance、remote-reject、overlapped、noninherit；client明确SQOS identification。实际测试回读DACL/继承位/first-instance/PID；remote拒绝flag及SQOS为源码检查，未另做攻击/impersonation试验。
- 最初生产失败日志保留：实际DACL为FA，expected为GA；最终源码直接使用FA，实际回读与expected仍严格相等，且2ACE/protected检查保留。没有把比较删除或简单忽略差异。该判断来自原失败日志和当前封存源码，本轮不重跑原失败候选，也不声称旧源码全量差异已重建。
- cancel_and_reap和Drop可能阻塞，必须留在独立owned-I/O线程。控制侧先接受取消并独立观察；超过观察预算可继续保留资源/线程并报告unconfirmed，不能RequestClosed/Released。若cancel_and_reap中某个后续reap失败，函数返回Err而此前已回收的完成记录不再以Vec返回；调用方不能仅凭部分slot空就报总体成功，建议正常监督使用逐项poll保存每个完成事实。

## 未验收范围

没有unexpected API error、CancelIoEx异常或未完成Drop的故障注入；保守留资源路径只做源码审查。没有全部host死亡/进程树/跨版本Windows或同用户强攻击者隔离测试；known PID与nonce的native身份关联也尚未运行。没有真实用户数据、账户、公网、HTTP、Core/SSE、新native状态机或信用链执行。

平台4项限定证据可供宿主/插件消费；native consumer仍须在自己的forbid(unsafe_code)模块内正确使用平台API，保留connect/read/write每个回收事实、线程join及会话真实退出/双EOF。已有M02继承句柄/恢复边界不因平台测试关闭。M03完整矩阵及原84项仍not_run，产品图0/2，各产品门槛不升级。
