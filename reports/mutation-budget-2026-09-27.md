# 文件变更预算与最大内容资格（2026-09-27）

本轮继续处理 [Wasm 接入报告](mutation-guest-wasm-2026-09-27.md) 中的最大内容限制。旧 IO v1 的单次 16 MiB、累计 64 MiB 以及 mutation import 的 128 KiB 最大回帧预留保持不变。以下预算设计与最终运行证据分开记录；SDK 未冻结。

## 瓶颈与计费范围

16 MiB 正文需要 273 个完整的 60 KiB 分块和一个 4 KiB 尾块，总共 274 块。标准流程包括 Prepare、274 次 Chunk、Commit、Execute、Query 和 Release，共 279 次 guest 调用；可信宿主另行完成选择、计划构造、签发租约和执行审批。

每次 guest 调用在效果前计算输入、import 请求以及最大回帧预算。即使只使用最大 128 KiB 帧的保守上界，279 次调用的传输预留也达到 109,707,264 字节。此外仍需计入原 owner 的内容缓冲、审批读取、Store 暂存、实际创建和历史核对，不能只为网络传输或最终文件大小计费。

还有独立的单次边界问题：审批读取需要 `content_length + plan_container_length`，创建与核对需要再计算响应开销。因此，正文恰好 16 MiB 时，即使累计预算足够，也不能通过旧的 16 MiB 单次准入上限。

预算是累计准入字节，不是常驻内存或物理磁盘的精确测量。一次 Query 可以顺序执行多个有界读取；每次读取受准入上限限制，所有读取都累计扣费。不得把单次额度当作整个调用的 RSS 上限。

## 显式 mutation 预算扩展

新增 `mutation-budget-v1` 必需特性及独立 `MutationBudget` 包声明：单次上限最多 32 MiB，累计上限最多 256 MiB。它只适用于 Create／Delete 的 mutation 包，不混用服务执行 profile。旧 IO 声明和原包无需改写；只声明预算也不能获得实际权限。

可信宿主通过独立 `Manager::bind_budgeted_mutation` 对精确实例、包摘要、当前批准和预算显式授权。实际批准值不得超过包声明；普通 `bind_io` 不接受此预算扩展包。单实例只允许首次绑定预算，释放资源、丢弃绑定或重新请求不能重置账本或隐式扩大预算。目标选择、内容审批与一次性执行许可仍分别强制执行。

`JobLimits::mutation` 提供受限的宿主配置；worker 启动同时校验包声明和原绑定的实际批准。它不是绕过旧 profile 上限的通用构造器。请求和绑定原有的 30 秒期限不扩大，取消与权限失效继续优先生效。

## 预检与实测要求

预检依据精确计划计算标准无重试流程的 worker／实例两个账本上界、最大单次准入及 submission 数量。在签发 guest 租约之前检查剩余额度，避免先接收大量正文后才发现必然不足。预检是快照，不预留整个工作流资源，也不能保证之后的权限、时间、并发使用或磁盘状态；逐次实际准入仍不可省略。

最大内容验证使用新建内容库、确定性的高熵 16 MiB 正文、真实单调时钟和实际 Wasm。必须核对最终文件长度与摘要、真实 Observed 结果、Release、实际累计计费不超过预检上界，并检查额外内容、较低批准预算与剩余额度不足时的提前拒绝。三语言资格脚本严格要求六个已注册测试，覆盖每种语言的普通流程和最大内容流程，零匹配不能报告成功。

## 执行证据

原生 Debug 宿主首轮 SDK 验证为 **5/6**：普通流程三语言全部通过，16 MiB 的 C／C++ 流程分别耗时 27,899／27,020 ms，Rust 最大内容流程领取 owner 回执时返回 Unknown。原始日志为 `build/mutation-budget-sdk-qualification.log`。Rust 单独复测在 32.61 秒结束，guest JobReport 明确为 `Err(Deadline)`、`cancelled=true`、`unknown=true`，见 `build/mutation-budget-rust-debug-diagnostic.log`。该复测没有定位到具体请求动作，不将初次 Unknown 直接等同于本次 Deadline；也没有自动重新执行同一操作。夹具现已为后续失败加入 owner 阶段、guest call_id、动作类型及耗时诊断，不输出文件正文。

