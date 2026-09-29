# fixture003 冻结实现第二阶段只读审查（001）

2026-09-30。本次范围是 host candidate001、guest fixture003，以及 passive fault005 runner candidate001/002 的冻结实现、历史本地回归和运行前判断。结论：**host/guest 候选在已审源码和本地证据范围内没有新增运行前阻塞；runner001 的两项阻塞在002已修，但002仍有结果封存失败边界，整体暂不授予 READY。** 此结论不是三个故障场景的真实整链验收。

本审查没有发 HTTP、没有运行 host/guest、没有重跑 Cargo、没有改实现、既有候选或旧证据，也没有执行 Git。唯一执行的外部工具是已核 SHA 的本地 Cap’n Proto 离线转换器，用于解码已有本地 pipe 测试的原始帧。两个审查脚本只读取/hash 文件，或执行从冻结 Python AST 提取的纯函数/异常处理体；封存故障注入仅写本报告目录下明确标为 synthetic 的文件。

冻结输入如下（完整路径/指纹见随附 manifest 副本与 input-verification.json）：

| 输入 | SHA-256 |
| --- | --- |
| host candidate001 manifest | 82de408a5f2307c8c592202de59cafc5e4fa874d42d2d3cf85d279f58a2a5696 |
| host feature exe | 56704ee0416b84e022bd4f855be0cde512aba7b6ebd4fe5d517c084492e54bb4 |
| guest fixture003 manifest | b810526cfac7c0123fa3d8ce27e59f15724abec5888891ac104aef2192f38106 |
| guest exe | 377b63f7ac39401a832de44e88894719e1c7be3b8b20cb63f80eebdce1722746 |
| runner001 | 836ba0339b53d6a018cb7dbf146ed8d5995a166e6c0257d0601587fbe7c38d32 |
| runner002 manifest | 60e2687cd895c3639dccc7d544f420c84e508b3ec0e7f590ba0f33fbaac985ed |
| runner002 | c3da0a4ae3214488979eb6156f521a8fb93102126dc03794949820d67272fcf5 |

初始核验包含643条路径摘要：host270当前源和270冻结副本、guest78输入、runner001三个当前源和三个冻结副本，以及相关manifest、exe和回执/日志。此后另核 host build 前后270源完全一致、11组命令日志、两个 evidence 工具和 CLI 回执；guest最终三次命令各21输入前后一致、六份日志、Core源子集与最终build一致、native测试全部输入与最终build一致。build日志中907个唯一 compiler-artifact package id、build-finished success，以及非fresh的native bin产物与冻结exe摘要一致。runner002三个源的当前文件与冻结副本、pure-check前后摘要和11项测试日志均吻合。原fixture002的79输入保持情况是生产方回执声明；本次没有重新遍历它们或历史传递依赖。runner001当前工作文件随后由主协调修订，001冻结副本保持原状。

host H1/H2 源码边界已核：Shared::event 使用 admission.created 的同一单调域同时生成 at_us/at_ns；authority_deadline_bound 指向实际 parent/effect gate 原D；Challenge remaining_ms 与 sample_offset 使用同一个 Instant。Http::cancel 记录实际 gate fence，持久首次取消来源和原因沿既有权威回执路径保存。PipeFaultPlan 是 feature-gated 的严格固定结构，绑定实际 child args 中唯一的 spec SHA；config 摘要先加入独立域分隔、旧config与canonical plan，再按既有流程加入authority上下文，原admission.created未更新。批准只允许一次计划，无rearm；默认CLI拒绝计划选项，feature CLI拒绝init模式计划。已有52次测试调用通过（九组，存在重叠，不能算52个新场景），默认与feature版本的普通pipe/write/revocation回归都有日志和摘要。

H2 的故障请求仅通过已有host operator控制接缝进入单一pipe owner。owner在实际gate锁内验证原D、撤权、prefix真实完成、计划/identity/spec/nonce/prefix和重复请求；释放gate后再排入诊断事件。目标来自实际编码的首个BodyChunk：sequence2/generation1/offset0，完整帧长度、原始字节和SHA全部保留。只请求写12字节，prefix completion单独处理；完整BodyChunk的WriteCompleted和body_end推进没有伪造，尾部不会重新发出。端点drop须等待原操作全部reap，drop后的owner事件与owner实际join分别记录。期限前接受witness只是请求事实；001缺少实际close<D判定，002已使用drop后形成的同域after_ns作为保守完成上界。

已有本地Windows pipe测试的限定证据也吻合：完整原帧1348B/declared payload1344B，离线独立解码为真实BodyChunk，session71/epoch23/pid26608/attempt1、schema/config/operation摘要一致，业务body1024B/offset0。客户端实际Read4+8并累积12B，完整帧解码失败；只有一次12B Write reap（无错误），待决服务端Read实际reap为0B/995，issued_body_end=0、完整帧完成数=0。源码还断言drop后客户端EOF、没有剩余Read、真实owner join。另一peer断开例保留Read0B/109及“pipe Read error 109”，owner仍实际join。本次只是核对历史日志/源码和原始帧，没有重新运行这些测试，也没有guest pair或HTTP。这两例不能证明自然原TTL故障或整链故障合格。

