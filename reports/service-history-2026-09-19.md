# 入站持久请求与唯一执行边界验证

日期：2026-09-19。隔离分支：`codex/io-safety-refactor`；前置提交：`c73016efa3eee02f5289f50ef832198d686b44aa`。结论：**PASS_SCOPED**，Windows 本机原生受管服务的请求持久化、唯一认领及响应恢复子集通过。继续执行选择性修补／重构，不直接接受未经验证的远端实现；整体插件底座、完整 SDK 和主应用接入仍未完成。

## 实现与决策

- 沿用原 HostRuntime 的 Store v17、IO 历史、材料容量、Manager／IoBinding／IoWorker，未另建存储、批准或业务提交路径。提取共享事务内部操作，既有独立 API 保留原语义。
- 新增严格 Protobuf＋LZ4 RequestRecord，保存原服务 Cap’n Proto 请求、固定 namespace、key 摘要与初始期限。没有数据库版本变化；新增实验性记录格式不代表旧消费者已取得读取资格。
- 一个 SQLite Immediate 事务原子提交 Prepared、后续事件与材料容量及请求原件；错误、撤权、容量不足、panic 或提交前退出均回滚。新流程不会生成只有 Prepared 而没有原件的残缺请求。
- Store 增加非幂等的执行认领操作。只有新提交成功者可以进入 guest／实际后端，已存在相同记录的成功读取不能代替执行许可。入站历史与原出站 Broker 均使用该认领。
- durable_route 要求稳定的宿主 namespace 和唯一 Idempotency-Key。操作身份绑定主体、服务、namespace 与 key；修改输入、处理器、包或策略发生冲突。同一已完成请求恢复原始响应且跳过 Wasm；结果不明不重发。
- 保留期限参与原作业 deadline、guest／import／外发／HTTP监控及 Ready/read。重试不续期；完成原件仍计入原实例共享字节预算。实际主体与当前发布／监听批准始终重新检查。

## 审查修正

子代理分别完成核心记录与事务提取、运行时恢复辅助层、独立审查与 Broker 竞争修正；主代理实现运行时作业／真实网络接线，统一执行验证。最终独立代码复核未发现新增明确 P1/P2；该复核不是独立重跑测试。

1. 初版只在开始／交付时检查保留期限：已贯通实际 IO 路径，过期不得继续启动后端。
2. 旧 Broker 使用可幂等成功的历史 append 作为发送边界：改为同一 Store 中严格唯一认领，两个独立宿主不能同时获得外发资格。
3. 请求原件与 Prepared 分开提交可能留下永久残缺状态：改为原数据库事务原子准备。已有损坏记录仍拒绝执行，不能猜造原件。
4. 内部 handler 进入操作身份可能在更名后得到新执行机会：从外部操作身份中移除，以完整输入匹配拒绝变化。
5. 恢复测试重开前未释放旧 Registry，导致 StorageBusy；现明确释放原宿主资源。竞争测试的屏障移出事务授权回调，避免自锁。新增网络测试的字节比较类型错误已修正。
6. 崩溃子进程入口改为 ignored 并由父测试显式启动，避免把无参数空跑计为通过；子进程隐藏窗口，30秒有界等待并回收。

## 最终验证

Windows，release／locked／offline；真实网络只用本机临时端口、合成主体和临时数据库。数量来自本轮最终完整日志，不与专项重复累加。

