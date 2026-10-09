# v25 ETS business handoff / Session / Draft 验证

2026-10-09。仅本轮 ETS model 与实际源码测试；未操作 Git、NDK、HAP、SDK 或设备。完整 Windows UI/多数实现追平目标仍 OPEN。Root 主页面集成、整包构建和新 native 设备验证须有独立证据。

## 冻结输入和实际结果

实际命令为 bundled Node `--test` 跑 `editor-business-handoff-model.test.cjs`、`editor-business-session-model.test.cjs`、`editor-draft-model.test.cjs`、`editor-draft-fork-model.test.cjs`。

- 97 tests / 97 PASS / 0 FAIL / 0 SKIP，7368.1284 ms；Handoff 21、Session 32、Draft 27、旧 Fork 17，均是实际 ETS 方法，未用页面字符串开关替代产品逻辑。
- 11 个生产依赖 / 测试 / actual Store fixture 输入运行前后 bytes 和 SHA256 完全一致；见 `editor-business-handoff-model-inputs-before.json`、`editor-business-handoff-model-inputs-after.json`。
- `editor-business-handoff-model-tests.log` SHA256 `3D3FBE20D6420FAEB035B60702B3F2BCCAE4226EB2DACC6F24BD7B101AE07EBA`。真实运行时间、测试列表、边界见 `editor-business-handoff-model-result.json`。
- `EditorDraft.ets` 37929 bytes，`B88A46D0371D8C6F193187D1A0E1ACF41671D4991CDBD59C451B8BCF66CF3775`。
- `EditorBusinessSession.ets` 40102 bytes，`58E992B7E8BDAE01F7BC07B3DEE9E66B7BCAE6712A945B5509382121E5AF52C6`。
- 新 `EditorBusinessHandoff.ets` 40477 bytes，`5454DE4C88133CCD04E27BE693870280F6BF9F126696B9684C3DB4AE025A6A55`。
- 旧 `EditorBusiness.ets`、`EditorDraftFork.ets`、`EditorFieldPolicy.ets` 未修改。生产 TypeScript transpile 通过，不宣称 ArkTS SDK 编译通过。

`editor-handoff-store-fixture.json` 是 native worker 本轮实际 Store 导出，91081 bytes，`9AF7CCAABAF247BBB06E60CD7302DA878F515BAA3704495735542BDB783E486F`。本测试精确 pin 该 hash，直接消化原 full Submission、准确 immutable publication、registered save/inspect literals、实际 strict inspect Reply / historical Card、full native child15 / parent16、固定 Retirement / Close 和 closed 七部分。

两项实际 fixture 测试先对 closed seven-part Session 做只读 restore，再发送已有精确 native inspect 字节到该实际 Reply，必须由旧 Business 模型完整校验历史 Card bytes / SHA / publication / request 才允许 Handoff.restore。没有从 child.source 拼造业务 DTO。随后只读恢复原计划，打开仍 active 的真实 child record，原 S1 description 与 child S2 raw 分开；saved_exact context 不复活旧 intent 或旧 raw。Retirement / Close fixture 的 outer transport 是 exporter 对象，本模型按固定 outer encoder 包装其逐字节保存的 inner literal；不把对象声称为存储了原 outer 字符串。

## 产品行为和调用边界

`prepare(session,parent,childScope,firstRaw,planOp,firstOp,retireOp,hooks)` 只构造本地固定完整请求，不发送 native。前置为 own strictly qualified business result、真实暂停/无 in-flight / Unknown / conflict 的 parent、全部独立 import owner 已处理、准确 parent proof、最新完整 raw 和真实输入 epoch。dirty S2 可保留，无需在已失效旧 source 上 flush。child scope 只接受实际 historical Card 的完整 source / revision，first selection 是准确 confirmed parent pins 的有序子集 / 全 aliases，转 origin4。普通全字段 selection/composition/category/stage/todos 不格式化或裁剪。

