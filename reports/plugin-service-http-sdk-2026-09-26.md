# 公共 SDK 入站到出站 HTTP 验证

日期：2026-09-26。基于主线 `d9c0431` 与本地底座检查点 `09682b4` 继续开发；应用版本仍为 `0.1.9-test.56+60`。本次执行在 Linux，本地构建与测试，不使用 Actions/CI、不推送或发布。

## 完成行为

Rust、C11、C++17 新增 `--kind service --service-http` 起始模板，仅依赖公共 SDK，不链接宿主核心。读取已认证服务请求、解析宿主目录、选择恰好一个批准端点，只把 POST 二进制正文发到该端点的 `/`。没有任意 URL、调用者认证／Cookie／目录头转发或自动重试。

C `mp_service_request_digest` 和 C++ `service_request.digest()` 新增精确原服务帧 SHA-256，沿用 Rust 原件摘要；失败保留输出。`service-http-<64 位小写十六进制摘要>` 是出站操作 ID。需要持久宿主路由保存原请求与稳定 namespace；外层结果已保存不意味着内层 HTTP 外部效果已完成业务核对。

项目 `[io].max_resources` / 核心 `--io-resources N` 显式接受 1–8；未设置时仍为 2。新模板声明 4：监听、服务发布、端点及在途调用。仍为单作业，单作业 1 MiB／30 秒。本次生成包显式声明运行上限 120000 ms、64 次累计任务保留、4 MiB 累计字节。能力及额度声明均不产生授权，包未在测试中重写或扩张预算。

## 真实原包证据

`network_node/tests/sdk_service_http.rs` 三项测试分别遍历三语言，总计 21 个场景。经真实 loopback TCP、认证、Manager、受管 worker 和原 Store 执行：

- 成功：原二进制正文经一次出站返回 201；宿主注入合成凭据，调用者 token/Cookie 和伪造资源目录头不会出站，原 IO 帧没有秘密。
- 等待：上游暂不回应时，原 owner 可提交并持久保存本地卡片；返回后核对原 HostBinding，停止时核对 worker、维护和断开均已实际收尾。
- 断线／撤权／停止：记录保留 OutcomeUnknown，未以缺少响应推断未执行。恢复批准并重开同一数据库后请求与历史查询均不触发新 guest IO；出站调用仍仅一次。
- 成功结果重开：持久 Observed 返回相同状态和正文，改正文复用幂等键冲突；查询／重放不再执行 guest。
- 前置拒绝：目录缺失、端点不允许 POST、正文超限分别返回 503／403／413，没有 guest IO 和出站网络请求。

重开后的内层 IO phase、原本地卡片以及 Store 完整性均核对。真实上游保持监听并统计重复连接；同时断言重放后路由导入次数为零，避免把连接拒绝误算为没有重复发送。停止／撤权与正常／断线场景共 12 次数据库重开。

此夹具使用低层服务授权及显式合成凭据提供者，不证明 Windows DPAPI；持久配置服务的资源依赖实时约束由已有宿主测试负责。生产端点/凭据变化应改变所选策略 scope，授权和历史都不能靠旧引用复活。

## 门槛与复跑

以下分项有重叠，不合计为产品通过率。文本日志仅规范化行尾，原始日志另存验收归档。日志、scope、包摘要和源码摘要见 [evidence/plugin-service-http-2026-09-26](evidence/plugin-service-http-2026-09-26/)。

| 门槛 | 本次结果 |
| --- | --- |
| Rust SDK | 66 项；严格 Clippy（all-targets、wasm-c、-D warnings）通过 |
| Python 工具 | 94 项通过；包含三语言模板、资源预算范围及失败前不编译 |
| 核心打包 CLI | 15 项通过；显式额度、默认不变、重复参数及越界拒绝 |
| 服务 codec | 3 项独立 host/SDK 互操作通过；原生专项 1 项覆盖 C/C++ 与原帧摘要、分段变化、生命周期、失败原子性 |
| 新三语言原包 | 实际生成、构建、准备、打包；真实网络 3 项覆盖上列场景；测试前后原包摘要不变 |
| transport-v1-rc1 | 6 对固定原件未变；模块一致性 1 项、HTTP 4 项、服务 2 项通过 |
| guest-v1-rc1 | 36 个固定文件不变，旧内容／转换／UI 9 项通过；Windows 专属依赖未运行 |

```sh
python tool/verify_plugin_service_sdk.py --service-http --native --sysroot /absolute/path/to/wasi-sysroot --output-root build/service-http-new-run --build-root build/reusable-service-cache
python -m unittest discover -s tool/tests
cargo test --locked --offline --manifest-path core/Cargo.toml --example plugin_package
```

门槛保留生成项目和日志，输出目录必须不存在；`--build-root` 只复用宿主 Cargo 缓存，三个新 guest 重新构建。可显式加 `--allow-network` 下载依赖；默认离线。`--native` 当前限定 Linux C/C++ ABI。本次工具链 Rust/Cargo 1.98.1、capnp 1.2.0、WASI SDK 34、GCC/G++ 13.3。

| 原包 | 完整归档 SHA-256 |
| --- | --- |
| Rust | `c1939799edb363b32aeeab133ed427b382b0a35a26efa5965c2839042278b0d9` |
| C | `1359644c8039c09088af19fbc9fb1c889a5f7d73079efd07675ecad33fc8605b` |
| C++ | `ea10e8139dc17ec5a65441517335e00f9e76337f77b576ae8cf5323559207f6a` |

这些是本轮验收原件，不刷新任何冻结兼容候选。Google Drive 检查点同时保存完整源码、增量 Git bundle、生成项目源码/配置、这三个包及 Wasm、证据与 SHA-256 清单；不保存编译缓存、工具链或账户凭据。

## 首次失败与修复

首次组合授权因工具固定两资源槽而在 ListenerGrant 处拒绝。增加显式、严格有界的资源声明，模板声明四槽；没有绕过宿主限额，也没有修改已生成包。第二轮发现模板操作 ID 使用冒号，而持久 IO identity 禁止该字符，发送前被拒绝并留下外层 Unknown；改为连字符，保留两份失败日志。第三轮在全新目录从源码生成原包，完整门槛通过。

编写测试时还纠正了 TOML 测试帮助器不支持浮点测试值、手动诊断使用相对包路径的问题。最终门槛使用三种绝对原包路径并检查全部语言，不按可用数量跳过。

## 剩余范围

本轮关闭公共 SDK 的入站→出站有界模板与 Linux 原包验收缺口；底座仍为 Linux 本地稳定候选。Windows 当前三语言原包及旧依赖、Flutter 实际发布/停止/恢复入口、实际 DPAPI/TLS 生命周期尚无本轮证据，不能宣称全平台稳定收尾。完整文件操作、异步续接、流式网络、OAuth／多账号和业务级 Unknown 核对继续保留。
