# 会话与安全执行 R2 修补及整体核查

日期：2026-10-05。开发线 `codex/windows-sdk-convergence-20261005`，基准提交 `679e16a6a44f8aac13f85da05204d9cd8afb30b8`。本轮修改仍在工作区，没有提交、推送或发布。

原审计确认的六项基础缺口已在独立 version 1／revision 2 修补。范围仍为 `session` 和 `safe-exec-basic`；扩展执行按要求延期。最终正式Rust156、Python32、实际Codex消费2与移地客户端离线编译通过；573个输入前后一致，新pin及617成员归档经独立核验。

| 原缺口 | 本轮修补 |
| --- | --- |
| 会话普通写入耗尽收尾／恢复容量 | 125 条普通回执之外保留 3 条一次性控制回执，并预付 80 KiB 收尾字节；满额仍可恢复 writer、封存和归档。保留完整原请求重试和历史事件身份；使用控制预留后关闭普通写入，需从封存尾继续新会话。 |
| Report 后无法递进退出与输出关闭事实 | 新增可信宿主 `reconcile_tool_observation`，完整身份与观察版本 CAS，单调更新最新事实；原 Report 事实和重试回执保持。 |
| Unknown 没有晚到事实入口 | 当前宿主可以核对原操作的可信晚到观察；Unknown 不转换成新许可，不重领 Claim，不再次调用执行器。 |
| 撤销／到期准入耗尽累计名额 | 回收失效 registry 项并永久撤销旧 Arc／nonce；名额回收不恢复原权限。 |
| 提议者离开后不能终结未消费操作 | 宿主独立核验原完整记录后可 revoke／retire；消费后的 Unknown 仍保留真实历史状态。 |
| SDK 接受超尾成功 Snapshot | typed 构造与 wire 解码共用严格 `after <= tail` 校验；请求、generation 和完整原始帧摘要相关性保持。 |

补齐了可信物理退休和代际防重放。业务删除与 metadata 墓碑在原 Core SQLite 库同事务提交，严格核对原 row revision／完整 container SHA。session 退休还要求原 writer 失活、关联工具已安全释放；started Unknown 必须有可信退出和输出 EOF 才能退休。未解决的副作用不能靠清理或换代丢弃。

Core 总账本上限仍是 128 行，与原 outbox 共享字节预算。R2 占用一条固定 64 KiB metadata 行，每代最多 127 个会话／工具身份，已退休身份仍计入。所有身份已退休且两业务 domain 均为空才可换代；旧 generation、owner 和 admission 永久失效，必须新宿主及新批准。预付的等量／缩小收尾事务可在重新打开后额度变小时完成，新增与增长仍拒绝。

## 整体核查中追加修复

独立复审在初次实现后又发现并修复了以下边界：

- 活 owner 下，旧通用 ledger CAS 可能改写受管记录。现在通用 CAS 与 owner batch 分别持有提交 guard，活 owner 时通用写入拒绝；再次可信接管原 Store 会原子废止旧 owner。
- metadata 与业务行可能漂移或回滚。宿主启动检查单例 metadata、active／retired 身份与两业务 domain 的双向闭包、当前 row revision／完整 SHA，以及所有留存请求的 generation；每次业务写入同步更新 metadata。
- leader 退出后过早 reap 可能使后续 group kill 使用被复用的 PID。固定执行器保持 leader 至两路 EOF，超时清理使用原进程组；Drop 交给全局最多 8 槽的异步回收，不在 SDK 权限锁内阻塞 wait。
- 实际 Codex 适配器初始化／封存失败后可能留存 writer、pending metadata 可覆盖，且多个合法大事件聚合会超过帧上限。现在失败路径撤销原 writer，拒绝空／重复 pending，提前校验 checkpoint 容量，Snapshot 每页一个事件。

## 消费与分发

新增专用 reviewed package、Wasm import 与长度定界 native 路由，绑定原 Runtime／Connection／Admission。完整 archive SHA 审阅、声明 ceiling、批准子集、有限 session 和固定执行域均由可信宿主决定。旧 factory 拒绝新 import，新 factory 拒绝混合／额外 import。旧 native 协商和能力位没有扩展。

真实上游 `ThreadStore` trait 已通过独立宿主进程消费原 Core／SQLite：覆盖 durable Append、persist／flush、恢复、大历史分页、显式 metadata 清除、可逆逻辑 archive、shutdown／discard 撤权和物理 delete，以及注入初始化／封存失败后的精确 Denied。没有用不可逆 SDK Archive 冒充上游逻辑 archive。

实际 `ExecBackend` trait 编译及启动前拒绝路径已验证；真实生产 `StartedExecProcess`／事件提供者与完整 Codex 注入仍 OPEN。该负向测试不证明生产进程提供者已接通。

