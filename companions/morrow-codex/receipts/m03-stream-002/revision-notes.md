# M03 修订002：关闭两处本地静态阻塞

001 与全部既有源、构建回执及固定候选保持冻结。修订只在 qualification/m03-stream-002、native/m03-stream-002、新 receipts/out 路径。沿用唯一 v3 wire-kit001 和共享 pipe-kit001，没有新增 schema/FFI，也没有启动 host/client/HTTP fixture。

输入问题来自 joint/native-driver-review-001/review.md，SHA256 12696ee0e2a171fceeaa0279992de611253fea4bee70704a411225c22966b364。它们是静态确认的问题，不宣称001已有实际host失败复现。

## 宿主取消同步传到真实 Core 事件边界

NativeHttpSession.bind_cancellation 在native状态锁下绑定唯一 CancellationSignal；若取消先发生，立即重放首次原因和token，不等下一次网络读、不使用异步转发任务。Stop、Denied、撤权与fatal协议错误同步关闭该信号。Operation、Core消费者和RequestTask使用同一信号，首次原因保留。

仅token的biased select仍可能与ready事件竞争，因此Core先异步reserve队列容量，再在信号的短锁内入队；RequestTask从队列取出后也在同一短锁内决定是否交付。锁内没有await或driver调用。取消赢过该线性化点则抑制事件；已先完成交付的事件不能撤回。已观察到的Completed(end_turn=false)审计事实不删除。

定向测试覆盖：已排队delta、已预留容量的迟到delta、真实解码的Stop/Denied/撤权帧分别使Operation token与交付门立即关闭，以及取消早于绑定仍不漏通知。Core私有RequestTask测试实际调用next_event检查队列事件被抑制。测试无网络read、无成功mock HTTP、无真实Core模型请求。

## 清理继续，终局协议错误不能被ACK洗掉

router使用统一handle_control保存sticky fatal并应用取消。请求cleanup仍可取回真实已知的RequestClosed及历史Observed事实。wait_close继续等待收尾ACK/写完成，但ACK之后必须检查fatal；main的最终成功条件再次检查final_control_status。

result.json改到Close观察之后写入，包含close_result、final_control_result及result_snapshot=after_close_observation。序列RequestClosed → 错误序号控制帧 → 合法Close ACK仍消费ACK，但等待与最终状态均失败；Observed和RequestClosed记录保留。没有等待自身退出或外部owner release。

## 验证边界

新Core本地测试4/4、新native本地测试9/9通过。只有新增候选本地测试与编译，没有重跑旧codec/platform/002宿主套件。native端到端撤权、取消竞争、HTTP成功、背压和外部完整释放仍待固定002候选的host/joint执行。构建与依赖固定记录见本目录候选清单；产品0/2、84not_run及既有门槛不升级。