guest源码实现的是 passive-observe：生产路径没有hold/release/read pause。严格spec限制场景、nonce、摘要、12B prefix及输出上限；诊断队列非阻塞、有界、串行写入并flush，失败/溢出不成为合格证据。真实Core生产event仍经events.reserve和原Operation::deliver_when_resolved；consumer next_event也用原gate，抑制与实际返回分开记录。native main真正消费并flush真实delta摘要，再读取task清理结果。Core cleanup预算与Close/control/fixture预算分开；后者共用一个500ms截止点，Close、控制EOF、三线程finished-only join、fixture停止/finished-only join不各自延长期限。保留既有blocking stdout逻辑admission到OS write的边界，未声称关闭此窗口。

guest Framer诊断包含实际4B长度头和实际Completed.bytes，最多12B/12片段；IO溢出、错误、长度不一致等均失去可用片段证据。完整原帧接受沿原decode/state路径，partial只产生诊断。错误/EOF且buffer仍有残帧不能变成clean boundary。first_data_failure独立保存，即使原State已经Unknown或HostCancelled也不清掉；data worker返回错误与实际finished-only join分别记录。cleanup先进行真实data join，再判断线程结果和控制协议；sticky control failure在等待不存在的RequestClosed前返回CleanupUnconfirmed。Core15/native35的日志、源摘要和最终构建吻合，但这些测试使用合成typed事件/completion、文件和真实本地线程，没有真实ModelClient HTTP、native pair或三个目标运行。

G2 到 H2 的信任边界须保留：运行前关闭请求依赖冻结guest producer写出的完整LF marker和read_fragments。runner先验证文件append-only、identity/spec/scenario/nonce/ordinal、actual prefix字节拼接、长度头、SHA和host原始目标帧，再把紧凑projection交给owner；marker行SHA不是guest签名或认证。此时整份native IO数组尚未取得，host也不能自行核查guest数组。运行后guest_reference_checks才把片段索引/Read id/transfer count/issue count、ResponseHead/frame及parser引用与最终整份IO/frames关联。IO数组没有独立raw Read字节，raw片段来源仍信任被钉住的guest源码/诊断；最终核查不能倒置成运行前已认证完整IO。这个接缝可作受控资格测试工具，不扩展为生产授权。

runner005其余三场景接线在此源码范围内符合设计：自然期限保持服务直到原host D自然取消；首因或清理不满足目标则unreached/unconfirmed。网络断开须在实际Core目标delta交付和parser进展后，以同一accepted connection执行真实SHUT_WR/close；没有提前Credit ACK或RST/抓包FIN声明。P须关联真实prefix/witness、实际owner关闭、guest残帧独立失败和真实join。每例只允许一个POST、没有重试；禁止BodyEOF/意图Observed材料化、检查撤权fence与资源回收。运行后仍需独立解码实际帧、核原生持久账本和所有证据摘要；runner源码或本地测试不是该整链结果。

已确认及复现的问题：

1. **runner001：晚pin失败可保留expected。** run先标expected_fault_observed，然后核末尾host/guest/runner pin；647–648行generic except只写error。提取冻结异常体执行“已expected后AssertionError”仍expected。002改为generic异常始终failed，checked_outcome仅全部后验通过后赋desired，清理也不把failed降成unconfirmed；本次纯提取反例确认修复。
2. **runner001：缺actual close<D。** pipe_fault_checks只比较接受witness时gate_sample<D。对真实本地事件副本仅把close before/after移到D+1仍接受；不能授予原expiry前的P目标。002返回close_before_original_deadline_proven=closed.after_ns<D，run把晚close标unreached；本次对同一纯反例确认修复，保留晚reap/join本身。
3. **runner002：seal写入后失败可留下expected文件。** seal_result先写manifest再写result；若result字节已写入后flush/close报错，catch修改内存unconfirmed，却因result已exists不更改盘上文件。纯提取封存函数，以“原dump完成再抛OSError”模拟该边界，得到内存unconfirmed/exit1，但盘上仍expected_fault_observed且与已写manifest预测hash吻合。hash阶段失败的现有测试可通过，未覆盖此情况。必须在新候选形成下游可验证的权威失败标记或盘上非expected结果，保留原件并加入post-write/close故障反例后才能READY。

本报告保留001与002各自结论。没有重分旧批次，没有授予M03整体通过、SDK冻结、产品接受或发布资格。修订完成后另建增量审查，核新manifest与纯负例；只有运行前问题闭合后，才由已授权的主协调决定实际运行。

审查产物：[摘要核对](input-verification.json)、[回执核对](receipt-checks.json)、[本地pipe原始证据](local-pipe-evidence.json)、[离线完整帧解码](local-pipe-frame-decoded.json)、[001纯反例](runner-blocker-reproductions.json)、[002修复与封存反例](runner-002-review.json)、[只读审查脚本](verify_audit.py)、[002纯审查脚本](verify_runner_002.py)。synthetic封存文件仅是负例输入/输出，不能当作runtime结果。
