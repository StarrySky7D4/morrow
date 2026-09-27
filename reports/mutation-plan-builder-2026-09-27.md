# 宿主文件变更计划构造（2026-09-27）

基线 `b9225f64f6c62584ad7243e30249d8a088bcb155`，分支 `codex/io-safety-refactor`。延续类型化客户端，补上 UI 不再需要自行构造 Core Protobuf/LZ4 计划的宿主入口。未修改应用版本、持久化 schema 或 guest SDK 协议。

## 实现与审查

- `IoWorker::build_mutation_plan` 在原 owner 命令队列中调用 `TargetBroker::build_request_controlled`，返回规范 RequestRecord。调用者只提供操作 ID、内容长度和摘要；原选择提供包、审批、目标、相对路径、预期对象身份及操作类型。
- 保留现场租约、权限、期限、取消及交付检查。无路径重新打开，无 Store 写入，无文件效果；内容摘要只是计划声明，后续暂存仍核验实际字节。
- 私有 Submit 追加 BuildPlan=9，Result 追加 Planned=11／plan@13；所有旧编号保留。去重摘要覆盖操作 ID、长度、可选摘要；混入旧命令负载拒绝。协议及 Dart native/web 绑定由标准工具生成。
- Dart 新增 submitBuildPlan、计划回帧、严格字段及命令关联检查。操作 ID 与 Core 控制字符／分隔符限制对齐，长度／摘要在发送前检查，UInt64 保留 BigInt，结果在回帧清理前取得有界副本。每次只发送一次，不自动重试。
- 审查修正：生成草稿不能赋给代表 Prepare 尝试的 resource.request。当前可修改未准备草稿，Prepare 之后由 same_request 拒绝不同计划；Query 不会因草稿生成而显示 Prepared。

## 当前证据

| 检查 | 结果 | 日志 |
|---|---|---|
| Windows Workbench lib 全量，三份现有真实 Wasm | 165 通过，0 失败 | `build/mutation-plan-host-full.log` |
| runtime mutation_owner／opt_in／reconciliation | 17＋2＋4＝23 通过，0 失败 | `build/mutation-plan-runtime.log` |
| Dart 客户端、生成绑定、文件客户端／会话组合 | 42 通过，1 既有 FileRead 夹具跳过，0 失败 | `build/mutation-plan-dart.log` |
| 最终操作 ID 修正后客户端专项（与组合重叠） | 11 通过，0 失败，实际读取 Rust 夹具 | `build/mutation-plan-dart-final.log` |
| Flutter 缓冲清理与受控子进程传输 | 23 通过，0 失败 | `build/mutation-plan-flutter.log` |
| 最终 Dart analyze --format=json --fatal-infos | exit 0，diagnostics=[] | `build/mutation-plan-analyze-json.log` |
| 宿主 Clippy | 通过，12 条既有其他模块警告 | `build/mutation-plan-host-clippy.log` |
| runtime Clippy all-targets | 既有 collapsible_if／let_and_return 例外下通过 | `build/mutation-plan-runtime-agent-validation.log` |

Rust 协议测试实际完成 Create／Delete 的选择、草稿、Prepare、内容提交（Create）、执行及真实文件结果核验，验证草稿的规范容器与人工构造基准逐字节相同、冲突去重与非法混合字段。runtime 测试通过重开 Store 证明仅构造草稿没有留下计划记录，并覆盖取消、过期、foreign session、释放、worker stop、Prepared 冲突及已消耗目标。**worker stop 不等同于 Registry 撤销审批；本轮未新增 Registry 实时撤权的专门计划构造用例。**

新增 `rust-mutation-planned.bin` 由 Rust 回帧导出，Dart 实际解码；测试计数分项有重叠，不累加。Flutter 用受控 Python 子进程验证 Dart 通道，不作为真实 UI 文件操作资格。

首次宽泛 mutation 过滤同时命中了依赖真实 Wasm 的既有 owner-lane 测试，因未设置 MORROW_WORKBENCH_WASM 失败；补齐三份夹具环境后，全量 165 项通过。两次默认 Dart analyze 在退出时无法删除 LocalAppData/Dart/perf 下的性能文件（errno 1920）；保留原失败日志，使用 JSON 输出和任务内 DART_DATA_HOME 后正常退出且无诊断。runtime 原始严格 Clippy 仍有既有 lint，未把例外检查说成原样全通过。

## 后续

优先补持久原计划发现及当前权限下的恢复入口，然后建立与 Widget 生命周期分离的变更会话，接审批、进度和结果 UI；再验证跨进程重启／真实崩溃恢复。窗口重建或 Unknown 不得自动重放文件操作。条件替换仍在执行前明确 Unsupported，公共三语言 guest 文件变更 SDK 与全平台资格未完成，SDK 未冻结。

本轮使用原生多代理协作，由主代理审查、整合与复验；未使用 SubagentBridge。没有提交、推送、安装包或 Release。
