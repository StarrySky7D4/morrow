# 主应用任务所有权与恢复验证

结论：`PASS_SCOPED`，Workbench Rust 应用入口的 IO 任务与存储状态接线。应用保持 `0.1.9-test.52+56`。本轮不代表 CLI／Flutter 已有网络任务界面，也不代表插件系统或三语言 SDK 已稳定。

## 实际变更

Workbench 原直接 Storage 字段改为无 Deref 的 StorageSlot；现有内容、捕获、凭据、插件管理和 Pool 调用显式借用原存储。存储由线程持有时，操作在修改计数、UI、捕获或注册表前返回类型化 Busy。既有私有协议提前检查依赖存储的请求，导出不能越过 Busy 尝试创建目标，完成上传不消费令牌；纯目录查询、本地缓冲区管理和关闭范围继续可用。私有 schema、摘要和冻结 SDK 均未改动。

Rust 应用新增 start／status／poll／read／cancel／repair／acknowledge。一次 Workbench 同时准入一项受管任务，随机任务身份隔离不同准入与工作台。原 Manager／Registry 和 Pool 留在调用侧，IO 实例在同一 HostRuntime 独立准入；准备阶段只读借用原运行时构建受管资源适配器。Worker 移交完整审计 Session、数据库和签名身份租约，原 Store 是唯一内容与历史来源。

任务 drain 保留 Ready，真正读取继续执行原授权检查；取消请求先撤权，不冒充线程退出。只有成功 join 才归还原存储。执行器退出、断连、维护诊断分别保留；断连失败保留确切实例，维护失败仍可读内容但拦截新写入，成功修复只清除当前恢复标记，不改写历史失败或重发外部请求。finish 等待回收的方式为返回 Busy，Drop 请求停止但不阻塞线程。

## 新增真实应用路径测试

6 项测试从 Workbench::open_managed 和实际应用 start_io 进入；HTTP 场景使用真实受管 Wasm、network_node 适配器和本机 TCP 服务器，没有公网账号或外部用户资料。

1. HTTP Ready 时原 Store 仍被线程持有；直接内容方法 Busy、插件批准修订保持不变。私有 ExportFile 对新目标和已有目标均提前返回 Busy；Preferences／Paste／CapturedSave 上传令牌均保留原内容。读取上限失败不消费结果，正常读取返回 owned，实际回收后 HostBinding 不变，原意图为 Observed、审计待封存量为零，原 Pool 与三类租约仍存在。
2. 旧任务和另一 Workbench 的真实任务身份不能取消新任务。路由在真实 HTTP 已发生后暂停返回，cancel、poll、finish、ack 显示停止待退出或 Busy；释放路由后只读取到 cancelled／unknown，禁止迟到的成功载荷。
3. 准备错误及准备 panic 不丢失原 owner／锁／Pool，随后同一 Workbench 可以完成一次新的明确 HTTP 准入。
4. 无效 worker 预算触发准入失败，原 owner 与独立实例可清理；有效 worker 上的超额输入提交失败则请求停止并实际回收。两者均保留可查身份、诊断与原存储，不执行路由。此项不是操作系统线程耗尽测试。
5. Drop Workbench 在未释放路由前立即返回，活动库、数据库、签名身份租约仍拒绝竞争打开。释放路由并实际退出后重开同一活动库，原 HTTP 意图保留 Observed，服务器只收到一次请求。
6. 使用原一次性封存故障注入，真实 HTTP 返回后维护失败；新写入、下一任务与确认均被拦截。显式 repair 只重试原存储封存，清空待封存量，保留原 HostBinding、Observed 与历史维护失败，服务器请求次数保持一次。

## 验证记录

| 检查 | 结果 | 本地日志 |
| --- | --- | --- |
| 新增应用任务组合测试，Release/all-features | 6 passed / 0 failed | `build/app-io-new-tests-final.log` |
| 既有重点回归，7 个目标 | 28 passed / 0 failed | `build/app-io-host-existing.log` |
| 宿主完整 Release/all-features 回归 | 138 passed / 0 failed / 0 ignored（24 个顶层目标） | `build/app-io-host-full.log` |
| 最终库测试（仅修正测试断言写法后） | 32 passed / 0 failed | `build/app-io-lib-final.log` |
| 生产库严格 Clippy，零新增豁免 | PASS | `build/app-io-lib-clippy.log` |
| 全目标严格 Clippy | PASS，零新增豁免 | `build/app-io-host-clippy.log` |
| 冻结 SDK 原件 | 36 固定文件、13 对原 Wasm／包 PASS；未重新构建或打包 guest | `build/app-io-sdk.log` |
| 修改的 Rust 格式与差异检查 | PASS | `build/app-io-format.log`、`build/app-io-diff-check.log` |

测试统计按每个 Cargo 顶层 Running／Doc-tests 区块的最后一份结果计算，不重复统计审计子进程。首次新增 4 项检查通过后补强为 6 项。完整 138 项通过后，全目标 Clippy 指出测试中的 err().expect() 写法（`build/app-io-host-clippy-initial.log`）；改为明确匹配错误，未新增豁免、未更改生产逻辑。严格检查及最终 32 项库测试再次通过。本轮子代理发生一次连接中断，恢复后完成文件；不将它计为测试失败或通过。实现与独立静态评审使用当前可调用的子代理，未计入不可调用的 DeepSeek 辅助结果。

## 仍需完成

- 原 Store 的具体端点批准管理及系统凭据解析，连接真实用户选择；准备回调不是 UI／guest 可提交的任意资源授权。
- CLI／Flutter 的任务启动、查询、读取、取消消息与界面。已有 CLI EOF 只调用一次 finish，活动网络任务的关闭待退出协议尚需接线；当前没有新增网络任务启动 action。
- 进程重启后的原历史查询与 Unknown 核对，API 节点管理、文件系统、完整因果链与三语言 IO SDK。临时 TaskKey 不持久化，不恢复旧活动授权。
- 本轮未修改 Dart、schema 或 guest，没有重新构建 Flutter 安装包、Web、Android 或其他平台，也没有推送／发布。

实现合同见[主应用任务状态](../docs/PLUGIN_APP_IO_TASKS.md)，后续顺序见[看板](../docs/DEVELOPMENT_BOARD.md)。
