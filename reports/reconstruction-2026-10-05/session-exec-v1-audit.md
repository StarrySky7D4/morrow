# 会话与安全执行基础接口缺口复核

日期：2026-10-05。开发线 `codex/windows-sdk-convergence-20261005`。

**存在缺失。R1 已封存的源码与验证记录保持完整，但基础层仍有 6 项缺口，需在新的 revision 修补。此前“基础接口已补齐”的结论需要修正。** 本次只做审计、独立复现及状态文档更新，没有修改冻结源码、schema、锁文件、基线或 pin，没有提交、推送或发布。

## 基础层需要补齐的内容

P1 表示优先修复的恢复阻断；P2 表示随后修复的基础校验或生命周期缺口。这些是本次审计的修补优先级，不是安全漏洞等级。

| 优先级 | 缺口与实际触发条件 | 影响及下一 revision 要求 |
|---|---|---|
| P1 | **会话未预留收尾和恢复容量。** Create、OpenWriter、126 次合法 Append 用满 128 条回执后，Checkpoint、Archive、OpenWriter 全部返回 Limit；31 条 32 KiB 事件后，合法 32 KiB Checkpoint 也因 2 MiB 状态上限失败。[session.rs:479、113](../../extensions/agent-session-exec-v1/src/session.rs) | fork 和 compact 要求当前尾已封存，无法解除此状态。接受普通写入前须预留封存、归档和新 writer 的最坏空间；明确有界回执保留或续接方式，保留完整原请求幂等与历史身份约束。 |
| P2 | **缺少执行事实递进入口。** 可信回调返回 exit=0、output_closed=false，Report 成功后，后续 output_closed=true 返回 Conflict；再次 execute_claimed 被拒绝，观察永久停在第一次结果。[safe_exec.rs:642、789、804](../../extensions/agent-session-exec-v1/src/safe_exec.rs) | 新增可信宿主的单调事实更新／核对接口，分开记录退出和输出关闭，禁止再次启动执行。R1 明示允许部分事实进入 Reported，因此这是缺少后续生命周期接口。 |
| P2 | **Unknown 缺少晚到事实核对入口。** 回调真实写入副作用后返回错误，留下 Unknown、observed_facts=None；后来恢复出的事实 Report 返回 Conflict。[safe_exec.rs:797、801、651](../../extensions/agent-session-exec-v1/src/safe_exec.rs) | Unknown 和禁止重放的行为正确；需要独立可信宿主入口补录已发生的事实，绑定原操作／执行域身份，不签发新 permit、claim 或执行权。 |
| P2 | **准入名额不能回收。** 128 次累计 admission 后，撤销或到期仍占满 registry，新批准返回 Limit。[authority.rs:154、159、165](../../extensions/agent-session-exec-v1/src/authority.rs) | 补充失效 admission 清理，永久废止旧 nonce，保留原 issuer／connection 与期限检查；不能以重开 host 来恢复旧许可。 |
| P2 | **提议者离开后，宿主无法终结未消费的操作。** Approved 的原 proposer 被撤销或断连后，宿主 revoke_tool 返回 Denied，当前 reader 仍读到 Approved。[safe_exec.rs:753](../../extensions/agent-session-exec-v1/src/safe_exec.rs) | 允许独立可信宿主终结未消费的 Proposed／Approved，不依赖已经失效的 proposer。Claim／执行目前仍拒绝；已消费的 Unknown 不能改写为“从未执行”。 |
| P2 | **SDK 接受超尾游标的成功快照。** after=1、回复 tail=0、空 events 的成功回复同时通过 Reply::new 与 decode_for；宿主却返回 Conflict。[lib.rs:1200](../../extensions/agent-session-exec-v1/src/lib.rs)、[session.rs:319](../../extensions/agent-session-exec-v1/src/session.rs) | 成功 Snapshot 的请求／回复校验必须显式要求 after <= tail，并覆盖 typed construction 和实际 wire consumer。 |

本轮没有复现授权绕过或重复执行。缺口集中在可用性、事实核对与生命周期；缺口探针通过表示上述行为已复现，并不表示已经修好。

## 持续运行还需处理的限制

- **持久账本只有 128 个总行名额，跨 domain 共享。** 每个新会话、每个新执行提案各占一行；Report、Revoked、Archive 及 compact 不释放行。至少一个会话时最多容纳 127 个新提案，其他 domain 记录还会减少额度。唯一 DELETE 是同事务替换后立即 INSERT，没有独立删除或 GC。硬上限已在 R1 README 声明，不是隐藏违约；长期 Codex 使用仍需有界保留／退休合同。退休记录必须保留防重放身份，不能直接删历史后复用 operation ID。见 [Core ledger:145](../../core/src/store/agent_ledger.rs)、[R1 限制](../../extensions/agent-session-exec-v1/README.md)。
- **profile 与 Core 的 all-writes lease 绑定。** 撤销一个无关 Configuration 资源也会使原 host 的 Inspect、revoke_tool 返回 Denied；实际配置写入会触发这个撤销机制。需要明确独立 owner lease 或重新解析生命周期；恢复时重新准入，历史执行权不能续回。该行为已独立复现，属于目前的生命周期耦合。见 [authority.rs:93](../../extensions/agent-session-exec-v1/src/authority.rs)、[epochs.rs:62](../../core/src/store/service_authority_lock/epochs.rs)、[service_config.rs:224](../../core/src/store/service_config.rs)。

## 基础消费链仍未接通

