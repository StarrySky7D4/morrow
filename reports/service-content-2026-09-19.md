# 受管服务的内容权限交集验证

日期：2026-09-19。隔离分支 `codex/io-safety-refactor`，前置提交 `8a4f56553cf0521b090e0c3f1544b40efcb665cf`。结论：**PASS_SCOPED**，可信原生宿主显式配置的内容服务已通过 Windows 本机限定验证；完整插件系统、持久服务产品界面与新版 SDK 仍待完成。

## 本轮进展

- 在同一个 HostRuntime／Store／Manager 实例上接通实际内容操作。新增只读 ContentAuthorization probe，绑定原对象 grant、连接、到期和单调时钟；原 grant 替换／撤销／停止／释放不可恢复旧 probe。
- ServiceContentPolicy 精确限定服务最大内容范围，真实认证 principal 获得其子集。范围至多128项，按操作种类、卡片和指定附件匹配；scope 摘要只描述请求语义，不颁发权限。
- content_route 通过真实 Principal.id 查映射，客户端提供的保留范围头全部剥离并由宿主重新生成。有效范围参与持久请求完整匹配，收窄或改变权限后不能重放旧缓存；普通 submit API 拒绝带范围字段的内容请求。
- 内容型持久服务才开放 core exchange，支持原7类命令并保留原逐对象权限和事务检查。内容／IO 共用一个串行回调、原 HostRuntime、job、调用数、字节和期限；内容提交前预扣完整64KiB响应上界，额度不回退。
- 准入、Store 授权、Ready/read、历史结果读取及 HTTP 最终返回复验当前范围；同一内容型服务的外发 Broker 和 HTTP 监控也携带权限约束。
- 新的 dispatch_guarded 在成功编码后再次检查原权限和额外约束。旧 dispatch 保持原采时语义。runtime 消息使用有界自有对齐分段，允许嵌在任意字节偏移的网络载荷中。

## 发现与修正

三名子代理分别负责核心事务边界、内容策略与只读审查、网络路由与真实读写测试；主代理完成执行器与外发检查接线、作业回归并统一运行测试。

1. 旧 dispatch 一并增加最终采时导致现有“提交已被独立观察后停止”测试的 host_calls 从2变1。保留旧入口时序，新 guarded 入口才增加最终检查；原测试没有放宽，另新增7类命令的原时序断言，均通过。
2. 真实 HTTP 的内容响应包含4字节长度前缀，原 flat-slice Cap’n Proto 解码在非8字节对齐地址失败。独立 offset0..7 测试复现后，改用有界 OwnedSegments；消息、尾随、截断和声明长度限制保留。独立复核还核对本地 capnp 0.24.1 在正文分配前限制段数、累计长度与遍历上限。
3. 测试最初调用不可 Clone 的 service Request，改为从原帧有界重建；不改原请求类型合同。静态检查发现重复闭包、复杂回调声明和不必要测试 Vec，均已修正。
4. runtime 新增直接使用现有固定 sha2 0.10.9，离线更新 network_node／workbench_host 锁文件，各仅增加一条依赖关系，没有升级依赖版本。新增核心方法按现有原生／web-storage 边界编译，默认 Web 没有新增警告。

两轮独立只读复核未发现新增明确 P1/P2；这不是独立重跑测试或完整安全审计。

## 实际结果

Windows，release／locked／offline；真实网络只使用本机临时端口、合成 token、临时数据库。测试总数取最后完整运行，不与专项累加。

