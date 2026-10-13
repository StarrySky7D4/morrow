# Windows startup quarantine：阶段 565 限定验收进度

日期：2026-10-13。状态：独立实际审阅 PASS（原始审阅工具 `1a7eaf/exit0`）。独立报告为 4418 bytes，SHA256：`3929894e78578530f6c4897bd1f62b161d1ff6b406ad308ed1a28ab98bcf16fa`。

本轮在 Windows 上实际执行一个受控 startup RPC fault 用例，唯一可见测试成功结束，耗时约 27.093 秒。独立实际审阅结果为 `PASS_BOUNDED_REAL_RPC_FAULT_RESOURCE_QUARANTINE_ONLY`，已核对原始证据字节、机器结果、Prepared 身份与 joins，以及自有会话闭合和策略恢复。ROOT 直接检查 `8e5a07/exit0` 仅作旁证，不是独立结论的唯一依据。这是该故障夹具及其资源隔离证据链的限定通过，不是完整产品、SDK 或安全验收。

本轮修复两项 P1：

- **最终 capability 广告丢失。** supervisor 最终从 `owner.record()` 重取记录时，丢失此前仅在输出记录上添加的 `abnormal_resource_closure_protocol=1`，使接收端拒绝 Prepared 记录。修复在最终发送 Prepared 的同一分支重新添加该字段。它只补齐最终 wire 广告，不改 ledger、durable joins、身份绑定或正常关闭判定。
- **PowerShell 5 测试入口数组包装。** `@($ExpectedSDKJson | ConvertFrom-Json)` 在实际 PS5 环境形成单元素包装，原五项 SDK 门因此拒绝。入口改为直接赋值，并保留显式数组类型、数量为五、角色唯一及逐文件 SHA 校验；没有放宽 SDK 或协议接受条件。

实际故障路径中，supervisor 以 exit2 结束且两输出流 EOF，Prepared 记录含三条 reader join，实际 host 子进程 exit0。所有者保持 `UnknownRetained`，没有转成正常关闭或 Accepted。主机捕获与本次自有会话均已闭合；Process 执行策略按原状态恢复。compiler 和 tester 的清理结果各实际记录一次 -1，不能写成“所有辅助进程退出码为 0”。历史 Unknown 记录没有被重放、接管或升级。

历史 normal535 的 scoped PASS 属于旧组合。本轮没有对新 555 supervisor 组合复验正常路径；新故障用例的一次通过不能转用为该组合的正常路径通过。资源结论来自原协议记录及独立字节读回，不是独立内核、job tree 或 ledger 查询。

capability 修复已作为一行追加集成回仓库源码；ROOT 确认其原始字节与本轮资格编译的 supervisor 源码相同。尚未执行本轮修改后的整库重编译；本报告也不表示已提交、推送或发布。

仍然 OPEN：SDK/G04、完整安全资格、生产 Codex 执行链及完整应用验收。受控 RPC 故障通过不等于自然操作系统故障资格，也不覆盖其他执行或隔离模式。

下一步已准备当前匹配的 harness、runner、setup 三角色隔离构建方案及独立 basic workflow 合同。37 项固定输入的双遍 SHA 与 20 个锁文件一致性检查通过，两主锁的外部依赖身份匹配；这仅是源与依赖元数据准备，不等于 Cargo 离线解析、编译或当前三角色产品通过。旧 H011 PE 不能替代当前源码产物。完整源码与依赖封存、三角色原始编译捕获、独立产物绑定及真实 workflow 验收仍待完成；账户、ACL、WFP 等 setup 效果不由本报告授权。