| 缺少的接线 | 当前源码证据 |
|---|---|
| 真实 Codex ThreadStore／LiveThread 适配 | 当前 [会话 caller:4537](../../companions/morrow-codex/upstream/p02-integration-004/codex-work/codex-rs/core/src/session/mod.rs) 使用 LiveThread；旧 [FixtureStore:150](../../companions/morrow-codex/qualification/p02-integration-004/src/store.rs) 的 append 明确返回 durable_append_requires_M04，persist 不支持。没有新 writer／checkpoint／gap 适配。 |
| 真实 ExecBackend 与批准交接 | [caller:1362](../../companions/morrow-codex/upstream/p02-integration-004/codex-work/codex-rs/core/src/unified_exec/process_manager.rs) 仍调用旧 backend；旧 [Boundary:43](../../companions/morrow-codex/qualification/p02-integration-004/src/exec.rs) 不产生 StartedExecProcess。缺少 reviewed package → admit → trusted approve → 将 permit 交给绑定 executor → Claim → execute_claimed → Report 的实际 bridge。 |
| package／import／native 路由 | [package 白名单:300](../../core/src/plugin_package.rs)、[import 白名单:281](../../plugin_runtime/src/lib.rs)、[工厂:35](../../plugin_runtime/src/package.rs)、[discovery:379](../../workbench_host/src/sdk_profiles.rs) 均未导出新 profile。旧 native v2 仅 Close／Query，v3 仍是旧 HTTP stream；准入参数由可信宿主传入，不等于已经有真实 package 审批桥。 |
| 固定输入的受控执行域适配器 | SDK 只提供可信 callback 边界。产物／句柄核验、隔离、input 递送、超时与有界输出由适配器负责。真实 printf 测试先 hash 路径再按路径启动，stdin(null) 和 .output() 不能证明这些义务已实现。[README:53](../../extensions/agent-session-exec-v1/README.md)、[测试:1067](../../extensions/agent-session-exec-v1/tests/safe_exec.rs)。 |
| 可消费的 SDK 分发 | [当前分发清单:25](../../tool/package_plugin_sdk.py) 未纳入新 profile；新 crate 是 publish=false 和 ../../core 路径依赖。237 输入 pin 是本地源码闭包，不包括真实 Codex/native 消费者、工具链二进制或执行产物，现有门禁也依赖原 workspace 及日志路径。 |

逐项对齐真实 ThreadStore 后，persist／flush 可由 durable Append 回执和适配器队列屏障组合；shutdown／discard 可关闭队列并撤销专用 writer admission，无须为每个 trait 方法新增 wire。Codex 的 archive／unarchive 是可逆逻辑状态，可投影为 opaque metadata 事件；SDK Archive 是不可逆退休，不能直接替代。**物理 delete_thread 目前无法等价兑现**：上游明确要求删除历史及关联持久状态，而 compact／Archive 保留检查点、身份和完整请求回执。需要独立清理／防重放合同，否则返回 Unsupported。Snapshot 出现 gap 时，也不能把仅有当前模型状态的 checkpoint 冒充完整旧历史。见 [ThreadStore:169、176、180、187、532、536](../../companions/morrow-codex/upstream/p02-integration-004/codex-work/codex-rs/thread-store/src/store.rs)。

这些接线此前已标为 OPEN，本次进一步核对实际 caller、白名单和 dispatch。库内持久验证不能替代它们。

## 核查证据与边界

统一 [审计记录](session-exec-v1-audit.json) 保存六项 finding、接线源码身份、真实命令与证据引用。会话 consumer 六条实际输出与预期一致；执行事实 5 项、准入／owner 6 项独立 Rust probes 通过，0 failed／ignored／filtered。其中包含正向边界检查，不把 11 项都当作缺陷数。已逐项复核 36 个来源／日志文件身份，归档复现源码。

- [会话证据](../../build/session-exec-audit-v1/session-findings.json)
- [执行与准入证据](../../build/session-exec-audit-v1/execution-findings.json)
- [最终 pin 核查](../../build/session-exec-audit-v1/final-pin-check.stdout.json)

冻结 pin 仍为 `2e958fde445c37671f3100e1f880875fe4d8c9173a7014519494a55f1b9c33f5`；最终门禁实际 exit=0，237 源码输入与原 81 项正式测试记录身份通过。**本次没有重跑原 81 项；门禁核验身份与已记录日志，不判断接口完整性。** 原验证事实保留，当前审计增加反例。

正向复核确认：Report 不要求原 proposer 仍有效；原 executor 有效时，proposer 撤销或仅 proposer 到期仍可完成事实回报。新的 read admission 可读取 Unknown 已落盘观察，却不能恢复执行。Store pin 到原 Store 关闭才释放、只允许从当前精确封存尾 fork 都属于 R1 明示边界。

PTY、交互 stdin、输出字节流、resize、signal、terminate 按用户要求继续延期；不列为本轮必须补齐的缺口。Windows owner、ProtectedSession、GUI、C／C++ 消费者、生产 transport／OS sandbox 和完整 SDK26 仍未验收。

## 修补顺序

1. 新 revision 先解决会话收尾容量死路和 Snapshot 校验矛盾，补充满额／恢复的实际 consumer 回归。
2. 补可信事实递进／Unknown 晚到核对、准入回收与宿主终结接口，同时明确持久保留及 owner lease 生命周期；坚持一次执行和历史不恢复授权。
3. 将基础接口映射到真实 ThreadStore／ExecBackend，接包审批、import／native 路由与固定输入执行域，然后建立可消费分发和相应平台资格。
4. 使用新 revision、独立基线与新验收证据冻结修补结果；扩展执行能力仍留后续阶段。
