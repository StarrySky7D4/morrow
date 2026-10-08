# C12：Codex 会话与进程接口增量

日期：2026-10-06。基线 commit `e83cdf1d3506b001d07ffceaaa9f98b688bcc150`，
tree `abcdee6e582ce18954118430e514f1d0805ce095`，含原 C11 未提交增量。
本轮本地实现与验证，未提交、推送或发布。完整 SDK26／G04 和产品资格继续 OPEN。

接口、实际 Windows 进程与真实 Rust Wasm 耦合已完成限定验证；下表只记实际运行结果。

## 实现范围

| 入口 | 新实现 | 权限与生命周期 |
| --- | --- | --- |
| `codex-session-exec-client-r2` | 会话 create/list/snapshot、writer/append/checkpoint、archive/sealed continuation/history、propose/claim/report/inspect；Rust 原生客户端和真实 Rust Wasm | 原 R2 canonical 请求与回应，每次交换一次；无自动重放 |
| `agent-process-control-v1` | discover、read/events、stdin write、close-input、interrupt、terminate、PTY resize 的独立 canonical 合同与可信 host | 真正支持与已批准能力的交集；固定期限、连接与代数；Unknown 后禁止新控制 |
| `codex-process-control-client-v1` | 有界类型化 Rust 客户端与真实 Rust Wasm guest | 请求／响应关联、唯一请求身份、预算；传输 panic／丢失回应也保留 Unknown |
| `agent-session-process-v1-host` | 包审查与单一 `morrow_agent_session_process_v1.call` 入口 | 包摘要绑定原 R2 与 process schema、scope 和 capability ceiling；旧入口继续拒绝 |
| `session-exec-windows-v1` | 固定上游实际 `ExecBackend`／`ExecProcess` 对接与合成 Windows 资格 | 可信 host 先完成原提案、审批、Claim；实际一次启动后注册有限 handle；原生资格单独计数 |

`close-input`／PTY resize 的协议存在不等于固定 Windows provider 已支持。固定上游的
`ExecProcess` 缺少这两个原生方法，provider 必须返回 Unsupported，并在 discover 中不声明。
Windows pipe 的 Interrupt 在该上游实际为停止进程；Windows PTY Interrupt 不支持，不能
把它宣传成可继续运行的 Ctrl-C。系统沙箱、生产 UI 与其他平台分别验收。

## 执行批准与 Unknown

进程注册同时保存原 executor Connection、Admission 与完整 ToolIdentity。
新增原 R2 的纯读取 `validate_started_tool` 重用原 live-executor、提议者、期限、session epoch、
owner 和工具状态检查，不生成新执行令牌，不修改 Claim 或工具行。

Inspect／session-read 不授予执行权。Claim 而未 invocation-started 的记录不能注册实际 provider。
控制前、控制后及回复编码后使用真实新鲜时钟复验；失权后的已发生效果保持 Unknown。
Unknown 不重新外发；同一写入的回执不能变成新的写入执行。

退出与输出 EOF 分离。资源收尾由可信 owner 管理，即使插件批准已失效也可读取实际结束事实。
未确认关闭的进程、启动和清理保留资源占位；不能用一个超时或一个“terminate accepted”代替
实际退出与 EOF。

## 已执行的限定验证

以下范围分别计数，重复运行、子案例、filtered 和零匹配目标不增加方法数。

| 范围 | 最终结果 | 证据边界 |
| --- | --- | --- |
| process-control canonical／host | 23 PASS；locked offline、Clippy、fmt、no-host wasm32 check PASS | 含逻辑 provider 的协议与生命周期资格；不是 OS 执行 |
| R2 类型化客户端 | 10 PASS | 原 SQLite host、预算与关联异常 |
| 原 R2 客户端真实 Wasm | 6 PASS | 真正编译的 Rust guest 经原 Wasm factory；不是 Windows 原生执行 |
| process 类型化客户端 | 10 PASS | 传输 panic 的 Unknown、声明能力及关联核验 |
| 新组合 host | 14 PASS；all-targets Clippy PASS | 原 SQLite owner 与逻辑 provider，含真实 session／process Rust Wasm；不是 OS 执行 |
| 新严格 runtime 入口 | 6 PASS | 旧工厂拒绝、单 import、取消与共享调用预算 |
| runtime 原件／目录／任务定向回归 | 21 PASS | 与上述新入口 6 项合为 27；非完整 runtime；首次零匹配结果保留且不计通过 |
| 原 R2 当前回归 | 106 PASS | 本机 Windows 执行的 Rust 测试；其中 Unix 原生路径仍 NOT_RUN |
| 新 Windows provider | 实际执行 7 PASS；纯资源注册表 3 PASS；原生编译、限定格式及 all-targets Clippy PASS | 使用固定上游真实 ExecBackend/ExecProcess；纯注册表不算 OS 收尾证明 |
| proposal Wasm → Windows → process Wasm 耦合 | 3 PASS | 正常交互、原执行者过期／撤权、实际写入后 Unknown；真实封存 guest 与实际 Windows 子进程 |

原始日志保留首次编译错误、零匹配、依赖下载失败、修复后结果和退出码。
最终编译输入与测试快照逐项核对。组合 host 收尾修补之后，重新执行新入口 6 项、
运行时回归 21 项与原 R2 回归 106 项；三组的新源码前后摘要均为
`60687490eeed36620a07ec56657ea2ec80256c1ea56258e984e6cb20ff397061`，无输入变化。
旧通过记录继续保存，重复运行不增加方法数。
依赖只在本轮私有缓存下载公开内容，保留原 crates.io／固定 Git source、revision 和 checksum；
不改系统代理或全局配置。

