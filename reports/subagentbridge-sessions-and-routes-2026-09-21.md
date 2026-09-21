# SubagentBridge 新会话验证与多端点路由

日期：2026-09-21。Morrow 基线 `8d5cfac`，应用版本不变。

## 插件更新的实测

安装版本为 `0.3.0+codex.20260920234522`，后端报告 `sessions-2026-09-21`。已读取安装包 README／sessions 说明及 CLI 实现；当前任务未加载新的 MCP 工具，使用更新后的官方 CLI 完成调用。

已实际验证 create/list/get 和同会话续写：

| 模型 | 档位 | 实际任务 | 会话状态 | 第二轮供应商缓存统计 |
| --- | --- | --- | --- | --- |
| GLM-5.3-flash | max | 多端点路由实现、按实际 API 修正 | revision 0→1→2，4 条历史消息，无阻断 | hit 512 / miss 1038 |
| DeepSeek-flash | max | 路由安全审阅、剩余验收缺口 | revision 0→1→2，4 条历史消息，无阻断 | hit 768 / miss 215 |

两模型的第二轮均能返回仅在初始会话背景给出的标记；不是仅检查配置。缓存值来自供应商 facts，不意味着后续必然命中，也不从授权输入 token 预算扣除。GLM 首稿仍有虚构 API，第二轮纠正后才采用；主代理独立编译、测试与审核。

DeepSeek 原授权过期，按用户既有授权通过官方 manager 申请 8 次有界授权，并在确认无运行任务后 reload；未改配置、账本或供应商限额。两会话保留供后续模块工作复用，未测试关闭、服务重启恢复或故障续写，不宣称十个 MCP 工具均已在此任务可用。

后续按模块保留简短固定上下文；每轮读取当前 revision、提交差量任务、使用新 request key。有效会话不替代有效执行授权。失败／未知结果不自动重试。

## Morrow 实现

新增 `HttpRouteSet`，显式持有最多 8 个已经批准的 live HTTP 端点；按 guest 请求中的精确引用选择，保留原请求字节、原 HttpCallGuard、原实例授权和 Deferred 调度。集合可由服务工厂克隆，不创建新批准、默认端点、通配地址或凭据输出。

已接入真实服务组合测试的 RouterFactory。四项新测试覆盖：

- 同一原实例的两个已批准端点分别到达正确的真实 TCP 服务，精确响应材料绑定原操作；等待期间原拥有者仍可处理业务命令。
- 空集合、重复引用、超过数量上限和未选择引用拒绝；未选择引用不生成发送意图。
- 外来实例或撤销端点即使在集合中仍不能外发。
- 旧集合跨越原实例断开，在同一原 HostRuntime 上重连新实例后仍被拒绝，不生成意图、不外发。这项补足了 DeepSeek 审阅提出的验收缺口。

相关 `managed_http` 24、`managed_content_service` 9、`managed_service_owned` 9，共 42 项通过；相关网络 lib/tests 严格 Clippy、修改文件格式与 diff check 通过。共享 TCP 屏障改为每个测试 crate 仅加载一次，修正严格 Clippy 发现的重复模块加载。

日志位于忽略目录 `build/bridge-session-*`、`build/bridge-deepseek-session-*`、`build/http-route-set-regression.log`、`build/http-route-set-clippy.log`。

主应用服务仍使用 `DenyOutbound`。本轮提供可复用的精确端点路由集合，下一步仍需服务配置／启动选项选择持久端点、原宿主批准与撤销，以及让插件获取明确允许的引用。没有重建 Windows 应用、推送或发布；没有宣布新 IO SDK 稳定。
