# 第六轮联合复核：P02 integration 004

2026-09-28。结论：**同一受限资格构建的执行、网络、存储三处接入成立；冻结原 exe 独立复跑 24 个场景、109 项计数断言通过，进程退出 0。** 本轮独立核验源码、补丁、锁、实际编译 feature 和产物关联，但没有独立编译。P-02/J-00/G0 仍 blocked，native/Wasm 产品图 0/2，84 项产品验收全部 not_run。

## 证据分层与固定身份

- `runs/p02-integration-004-readonly-001/result.json`：源码、构建证据及生产方回执的只读审查，状态 `verified_read_only`，SHA256 `5ec25f684f5fa448aec95f32a053d0993b70c7437d36c913f7fd0be788b15862`。其中没有独立运行声明。
- `runs/p02-integration-004-replay-001/result.json`：独立执行冻结原 exe 的限定结果，状态 `verified_limited`，包含运行输入、独立回执及前后摘要。独立运行与生产方运行明确分开。
- 新 handoff 83 项，SHA256 `513b1cfbf4b19b32c247b06810430ae29b1e1b779078b9761100a86c7a8d1751`；旧 17/35/129/53 项及 003 kit 保持冻结身份。合并快照 1245 路径在只读审查和复跑前后均一致；原始/各批工作源码与共享依赖另行重核。
- 原 exe SHA256 `0592d5e69bfe7fe1c6a5ecdcb86eb3e6f6ce703e4a4f80a66c409146e4bfb682`。独立回执 `runtime.json` SHA256 `0addd149df4e4ea15cacd1968589601fd1e8bbe81c6ab58b3afdb94dd533cee9`；生产方回执 SHA256 `8c8c681c597a86e993c7252ba60048bb5f445d6ccc1f1deea700051c97643f35`。

固定 Codex 原件 8697 文件，新副本 8700 文件；9 修改、3 新增，共 12 处差异，独立重产 693 行补丁逐字节相同，SHA256 `ce4af5db99badb4fc0c8d86e6701db710d5df8c1a989a101dd53b15f6d462a01`。该副本合入执行、Core 网络和 LiveThread 受限入口，不再只是三份独立旧程序的结果相加。

同一锁含 1117 包（1013 registry、104 local），90 个 Codex path 包指向同一新源码根。成功编译消息重核 905 个 compiler-artifact 包身份，Core 实际启用 `morrow-p02-qualification`、`morrow-p02-network-qualification`、`morrow-p02-restricted-qualification`，thread-store 实际启用 restricted feature。原 exe 对应这次生产方成功构建；生产方先前失败记录保留。正常产品与 Bazel 构建未运行。

## 独立运行与计数

| 场景组 | 场景 | 计数断言 | 本轮事实 |
|---|---:|---:|---|
| 三接缝共享生命周期 | 1 | 9 | LiveThread 存活期间并发调用真实 prepared exec 与 Core HTTP；随后拒绝持久化并显式 discard |
| 新 Tokio task 中的守卫 | 8 | 14 | 两种 TTY 默认执行、独立执行、HTTP/WS 缺注入、Legacy/Paginated 本地 store resume、create 均命中特定拒绝 |
| 重新编入同一 exe 的执行场景 | 3 | 27 | 原有限提案/拒绝语义 |
| 重新编入同一 exe 的存储场景 | 6 | 12 | 真实 LiveThread 与受限 fixture |
| 重新编入同一 exe 的 Core 网络场景 | 6 | 47 | 本地断开错误、合成 426 自然回退及同一 ModelClient 新 session 保持 HTTP |
| 合计 | 24 | 109 | 独立原 exe 运行通过，未另跑旧批 exe |

复跑使用 joint 内专属 HOME/USERPROFILE/APPDATA/LOCALAPPDATA/TEMP/TMP，固定系统 PATH，禁用个人 Git 配置；仅按名称读取少量系统环境项，未枚举环境值、读取个人凭据或使用真实账号/模型。未运行 Cargo。回执、stdout/stderr、前后输入快照和检查器副本均留在新运行目录。独立检查测试 store 目录在退出后仍不存在。

生产方与独立回执字节不相同。比较前先重算 6 份 HTTP 正文原始 UTF-8 长度和 SHA256（各 22025 字节），并重算执行参数摘要；随后仅归一化 JSON 对象键序、随机进程 UUID 及其派生参数/提案摘要，语义比较通过。动态提案 wire 未独立解码，不宣称整份回执逐字节重现。

## 守卫先于目标副作用

位置均相对新副本 `codex-rs/`。源码顺序审查与上述新 task 动态拒绝互相补充，不等同全系统副作用监测。