### 实际 Windows 与 Wasm 链路

原生 7 项覆盖实际 stdout／stderr、退出与 EOF、原 R2 身份与事实、stdin 写入、
终止／pipe interrupt、截止期限、撤权、过期 Claim、运行时／工件变化及被上游删除的
环境键拒绝。3 项纯注册表测试覆盖未决占位、共享限额、外部 owner 释放和准确的资源回收。
最终原生执行收据为 `windows-provider-001/logs/native-20261006T084902/run.json`，
7 项、0 失败、退出码 0、前后源码相同；测试 exe SHA 为
`0de2fbcdc241a1210fd78e06bca0e98be9088c52a3cb62f5ec9c64caeb5b0bd9`。

耦合测试用原 session／process 两种真实 canonical 帧。proposal Wasm 先提交提案，
可信宿主核验实际提案摘要并审批、Claim；原 `execute_claimed` 中启动真实 Windows
子进程。process Wasm 再执行 discover、输出／事件分页、输入与终止，核对原事实、
实际退出／EOF 和资源返还。另两项分别确认原 executor 过期／撤权时控制被拒绝，以及
实际写入后回应被否决形成 Unknown 时，同请求重放、新写入和终止均不再外发，实际
回显严格一次。三项合计 3 个方法，不把过期与撤权子案例重复计数。

三份耦合记录 `wasm-windows-coupled-runs/{positive-final-003,expiry-revoke-final-002,unknown-final-002}`
均为退出码 0、各 1 PASS，真实运行且未改变源码。统一前后输入摘要为
`02a41ad2dcece2ab3eaa2a81ca1b764164c167611975260651a1fd118a389bad`；
实际测试 exe SHA 为 `1e25b924e3d8569737496bc38608aa679c666bdce62027ff4d98a9cae76f5bbd`。
两份 Wasm 始终复用原封存字节，未重新编译。测试的 sandbox provenance 为
`Some(None)`，不构成隔离沙箱、后代进程隔离、PTY、托管网络或 Linux 资格。

未决资源通过强引用保留，可信宿主须保持原状态、清理入口和驱动中的多线程 Tokio
运行时可达，直至 `cleanup_status` 为空。全部清理入口或运行时丢失可能使 Unknown
资源长期保留；本轮不声称全局无泄漏退出。已证明 backend 从未调用的取消预约可以
立即返还；不确定的启动不得按此例外释放。

### 最小兼容修复与原始失败

新 Windows crate 单独启用 `winapi 0.3.9` 的 `std` feature，修复固定上游 PTY
依赖的 `c_void` 类型兼容；没有修改原上游源码、unsafe 范围或旧锁。新锁 SHA 为
`d2292e6e06ecf6e9df609ce53ee3d48126cb24b136ebbf0fedd4e0b8fae5d752`，
仍为单一 SQLite links（0.38.2），没有引入 SQLx；来源／版本／校验值没有因 feature
修复而漂移。

首轮实际测试因测试夹具在构造 Environment 时未进入相应 Tokio runtime，7 项均失败；
仅修正夹具的 Handle.enter 后稳定复验通过。另一次通过期间发现非本测试的兄弟源码
变动，该记录保留并由稳定复跑代替。耦合首轮由于受限环境无法读取本轮私有 Git
checkout 而停止，属于编译受限、runtime NOT_RUN；使用同一离线锁和缓存完成后续
实际运行，没有将这次失败解释为功能测试通过。所有记录保留在有界交接包中。


最终 Windows 代码还完成新 src／tests 的限定格式检查与 locked offline all-targets
Clippy（`-D warnings`）。首轮 Clippy 的三处规范问题仅作构造器局部参数数目豁免、
两处等价 let-chain 合并；独立只读核对确认锁边界和权限语义未变。此后重新运行原生
7 项、纯注册表 3 项及实际 Wasm 耦合 3 项，最终收据与当前编译输入逐项一致。
修复前的通过与失败记录保留作历史，不累计方法数。

## Wasm 工件

| 工件 | SHA-256 |
| --- | --- |
| 原 R2 session guest | `07b202f95cf4e438b06938f44e63025ff6370211903229078f74140a1f1a8fff` |
| 新 single-import combined session guest | `b1f0f44ad2bbe2abc89eee0ee971683419bf3407f370194f5557df57fa45799e` |
| process control guest | `48e2dd065c0f6ec4971650dd34ec25f852469479344727bef93f5f90334e1b36` |
| 单独 proposal guest | `9552e969b09c9f982914e71ea99a3c93fddc2bdd30986f5932883bcaa3ac1243` |

proposal 输入帧摘要仅用于 nonce；可信审批比较实际 guest 生成并提交的 canonical Propose
摘要，不能用 task 输入帧 SHA 冒充实际提案 SHA。

## 保留与未覆盖范围

327 个原 SDK 文件、57 个冻结输入、原 R1／R2 schema／lock／封存归档保持原字节。
原 live R2 `safe_exec.rs` 仅新增上述纯读取授权方法；候选源码身份发生变化，不能继承旧冻结 pin。
C11 已有改动和报告保留；不更改云端 Linux supervisor／恢复路径。

完整 SDK26／G04、第三方产品安装批准链、生产 GUI、ProtectedSession、picker 来源、账户／认证
transport、系统沙箱、PTY resize／stdin closure 的实际原生能力、跨重启恢复、其他平台与完整
Codex 应用注入仍为 OPEN／NOT_RUN。本轮不接触真实内容库、密钥或 DPAPI，不运行 CI，
不把普通合成执行资格算成生产安全沙箱资格。

接口用法见 [Codex 会话与进程接口](../../docs/PLUGIN_AGENT_PROCESS_CONTROL.md)。
