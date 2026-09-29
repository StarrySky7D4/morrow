# 未封存实现的提前源审

2026-09-29。以下来自实施中的 `native_session/src/lib.rs` 只读观察，已通知宿主和主协调；不是固定 ready 候选结论，也不是动态失败复现。未修改宿主源码。最终必须重新核对封存代码和实际行为。

1. **到期分类使用过期的时间快照。** `let now=Instant::now()` 位于 `select!` 之前；`sleep_until` 分支醒来后仍按该 `now` 判断 expires/handshake_deadline，可能返回 Protocol 而非 Expired/Handshake。应在醒来分支重新取得单调时间，并动态覆盖实际到期和重试不续期。
2. **等待失败与退出确认混淆。** `child.wait()` 错误分支把局部 `exited` 置 true，但状态 `exit_observed` 为 false。顶部释放条件使用局部 `exited` 和两个输出 EOF，没有要求真实退出确认，可能错误释放 owner。等待失败应保留不确定事实；没有真实确认不能 Released。
3. **证据日志静默丢弃。** 1024 条 events 达上限后不再记录；碎片化 stderr 可能先占满，控制/退出证据丢失没有 gap。需要保留关键事件额度或显式丢失计数。此项涉及可观测性，不能把事件不存在直接推断成控制未执行。

另：已提醒请求 `code == 0` 必须单独验证，不能依赖将 code 清零的 `matches_admission` 比较；当前所读实施代码已显式检查 code。正式合同现等待新 Cap’n Proto 版本，撤回候选不用于运行。
