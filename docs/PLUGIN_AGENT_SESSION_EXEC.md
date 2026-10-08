<!-- C16-CURRENT-BEGIN -->
2026-10-06 C16 本地实验性候选：新增可信 Rust Agent start/submit/poll/cancel/recover/acknowledge
入口，移动完整原 owner，沿用同 Core/Store/runtime/连接；每 import 维护原 owner。
借用执行与 scheduler 回收有界；真实 join 后仅同步唯一 Runtime 所有权可回收，别名保留 16 有界债务。
普通无沙箱、非 ProtectedSession 的封存 Rust Wasm＋实际 Windows worker 耦合 3 项通过；各组收据见报告，
不作为生产 GUI、真实受保护库/DPAPI 或生产沙箱资格。旧 SDK327/57、合同、Linux 与 C15 工件守卫通过。
接口仍 experimental，SDK26/G04 OPEN；未提交、推送、Release 或 CI，不代表正式 SDK 冻结。
见 [原生 owner 接口](PLUGIN_AGENT_NATIVE_OWNER.md) 与 [C16 报告](../reports/reconstruction-2026-10-06/codex-sdk-c16.md)；下方 C15 及更早内容保留其历史范围。
<!-- C16-CURRENT-END -->

<!-- C15-CURRENT-BEGIN -->
2026-10-06 C15本地候选：工作台完整会话插件管理已接独立admin协议/owner与设置界面，
借原Manager/持久catalog，保留原busy/lost/恢复门禁和顶层native管道，不改旧Core/IO。
admin10、owner18、旧host18/14/13/1回归分别通过；Dart18（含实际Rust五对向量互验
和原管道Busy路由）通过。新库strict Clippy、Dart fatal-infos/限定格式和Windows宿主check
通过；不是新OS执行/真实桌面/ProtectedSession资格。327/57/旧合同/Linux/封存工件保持。
实际native port借原runtime/worker、生产GUI、认证/sandbox、交互控制及全SDK26/G04仍OPEN。
没有commit/push/Release/CI。详见 [C15报告](../reports/reconstruction-2026-10-06/codex-sdk-c15.md)；下方C14及更早内容保留其历史范围。
<!-- C15-CURRENT-END -->

<!-- C14-CURRENT-BEGIN -->
2026-10-06 C14本地候选：完整Agent wrapper持久catalog/approval接口已实现，原Manager撤销seam
和同原connection/freshadmission接线保持。新catalog18、Manager新3与其原6、原回归11/1/6、
旧managed/route/schema14/13/1分别通过；实际Windows新1通过（helper1过滤，sandbox=None）。
大包装不受原快照512KiB限制，static审批不恢复livegrant；保存故障/Unknown即时撤旧Core，
旧效果不重放。327/57/旧合同/封存工件保持；Workbench新协议/GUI、ProtectedSession/native
owner、认证transport、sandbox、交互控制/其他平台及SDK26/G04仍OPEN。未commit/push/Release。
详见 [C14报告](../reports/reconstruction-2026-10-06/codex-sdk-c14.md)；下方C13及更早文字为各日期历史，不替代本条实际范围。
<!-- C14-CURRENT-END -->

# Codex 会话与安全执行基础接口

<!-- C13-CURRENT-BEGIN -->
当前 C13（2026-10-06，本地未提交）：会话/进程新入口已接原 Catalog/Registry/Manager，
共用原实例限额、Control 与连接；基础包与完整 wrapper 仍独立批准，预算取交集。
新增 managed host 14、Manager 6、HostIdentity 3、真实 Windows managed Wasm 2 分别通过；
原 host 1+13、Manager 11+1、strict 6、process 18+5 回归另计，重复不累计。
修正提交前取消、effect 后 Unknown 与错配收尾；新 host strict Clippy/限定格式通过。
旧 327 SDK／57 冻结输入、wire/schema 与封存工件保持；新候选不继承旧冻结 pin。
完整 wrapper 持久审批、Workbench/ProtectedSession owner、sandbox/认证/其他平台仍
OPEN/NOT_RUN；SDK26/G04 未冻结。见 [C13 接线与实际边界](../reports/reconstruction-2026-10-06/codex-sdk-c13.md)。
下方 C12 及更早检查点保留历史结果与当时身份；本轮现状以 C13 为准。
<!-- C13-CURRENT-END -->

