# lifecycle004 candidate-002 与新 A/B 联合复核

结论：新固定宿主与本轮两次生产方运行的独立只读复核均通过。每次为一个新的合成 loopback POST，沿用冻结 fixture002/harness011 的严格断言。此结论限定于 Core 撤权与背压取消两个案例，不升级整体 M03、产品 G0 或 SDK 冻结。本聊天没有启动任何测试、build、child、HTTP 或新场景。

## 固定候选与失败记录

候选 `host/m03-lifecycle-004/candidate-002/manifest.json` SHA-256：`fa206f562d30b59686fcb7abfad7ff84467a0b7659f7a32db343cd782f4c2a13`。宿主 exe：`4661e2051da9770f9d040f386b3e7f26b3878670c274791244da0ce28bad756a`；build receipt：`03c093b0fd1a54711e7ef23d598ae8428591e79061263aa698c0b1b38a053b4d`。

263 项候选源码在 producer build/test 前后相等，本次逐项核实当前源码与冻结副本均一致。现存 build 与 library test 回执退出码均为 0，日志为 28/28；该数字与窄 pipe probe 集合有重叠，不能相加。未重建依赖，也未核验完整 compiler artifact/dependency trace；本报告验证保存的 build/source/产物/启动绑定。

冻结 helper011、matrix008、fixture002 manifest 与 exe 摘要一致；未重复扫描旧 fixture 的 79 项历史输入。两次启动均使用候选 exe 和固定 guest exe，原 TTL10000、handshake6000、close1000、单请求与禁止自动重试约束保留。每个批次的 `candidate-binding.json` 绑定候选 manifest、runner 源摘要与原 helper 摘要。

与 candidate-001 相比，263 项源码中只有 wrapper `tool/m03_lifecycle_004.py` 改变，宿主 exe 摘要相同；修订把 fixture 证据目录恢复到插件仓库 `out`，没有改原场景断言。candidate-001 原失败结果 `2d01efa63f11a671814e7e8df3d5d5b8d80f248317b594afaed6bb781803dee1` 保留：failed、0 POST、host exit2、reader/fixture 清理线程已 join。它不翻判为通过，也不计入下述两次通过案例。

## 两次实际运行

| 项目 | 新 A：Core 撤权 | 新 B：data-pending |
| --- | --- | --- |
| 批次 UTC | 20260929T213241184115Z | 20260929T213307996034Z |
| 原 result SHA-256 | `21810ed793cfc80c372c30275aad6acf46d670e32ad2aaceae72f3aa7f448f43` | `f12799423ab91449432dae1d0465ee39a66e8d7ac19608e9f9631ec176425d1b` |
| evidence manifest SHA-256 | `5dfc3478989e586e3aa1590b5066d10136a77c166f7a470dd0410af002c36e40` | `7b56d14fa3518b96c01f5d16f96dba7192b8529a6bcf93643ecdae254f8bb58a` |
| 原始批次文件 | 15 host + 5 guest | 16 host + 4 guest |
| 请求 | 1 POST、22025B | 1 POST、22025B |
| 撤权回复观察上界 | 15.3024ms | 15.2742ms |
| guest PID / session / epoch | 19816 / 14363750692603973175 / 1 | 25812 / 17267034834078408336 / 1 |
| 五个 offset：received / reserved / issued / OS completed / peer consumed | 197 / 197 / 197 / 197 / 197 | 8033 / 2048 / 1024 / 0 / 0 |
| parser / 最终消费 ACK | 197 / 197 | 0 / 0 |
| host / guest exit | 0 / 2 | 0 / 2 |

请求 body 与 server POST 原字节相同；独立重建协议 request preimage，确认 method、target、headers、response cap、body 及 request digest。目标为本机 `127.0.0.1` `/v1/responses`，无 Authorization header。撤权回复观察上界只使用 harness 同一 monotonic 时钟，从写命令之前到完整行接收、解析之前；不代表 guest 或 kernel 的撤权延迟。

A 的 actual Core `barrier-A` 为已排队、consumer 取出并保持的同一事件；`barrier-B` 为 producer 已预留并保持的不同事件。目标事件摘要、delta ordinal、stream ordinal 和 batch identity 全部匹配。真正 host HttpTerminal 接收时，取消门原先打开、first reason 为空，原子转为 HostCancelled，`host_control_closed_gate=true`；任务首因也是 HostCancelled。consumer 只在观察到真实宿主撤权与关闭门后获准放行；两目标均由原门抑制，producer permit 已释放，实际业务事件中没有这两条输出。

