# 可空启动观测 JSON 兼容测试候选

仅归档一项纯模型测试，不激活生产代码。测试保留 failed_startup_quarantine 的 null 字段、JSON 往返及不可变快照，并验证它不代表退出、EOF 或释放。

本项在本轮 23 项回归中实际通过；整轮仍为 22 通过、1 失败，不是全通过。依赖 Dart r003 候选，不能直接套用旧生产模型。详见[限定结果](../../startup-integration-progress-v10.md)。