`begin/retry` 派发同一 Handoff literal。native intent plan effect 与 child effect 分开；只有准确 first child generation1 / source / full raw / link / pin metadata 的完整 DTO 可资格化。任何 child committed effect 在坏 DTO、old owner、后续 not_committed 中都保留。history first ACK 的 current 较新 / inactive 不安装或复活；可以由 Root 显式读取真正 current child 后 `openCurrentChild(record,raw,guard)`。

`openChild(latestRaw,guard)` 返回真实 `EditorDraftCoordinator`，不安装页面。guard 由 Root 冻结真实 stable lease / current input epoch；完整 latestRaw 必须等于 paused parent.current。第一 ACK 不替换晚 S3。真实 child 普通 save 持续保留 incoming business15，已确认 origin4 pins 后继提升 origin3；旧 raw fork13/14 行为不变。incoming13 与 outgoing16、incoming15 与 outgoing14 可合法混合，双 incoming / 双 outgoing 拒绝。

`loadRetirement/retryRead` 显式只读 native handoff / retirement parts，保精确返回的 inner literal及其 key order / whitespace。`retire/retryRetire` 只派发固定 retirement，要求 Root 实际安装该 child writer、最新 full raw 已确认且没有独立 imports / Unknown / conflict；DTO 必须是准确 inactive parent16，full parent raw / consumed imports / pin metadata 不变。晚 S2 不因 ordinary discard 被吞；已经证明的 business / child 在 retirement cleanup 失败后不降级。

`closeRetired(op)` 只关闭 intent，child S3 仍 active。`savedExact(session,parent,closeOp,discardOp,hooks)` factory 本地严格核准确 S1 publication/proof/full values、parent clean、原 session.mayConsume，然后冻结 cutoff。Root 真实 revoke/detach SDK lease 后 `.close()` 仅再核固定 full raw 和 fresh boundary hooks，不伪造 session.mayConsume，也不强迫旧 view 重新拥有。任何晚 text、selection、composition、assets、alias/order 或 epoch 变化在 native 派发前拒绝。`retryClose` 核对同一原 literal，不新建操作，不消费晚 S2。

Session 新阶段 handoff_planned3 / saved_exact close_planned3→closed4 / handoff_retired close_planned4→closed5。closed success 可读取原 transport / 显式历史 inspect；new save / retrySave 在 planned / closed 不准入。`rebindOwner({changed,isCurrent,isExact,parentReady})` 只在 idle 修改动态 owner guards；原 sender、field protocol、hash provider、全部 literal/proof/qualified receipt/committed hint/Unknown 保持。实际测试覆盖坏 committed DTO 后重绑、只读失败、后续 not_committed 都不洗已知事实，新 owner 未实际准入时不可消费。

Draft 新 `settleIssued()` 只 await 调用时已发 inFlight，允许 paused；不 flush dirty S2、不新派发、不 retry/clear Unknown。实际 coordinator 测试证明已知 ACK 后 S2 全值保持 dirty 与 paused，已发失败保 exact original Unknown，第二次 settleIssued 不重试。

## 限制

控制 receiver 测试证明模型固定 wire、实 coordinator 状态和 DTO 校验，不能替代 native CAS / Store crash / blob permission / 全局 quota / ArkUI queue-origin 或 SDK/device 证据。实际 native fixture 的两项测试范围为已导出 complete DTO，而不是本工具重新开 Store 或做设备操作。

暂停不是 SDK 回调队列已排空证明；caller 的 stable owner / epoch / revoked detach guard、Workbench 真正序列 sender、imports / pending pins 协调必须由 Root 页面集成保持。Model 不从 current cards 猜原请求，不自动重放未知结果、不重建新身份、不处理系统 picker / HUKS / protected storage。

favorite / category / TaskId rename 后“显式当前 revision open + 原子真实 TaskId V2 全篇编辑”仍是 native 和产品接口缺口。本模型不会靠 copied marker 永久拒绝以冒称完整追平，也未放松旧 strict historical continuation。完整目标未达成。
