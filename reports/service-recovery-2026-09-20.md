# 持久服务配置与只读恢复查询验证

日期：2026-09-20。隔离分支 `codex/io-safety-refactor`，前置提交 `608ccc39b182ffc3c94121221233c2fee560f462`。结论：**PASS_SCOPED**。本轮属于原生宿主服务底座，不代表完整插件系统、主应用发布UI或新版SDK稳定。应用版本仍为 `0.1.9-test.52+56`。

## 实现

- 严格 Protobuf＋LZ4 配置契约进入原Store v18，持久保存稳定namespace、服务／处理器／包、保留策略、禁用修订、主体认证引用与精确内容范围、资源批准引用。CAS替换、身份不复用、行数／字节上限和共享容量已接原事务。
- 从原数据库重新读取配置，经新实际ServiceGrant校验后恢复ServiceJournal。配置不恢复旧句柄或权限，不包含明文token字段，不启动监听器。
- 原IoWorker增加只读历史查询，沿用原实例／队列／job／bytes／Store。查询不执行guest或路由器、不准备或认领，不补写或重发Unknown。保存的原件、完整输入、主体、内容范围、当前授权和TTL继续校验。
- HTTP查询只能从同宿主持久路由派生，继承原方法／业务路径／授权策略。原响应完整保留，边界头部预算不会因附加元数据而溢出。机器可判别状态以原生JobReport为准，统一外部状态信封仍待协议化。

## 修正与审查

初次核心全量检查发现四个读取／归档模块仍限制数据库版本至17，两项快照测试失败；修正为支持18并保留原版本下限、完整性与旧版伪造拒绝。旧版本测试夹具显式移除新增表后再模拟真实旧库，未弱化损坏拒绝。

宿主初次调试构建检查发现归档断言仍期待17，两项测试失败；更新为18。其后长时间审计测试仍正常耗用CPU，停止该次调试运行并用Release完整重跑，保留初次日志 `build/service-recovery-host.log`。没有把中断的运行计为通过。

网络边界审查发现额外附加查询状态头可能让恰好达到头部预算的原响应失败。Observed改为原样返回全部保存头／状态／正文，新增1024字节精确边界测试；非Observed查询状态仍使用固定状态头。

两次子代理只读复核未发现新增阻断问题，主代理统一验证。用户指定的SubagentBridge `deepseek-flash / max`已实际调用：首项较宽审查因1024输出上限截断（`task_17dd07b6bec627b76a94ad36`），未作为审查通过或代码修改依据；随后独立的配置解码预检专项（`task_16456de852ccaee4d35afeed`）实际完成，未发现具体问题。只发送本轮必要代码片段，无凭据。辅助模型审查不代替编译、测试或完整安全审计。

## 验证记录

Windows本机，临时数据库与本机HTTP/TLS，合成测试主体。最终统计不累计早期专项或子进程重复输出。

| 检查 | 结果 | 日志 |
| --- | --- | --- |
| core完整测试，fault-injection | 521通过，0失败，4个崩溃子入口ignored | `build/service-config-core-final.log` |
| runtime完整all-features，locked/offline | 384通过，0失败，2个崩溃子入口ignored | `build/service-query-runtime-all-final.log` |
| network_node完整plugin-adapter | 86通过，0失败 | `build/service-query-network-final.log` |
| workbench_host完整Release/all-features，locked/offline | 116个顶层入口通过，0失败 | `build/service-recovery-host-final.log` |
| core/runtime/network全目标严格Clippy | 通过，`-D warnings` | `build/service-recovery-{core,runtime,network}-clippy.log` |
| core默认wasm32库检查 | 通过，7条既有dead_code警告 | `build/service-recovery-core-wasm.log` |
| runtime默认wasm32库检查 | 通过 | `build/service-recovery-runtime-wasm.log` |
| 冻结SDK原件完整性 | 36固定文件、13原Wasm／包对通过，无重封装 | `build/service-recovery-sdk.log` |
| 冻结SDK原包执行 | 已包含runtime完整测试 | `build/service-query-runtime-all-final.log` |
| 本轮新增／实质修改Rust格式 | 20文件通过；迁移旧夹具仅作必要版本调整 | `build/service-recovery-format.log` |

宿主116个顶层入口包含3个既有无环境参数时直接返回的子入口；另113项为有效顶层测试，父测试启动的子进程输出不重复计入。

新增有效测试33项：核心11、运行时14、网络8。真实进程退出覆盖v17→18迁移及配置替换前／后提交；历史查询测试比对原意图字节、材料和预留容量未改变。真实HTTP覆盖Missing后正常首次执行、Prepared／Cancelled／Unknown只读、其他主体隔离、正文／查询串／范围冲突、到期／撤权、重启后fuel不足但返回原Observed、RouterFactory调用数为零、持久配置原namespace恢复和精确头部预算。

宿主使用未修改的既有真实Rust guest：SHA-256 `a0eaadc910c086fb46f46125e9c0e720bf9d29e32393651bd44f2653f4ab67ab`，不是本轮重新构建的UI或安装包。默认wasm检查仅证明相关默认库可编译，不是浏览器原生服务运行或跨平台验收。

## 后续

IO-D2b整体仍进行中：主应用须解析受保护的认证／资源批准引用、重新建立当前授权、正确实施配置禁用／切换；另需配置审计、可信时间高水位、Unknown有依据核对、入站与子操作因果链及证据退休。文件系统后端、三语言SDK和全平台支持仍在原目标中。

合同：[持久配置与恢复查询](../docs/PLUGIN_SERVICE_RECOVERY.md)。任务入口：[开发看板](../docs/DEVELOPMENT_BOARD.md)。本轮仅本地开发与验证；前置608ccc3已在远端，本轮没有推送、发布Release或操作用户实际资料库。
