# 文件变更 guest Wasm 接入（Windows，2026-09-27）

分支 `codex/io-safety-refactor`，基线 `b9225f64f6c62584ad7243e30249d8a088bcb155` 上的未提交工作树。本轮在用户明确授权的 Create／Delete 范围内，实现独立 Wasm import、三语言调用封装和原 owner 接入；测试实际文件均位于临时目录。SDK 未冻结，未提交、推送或发布。

## 实现与边界

- 包显式声明 `mutation-v1`、guest ABI 2、IO 能力及精确 mutation schema SHA-256。旧 IO v1 契约和冻结原件不变，旧任务／IO／服务入口拒绝 mutation 包。
- `morrow_mutation_v1.call` 使用有界且不重叠的内存区域；没有公开的原始回调执行旁路。运行时通过私有 continuation 在原受管 owner 中直接分派，不另建 Store／Manager，不等待自己的队列。
- `Stage(lease)` 和 `Execute(permit)` 分离。每个 job 仅接受一条与宿主输入一致的 import，最终完成帧必须与该次调用的实际响应完全相同。执行审批由可信宿主在准备任务返回后签发。
- 租约绑定原 worker、实例、选择和不可变计划；签发及执行许可必须已被领取。每个 worker 最多签发 128 个选择引用，Release 不返还引用身份；submission 账本最多 512 项，不缓存分块正文。
- 重复 submission 绑定规范化参数和原始期限，只允许改变交付 callId；命中缓存仍检查当前授权和调用模式。执行许可一次性消费；Unknown 不自动重发。
- 请求与最大 128 KiB 响应预算在效果前预留。请求期限关联原任务取消；提交后无法交付时保留不确定性，Observed 必须来自原 Store 中的真实 OS 结果。
- Rust／C／C++ helper 负责请求及响应校验、独立缓冲与相关性检查；传输错误不重试。示例只执行一次宿主绑定请求，不自行审批或选择路径。

## 本轮验证

| 项目 | 结果与范围 |
| --- | --- |
| 包声明、新旧 IO 协商 | Core 13 项通过 |
| Runner／continuation／package 单元测试 | Runtime 57 通过，2 个既有显式忽略 |
| SDK 原生测试 | 78 项通过；原生及 wasm32 严格 Clippy 通过 |
| SDK 契约同步与共享向量工具 | 10/10 通过 |
| 三语言 Wasm 编译 | Rust、C、C++ 均生成真实模块 |
| 三语言 Wasm 原 owner 执行 | 3/3 通过，实际 Create／Chunk／Commit／审批／Execute／Query／Release 及 Delete |
| 原 owner／opt-in／历史核对回归 | 31＋2＋4＝37 通过；普通运行跳过的三个 SDK 资格测试已另行显式通过 |
| 旧任务／UI／转换原件 | 9/9 实际运行通过，无 guest 重编／重包 |
| 旧依赖原件 | 3/3 实际运行通过，无 guest 重编／重包 |
| 旧 transport 包结构与能力预算 | 1/1 通过，六包原模块保持一致；此项本身不是 HTTP／服务执行证明 |
| 旧 HTTP／服务六个原包 | 服务 2/2、HTTP 4/4 实际运行通过；含七种方法、权限拒绝、Unknown 不重发及重开历史 |
| Workbench 变更回归 | 30/30 通过，使用真实工作台 Wasm 及 Windows 宿主路径 |
| 旧原件摘要校验 | 17 个 transport 原件；36 个 SDK 文件、13 对原 Wasm／包未变 |

三语言共用原 owner 夹具，验证非空创建、相同提交重取、同提交变更参数拒绝、Stage 越权 Execute 拒绝、实际文件字节、真实 Observed 查询及 Release 重取。新增结果读取容量不足时保留结果、随后足额领取的断言。每次结果读取后释放并发任务容量；IO 的 `max_jobs=4` 是并发／保留容量，不是 worker 生命周期累计任务数。

真实 WAT 反例还覆盖取消计划、foreign reference、未启用的 worker，以及两项期限测试：原 submission 超过其 1 秒期限后更换 callId 仍被拒绝；原 owner 在屏障处等待至请求期限过去，继续 Execute 时不会产生删除或 claim，原历史保持 Prepared。最终记录在 `build/mutation-guest-dispatch-runtime-final.log`。

Runtime 格式检查通过。严格 Clippy 仍被三个既有 `collapsible_if` 诊断阻断；仅放行该类诊断后的检查通过，没有宣称严格零警告。两份日志为 `build/mutation-guest-dispatch-clippy-strict.log` 和 `build/mutation-guest-dispatch-clippy-allowed.log`。SDK 严格检查与 runtime 的这项限制分开记录。

可复现入口：

```powershell
./tool/verify_plugin_mutation_wasm.ps1 -Sysroot <WASI-sysroot> -Python <python>
```

脚本先构建三种语言，严格枚举三个已注册测试，再显式运行 ignored 资格测试；零匹配测试不能报告成功。构建支持绝对或相对输出路径。构建日志 `build/mutation-sdk-wasm-build.log`，资格日志 `build/mutation-guest-wasm-qualification.log`，旧原件运行日志 `build/mutation-guest-frozen-regression.log`。`tool/plugin_transport_baseline.py run` 的网络／服务原件记录在 `build/mutation-guest-transport-regression.log`；宿主回归在 `build/mutation-guest-host-regression.log`。

| 文件 | SHA-256 |
| --- | --- |
| `rust_mutation.wasm` | `fad599e58b7d540bef1ae1ee1407b6454508684bc76b9a1579487b9d6f68c1fa` |
| `c_mutation.wasm` | `ca1529e768a1c3b39aca7a2bf0ceff7e7992309b24159d9098c15c93078d81c0` |
| `cpp_mutation.wasm` | `157a5a4102752f30c2a37027a75993d171982df781e555105a99cafe769bb940` |

## 未关闭的交付门槛

后续更新：本节保留本轮原始结论；随后通过显式 `mutation-budget-v1` profile 完成 Windows Release 三语言最大内容资格，旧 profile 不变。新设计、Debug 失败与 Release 通过的证据见 [预算与最大内容报告](mutation-budget-2026-09-27.md)。

16 MiB 是协议内容上限，当前 64 MiB worker 累计硬上限无法容纳该最大内容的全部分块响应预留、准备和执行审批；不能通过提高调用参数突破此上限。本轮未降低响应预留，也未宣称最大内容可完成。需要后续单独设计预算、预检和最大值运行验收。

还需非空多块内容在写入、刷新、发布及领取丢失位置的真实故障矩阵，产品侧插件动作与可信选择／两阶段审批整合，系统选择器人工验收及完整应用构建。Linux、其他原生平台与 Web 尚未获本轮资格。Replace 仍不提供缺少条件保证的降级实现。
