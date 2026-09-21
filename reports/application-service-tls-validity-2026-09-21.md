# 应用 TLS 有效期与运行授权

2026-09-21；基线 `75d69e1`；应用版本仍为 `0.1.9-test.52+56`。本轮完成证书时间约束、私有协议、双语 UI 和真实 Windows 验证；不宣布公共 SDK 稳定。

## 实现与边界

`TlsValidity` 对所提供 PEM 链的 1–16 张证书解析共同区间：最大 notBefore、最小 notAfter。输入最多 65,536 字节，拒绝无证书、DER 尾随数据、倒序、无交集、超出支持年份或证书过多。采用固定 `x509-parser 0.18.1`、与 TLS 身份相同的 rustls PEM 迭代器；未启用签名验证功能。

证书字段以秒表示，包含起止秒；转换为 `[notBefore*1000, (notAfter+1)*1000)` 授权区间。1970 年之前的起点在无符号授权时钟中缩至 0，不扩大实际证书区间；旧证书不能因此恢复运行。有效期边界依据 [RFC 5280 §4.1.2.5](https://www.rfc-editor.org/rfc/rfc5280#section-4.1.2.5)。这不是域名、签名、信任链或吊销验证，连接客户端仍负责这些政策。

启动重新读取和解析实际证书，在移交原拥有者前调用 `ResolvedService::restrict_validity`。它消费原解析对象，保留 Store lease、共享 UTC 高水位时钟、单调 deadline 和全部出站依赖，只缩小时间范围。没有新增批准或占用出站依赖槽位。到期或时钟回拨永久使该次 live authority 失效；回调、监听和缓存交付继续检查同一授权。服务通过既有监督循环请求停止并回收，不承诺强制抢占任意同步回调。

检查命令可以展示未来/已过期证书；私有响应增加时间字段，启动请求仍只携带路径及摘要，Rust 不信任界面时间数据。Flutter 展示 UTC 共同区间，到期清除可启动选择；时间恢复或未来证书进入有效期均需显式重新检查。计时器随组件销毁取消，旧异步结果不能恢复选择。

真实窗口截图发现初稿本地化参数顺序反转（生成接口为 end/start），已修正调用，增加中英文起止顺序断言。最终截图 `build/tls-validity-window-fixed/01-checked.png` 已人工检查。

## 验证

| 范围 | 结果 |
| --- | --- |
| 宿主 lib 与私有协议 | 82 + 7 通过 |
| runtime service_io | 20 通过 |
| network configured_service | 8 通过 |
| 客户端五文件组合 | 原组合 66 通过；新增缺失/损坏时间测试所在文件 10 通过，合计 67 个不同用例；修复后的 picker 3 通过 |
| 真实 Dart→Rust 进程 | 3 通过 |
| 完整 Windows 窗口 | 1 通过，修复日期后重新验证 |
| 语言资源 | 4 通过 |
| 静态检查 | 9 文件分析、宿主 lib 严格 Clippy、Rust 格式、协议及 i18n 生成一致性通过 |

原生短期证书测试实际建立 TLS 连接，在未显式取消的情况下观察证书到期、监听退出、原拥有者返回和端口关闭。缓存测试覆盖过期及回拨后拒绝缓存交付、时钟恢复不能复活。真实进程测试覆盖伪造有效时间元数据仍不能启动未来/过期 PEM，拒绝后原 submission 与拥有者仍可用于有效证书。

Windows 测试使用完整 MorrowApp 和实际 Rust host；文件选择结果由测试注入，不代表系统文件对话框或物理输入验收。窗口中的 1975–4096 年证书仅为合成夹具。日志：`build/tls-validity-*.log`。最终 Release 构建和哈希见 `build/tls-validity-build-receipt.json`。

## SubagentBridge

后端当前为 `sessions-2026-09-21`，无 provider block；本会话未暴露 Bridge MCP，使用部署 CLI。新会话与缓存能力的先前实际验证见[会话报告](subagentbridge-sessions-and-routes-2026-09-21.md)。DeepSeek/GLM 模型均声明 max；本轮实际执行 DeepSeek max。

- `task_ad80a83541ea403bd249e424` 提供收窄方法候选，主代理修正错误类型并独立验证。
- `task_26811ff806ba89611a435a2c` 审阅被 1024 输出 token 截断（FAILED/PARTIAL），未作为通过证据。其“deadline 可延长”和“回拨不持久失效”等判断与代码中的 min/shared Clock 不符；未采纳，也未自动重放。
- `task_70363fc4d7abdcd5c17932d5` 缩小为合成证书夹具，SUCCEEDED/COMPLETE，265 输入/365 输出 token。代码经人工审核、编译及真实过期/未来证书测试采用。缓存命中为 0，不推断节约量。

## 后续

下一项为受保护密钥存储与显式续期策略：保存引用而非私钥明文；绑定原用户/平台能力；明确停服、轮换和失败恢复语义。自动续期/ACME、客户端证书认证、Unknown 持久核对、完整文件系统和 C/C++/Rust IO SDK 仍未完成。当前仍为本地开发阶段，未推送或发布。