| 检查 | 结果 | 日志 |
| --- | --- | --- |
| core 全量 all-features | 510通过，0失败，3个崩溃子进程入口 ignored | `build/service-content-core-final.log` |
| plugin_runtime 全量 all-features | 370通过，0失败，2个崩溃子进程入口 ignored | `build/service-content-runtime-clean-final.log` |
| network_node 全量 plugin-adapter | 78通过，0失败 | `build/service-content-network_node-final.log` |
| workbench_host 全量 all-features＋本轮实际Rust guest | 116个顶层测试入口通过，0失败 | `build/service-content-host-full.log` |
| 核心／运行时／网络全目标严格 Clippy | 通过，`-D warnings` | `build/service-content-{core,plugin_runtime,network_node}-clippy-pass.log` |
| core 默认 wasm32 库编译 | 通过；7条既有 read_archive/read_capture dead_code 警告 | `build/service-content-core-wasm-final.log` |
| runtime 默认 wasm32 库编译 | 通过，仅为编译证据 | `build/service-content-runtime-wasm.log` |
| 冻结 SDK 原件完整性 | 36固定文件、13原 Wasm/包对通过，未重封装 | `build/service-content-sdk-baseline.log` |
| 冻结原包执行 | 包含在 runtime 全量 | `build/service-content-runtime-clean-final.log` |
| 本轮16个 Rust 文件格式 | 通过 | `build/service-content-format.log` |

新增31项有效测试：核心11（guarded9＋对齐2），运行时14（策略11＋作业3），网络6（内容5＋普通路线剥离保留头1）。core/runtime 的5个 ignored 入口由父测试实际启动。宿主116个顶层入口包含3个既有无环境参数时直接返回的子进程入口；另113项为有效顶层测试，父测试中的子进程结果不重复计数。没有将空跑入口增加为业务完成证据。

主应用宿主所用 Rust guest 来自本轮 `plugins/workbench` 实际编译，构建日志 `build/service-content-workbench-guest.log`；SHA-256：`a0eaadc910c086fb46f46125e9c0e720bf9d29e32393651bd44f2653f4ab67ab`。现有目录升级／冻结原包继续按原字节验证，未重封装历史包。默认 wasm32 库编译不表示浏览器或其他平台已可运行该原生服务。

真实网络5项验证：从构造 guest 后才写入的数据库读取二进制／中文正文；实际认证但无映射或越卡片范围拒绝；相同包、namespace、key 在主体范围收窄后以1 fuel返回409且不泄露旧正文；真实 Rename 回执与重开后的标题／修订一致，即时和重启重试返回原回执且事件数量不增加；原插件有 Rename 但主体仅能读时拒绝提交，原标题／修订／正文不变。

运行时验证 Ready 后主体撤权／策略撤权／原对象到期都清除结果，Observed 事实仍保留；router 内撤权后实际脚本后端不执行；无内容授权入口不能读取已保存内容响应。脚本后端不是实际远端服务；本轮未做真实第三方副作用或公网验证。

## 保留的边界与后续

- 这些是宿主显式、当前实例内的授权和服务范围，不是持久用户账号／权限管理产品。主体与配置持久化、稳定 namespace 管理、证书与凭据生命周期、UI及恢复查询仍待接入。
- 内容的 operationId 和事务记录仍是提交事实；服务原件、范围摘要和回执不构成完整入站→子调用→内容提交因果审计。写入已经提交后撤权不会回滚，Unknown不会自动重发。
- 范围变化导致旧 key 冲突是有意的保守行为；scope digest 不含临时内存身份或到期时间，重启使用相同范围仍须真实重新批准。
- 全部7类命令在核心与范围层得到验证；本机 HTTP 实际端到端证据为读内容和改名，未将该范围扩大为所有内容命令、所有平台或所有三方 API 的产品验收。
- 下一项推进持久服务配置与只读恢复查询／核对；文件选择、枚举与受控变更可沿原 Broker 边界独立推进，随后补三语言 SDK、主应用授权与恢复界面。

合同：[内容权限交集](../docs/PLUGIN_SERVICE_CONTENT.md)；任务状态：[编码看板](../docs/DEVELOPMENT_BOARD.md)。应用版本保持 `0.1.9-test.52+56`。本轮没有推送、主线合并、Release、安装包或用户数据迁移；仅保存隔离分支的可评审实现。