| 入口与守卫位置 | 后续目标位置 | 结论与边界 |
|---|---|---|
| `core/src/unified_exec/process_manager.rs:1324` | inherited_fds 回调 1327，随后 remote/snapshot/backend/default spawn | 缺注入先拒绝；两 TTY 的 lifecycle 调用为空。未动态构造 remote/snapshot 或 policy decider 变体 |
| `core/src/exec.rs:452` | 请求解构 455，随后 cwd/sandbox/spawn | restricted 独立入口先拒绝，after_spawn 为 0；不覆盖更低层直接 spawn |
| `core/src/client.rs:1197` | create_client_for_route 1208 | HTTP 缺注入在默认 transport 构造前拒绝；有注入仍提前返回 |
| `core/src/client.rs:1266` | 默认 API WebSocket connect 1269 | WS 缺注入先拒绝；外层 header/auth/telemetry 处理仍需完整资格 |
| `thread-store/src/live_thread.rs:144` | for_create/Git metadata 147 | create 在 Git 探测及 store.create_thread 前拒绝，fixture 无调用；所有 create（包括注入 store）均拒绝 |
| `thread-store/src/live_thread.rs:197` | load/metadata 200/204 | concrete LocalThreadStore resume 先拒绝；Legacy/Paginated 均命中 |

LocalThreadStore 守卫依赖具体类型 downcast，**不能约束包装类型、自定义 store、直接 LocalThreadStore 方法或调用前已初始化的数据库**。本例 LocalThreadStore/WriterLockCoordinator 构造只分配对象、路径和原子状态，不初始化磁盘数据库。restricted 是构建 feature，既非运行时权限令牌，也非操作系统隔离；调用者任意注入 backend 不等于可信授权。

## 生命周期证明范围

共享用例在真实 LiveThread 存活期间 join 执行与 HTTP 调用；两者各调用一次，附带 FS/HTTP capability 调用为空。随后 Standard persist 返回 Unsupported，显式 await `LiveThreadInitGuard.discard()`，guard 变空；释放句柄后四个 Weak 的 strong_count 为 0。

执行源码断言 store 顺序为 `resume_thread → load_history → persist_context:Standard → persist_thread_requires_M04 → discard_thread`。该完整序列未序列化到共享场景回执，应表述为已审查并执行的源码断言。discard 本身返回 Unsupported，InitGuard 记录 warning；四对象所有权释放与 guard 变空**不证明远端 writer 释放**。acquisition 取消和异步 Drop 清理未测试。Tokio runtime 在写回执前显式 drop，进程退出 0 是另一项事实，不能替代真实 child/socket/durable writer 生命周期。

## 尚未闭合的资格

网络断开仍是本地 `TransportError::Build`，并非真实 Connection/Timeout；426 是 `local_injected_error`，不是真实握手或宿主 HTTP 状态。setup Ok 不等于成功流。HTTP 注入仍跳过原 account-routing 的 redirect Reject，且未传递 redirect_policy；本轮集成不修复这一合同缺口。execute/unary、Realtime、成功 HTTP/SSE/WS、缓存复用、reconnect/auth、缺注入 prewarm/preconnect 的独立动态场景均未闭合。

准备入口前的 shell/environment capture、直接 LocalProcess/LocalStore、其他网络消费者、其他 metadata context、非空事务、生产 ThreadManager/完整 agent loop、正常产品构建、操作系统旁路仍未验。没有以目录未创建推断全局文件系统无读取，没有宣称完整无旁路。

下一项最关键缺口是**生产权限上下文与真实宿主后端生命周期合同**：M-02 的授权/撤销，以及 M-03/M-08 网络、重定向与认证、M-04 持久化与 writer、M-06 执行/PTY/IO/终止。随后才可能用同一产物证明真实 IPC 的成功、失败与释放；继续增加本地 feature 拒绝用例无法替代这些证据。本片结束，不开启下一实施批次。

## 交叉复核与台账

宿主只读报告 `../host/p02-integration-004-host-review.md` SHA256 `d8d0a102b62778295902d43537363df0bd06a3500478db47d9a3269ef4df6530`、输入检查 `../host/p02-integration-004-host-input-check.json` SHA256 `2f137d7b244de660c3e00684dea913f875c9b0eee98e969a1c2c60de0661a971` 已核对。其未运行 exe，本联合复跑是独立的一层证据；双方对具体类型守卫、owner/writer 区分和未覆盖入口边界一致。

本轮仅写 joint 证据、报告与验收台账，未改宿主/插件实现、003、计划原件或权威 Schema。最终一致性检查见 `runs/round6-final-gate/result.json`：31 工作包、7 门槛、84 验收及来源/引用一致性 verified，issues 为空；G0 仍 blocked，实际检查器退出 2。没有提交、推送或发布。
