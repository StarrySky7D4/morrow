# 配置服务统一接线与三语言原包复验

2026-09-26，基于本地 `7d08b95` 继续开发。临时工作区经维护清理后，从既有 Google Drive 检查点恢复完整源码、增量 Git 历史及三语言原包；三份归档 SHA-256 与上一轮记录一致。远端主线核对仍为 `d9c0431`，已恢复原分支，应用仍为 `0.1.9-test.56+60`。未使用 Actions/CI、推送或发布。

## 本次关闭的缺口

上一轮服务出站原包以低层服务授权验证；主应用则需要持久配置、认证及所选端点/凭据的实时依赖，历史缓存同样受这些条件限制。原来工作台分别拼接 scope、依赖、router 和目录，其他原生宿主复用时容易漏接其中一项。

新增 `network_node::service_outbound`：

- `ServiceEndpointSelection::validate` 在读取记录前拒绝重复、超量、零引用和零修订；工作台保留原类型路径的 re-export。
- `SelectedService::resolve` 核对预期修订、原 Store 和资源租约，一并附加实时依赖并计算所选策略 scope。解析阶段不打开凭据或网络。
- `approve`／`approve_windows` 在原 Manager、Host、实例和运行绑定下签发授权，从同一批准端点集合生成 HTTP router 和可选 guest 目录。Windows 继续使用原系统保护凭据路径；其他平台没有明文自动降级。
- `PreparedService::attach` 再核对配置有效性和 worker 身份，返回配对的 ServiceHost/ConfiguredService。失效或外来 worker 由错误结果归还，保留显式停止与回收责任。

工作台 `start_service_with_outbound` 已复用上述路径，替换原手写接线。没有新增 guest 能力、改变 wire/schema、放宽预算或修改旧兼容原件。目录仍须 guest 显式声明；空选择不产生出站批准。入口错误仍由原清理路径处理，运行失败不打开第二份 Store 替代原 owner。

## 原包真实配置服务验证

`network_node/tests/support/sdk_configured_http.rs` 新增三项测试，均遍历上轮生成的 Rust/C/C++ 原包，不改模块、声明或额度：

1. 成功经配置服务转发，持久保存 201 与二进制正文；将宿主 fuel 降至 1 后重开，直接请求和历史查询仍返回相同原结果，证明缓存不执行 guest。更新未选择的资源，原服务与缓存继续有效；更新所选端点或凭据，接受入队即撤销授权，事先建立的直接/历史连接均不得交付旧正文，旧配置不得重新绑定，禁用资源重开也不得恢复。
2. 真实 HTTP 等待期间，原 owner 保存本地卡片；所选端点/凭据撤权或主动停止后，无迟到成功正文。实际 join 检查执行、维护和断开结果，再重开同库核对卡片。重新批准后，未变策略的 Unknown 或已变策略的 scope 冲突均返回 409，同一旧键不再出站。
3. 拒绝重复、超量、零引用/修订、不匹配的预期修订和未知引用；有已保存端点但未显式选择时，不打开凭据，guest 返回 503；把已批准服务接到另一 Store/实例的 worker 时拒绝，并将该原 worker 交回，随后它仍能接受自己的配置并正确回收。

上游保持真实监听并统计连接，所有成功/撤权/停止场景都核对仅一次出站。原有低层三项继续执行；外部伪造资源头、敏感头隔离和合成宿主凭据检查沿用同一个真实上游。合成提供者不是 Windows DPAPI 验收，HTTP 409 也不是业务级未知效果核对。

## 可迁移的复验入口

`tool/verify_plugin_service_sdk.py` 新增 `--original-packages` 与可选 `--package-root`。严格要求三个语言条目、SHA-256 和原包齐全；迁移后按 `LANGUAGE/dist/SHA256.mplugin` 定位，缺失/损坏就失败，不选择别的文件，不重新编译或重包，也不刷新摘要。输入无效时不创建证据目录或启动编译。

```sh
python tool/verify_plugin_service_sdk.py --service-http --native \
  --original-packages reports/evidence/plugin-service-http-2026-09-26/packages.json \
  --package-root /absolute/path/to/restored-evidence \
  --output-root build/configured-new-run --build-root build/reusable-cache
```

原包模式不需要 guest 编译器或 WASI sysroot；仍需当前宿主 Rust/Cargo、Cap'n Proto 和适用的本地 C/C++ 工具。默认离线，首次下载依赖可显式 `--allow-network`。`--native` 仍仅支持 Linux；其他平台可运行原包门槛，但只能据实际结果记录资格。输出目录必须新建，scope 记录 `guest_source = original-packages`。原生成模式继续保留。

原包摘要保持：Rust `c1939799edb363b32aeeab133ed427b382b0a35a26efa5965c2839042278b0d9`；C `1359644c8039c09088af19fbc9fb1c889a5f7d73079efd07675ecad33fc8605b`；C++ `ea10e8139dc17ec5a65441517335e00f9e76337f77b576ae8cf5323559207f6a`。

## 实际结果

分项有重叠，不相加为产品通过率。证据见 [evidence/plugin-configured-service-2026-09-26](evidence/plugin-configured-service-2026-09-26/)。文本日志仅规范化行尾；初次两份宿主日志未包含完整测试摘要，固定原件与 codec 已单独复跑，使用 `transport-complete.txt`、`frozen-and-codec-complete.txt` 的完整结果。

| 门槛 | 结果 |
| --- | --- |
| network_node 常规回归 | 134 通过；12 个 ignored 专项另由以下入口执行 |
| 服务出站原包 | 6 项通过，包含新配置路径 3 项与原低层路径 3 项，覆盖全部三语言 |
| 固定 transport 原件 | 模块一致性 1、HTTP 4、服务 2 通过；17 个固定文件不变 |
| 固定 guest 原件 | 旧内容/转换/UI 9 项通过；36 个固定文件不变；Windows 依赖目标在 Linux 为 0 项，不计通过 |
| SDK / codec / native | Rust SDK 66；独立服务 codec 3；C/C++ 原生专项 1，通过 |
| Python 工具 | 97 项通过，包含缺语言、坏摘要、文件变更及显式迁移布局检查 |
| 静态检查 | network_node lib 严格 Clippy `-D warnings` 通过；workbench_host Linux `cargo check` 通过，保留 6 项既有平台/死代码警告 |

重新安装了 Rust/Cargo 1.98.1 与 Cap'n Proto 1.2.0；工作区权限不支持系统 apt 初始化，改为工作区内构建安装编译器，没有修改系统权限。未重编三个验收 guest，没有将“源码可构建”替代原件兼容验证。

## 收尾状态与下一门槛

本次补齐原生宿主的配置服务集成及恢复原包验收入口，仍是 **Linux 本地稳定候选**。当前 workbench_host 在非 Windows 的内容库打开路径明确不支持，因此本轮只是工作台共享代码编译与原生适配器实跑，绝非 Linux 工作台或 Flutter 页面验收。Windows 条件编译的 DPAPI 分支、旧依赖、实际启动/停止/恢复窗口及 TLS 身份生命周期仍需 Windows 环境；未因缺少该环境而放宽保护或添加明文后备。

完整文件系统、异步续接、业务级 Unknown 核对、OAuth／多账号和流式网络仍按开发看板推进。源码、Git 增量历史、原包与最新证据继续保存 Google Drive，恢复说明标明基础提交及复跑方式。
