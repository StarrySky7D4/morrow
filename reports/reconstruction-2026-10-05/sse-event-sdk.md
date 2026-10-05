# C05 Windows SSE 类型化 SDK

2026-10-05。先完成 C02 Windows 复验和 C04 WS 阶段，再在云端 `468ef2e` 的独立本地候选实现 SSE 三语言 payload SDK。应用仍 `0.1.9-test.58+62`；原 Windows 分支、SDK327／冻结57、原 runtime/Core/网络生产源码和历史客体保持原字节。没有提交／推送、tag／Release／CI，未恢复已丢失 stage16，也未运行 Linux 或受保护生产 owner。

## 实现与限定结果

新增 independent sse-event-v1：Rust Event／EventRef、C合计有界65536 storage、C++17move-only owner、严格UTF-8／None/0/MAX、原raw schema、有限 traversal／nesting及原默认allocator。实际源预算可以更窄；非规范 wire 可接受但重新编码可能超 cap，不截断或调整 allocator。接口见 [SDK](../../docs/PLUGIN_SSE_EVENT_SDK.md)。

| 资格 | 实际范围 | 结果 |
|---|---|---|
| 原冻结件与 C04 | SDK327／冻结57和C04输入逐字节恒同；引用C04原件42的已封存实测 | 没有将旧42记为本阶段重新执行 |
| 新独立库 | Windows Release、offline、locked，6个唯一Rust方法；严格Clippy／自身格式检查 | 6通过，零失败／忽略／过滤 |
| 准备策略 | 11个Python方法；完整WASI sysroot16,800文件、实际tool／packer／源码前后摘要 | 11通过，旧客体不重建 |
| 最终网络回归 | 11个真实程序：lib28、client10、managed_sse14、managed_ws10、sse13、sse_envelope3、sse_sdk2、stream6、websocket9、ws_envelope3、ws_sdk2 | 100通过；原C04的98＋新增2，零失败／忽略／过滤 |
| 原生与三语言 conformance | 83 wire vectors：22接受／61拒绝，19个独立检查实现、11条最终命令；Rust/C11/C++17实际消费者 | 每语言21份成功编码由原生程序复读，默认allocator字节一致 |
| 项目外分发 | 90文件＋manifest、独立验证器、两库与六新客体，实际包装与运行 | 见 [分发资格](channel-payload-distribution.md)，不重复加运行方法 |

语料覆盖三完整字段、UTF-8／NUL、hasRetry、None／0／MAX、版本／digest、trailing／capability、multi/far pointer、扩展struct、owned alias与错误输出保护。一个65,536-byte有效wire在默认allocator重编码时真实返回Limit；三处共用24KiB text指针的24,680-byte wire实际被原生拒绝为Invalid，不能声称其达到aggregate分支。普通owned／borrowed合计65,537的独立检查另证Limit。

## 实际 guest 与生命周期

三个新guest走原 nonduplex Events Receive／exact ACK，冷启动消费5个完整事件：Unicode多行及300-byte id、空ID／retry0、忽略NUL id行／retryMAX、字面 `[DONE]`、`[DONE]`后的事件。原Store ACK5之前不放HTTP EOF；检查完整原 frame／payload／cursor／digest 和 transcript，而不是只看guest exit。

另执行三语言×source撤权／原instance Stop共6种控制组合，待原ACK1提交后控制。ACK1保留，无ACK2／新业务output，来源grant失效，原HTTP worker和broker producer分别join，只有一次POST。成功EOF／ACK及已释放资源仍只证明消费和清理，业务为OutcomeUnknown；竞争同command不产生第二次请求，不自动重放。

撤权首错有原生Source(Denied)、awaited HTTP的Transport(Denied)及source.revoke直接取消产生的Transport(Cancelled)。断言只接受精确类型，并核对同原grant Denied及所有持久回执／join。Stop本轮只接受并观察两种Denied，Guard也可能取消token；没有宣称全部Stop交错均被覆盖。

## 失败与身份

保留原support被rustfmt递归格式化、精确恢复和第一次source fence失败；此问题只在新测试authoring阶段，原support恢复到C04字节。随后保留首次CPP Stop的不同typed Denied、一次未输出首因的runtime002失败（仍UNLOCALIZED）、full-network001执行80／79通过／1失败及实际Cancel首错定位。最终新源码重复全量100通过，重复不加方法；只修注释的第三次最终全量100另封存，前面记录未改。

最终B封存 `8a4d58acb15a96d4b67fb9ba10fc7e0502b2c129da43ba0908be881e2996aef1`；库回执 `9ba715d7674681271d20f4f4bef93ce65521ac5512049506eb111e2958f59ccf`；conformance `caa735957e4c230477602603839bd70e753bea5ec3cf3deed81c2fe12b55b4c6`。回执绑定raw输出、退出码、源码／工具／实际binary及输入前后哈希，本地有界交接另保存可恢复增量。

生产catalog/owner/GUI、protected session/IoWorker、普通desktop token、真实数据库/DPAPI/账户、TLS/真实上游、其它平台、完整恢复与SDK26门槛仍OPEN／NOT_RUN。本阶段没有将普通合成Store替代生产批准，也不把合法payload升级为包能力。