<!-- C12-CURRENT-BEGIN -->
当前 C12 接口增量（2026-10-06，本地未提交）：已补会话 Rust/Wasm 客户端、类型化进程控制、
严格 single-import 组合运行入口和原 R2 执行权实时复验。process 合同 23、R2 客户端 10、
旧入口真实 Wasm 6、process 客户端 10、组合 host 14、运行入口 6、定向 runtime 回归 21、
原 R2 回归 106 分别通过，重复方法不累计。Windows 原生实际执行 7 项、纯注册表 3 项、真实 proposal Wasm→Windows→process Wasm 耦合 3 项分别通过；封存 Wasm 未重建，Unknown 不重放。
327 SDK／57 冻结输入及旧封存归档保持；新候选不继承旧冻结 pin。
原生 close-input／PTY resize 尚 Unsupported；完整 SDK26／G04、产品 GUI、受保护内容库、
OS sandbox、认证 transport 和其他平台仍 OPEN／NOT_RUN。详见 [C12 接口与限定证据](../reports/reconstruction-2026-10-06/codex-sdk-c12.md)。

下方 C11 及更早日期的文字保留当时的身份、结果与未运行范围；本轮现状以 C12 为准。
<!-- C12-CURRENT-END -->

2026-10-06 Windows 后续限定复验：R2 本机实际运行 **106 项通过**；Unix 真正进程执行器本轮 `NOT_RUN`。新版外层归档校验器在 Windows 核验原 Linux R2 包，通过 573 个源码输入、156 项 Rust 测试记录及 32 项 Python 测试记录；这是封存字节与历史记录核验，没有重新执行这些 Linux 测试。原 R1／R2 归档、基线与 pin 保持原字节，新候选源码仍须独立审阅，不能复用旧 pin 声称已冻结。

四组 Python 门禁共运行 102 项：99 PASS、3 项因 Windows 不支持 FIFO 而 SKIP；该结果不满足冻结门禁的零跳过日志要求。完整 SDK26／G04 仍 OPEN，生产 GUI、ProtectedSession、picker 来源与其他平台本轮 `NOT_RUN`。详见 [Windows C11 限定复验报告](../reports/reconstruction-2026-10-06/windows-sdk-c11.md)。

在已准备锁定离线依赖、Cap’n Proto 编译器和 Windows Rust 工具链的候选仓库根目录，使用独立外部输出目录复验：

```powershell
cargo test --manifest-path extensions/agent-session-exec-v1-r2/Cargo.toml --locked --offline --target-dir $R2Target
$env:TEMP = $PythonTemp
$env:TMP = $PythonTemp
$env:CARGO_HOME = $CargoHome
python -B -m unittest tool.tests.test_session_exec_freeze tool.tests.test_session_exec_r2 tool.tests.test_codex_sdk_candidate tool.tests.test_agent_host_build -v
python -B tool/verify_session_exec_r2.py verify-archive --baseline reports/reconstruction-2026-10-05/session-exec-v1-r2-freeze.json --source-archive reports/reconstruction-2026-10-05/session-exec-v1-r2-source.tar.gz --expected-pin 7c86d0cfa2f5714bbaae7031a1526a08ed61a48068f3210258d4002d33aa2e6f --json
```

`$R2Target` 须设为仓库外独立目录。`$PythonTemp` 与 `$CargoHome` 须设为报告中的独立临时目录与 Cargo 缓存配置，避免用户祖先 `.cargo` 污染夹具；不修改或删除用户配置。

