# 文件变更发现断点续扫验收（2026-09-27）

本轮将同一存活内容库中的分页位置接通 Core、原 owner worker、Windows Workbench 私有协议与 Dart 恢复会话。预算或租期耗尽后，调用者可以清理并确认旧任务，再显式准入新任务接续读取。未提交、推送或发布；SDK 仍未冻结。

## 实现与边界

- Core 不透明 checkpoint 绑定同一 Store 实例、主体、包摘要和 Create／Replace／Delete 范围；Debug 隐去内部标识。只读位置不携带授权、文件句柄或数据库快照。
- runtime 对位置内存计费，新任务重新验证审批、期限与额度；失败或未领取页不交付位置。旧游标失败后不能继续使用，但此前成功位置可显式重新准入。
- 私有请求 checkpoint@8、结果 checkpoint@17 与生成绑定、hostDigest 同步更新。宿主仅在成功消费非完成页后保存映射，最多 64 份，先进先出；混合字段、零令牌、篡改、范围错配、过期和其他宿主令牌均拒绝。
- Dart 保留上一成功页及不可变位置副本。关闭与 ack 后使用新 submission 明确启动，没有自动续租、翻页、效果重放或授权恢复。
- checkpoint 只在同一宿主进程、同一存活 Store 中有效。进程重启后可以重新发现持久计划，但不能使用旧 checkpoint 续接。
- 每页独立 SQL 视图，较早排序位置的并发插入可能不在后续页中；并非稳定全库快照。每宿主共用的 512 个启动身份上限继续生效，不能宣称任意规模库都能在一个会话内完成扫描。

## 本地验证

| 检查 | 实际结果 | 日志 |
| --- | --- | --- |
| Core 发现与变更 Store | 19 通过 | build/mutation-checkpoint-core-combo.log |
| Core 进程退出故障注入 | 1 通过，1 子进程 harness 按设计忽略 | build/mutation-checkpoint-core-fault.log |
| runtime 变更、权限与独立核对组合 | 26 通过 | build/mutation-checkpoint-runtime-combo.log |
| Workbench 默认并发全量 | 171 通过，1 服务短期限测试失败；新 checkpoint 用例通过 | build/mutation-checkpoint-host-full.log |
| 服务期限失败单独复跑 | 1 通过 | build/mutation-checkpoint-service-recheck.log |
| Workbench 限制并发全量（2 threads） | 172 通过，0 失败，193.87 s | build/mutation-checkpoint-host-bounded.log |
| Dart 五文件组合，使用新 Rust mutation 回帧 | 62 通过，1 既有 file-task 夹具未配置而跳过 | 本轮子代理实测 |
| Flutter 真实 Windows 宿主＋受控传输 | 27 通过 | build/mutation-checkpoint-flutter.log |
| 严格 Dart JSON 分析 | exit 0，diagnostics=[] | build/mutation-checkpoint-analyze.log |
| Core／runtime scoped Clippy | exit 0，保留既有 collapsible_if 警告 | build/mutation-checkpoint-core-clippy.log、mutation-checkpoint-runtime-clippy.log |
| 宿主 Clippy | exit 0，保留 12 个既有非阻塞警告 | build/mutation-checkpoint-host-clippy.log |
| 私有绑定生成检查、git diff --check | 通过；Git 提示既有 CRLF 转换 | 本轮命令输出 |

各层分项存在覆盖重叠，不相加作为独立测试总量。真实原生测试先正常关闭并重启宿主，再验证新会话中的分段读取和原计划核对；不等同于完整应用的真实崩溃恢复验收。

默认并发全量中的服务用例要求请求在 200 ms 内返回 202，实际得到 504。单独复跑通过，断言未修改；保留首次失败证据，限制并发全量结果另列，不把一次重跑当作默认高并发稳定性的证明。

## 后续

1. 将独立恢复会话接入恢复 UI，展示不确定状态与显式停止、续扫、核对流程。
2. 验证真实宿主异常退出后的发现／核对完整链；跨进程 checkpoint 持久化单独设计。
3. 继续 guest C／C++／Rust SDK、权限 UI 与平台资格验收。Windows 条件替换仍明确 Unsupported。