WAT 最大内容流程的独立初次验证耗时 13,003 ms；完整流程累计 worker 170,987,754 字节、instance 204,479,392 字节，均小于预检上界及 256 MiB 批准额度。它不能替代实际 SDK Wasm 的验证。最新 scoped 原 owner／opt-in／核对测试 39/39 通过（另有六个需要构建 SDK 的显式忽略测试），包含释放后第二份最大内容计划在 Prepare 前拒绝；日志为 `build/mutation-guest-budget-runtime-final2.log`。

库严格 Clippy 仍有三处本轮前已存在的 `collapsible_if` 诊断；仅允许该诊断类别后通过，库 check 通过。相应日志为 `build/mutation-guest-budget-clippy-{strict,allowed}.log` 与 `build/mutation-guest-budget-runtime-check.log`。不将限定例外表述为严格 Clippy 全绿。

发布模式宿主的六项资格 **6/6 通过**，总测试时间 8.85 秒。资格脚本明确使用 `cargo test --release`，保持同样的 16 MiB 正文、真实时钟、30 秒绑定与请求期限以及 128 KiB 回帧预留；不以假时钟、延长租约或缩小文件回避 Debug 失败。日志为 `build/mutation-budget-sdk-release-qualification.log`。

| 实际 SDK Wasm | 16 MiB 流程耗时 | worker 累计字节 | instance 累计字节 |
| --- | ---: | ---: | ---: |
| C | 1,872 ms | 170,987,754 | 204,479,392 |
| C++ | 2,102 ms | 170,987,754 | 204,479,392 |
| Rust | 2,496 ms | 170,987,754 | 204,479,392 |

流程耗时从可信绑定前开始，涵盖选择、计划、分块、提交、独立审批、执行、Observed 查询及 Release；正文生成与预计算摘要在计时前，最终输出文件摘要核验在计时后。三者均核对实际输出文件长度／SHA-256、额度上界及释放后第二计划提前拒绝，既有普通流程亦通过。数字是本机新建临时内容库单次资格结果，不是跨设备性能承诺；Debug 不具备该最大流程的时限资格。没有为吞吐改为复用 guest 可变状态或削弱原隔离。

构建后 mutation 三个 Wasm 摘要与前一轮一致：Rust `fad599e58b7d540bef1ae1ee1407b6454508684bc76b9a1579487b9d6f68c1fa`、C `ca1529e768a1c3b39aca7a2bf0ceff7e7992309b24159d9098c15c93078d81c0`、C++ `157a5a4102752f30c2a37027a75993d171982df781e555105a99cafe769bb940`。17 个旧 transport 原件及 36 个 SDK 固定文件／13 对原件摘要核验通过。

预算边界专项回归：Core 包声明及旧 IO／服务 profile 共 31 项，runtime 显式绑定／旧 IO／服务累计预算共 20 项通过。覆盖低批准额度、单实例不可重新绑定、旧 profile、摘要／身份不符、撤权、非法 feature／预算编码等情形；不将这些单元范围等同于系统选择器的人工验收。

最后一轮兼容回归全部通过：

| 范围 | 结果 | 日志 |
| --- | --- | --- |
| 旧任务／UI／转换 Wasm 原件 | 9/9 | `build/mutation-budget-frozen-regression.log` |
| 旧依赖 Wasm 原件 | 3/3 | 同上 |
| transport 包结构验证 | 1/1，仅结构 | 同上 |
| 旧服务／HTTP 六个包的实际运行 | 2+4 项通过 | `build/mutation-budget-transport-regression.log` |
| 宿主 mutation／私有协议／原 owner | 30/30 | `build/mutation-budget-host-regression.log` |

所有网络回归使用本机测试服务；没有借此向第三方 API 发送请求。未重打包旧 guest 原件来适配新宿主。

## 后续门槛

后续更新：[guest 恢复报告](mutation-guest-recovery-2026-09-27.md) 已补三语言非空四块的丢回执和 36 个真实退出案例；下述开放项是本轮原始状态，产品与平台门槛仍未关闭。

仅关闭显式预算 profile 在 Windows Release 下的最大内容正常流程门槛。非空内容故障注入、跨进程恢复、产品审批额度接入、系统选择器人工验收、完整应用构建及其他平台资格仍待完成；Replace 不提供缺少条件保证的降级实现。SDK 继续保持未冻结状态。本轮未提交、推送或发布。