以下为原Linux R2合同与封存的历史说明，Windows新候选资格以上述C11报告为准。

2026-10-05：原审计的六项基础缺口已在独立 **version 1／revision 2** 修补，
完成当前基础范围的统一执行与独立复核。冻结范围为 `session` 和
`safe-exec-basic`；扩展执行继续延期，完整 SDK26、生产执行接线和平台资格仍 OPEN。
详见[修补与整体核查](../reports/reconstruction-2026-10-05/session-exec-v1-r2-repair.md)
和[结构化验收](../reports/reconstruction-2026-10-05/session-exec-v1-r2-validation.json)。

## 当前接口

| 接口 | 冻结行为 |
| --- | --- |
| Create／List／OpenWriter／Append | 有限集合、原活 admission、writer epoch／tail CAS；完整原请求幂等和历史事件身份防重放；精确封存尾分支保留父检查点 |
| Snapshot／Checkpoint／Archive／compact | 有界连续窗口、明确 gap、拒绝超尾成功回复；同尾检查点不可换正文；满额仍能恢复 writer、封存和归档；compact 保留身份及回执 |
| 宿主 retire_session | 独立 Retire 准入、精确 revision／完整记录 SHA、writer 失活及工具安全释放；删除与墓碑同事务提交 |
| Propose／review_tool／approve／Claim／execute_claimed | 固定输入和执行域、完整提案独立批准、单次持久 Claim；回调前落盘 invocation_started，Unknown 不重放 |
| Report／Inspect／reconcile_tool_observation | 原 Report 事实及重试回执保留；可信宿主以完整原调用身份和观察 CAS 单调递进退出／EOF，核对 Unknown 晚到事实，不重启执行 |
| revoke／revoke_tool／retire_tool | 失效 proposer 不阻止宿主终结未消费操作；已消费 Unknown 保留真实历史；已开始执行须可信退出及输出关闭后退休 |

每会话仍限 256 个事件身份、128 条回执及 2 MiB 状态。普通请求最多使用
125 条回执，预付 80 KiB 字节并保留 3 条一次性 writer 恢复／封存／归档回执。
使用控制预留后关闭普通写入，可从封存尾继续新会话。compact 不擦除身份和重试。
`ToolInfo.facts` 保留原 Report，`observation`／`observation_revision` 显示最新事实。

## 生命周期及实际消费

Core 单库 v25 ledger 与 outbox 共享字节预算，总行数仍限 128。R2 使用一条
固定 64 KiB metadata 行，每代至多 127 个 session／tool 身份，包括退休墓碑。
退休释放业务行，同代身份不可复用；全部身份退休且两业务 domain 为空才可换代，
须新宿主和新批准。已预付的等量／缩小收尾不因后来下调预算而阻断，增长仍拒绝。

宿主绑定原 Store 的独占 ledger owner，无关 Configuration 撤销不使其失效；
全局撤权、Store 关闭和可信接管废止旧 owner。通用 CAS 不能绕过活 owner。
启动核验 metadata／业务行双向闭包、当前 revision／完整 SHA 及 generation。
失效准入回收名额时永久撤销旧 Arc／nonce。

[专用宿主包](../extensions/agent-session-exec-v1-r2-host/README.md)提供完整 archive
SHA 审阅、声明 ceiling／批准子集、Wasm import 与有界 native frame 路由，使用
原 Runtime／Connection／Admission。旧 factory 拒绝新 import。Linux 固定执行器
核验主 ELF、密封 memfd、opened cwd、固定环境和 stdin，限制输出、核对退出／EOF，
并在 SDK 权限锁外回收进程。

[真实 Codex 适配器](../companions/morrow-codex/session-exec-r2/README.md)实现实际上游
ThreadStore，跨进程使用真实 Core，覆盖 durable Append、恢复、分页、flush、
可逆逻辑 archive、撤权及物理 delete。ExecBackend 实际 trait 与拒绝路径已验证；
真实生产进程／事件提供者和完整应用注入仍 OPEN。

