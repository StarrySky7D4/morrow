# partial-write-003 新增 Windows probe 联合只读复核

结论：新增期限触发和对端断开两个窄范围 probe 的原始证据成立，限定通过。它们使用真实同进程 Windows 管道与生产 pipe driver，未启动 Core、原生子进程、HTTP 或产品图。本报告没有重跑 producer 测试，也没有执行新的场景。

复核输入为 `host/m03-partial-write-003/windows-20260930-002`。receipt SHA-256：`00d3b039c809eb5e9160192c23c62846cec42c33576f235da0bf55ec997170bd`；windows-pipe-tests.log SHA-256：`4de8a088898b7a005974ff054fff77bab7a0086c2a72047d6336e84264c16238`。42 项源码摘要在 producer 执行前后相等，且本次独立核对的当前文件全部一致；复核前后再次检查未变。原日志为 15/15 通过，其中 8 项纯状态模型与 7 项同进程管道测试相互属于同一集合，不能与其他回归数量相加。

| 新增场景 | 实际证据 | 判定 |
| --- | --- | --- |
| 期限触发 | peer 读到 1024/8192 字节；同写操作 id2、issue ordinal1 初始 997，采样 996；gate/deadline 触发取消，取消前 poll 仍 996；随后同操作实际 0B/995 reap，confirmed prefix0、完整帧0、只有一次 write issue，owner 实际 join | 本地 driver 取消回收成立 |
| peer 断开 | peer 读到 1024/8192 字节；同写操作 id2、issue ordinal1 初始 997，采样 996；释放 peer 后实际 0B/109 reap；`write frame completion: Os(109)` 同时保留于 Closed 与 Error 事件，confirmed prefix0、完整帧0、只有一次 write issue，owner 实际 join | 错误保留与回收成立 |

两个场景各有自己的 pipe-owner `std::time::Instant` 时钟域。所有采样和回收区间合法，同一场景操作身份匹配；未跨域减算延迟，ns 只是记录单位，精度未测量。peer 已读的 1024 字节不能变成成功 OS completion 或确认整帧交付；原始 OS completion 均为 0 字节并带错误，driver 的 confirmed prefix 保持 0。

期限测试源码在获得 qualifying prefix/pending 之后，把测试 `EffectGate.deadline` 主动设为 `Instant::now()`，没有调用 `Driver::cancel()`来制造本次结果。这证明 driver 检查 deadline 后的取消/reap 路径，不证明原批准 TTL 自然届满、持久 source/reason20、guest 控制接收、Close ACK 或完整 deadline 链。

peer 场景在无在途 peer Read 时释放 peer，生产 driver 先取得 109 并保留错误，之后才进入停止清理；错误后的 cancel probe 为 no_operation。实际 join 证明线程回收，不能将该场景升级为业务或控制 clean。

真正的非零短成功 OS completion、完整 child/Core/HTTP fault 链、取消临界交错、并发、产品 G0、跨平台与 SDK 冻结仍未获得本报告资格。旧 A010 失败、A011/B011 原限定结论及冻结候选不变。本次未审查新 lifecycle004 固定候选的编译绑定或运行批次；其后续资格应根据新 manifest 与原始结果分别复核。

复核工件：[result.json](result.json)、[probe-chains.json](probe-chains.json)、[verified-inputs.json](verified-inputs.json)、[review_evidence.py](review_evidence.py)。校验脚本只读取已有原始证据与源码，输出本目录新报告，未导入 producer runner，未执行测试、build、HTTP 或账本写入。