A 实际发生 data-first 0B/109 disconnect：只在完整 framing 边界单调暂停，固定 500ms 上界、原 authority 期限不续期、不恢复 delivery。暂停索引之后没有新 data OS issue、HttpCommit 或正向 credit 逻辑准入。已准入完成与真正消费的 window0 结算仍合法；不把控制逻辑准入时间当 OS issue 时间。

B 在 DataBound 完整帧边界停止新 Read，framer0/expected4、无在途读，Read issue 总数2、last id3 的实际464B完成吻合，暂停后计数不变。宿主写操作 id12、issue ordinal3、body_end1024、frame offset0、requested1364 初始997；三次实际996采样为同一个操作，间隔26.2300/25.3732ms；取消前第四 poll 仍996，`completion_claimed=false`，随后同操作实际0B/995 reap。独立 supervisor `data_operation_reaped` 一致，新增 `write_frame_state` 为 `Ok(Cancelled)`、confirmed prefix0，未报告完整帧。B 的控制先到，没有发生 A 的 data-end 暂停；主动 DataBound 停读与 disconnect 暂停不混同。

## 原始控制帧、回收和账本

每次离线解码21条已保存的 Cap'n Proto 原始控制帧，长度前缀正确、所有 envelope 的 admission tuple 一致，并与 guest/session/artifact/schema/config/operation identity 对照。未执行 native client 或 harness。

- A：HttpTerminal53 → zero HttpCredit6/59 → CreditState6/60（确认 parser/peer197）→ RequestClosed62（peer197）→ Close7/63 → State ACK7/68。它与旧 A011 的 RequestClosed 先到顺序不同，仍满足真实消费确认及资源关闭均先于 Close。
- B：HttpTerminal58 → RequestClosed63（peer0）→ zero HttpCredit6/64 → CreditState6/65（peer0）→ Close7/66 → State ACK7/71。

两个实际 State ACK 均 code0、generation2，宿主写出420B后才记录 stdout EOF、stderr EOF 与 guest exit。独立 control sticky failure 为 None、end EOF clean；aggregate/Close/final result 保留 Unknown19。Core 为 Cancelled、transport drain 为 None、无 HTTP EOF、无 response material，未当作业务成功。

guest cleanup 为 Ok，data actual join、3个控制线程及 evidence writer 实际 finished/joined，无 retained handle；harness reader/fixture 线程 join。宿主 network worker、data 资源和进程 exit/双 EOF 最终 Released。取消请求与实际回收分别核实，未仅靠 cancel 或 cleanup bool 授予完成资格；没有另外持有联合 guest 进程句柄。

两份 native ledger 使用 SQLite `mode=ro&immutable=1`，无 WAL；检查 MRNADM04 版本、大小上限、LZ4 与未压缩 payload SHA，再按字段解析。Owner Released 的 PID/session/epoch、exit 和双 EOF，与实际批次一致；parent state3/source1/reason19/TTL10000，HTTP state3/sendbudget1/cap65536，parent/request/body/artifact/config 绑定正确。读取前后数据库摘要不变：A `9f922eee853ade5b2adde03254a35b84bcf543575ebe81d582f633389edd4132`；B `cad0caea7fa63fbabc71050d23c2bcc9d0701c085ea94391ad1f843f83de1e1a`。Core coordination payload 未解码。

## 资格边界

这是两次生产方实际运行后的独立审证，独立 HTTP 次数为0。原 candidate-001 和 A010 失败继续保留，旧 A011/B011 结论不重写。真正的非零短成功 OS completion、完整原 TTL 自然到达/对端断开链、取消临界交错、并发、同用户隔离、崩溃后 owner 核对、跨平台、完整产品图与 SDK 冻结仍待各自证据。

复核工件：[candidate-result.json](candidate-result.json)、[candidate-inputs.json](candidate-inputs.json)、[wire-settlement.json](wire-settlement.json)、[A result](core-revoke/result.json)、[A ledger](core-revoke/durable-native-ledger.json)、[B result](data-pending/result.json)、[B ledger](data-pending/durable-native-ledger.json)、[B 操作链](data-pending/pending-chain.json)。本目录校验脚本只读取已有工件、离线转换原始帧并输出新联合报告；没有导入生产方 runner。