客户端以 `default-features=false` 消费 codec，避免 Codex SQLx 与 Core SQLite
链接冲突；host feature 默认启用。两者直接从唯一原 Core schema 派生身份，
导出包保留真实相对路径。请求与回复绑定非零 generation 和完整原请求 SHA。

## 冻结与验收

R2 schema 为 `extensions/agent-session-exec-v1-r2/contracts/session_exec.capnp`，
raw SHA256：`f905eaed5107bebc9612a9dede757fa54ce674f13120adce31ed331cf7557685`。
R1／R2 帧不互认。

[新基线](../reports/reconstruction-2026-10-05/session-exec-v1-r2-freeze.json)绑定
573 个声明源码／依赖和消费者参考输入、锁文件、工具二进制身份及实际日志。
独立审阅 pin：`7c86d0cfa2f5714bbaae7031a1526a08ed61a48068f3210258d4002d33aa2e6f`。
[可移地核验源码包](../reports/reconstruction-2026-10-05/session-exec-v1-r2-source.tar.gz)
含 617 个普通文件成员，SHA256：
`59a1a276a6e432a9bfa78c42e2b7bbe81f21e236579de5890fa96ce0593fbb8b`。

最终 Linux 实测 Rust 156 项（新 SDK 107、Core 22、宿主 11、原运行时 16）及
门禁 Python 32 项，失败／忽略／过滤均为 0；真实 Codex 消费 2 项另计。仓库外
导出源码的独立客户端及默认宿主库实际锁定离线编译通过，573 个输入前后一致。补充客户端
codec 47、WASM 编译、严格新 SDK／host Clippy、相关格式、旧内容层 60 回归通过。
提交故障组 12 PASS、1 个 ignored child harness，parent 实际执行两次并断言 exit86，
单独记录，不与正式计数相加。

全新检出可直接只读核验随源码包封存的记录，从仓库根运行，无需本地 `build/` 日志：

```sh
python3 tool/verify_session_exec_r2.py verify-archive \
  --baseline reports/reconstruction-2026-10-05/session-exec-v1-r2-freeze.json \
  --source-archive reports/reconstruction-2026-10-05/session-exec-v1-r2-source.tar.gz \
  --expected-pin 7c86d0cfa2f5714bbaae7031a1526a08ed61a48068f3210258d4002d33aa2e6f \
  --json
```

门禁核验归档字节、记录和明确 pin，不重执行测试、不认证审阅者，也不检查当前工具二进制。
仍保留原执行日志的工作区可使用 `verify --recorded-executables-only` 检查 live 源码和记录。
生产 transport、OS sandbox 和完整 SDK26 资格均为 false。

## 历史与延期

[R1 审计](../reports/reconstruction-2026-10-05/session-exec-v1-audit.md)、原基线／pin
和 R1 crate 保持。原 237 输入和 81 项记录封存在
[R1 源码归档](../reports/reconstruction-2026-10-05/session-exec-v1-r1-source.tar.gz)，
SHA256 为 `c8ca0b529f3cc33177b36a7a3c396f42168a4303532f0a93ce28fd6aa3970d0c`。
当前 live Core 为 R2 演进，R1 身份须对历史归档核验。旧 SDK 冻结 57 原件与既有
锁文件保持；不以历史 Core 817 分轮覆盖代替当前全套绿色结果。

PTY、交互 stdin、输出流、resize、signal、terminate 按要求延期。认证生产
transport、GUI、Windows owner／ProtectedSession、C／C++ 和其他平台未验收。
同步 spawn 不可抢占，主 ELF SHA 不覆盖动态加载器／共享库，进程组控制不保证
任意后代隔离。既有依赖警告保留，不声称整库 Clippy 通过；源码提交身份以
开发分支历史为准，历史验证报告中的工作区状态按记录时点理解，未创建 Release。
