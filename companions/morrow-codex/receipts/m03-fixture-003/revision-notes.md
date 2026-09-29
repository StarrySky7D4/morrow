# fixture003 本地交付边界

2026-09-30，Asia/Shanghai。来自已接受的独立设计003；保留002与全部旧回执，没有修改host/upstream/harness011/Schema，没有Git提交、HTTP或真实guest启动。

本批实现严格v2三scenario被动观察、真实Core事件正常交付、原latch原子首因诊断、真实头/consumption/部分帧与worker结果观测。协议/业务失败、实际资源回收与线程finished/joined独立。nonce/spec本身不授予HTTP或fault权限。

相对提案的具体澄清：ResponseHead实际走control接受路径；G3早期错误没有framer信息时长度也nullable；小前缀不强求门槛前CreditState；取消marker的捕获ordinal与入队时间分开。H2 marker行含LF的SHA由新host harness计算，guest不读取release、不暂停Read、不认证operator转交。

发现并修复003的清理等待缺口：原cleanup_progress先等closed再看sticky control_failure，Stop20后真实EOF/缺ACK、缺RequestClosed会空等。003先检查真实control失败，data实际join后返回Unconfirmed；保留原失败/Unknown/缺ACK，没有延长TTL或将cleanup改Ok。原控制仍活无失败路径、2000ms最大预算和最终共享500ms保持。

最终本地检查：

- Core **15/15**：strict全部字段/显式null/旧模式/限制拒绝、exact-byte绑定/重放、队列/磁盘失败/真实writer join、原取消先后、真实型别事件无release正常交付（测试值合成）、原门取消后抑制与既有网络边界。
- Native **35/35**：既有原生状态/credit/控制失败回归；新增合成部分framer/109失败在Unknown与HostCancelled之后仍独立保留，并实际join本地失败线程；真实形状4+8两fragment与IO关联、12项上限/超过12B丢弃/完整边界重置（输入为合成completion）；被动消费不人工发小credit；实际控制终止后缺RequestClosed在100ms内Unconfirmed并实际join；control/evidence使用同一500ms deadline且超时留句柄。
- Windows Rust 1.95.0 `--offline --locked` native/Core二进制构建。依赖警告保留，未修改只读依赖以消除警告。

最终来源：core `test-core-20260929T221031Z-5e54aa28`；native `test-20260929T221930Z-863b46cb`；build `build-20260929T222011Z-7d1a4b8e`。初次通过的native/build、索引加固及增加碎片链前的中间回执均保留；中间编辑脚本的一次断言只应用head索引改动，随后用明确补丁完成body索引改动，再重跑native并重建；没有失败测试被翻判。Core源未变，最终native/build对同一Core输入核对。

所有阶段只运行本地合成状态/事件/文件/自有线程测试；没有实际ModelClient HTTP、host/guest pipe配对或三场景因果验证。正常BodyChunk/partial引用仍需新host/harness的真实单POST证据，有限OS前缀截断也不证明原完整WriteFile出现非零短成功completion。完整M03、产品、SDK冻结未完成。

`native-candidate-fixture-003.json`、exe与源码/锁/日志/本地测试文件摘要供主会话绑定新harness/host。不可复用002 A/B有限通过替代003自然期限、网络断开、pipe残帧场景。