| 检查 | 结果 | 日志 |
| --- | --- | --- |
| core 全量 all-features | 499通过，0失败，3个子进程入口 ignored | `build/service-history-core-full.log` |
| plugin_runtime 全量 all-features | 356通过，0失败，2个子进程入口 ignored | `build/service-history-runtime-full.log` |
| network_node 全量 plugin-adapter | 72通过，0失败 | `build/service-history-network-full.log` |
| 三 crate 全目标严格 Clippy | 全通过，`-D warnings` | `build/service-history-{core,runtime,network}-clippy.log` |
| core 默认 wasm32 库编译 | 通过；7条既有 read_archive/read_capture dead_code 警告 | `build/service-history-core-wasm.log` |
| runtime 默认 wasm32 库编译 | 通过，仅为编译证据 | `build/service-history-runtime-wasm.log` |
| 冻结 SDK 完整性 | 36固定文件、13原 Wasm/包对通过，无重新构建／封装 | `build/service-history-sdk-baseline.log` |
| 冻结原包执行 | 已包含 runtime 全量测试 | `build/service-history-runtime-full.log` |
| 本轮21个 Rust 文件格式 | 通过 | `build/service-history-format.log` |

全仓格式检查在未修改的既有 core 测试文件（如 io_evidence_crash.rs、io_intent_reservation.rs、io_intent_store.rs）报告格式差异；这些文件保持原状。本轮21个新增／修改 Rust 文件独立检查通过，未将全仓格式检查记为通过。

新增44项有效测试：核心23项（记录9、认领7、原子准备7），运行时15项（跨宿主 Broker 2、恢复辅助层10、真实受管作业3），网络6项。5个 ignored 都是由父测试调用的崩溃子进程入口，不是跳过业务回归。

核心真实进程退出覆盖原子提交前后，重开验证没有残缺新请求；另覆盖错误／panic回滚和历史一致性。运行时在认领提交后真实退出，重开保持 Unknown、原件完整、禁止第二次执行。独立 Store／独立宿主竞争验证实际脚本后端最多调用一次。实际 SQLite CommitUnknown 返回值没有单独注入；已有真实进程退出覆盖丢失提交回执，代码对提交不明拒绝执行。

网络测试实际通过 HTTP→认证→原管理器／worker→Wasm→Store：客户端收到响应首字节即断开，释放并重建宿主后以不足以执行 guest 的 fuel 读取完整已保存响应；Unknown 重启后仍不执行。另验证同 key 改输入冲突、缺失／重复／不合法 key、过期410、主体间隔离、撤权拒绝。主体 scope 仅为服务访问权限，未由这些测试证明内容 ACL。

3项受管作业测试验证：Ready 后 UTC 到期不交付正文但保留真实观察；重试无需 guest fuel 但计费完整输入与完成帧；guest 路由内推进时钟后进入 dispatch 不调用脚本后端并保持 Unknown。这3项不是实际 socket 测试，真实 socket 证据来自网络测试。

## 实际边界与下一步

- namespace 由宿主稳定保存；当前没有主应用中的持久发布配置。改 namespace／key 意味新操作，可能再次产生效果。
- UTC 高水位只在同 journal 与 clone 内共享；跨进程持久高水位和可信时间服务未实现，宿主须保证重启间 UTC 可信。连续 guest 计算依靠 fuel，deadline 是边界检查，不是实时 CPU 抢占。
- 不承诺端到端恰好一次、外部业务回滚或重放自动修复 Unknown。响应原件保存与 Observed 是两个事务，中途崩溃仍保守保持 Unknown；独立状态查询与有依据的核对留待后续。
- 历史不自动删除以恢复执行机会；合法证据退休、空间回收和整条入站→子调用→内容提交审计尚未完成。
- 下一步优先补远端主体／服务／内容权限交集及持久发布与恢复操作；受控文件选择、枚举、写入可在固定 Broker 边界下独立推进，然后完成三语言扩展 SDK 和主应用界面。
- 未验证真实第三方 API、公网部署、浏览器持久服务运行、其他平台或用户数据库；没有构建安装包、改应用版本、推送、合并主开发线或发布 Release。

合同：[持久服务历史](../docs/PLUGIN_SERVICE_HISTORY.md)。当前任务状态：[编码看板](../docs/DEVELOPMENT_BOARD.md)。本轮只形成可评审的隔离实现，不把局部通过视为完整重构完成。
