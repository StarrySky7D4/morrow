# 托管 IO 子调用与持久执行接线

日期：2026-09-19。结论：**PASS_SCOPED**。基线：`0470addf5617bb7b128d81fa8a3ddcc77fc58ca8`，隔离分支 `codex/io-safety-refactor`。承接 [托管准入](managed-io-jobs-2026-09-19.md)；本轮没有完整插件网络或文件功能的产品验收。

## 实现与取舍

- 新增 `submit_brokered`、可信宿主 `BrokerRouter` 和私有身份的 `RouteContext`。仅托管执行器可用，真实 Host、ManagedInstance、原请求、取消令牌与共享作业租约不能由路由替换。独立 Broker 与既有低层 Router 入口继续保留。
- `IoCallLease` 绑定实际包摘要、协议摘要、原请求摘要/长度、operationId 与能力交集。凭据引用还须 CredentialUse。它持有原作业租约，只增一个 resource 与完整响应帧额度，不再占用第二个 job 或重复扣请求字节。
- 发送前同时检查 worker、job 与实例累计额度，按 `input + 每次 request + 每次获准 response_limit` 计费。实际响应不再加计，剩余额度不退款。当前预留早于读取持久终态，已获准预留后才发现重复/冲突同样不退款。
- 实际顺序为：校验/预留 → Prepared 与受保护请求材料/收尾容量 → OutcomeUnknown 发送边界 → 回调最多一次 → 完整 HTTP 响应验证 → 响应材料与 Observed → 最新授权与原响应一致性检查。网络回调期间不持作业、时钟或 Broker 注册表锁。
- 原 Unknown/Observed 拒绝再次发送；Unknown 不能被路由错误映射成普通拒绝。未经过 context 的伪造成功、已记录响应被替换、成功后改报拒绝均不交付。执行中撤权保留真实 Observed 原件，但撤销成功载荷。
- 公有 Broker 的最终时钟回调在注册表锁外采样，随后复核条目/身份，保留时钟回调可退休该操作的语义。没有将这项修正泛化为任意回调重入支持：托管 worker 时钟仍须是有界、纯单调、非重入的取时器，不能调用 worker/handle 方法，契约已写入代码和文档。

## 验证

运行环境为 Windows；release、all-features、locked、offline。所有数据使用合成临时库，真实外部后端为明确标记的脚本回调。

| 项目 | 实际结果 | 日志 |
| --- | --- | --- |
| 运行时全量 | **328通过、0失败、1个由父测试调用的子进程入口ignored** | `build/brokered-io-runtime-full.log` |
| 全目标严格 Clippy | 通过，`-D warnings` | `build/brokered-io-clippy.log` |
| 默认 wasm32 库编译 | 通过，仅编译边界 | `build/brokered-io-wasm.log` |
| 冻结 SDK 完整性 | 36固定文件、13组原Wasm/包对通过，未重编译/重封装 | `build/brokered-io-sdk-baseline.log` |
| 冻结 SDK 原包执行 | 9项基础+3项依赖通过，包含在全量内 | `build/brokered-io-runtime-full.log` |

新增11项 `io_jobs_brokered` 集成测试，使用真实 Wasm/Registry/Manager/Store，覆盖：仅一个job/resource槽下成功；精确预留及不重复计费；重复Observed不重发；六种命令身份不匹配；两级额度差1时发送前拒绝；后端报错/真实响应超限的Unknown；回调阻塞期间撤权；伪造或替换响应；成功后改报拒绝；同一context重复dispatch；既有Unknown不得降级为Denied。另新增公有Broker最终clock退休操作的死锁回归。新增12项包含在328项内，不重复相加。

初次新集成测试7项均在fixture准入失败：manifest max_jobs=1，测试却传入capacity=2。修正为1后相关专项56项通过；补入第11项恢复分类后全量328项通过。初始日志 `build/brokered-io-scoped.log` 与专项 `build/brokered-io-scoped-fixed.log` 保留。未将fixture问题记为已发生的产品漏洞，也未删除失败证据。

两名代理独立复核计费、租约生命周期、命令绑定、发送边界与错误分类；root修复反馈后重新执行全量。没有把只读审查当作运行验证。

## 后续编码与边界

1. 下一项 IO-D1：把真实 origin、HTTP 方法、重定向、凭据引用与响应/期限限制绑定到可撤销的资源批准，再接现有 network_node。当前 command 的 approval_sha256/target_sha256 是可信路由提供的证据字段，不等于资源批准。需覆盖有副作用方法、远端已执行但响应丢失、取消与核对，不能用一次 GET 结项。
2. IO-D2 保留在首批双向网络目标：guest 发布 API 路由、远端主体与插件权限交集、认证/冲突/限流/撤权/节点停止。IO-D3 继续推进受控文件选择、枚举、创建、替换、删除及崩溃核对。
3. 接入产品后，还须独立验证宿主重启恢复、真实资料副本、主应用授权与恢复展示、三语言 SDK 扩展、录制隔离重放和平台资格。现有冻结 SDK 通过不代表新的 IO 扩展已稳定。
4. 本轮核心 Store/schema/冻结SDK/应用版本未改，没有迁移用户资料，未构建安装包或启动真实网络服务。此前核心468项、宿主116项属于前一批证据，本轮没有重跑或重新计入。更新仅保存至隔离分支，未合并开发线、未推送、未发布。