客户端使用 `default-features=false`，避免真实 Codex SQLx 与 Core SQLite links 冲突；默认 host feature 仍链接原 Core。schema identity 直接从原 Core schema 派生，独立导出保留相对路径，不增加第二份 schema。新门禁绑定完整声明源码闭包、锁文件、实际工具二进制、命令、固定测试名单及日志摘要，并要求仓库外独立 consumer 实际离线锁定编译。

## 验收与身份

最终统一执行覆盖下列固定方法，正式组均为0 failed／ignored／filtered：

| 正式范围 | PASS |
| --- | ---: |
| 新SDK codec／session／safe_exec／authority | 48／24／29／6，合107 |
| Core原ledger／新owner和batch | 11／11，合22 |
| reviewed package路由／固定执行器 | 4／7，合11 |
| 原runtime bounds／task_bounds／task_failures | 11／2／3，合16 |
| 正式Rust合计 | 156 |
| Python冻结门禁完整方法 | 32 |
| 实际Codex ThreadStore／Exec否决边界，外部资格另计 | 1／1 |

移地导出后的独立consumer实际 `cargo check --locked --offline` 通过，默认宿主库也从同一导出闭包实际锁定离线编译通过；两次编译均核对来源不变。
源码before／after／manifest三者一致，共573输入。正式门禁核验19条记录；另两条准备命令（真实宿主fixture构建及consumer锁文件准备）使实际pipeline命令总数为21。补充客户端codec47、WASM客户端编译、严格新SDK／host Clippy、相关格式、diff检查及旧内容层60回归分别通过。Core提交故障组12 PASS／1 ignored helper，parent实际启动helper两次并断言退出86；它含已正式通过的11项，不与156简单相加。

- [新基线](session-exec-v1-r2-freeze.json)，独立审阅pin `7c86d0cfa2f5714bbaae7031a1526a08ed61a48068f3210258d4002d33aa2e6f`。
- R2 raw schema SHA256 `f905eaed5107bebc9612a9dede757fa54ce674f13120adce31ed331cf7557685`。
- [可移地核验源码包](session-exec-v1-r2-source.tar.gz)，617成员，SHA256 `59a1a276a6e432a9bfa78c42e2b7bbe81f21e236579de5890fa96ce0593fbb8b`。
- [结构化验收](session-exec-v1-r2-validation.json)索引正式执行、补充回归、独立pin／归档复核和原始失败。

最终独立复核实际CLI verify及verify-archive均exit0，归档移地核查通过，没有发现本轮基础范围内新增待修项。门禁只核验记录，不重执行Cargo、不认证审阅者；实际工具身份只绑定版本命令的二进制，不声称封存整套工具链／动态依赖。额外Codex记录明确为external qualification，未封存整个上游构建图。此前8331输入／六命令资格是独立历史附件，不冒充最后全部上游源码冻结。


旧 SDK 的 57 个冻结原件重新核验；既有 Core／runtime／SDK 锁文件保持。R1 crate 的 12 个输入不变，原 237 输入和 81 项测试记录封存在 `session-exec-v1-r1-source.tar.gz`，SHA256 `c8ca0b529f3cc33177b36a7a3c396f42168a4303532f0a93ce28fd6aa3970d0c`。当前 live Core 为 R2 演进，历史 R1 身份须对归档核验，不能用当前 live Core 替代。

原始编译／夹具／格式失败日志保留。最终门禁首轮继承真实RUSTC／CC而使用假工具夹具，32项setup错误；仅隔离夹具环境后32项全通过，生产门禁未放宽。原运行时新路径有一个多余mut诊断，清理后当前来源漂移导致中间pipeline正确拒绝建立新基线；最终按最新字节统一执行通过，旧PASS日志保留。整体核查还发现不同 workspace／feature 共用 target 会覆盖 Core 的无 hash rlib／cdylib，导致故障注入重跑使用默认产物；改用隔离 target 后复验，不修改源码或放宽 crash 断言。故障组中的 ignored 方法是由 parent 实际启动的 child harness，另行记录，不混入正式零忽略计数。

## 明确保留的范围

PTY、交互 stdin、输出流、resize、signal、terminate 延期。认证生产 transport、Workbench／GUI 批准链、Windows owner／ProtectedSession、C／C++ 与其他平台未验收，完整 SDK26 仍 OPEN。

Linux 固定执行器实际验证主 ELF SHA、密封 memfd、opened cwd、固定环境和输入、有界输出与运行时限。同步 spawn 本身不能被 deadline 抢占；动态加载器／共享库不在主 ELF SHA 闭包内，逃离进程组的后代不受完整进程树控制保证。没有 OS sandbox 资格。

新冻结只绑定本地基础接口和已记录证据。门禁不重执行 Cargo，不认证审阅者，也不把额外 Codex 消费附件提升为整个 upstream build graph 的源码冻结。
