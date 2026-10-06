# Codex 会话与安全执行基础接口

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
