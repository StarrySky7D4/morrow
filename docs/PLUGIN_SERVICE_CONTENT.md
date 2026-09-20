# 受管服务的内容权限交集

状态：Windows 本机原生内容服务子集验证通过（PASS_SCOPED），见 [本轮报告](../reports/service-content-2026-09-19.md)。范围为可信宿主显式配置的持久服务；不等同主应用发布 UI、持久账号系统或新版 SDK 稳定承诺。

## 唯一授权来源

内容访问必须同时具备插件声明／Registry 批准、原 Manager 实例与连接、原逐对象内容 grant、原服务／监听批准，以及当前认证 principal 的有效内容范围。主体拥有服务访问权，并不自动拥有任何卡片访问权。

`ServiceContentPolicy::issue` 绑定实际 ServiceGrant，并从原 HostRuntime／ManagedInstance 取得既有对象授权的只读 probe。它不创建对象 grant、连接或数据库。probe 克隆保留原撤销、到期和连接生命周期；原 grant 替换、撤销、停止、宿主释放均使旧 probe 失效，Draining 不开放新的内容访问。probe 与原 HostPolicy 共用单调时钟约束。

内容 scope 精确到 `(GrantKind, card_id, attachment_id)`，至多128项，排序规范化并拒绝重复；只有 ReadAttachment 必须指定附件，其他操作不得携带附件。没有通配符或类型推导：ReadContent 不代表 ReadSummary，读权限不代表写权限。主体有效 scope 必须是服务策略的子集；空集合法但不允许任何内容命令。

## 宿主路由与请求

宿主先建立原内容 grant，再创建 ServiceContentPolicy。`ServiceHost::content_route` 接受实际 service grant、路由、ServiceJournal、policy 与 principal→scope 固定映射。服务配置与持久 namespace 仍由可信宿主负责。

HTTP 完成认证后以实际 Principal.id 查映射；没有映射拒绝。授权持有原 Principal 的 live 回调，重新检查原服务 scope、主体到期和撤销。guest 不能通过正文、卡片 ID 或 HTTP header 自报主体来取得权限。

宿主剥离所有用户提供的 `morrow-content-scope`，再注入有效 scope 的规范 SHA-256。这个保留字段仅绑定输入语义，不是凭据。它进入原 service Request 的完整匹配：同幂等键在有效权限范围改变后发生冲突，不能读取旧范围的已保存结果；重新颁发相同范围也必须重新取得当前真实对象 grant。`submit_service` 和普通 `submit_service_durable` 拒绝带此字段的请求，避免无内容授权的入口重放内容结果。

## 执行、预算和交付

只有 `submit_service_content` 的持久服务作业可以使用 `morrow_v1.exchange`。普通 IO 作业和普通受管服务仍拒绝内容 exchange。所有原7类命令可走同一个 core 入口：创建、编辑、改名、读内容、读摘要、读附件、查询操作结果；每个命令仍受其原有格式、修订和逐对象授权约束。

内容与 IO import 共用同一个串行回调、原 HostRuntime、队列、job/calls/bytes 上限和取消信号。内容请求先计费，进入可能提交的事务前再预留完整64KiB响应上界；响应按这个固定上界计入累计额度，不按实际小响应退还，避免提交后才发现响应预算不足。

`HostRuntime::dispatch_guarded` 在原读取／事务边界检查额外约束，并在成功响应编码后再次检查。内容事务内只检查当前边界传入的原时钟值，不递归采样或另开权威。保留旧 `dispatch` 的既有采时行为。对任何字节偏移嵌入的 runtime 帧，codec 使用受限的自有对齐缓冲区，保留原64KiB输入、遍历、嵌套及尾随数据限制。

准入、排队、实际执行、历史结果读取、Ready/read 与 HTTP 最终返回都检查当前内容授权。内容型服务的后续外发也携带同一访问约束，覆盖 Broker 存储／发送边界和 HTTP 监控。撤权阻止后续调用及交付，不撤销已经提交的内容，也不能追回已经传出的字节。

## 持久结果与剩余工作

沿用 [持久请求恢复](PLUGIN_SERVICE_HISTORY.md) 的 Prepared／唯一认领／Observed 原件；范围变化产生409冲突，Unknown 不自动重新执行。实际内容提交沿用原 content operationId 和事务，不把 HTTP 成功当作内容提交证据。范围摘要不是完整因果审计，保存服务响应也不替代内容事务记录。

原Store的配置记录和保持内容范围校验的只读查询已建立，见[配置与恢复合同](PLUGIN_SERVICE_RECOVERY.md)。后续仍需：持久主体和批准引用解析、主应用逐资源授权和恢复操作、内容范围管理 UI、Unknown 核对、完整入站→子操作→内容提交证据关联、证据合法退休及更多平台验收。新服务接口的三语言 SDK 与独立开发者示例仍待建立，不提供 TS/JS 或动态 Dart 插件。
