# 插件底座本地稳定候选收尾记录

日期：2026-09-26。开发基线 `d9c0431`，实现提交 `b2fa7d0`，应用仍为 `0.1.9-test.56+60`。全部验证在本地执行，没有 Actions/CI、发布、版本升级或远端推送。

## 当前结论

有界 IO／服务底座已具备可重复的 Linux 本地验收与独立旧原包回归门槛。长时服务声明、三语言资源发现、累计预算与续租、原 Store 重开后的 Observed／Unknown 核对已接通并验证。旧 `guest-v1-rc1` 不变；新增 `transport-v1-rc1` 兼容候选。

这不是完整插件系统或全平台稳定版。Windows 专属冻结依赖、Flutter 实际服务入口、Windows 凭据／TLS 生命周期没有本轮实测，仍是整体收尾阻断项。完整文件系统、流式网络、业务级 Unknown 核对及第三方独立 SDK 接入也仍开放，不由本报告替代。

## 代码与行为

- `[service_run]` / `--service-run-ms/jobs/bytes` 显式声明有限期限、累计任务及字节上限。缺失、错类型、越界或非 service 项目在构建前拒绝。核心 CLI 使用同一界限，并保留单任务 1 MiB／30 秒约束。长时包必须走专用预算运行授权，不从声明自动取得能力。
- Rust `service_resources`、C 独立资源句柄、C++ move-only RAII 解析宿主资源目录；视图可独立于原请求存活。限定 8 端点、4096 字节规范原帧，拒绝重复、非法编码、版本错配和超限。引用不含凭据值且不授予 IO 权限。自定义服务可显式选择 HTTP 出站能力与 `service_resources = true`。
- 生成三语言原包经过真实 TCP、Manager 和 worker；续租绑定原修订且不重置用量，最终允许响应可交付，后续超额返回 429，撤权后读取／续租拒绝。31 秒以上的可信时钟推进沿同一监听器生效；真实墙钟长时运行由既有网络回归另验。
- 重开原 Store 后必须重新签发授权。低 fuel 重开仍返回已观察响应，证明未执行 guest；此前陷阱留下的 Unknown 在恢复正常 fuel 后仍不重发。三条 Observed 与一条 Unknown 历史共 11 条事件保持不变。历史重放／查询仍计入准入预算，不等于免费任务。
- 工程清单支持现有 `SCHEMA_VERSION` 常量，核对八份 host/SDK schema 和七项协议版本，拒绝迁移目标或版本漂移。

## 验证结果

以下分项存在重叠，不相加为产品通过率。完整输出在 [evidence/plugin-foundation-2026-09-26](evidence/plugin-foundation-2026-09-26/)。

| 门槛 | 结果与限制 |
| --- | --- |
| core，启用 fault-injection | 645 通过；8 个 ignored harness 条目保留，故障子进程由父测试显式驱动 |
| plugin_runtime，启用 fault-injection | 397 通过；8 个 ignored 专项条目，适用的原生／HTTP／服务专项另行显式运行；Windows cfg 用例未运行 |
| network_node，plugin-adapter | 134 通过；6 个 ignored 三语言专项全部另行执行（HTTP 4，服务 2） |
| 当前 Rust SDK | 66 通过；严格 Clippy `-D warnings` 通过 |
| 核心打包 CLI | 14 通过，包括长时声明及资源发现的显式准入 |
| Python 工具 | 92 通过，包括清单、项目、契约、固定原件和失败不修补 |
| 原生 C／C++ | 一个跨语言用例覆盖无目录、有效目录、重复目录、句柄独立生命周期、移动和输出失败原子性 |
| 原包兼容 | 六个固定 IO／服务包：模块一致性 1 项、真实 HTTP 4 项、服务运行／恢复 2 项通过；执行前后摘要不变 |
| 旧内容／转换／UI 原件 | 九项执行通过；36 个受 pin 约束文件未变；Windows 专属依赖三项不计入通过 |

`tool/verify_plugin_service_sdk.py` 已先复验固定 transport 原包，再构建当前三语言包；可使用独立 `--build-root` 缓存，但证据目录必须新建。`--native` 当前限定 Linux C/C++ ABI。缺失工具或原包直接失败。

```sh
python tool/verify_plugin_service_sdk.py --sysroot /absolute/path/to/wasi-sysroot --native
python tool/plugin_transport_baseline.py verify
python tool/plugin_transport_baseline.py run --build-root build/transport-check
python tool/verify_plugin_sdk_baseline.py
```

本次工具链为 Rust/Cargo 1.98.1、capnp 1.2.0、WASI SDK 34、Linux GCC/G++ 13.3；首次下载依赖时显式使用 `--allow-network`。各 Rust workspace 使用独立目标目录。

## 兼容候选的范围

`transport-v1-rc1` 保存三语言 HTTP IO 和有限长时服务各一对 Wasm／完整包，共六对；另有三份契约、源码摘要和构建来源，共 17 个受根摘要约束文件，清单根摘要为 `341a40bf335b468954734613c2e8d43999c6523249ac06b8648558d6593f4cb0`。服务声明为 120000 ms、16 次累计保留、4 MiB 累计字节；单任务仍为 1 MiB。IO 使用既有工作台 forward handler。固定原件的模块必须与完整包内嵌模块一致。

捕获只允许新 ID 且要求对应源码已经提交；常规 verify/run 不编译 guest、不重包、不刷新 pin。新候选不扩大 `guest-v1-rc1`，不承诺整个原生 ABI 或所有能力稳定。资源 schema 已记录，但回显 guest 不执行资源发现；该功能的证据来自当前 SDK、独立核心互操作及原生 C/C++ 测试。

## 验证中发现并处理的问题

1. 既有并发续租测试直接共享非 Sync 的 Registry，导致整个 runtime 测试目标编译失败。测试现通过原 Manager 的 Mutex 协调并发调用，保留相同运行修订仅一次成功的断言，未放宽生产类型约束。
2. fault-injection 并发进程下，连续写入授权记录上限测试遇到 StorageBusy；独立用例通过。记录上限测试现持有原 Store 的 pin 完成批次，避免与进程继承的短暂文件锁生命周期交叠。实际跨 Store 排他、撤权、迁移崩溃测试继续保留并通过，未修改生产锁或吞掉错误。
3. 工程清单仍只解析字面数据库版本，不能读取主线的常量形式，18 个失败/错误项阻断工具回归。已支持明确常量并继续校验三处接纳谓词与最终迁移版本；完整 Python 回归通过。
4. 测试编写阶段纠正了 Store 接口调用、短绑定与长时包混用、将 router 创建次数误作 guest 执行次数的断言；最终以低 fuel、原事件数、累计用量核对重放。IO 首次生成使用了与既有测试夹具不同的包 ID，未通过；随后在新目录按夹具 ID 重新生成并验证，未修补合格原件。
5. 广域调试构建曾耗尽临时磁盘导致链接及 IO 失败；仅删除可重建 Cargo 缓存，保留源码、原包与日志；降低核心测试调试信息后完整复跑通过。首次原生链接使用了不存在的静态库，已改为实际共享库。最终门槛均以明确成功日志为准。

## 整体收尾仍需的外部条件

需要 Windows 执行环境验证旧依赖原件、当前三语言新原包与 Flutter 服务入口；还需实际凭据／TLS 与停止、撤权、重开、Unknown 页面核对。当前 Linux 结果不能替代这些检查。完整文件操作、异步续接与业务核对、OAuth／多账号／流式网络、复杂 UI 和 SDK 独立分发保留在开发看板。
