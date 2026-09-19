# 受管入站服务接线与限定验证

日期：2026-09-19。隔离分支：`codex/io-safety-refactor`，前置提交 `5ac3b5b9b5aee746d530554e8e3a48a34c9ca992`。结论：**PASS_SCOPED**，原生宿主显式发布受管服务的 Windows 本机 HTTP/TLS 子集通过；不是完整网络产品、服务 SDK 或公网部署验收。

## 本轮实现

- 新增独立 `service.capnp` 与有界 codec，请求含宿主确认的 principal、service、handler、方法、目标、业务头和正文；响应同时绑定原帧 SHA-256 与 call ID。IO 声明新增可选 tag 7，旧空值兼容，固定摘要与 HttpPublish 同时校验。
- 原 Manager/Host/ManagedInstance/IoBinding 颁发 `ServiceGrant` 与 `ListenerGrant`，占原实例 resource。监听单次激活，clone 不增加配额或续期；相同插件包不能跨实际实例借用批准。
- 原 `IoWorker` 新增 `submit_service`，零 IO 计算合法，完整服务完成帧另计共享字节。服务作业仍走原队列、job 租约及 Broker 子调用；无效/超额完成不交付，已发生子调用的失败不假装回滚或自动重试。
- `ServiceHost` 与不透明 `ManagedRoute` 固定实际 worker；`ManagedNode` 发布前核对监听与所有路由属于同一执行器。纯 HTTP 仍限定 loopback，TLS 显式配置；未创建公网监听。
- `Principal` 固定 token 摘要、主体 ID、service scopes、有效期与共享撤权。真实认证产生身份，重复认证头拒绝，自报 HTTP 字段不能替换 principal。原始认证、Cookie、代理与连接头及 Connection 指定字段不交给 guest。
- typed 服务可以返回状态、重复头、二进制正文及合法 obs-text 响应头。入站原生头仍经过严格文本转换，未承诺所有非 ASCII 请求头兼容。

## 审查与修正

三名子代理分别负责核心契约、运行时及服务鉴权；主代理实现原生受管适配及真实网络/Wasm 集成，统一执行 Cargo。独立复核发现并修复四项问题：

1. 监听许可与服务实例可错配：不透明路由持有实际 ServiceHost，节点以同一执行器和原 binding 核对。
2. 两套取时时序可能误判回退：监听监控只经 `IoWorker::check_listener` 使用原 Authority 同锁取时及校验。
3. 监听租约可能提前释放：关闭超时先 abort 再等待监听任务销毁，监督任务持有 grant 直到 shutdown 完成；不宣称所有独立连接或外部业务都已回滚。
4. 撤权到后台监控存在窗口：服务批准绑定真实 ListenerGrant，共享取消信号并参与 submit/import/Ready/read 检查，撤权无需等待 5ms 监控。

严格 Clippy 另发现服务元数据增大消息枚举，已将该可选元数据装箱，最终运行时和网络全量回归重跑通过。最终独立代码复核未发现新增明确 P1/P2；该复核不是独立重跑测试。

## 实际验证

Windows，release/locked/offline。网络使用本机临时端口、合成凭据、临时数据库和测试证书；所有生产测试由主代理统一执行。

| 检查 | 最终结果 | 日志 |
| --- | --- | --- |
| core 全量 all-features | 476通过，0失败，2个父测试调用的子进程入口 ignored | `build/managed-service-core-full.log` |
| plugin_runtime 全量 all-features | 341通过，0失败，1个父测试调用的子进程入口 ignored | `build/managed-service-runtime-final.log` |
| network_node 全量 plugin-adapter | 66通过，0失败 | `build/managed-service-network-final.log` |
| 三 crate 全目标严格 Clippy | 通过，`-D warnings` | `build/managed-service-core-clippy.log`、`build/managed-service-runtime-clippy-final.log`、`build/managed-service-network-clippy-final.log` |
| core 默认 wasm32 库编译 | 通过；保留7条既有 read_archive/read_capture dead_code 警告 | `build/managed-service-core-wasm.log` |
| runtime 默认 wasm32 库编译 | 通过，仅验证编译边界 | `build/managed-service-runtime-wasm.log` |
| 冻结 SDK 原件完整性 | 36固定文件、13原 Wasm/包对通过；没有重新构建或封装 | `build/managed-service-sdk-baseline.log` |
| 冻结原包执行 | 已包含在 runtime 全量，9项基础与3项依赖通过 | `build/managed-service-runtime-final.log` |

新增37项包含：core codec/声明8项、runtime service_io 10项、Principal真实HTTP/TLS 11项、managed_service真实HTTP/TLS→Wasm 8项。以上是当前各crate总数，不与专项或前一轮数量重复累计。

端到端Wasm逐字节校验实际服务输入，包括真实主体、固定处理器、方法、重复查询和二进制正文；预计算回包绑定精确输入，用于验证实际宿主传递而非动态SDK实现。七种方法及TLS真实通过；错误 token 不消耗guest调用序号；错误call ID响应不交付；独立服务撤权、监听单次使用、跨实例混接、Manager停用、到期及Drop实际关闭端口通过。

运行时另验证完整完成帧计费、零IO、错owner/handler/schema/缺审批拒绝、Ready后撤权、运行中listener撤权和超限无载荷。Principal验证互斥scope、重复/缺失认证、读正文前拒绝、读取中撤权、handler完成后撤权/过期、配置冲突预先拒绝及原字节响应头。

## 仍未完成

- native host 显式发布已实现；guest 动态注册、持久服务/监听资源批准和主应用授权/任务/恢复界面仍待接入。
- Principal scope 当前是 service 调用范围，不是内容 ACL；原始 core exchange 仍拒绝。细粒度远端主体与内容/出站操作权限交集尚未完整建立。
- 入站持久幂等、响应丢失后的稳定查询/恢复、整条服务调用审计关联；现有出站子调用证据不等同完整入站证据。
- 新服务协议的 C/C++/Rust SDK、可分发示例、独立开发者验收；本轮真实 guest fixture 不是新版 Rust SDK 完成。
- 受控文件系统写入/选择/枚举、主应用集成、流/大型正文、SSE/WebSocket/OAuth及各平台资格。

服务合同见 [IO-D2](../docs/PLUGIN_MANAGED_SERVICE.md)，后续任务见 [看板](../docs/DEVELOPMENT_BOARD.md)。下一步先完善入站持久请求/结果与内容权限交集，再推进受控文件系统和三语言扩展；完整目标保持不变。

本轮没有修改应用版本、用户数据、冻结SDK原件；未构建安装包、迁移主应用数据、推送、合并主开发线或发布。主工作树保持原状态，本地隔离提交供下一步整合。
