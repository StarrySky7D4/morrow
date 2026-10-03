# W14 Windows 原宿主批准 POST SSE → 原 managed channel

2026-10-03。本阶段已落地无凭据、显式 opt-in 的 native SSE source，并验证真实插件消费与原 ACK 事务。完整 SDK／G02 仍 OPEN，生产 Workbench 可用性未改变。分支 `codex/windows-sdk-qualification-20261003`，基准 HEAD `63f38d4a8a5bf453248dc7532197bee6980dc86f`；源码增量未提交，实际 index 未改，未推送／CI／tag／Release。

## 修改及契约边界

- Runtime 只修改 `channel.rs` 并新增 `channel/source.rs`：完整命令与当前 owner 验证，一次附着的 SourceGrant、immutable收紧 deadline、撤权/队列/最终 Store ACK guard、exact wait_acked。旧 Source、bind_channel、import、单profile拒绝规则和基础 SDK pins 保持。
- 新 `network_node_stream_001` 可选 `managed-channel`：先原 Store retain/reserve/strict claim，后 native producer 一次 POST；SSE事件完整 envelope 走原 Receive/ACK。一个 frame 真实 ACK 后再取下一事件；首事件可有界预取。
- 原 Client/stream 增加 opening cleanup receipt，旧 send_stream 复用相同路径/相同Error。独立 event schema 不替换 Core/SDK schema。生成 metadata 使用仅生成模块的 unsafe lint 例外，人工模块仍 forbid unsafe；没有新增手写 unsafe。
- 正常 source EOF、HTTP EOF、parser complete、ACK、HTTP worker join 和 producer实际 join是不同事实。streaming command 仍OutcomeUnknown，没有伪造Observed/响应material，没有自动 replay。

具体宿主调用、预算与错误语义见 [指南](../../docs/PLUGIN_SSE_CHANNEL.md)。没有修改 Flutter UI、Workbench production availability、Linux process/controller/recovery、旧57原件或SDK327。W11合入的17路径与原本25项工作仍保留；原第16阶段丢失增量仍未恢复。

## 实际测试与工具链

Windows x64、Rust1.95/MSVC19.44/SDK10.0.26100、Release、offline、locked。工具/输入前后Hash逐轮记录，执行环境来自明确白名单；实际 child token **NOT_SAMPLED**，外层orchestration使用获准的提权执行，不借本阶段结果宣称普通用户资格。临时数据均为新普通合成 Store、文件与无密钥localhost，不触及真实库、DPAPI/保护owner或账户。

| 实际运行 | 实际结果 | 对应范围 |
| --- | --- | --- |
| guest-build-001 | 10条new/build/Core-pack命令exit0 | NEW Rust/C/C++ guest：模板原字节、SDK lock、真实Wasm/Core pack；此行单独不算执行 |
| runtime-execution-001 | 62个唯一方法全部通过，0 failed/ignored/filtered | channel_compat5、controls15、faults21、原base9、原dependency3、Windows executor9 |
| stream-execution-004 | 72个唯一方法全部通过，0 failed/ignored/filtered | lib26、client10、managed_sse14、sse13、envelope3、stream6 |
| stream-default-check-execution-001 | cargo check exit0，**0测试** | 未启用 managed-channel 的传输lib，独立target，非产品/测试证明 |

本阶段最终共 **134 个唯一实际通过方法**：17新桥接/codec方法与117相关回归。三语言真实执行在同一个 managed_sse 方法中循环3种新包，不能额外计成3个测试方法。编译和测试target按manifest分开，不清理旧构建目录；本阶段所有实际TEMP目录零残留。Runtime stderr的真实producer/executor panic是已存在的预期故障注入，与对应通过方法匹配，不应误计测试FAIL。

新桥接测试实证：

1. Rust/C/C++实际 Wasm import/run_invocation消费完整3事件；300长ID、split UTF-8/CRLF、多行/空data/`[DONE]`、inert retry元数据均保留；CHV1 mode/status/count/bytes/hash与原Store逐事件receipt一致，首ACK早于服务端许可EOF，实际HTTP和producer分别join。
2. ACK-before-Receive、错误hash/cursor不释放后续事件credit；source-only revoke在原Control仍active时抑制queued Receive/ACK；已提交history保留且下一事件不交付。
3. 原public dispatch的第5次实际批准probe位于Store两行INSERT后的final guard，第6次位于commit后的结果交付gate；真实数据库rollback／commit-history与拒交付分别核验。只在ACK主线程计数避免后台probe竞态，不mock Store、queue或commit；调用次序变动必须重新静审，不能放宽断言。
4. source期限精准Expired；close精准Closed；cancel/disable/新WAT trap精准Denied；各路径保留原HTTP清理结果和双join。坏MIME/status/UTF8/半字符/截断/decoder/raw/encoded预算、opening failure、NoWorker前置拒绝独立验证。
5. 两个独立adapter竞争原durable operation、重复start、错误实例/revision/包/subject/protocol/foreign broker、凭据/Last-Event-ID/profile拒绝均不多POST；原Store重开仍Unknown。未执行真实进程终止或跨重启重新批准，不因此宣称crash闭环。
6. envelope roundtrip及真实wire version/digest/Text UTF8/trailing/aggregate/encoded ceiling负例通过。

