# 持久入站认证与发布批准验证

日期：2026-09-20。隔离分支 `codex/io-safety-refactor`，前置提交 `fb94051`。结论：**PASS_SCOPED**。应用维持 `0.1.9-test.52+56`；本轮是原生宿主底座，未发布安装包，未修改用户实际资料库。

## 完成范围

- 原 Store v19 保存严格 Protobuf＋LZ4 认证验证摘要、精确发布批准和稳定数据库身份。认证摘要不能用于向外部 API 发送明文凭据。CAS、身份不复用、最多512条记录、32KiB单条上限和原共享容量均已接入。
- `ResolvedService` 在原数据库持有授权协调锁，核对配置摘要、全部引用、时间和禁用状态；签发阶段仍须通过原 Manager／IoBinding、实际包和内容授权。没有把持久化数据当成活动权限。
- 原生 `bind_configured` 使用批准中的地址、TLS模式、方法、业务及查询路由，真实 HTTP/TLS 已连接 guest 和原请求历史。原 Store 销毁、修改、记录过期及运行中时钟回退均使旧授权失效；克隆不续期。
- 原 worker 接受有界管理修改，立即撤销旧服务，随后串行执行原 Store CAS；回执明确区分接受和提交。Ready/read 与网络迟到交付均复验，无外部效果重发。
- 数据库副本、独占模式和显式原生 VFS 共用按持久数据库身份选择的用户级协调锁。未知原生存储拒绝绕过；Web／内存仅保留存储能力，不恢复活动原生发布权限。

## 审查修正

独立代理发现一个实际绕过：最初使用快照可用性判断是否启用原生授权锁，而独占模式会关闭快照，导致其配置写入可能不参与授权协调。已将识别改为实际 SQLite 主文件，覆盖独占和显式原生 VFS；用相同身份数据库副本排除 SQLite 偶然竞争的影响，回归证明配置和批准写入仍被拒绝。

直接 Store 调用的前置 CAS 冲突不撤销旧租约；通过 CAS 后的容量失败仍撤销租约，即使原数据回滚。原 worker 入队接受即撤销，因此后续 CAS 失败也保持撤销。对应差异有测试和合同说明。

使用用户指定的 SubagentBridge `deepseek-flash / max` 进行必要代码片段审查，任务 `task_18dc9c2b339c9b7ea8d8b247` 返回 SUCCEEDED／COMPLETE。模型指出“expires可能是秒而now来自另一时钟”的疑点，但本契约的 created／expires／now 明确全部为 UTC 毫秒，并有绝对期限与回退测试，该假设不成立，未据此改动代码。此次调用不记为独立安全审计通过，也未发送凭据或无关源码。

## 验证

Windows本机、合成主体和临时数据库。只累计每个顶层测试套件最终结果，不重复累计子进程输出或前期专项。

| 检查 | 结果 | 日志 |
| --- | --- | --- |
| core完整，fault-injection | 543通过，0失败，6 ignored子入口 | `build/service-authority-core-full.log` |
| runtime完整，all-features | 389通过，0失败，2 ignored子入口 | `build/service-authority-runtime-full.log` |
| network_node完整，plugin-adapter | 95通过，0失败 | `build/service-authority-network-full.log` |
| workbench_host完整Release，all-features／locked／offline | 116顶层入口通过，0失败 | `build/service-authority-host-full.log` |
| core/runtime/network全目标严格Clippy | 全通过，`-D warnings` | `build/service-authority-{core,runtime,network}-clippy.log` |
| core默认wasm32库检查 | 通过，7条既有dead_code警告 | `build/service-authority-core-wasm.log` |
| core含web-storage的wasm32库检查 | 通过 | `build/service-authority-core-web-storage.log` |
| runtime默认wasm32库检查 | 通过 | `build/service-authority-runtime-wasm.log` |
| 冻结SDK原件 | 36固定文件／13原Wasm与包对通过 | `build/service-authority-sdk.log` |
| 冻结原包执行 | 已包含runtime完整测试 | 同runtime完整日志 |
| 新增／实质修改Rust格式 | 19文件通过；旧迁移夹具只作必要调整 | `build/service-authority-format.log` |

宿主116个入口含3个既有无参数时直接返回的子入口，其余113项有效顶层测试；8个ignored是父测试使用的崩溃／跨进程竞争入口，不是被遗漏的功能验收。相对前置提交新增36项有效测试：核心22、运行时5、网络9。

新增真实HTTP测试覆盖批准路径与 bearer、精确 TLS 模式、错误原实例、重启后低 fuel 查询且路由器调用为零、认证／配置更新与到期撤销；等待实际持久发送边界后撤销，证明未交付旧 guest 响应。存储测试覆盖旧版迁移、前后提交崩溃、共享容量回滚、跨进程授权锁与原拥有者销毁。

宿主采用未修改的既有 Rust guest，SHA-256 `a0eaadc910c086fb46f46125e9c0e720bf9d29e32393651bd44f2653f4ab67ab`；没有用新构建 UI 或安装包替代底座证据。wasm检查是编译证据，未运行浏览器OPFS或Linux／Android／macOS服务验收。

## 仍待推进

主应用授权／发布界面、出站API受保护凭据和持久端点批准、真实账号与TLS运维、配置审计链、跨进程可信时间、Unknown核对、完整因果链与证据退休。当前最早主体到期会保守关闭整个发布，配置变更后需要重新解析与签发，没有热替换。完整文件系统接口、三语言SDK和全平台资格保持原范围，不能因此标为完成。

合同：[持久服务授权](../docs/PLUGIN_SERVICE_AUTHORITY.md)。任务入口：[开发看板](../docs/DEVELOPMENT_BOARD.md)。本轮仅本地开发、验证与提交，没有推送或发布Release；远端仍是此前已授权推送的前置开发线。