## 新三语言产物与冻结原件严格区分

没有可复用的原 SDK014 channel 产物，不能声称已执行那些原件。新包ID为 `org.example.w14.channel.rust/c/cpp`，由未改动的SDK examples通过项目new＋build与本地Core pack-v2生成；`morrow_plugin.py pack/check`未执行。测试必须提供6条新产物路径与各自完整SHA，不设ignore/缺文件跳过。SDK327、冻结57及所有工具/sysroot在相关构建前后恒同。

| NEW语言 | Wasm SHA256 | .mplugin SHA256 |
| --- | --- | --- |
| rust | `37469e698437c24b4bd6ce9ea836b7492688df4dd89e1a3b0a8f0c44e26444f1` | `d1c618ae1d7b8bc0f888aa3bf98df91478c7264455fa76e2b7943d802f61216b` |
| c | `3c8ced2be93790d15bb4bc39a889aabbfcedcb66d85a06c3e59ab51c8b383df8` | `3139b094514650db31fa99032aa03b94efbb7b7bff9d304b6905c6e504dd4969` |
| cpp | `a33aebec3f9dd0a2b1eab5f4760738f09031086a387e9363ec170ea9960845b8` | `e8837017a4a8e654383346689892d3eb123cd87e576f8bcbbc5c9d1bfed4883c` |

旧9个基础原包、3个依赖caller及原provider仍来自冻结基线；原Wasm和archive始终保持原字节，没有重编译、重打包或重新封baseline。它们在runtime-execution-001中真实回归，与上述NEW产物分列。W14未扩称全部生产shared-descriptor或非Windows资格。

## 失败保留、定向修正与NOT_RUN

| 原始轮次 | 实际结果 | 修正及限制 |
| --- | --- | --- |
| stream-execution-001 | cargo exit101，0方法/所有6suite NOT_RUN | Capnpc生成RAW_SCHEMA unsafe metadata与crate forbid冲突；仅生成module使用项目既有allow，四手写module保持forbid |
| stream-execution-002 | cargo exit101，0方法/所有6suite NOT_RUN | 新fixture误调Core私有receipt.encode；一行改完整public receipt Eq，未开放Core API |
| stream-execution-003 | cargo exit101，48 pass/2 fail | lib26+client10+managed12；managed guard/三语言两方法在server读取POST处遇WSA10035；sse/envelope/stream未运行 |
| stream-execution-004 | cargo exit0，完整72通过 | accepted socket明确阻塞模式，原超时/断言/EOF gate不变，无retry；第三轮失败完整保留 |

Windows非阻塞listener的accepted socket模式继承是合理推断，原失败日志没有直接查询socket mode，WSA10035也可能表示timeout，不能宣称已证明唯一OS根因。窄修改后的完整成功只证明当前fixture执行可通过，不算生产transport逻辑修复。首次静审还校正了ACK gate计数、EOF与撤权分离以及source期限的typed首因；静审修改不冒充红绿测试。

每轮started/env/runner/PID/原始stdout/stderr/raw-exit/result与工具/输入/产物SHA均保留。三份编译/运行失败、源码before/after/diff的独立fix receipt不覆写。Runtime全量recorder快照与最终bundle仅有3个未链接的network_node_stream_001路径不同：lib.rs生成模块lint、新managed_sse测试receipt Eq、support socket模式。默认check则仅有之后修改的未编译support文件不同。最终seal逐路径列明，实际Runtime/默认lib依赖输入保持原字节，不谎称所有recorder快照就是最终源码。

## 交付身份与剩余工作

本地证据根 `windows-sdk-20261003-evidence/w14-sse-channel`。`before.json`绑定W13 parent、327/57、原index和HEAD；`source-seal-001`提供本阶段及累积binary patch、增量完整文件、继承原tree的logical source manifest、临时index restore复验与逐运行输入关联。source identity中的pending/restore树必须一致，但不是commit或全物理源码重扫。交接ZIP保留原W13完整ZIP、原日志与新产物，archive read-back和最终receipt另附；不能将内部交叉审查称成独立第三方SDK认证。

完整SDK仍OPEN。优先下一项：明确WS独立源合同/有界生命周期、生产Workbench owner＋批准目录＋接口发现资格，再推进目录/blob、异步组合和跨进程Unknown矩阵；Account/OAuth、特殊认证、changes/富UI与所有目标平台按原26需求继续完成。G01/G02/G03/G04/G05/G06/G07/G08/G09/G10只有当前子范围证据更新，未收窄用户完整目标。protected owner、真实DB、GUI、TLS/公开API/账号、普通用户token、其它平台与原committed-tool runner均未运行；不为gate自行commit、安装或修改安全设置。
