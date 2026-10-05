# Morrow 当前开发状态

更新：2026-10-05。本文是当前状态入口；[开发看板](DEVELOPMENT_BOARD.md)保存任务，[SDK 门槛](../reports/reconstruction-2026-10-05/sdk-next-gates.md)保存验收顺序。带日期的报告、冻结夹具和历史源码快照保留各自身份，不因文档同步获得新的测试资格。

## 版本与开发线

| 项目 | 当前身份与边界 |
|---|---|
| 本次源码检查点 | `codex/windows-sdk-convergence-20261005`，承接云端 `codex/cloud-sdk-convergence-20261004` 的 `468ef2e912ac74e5f97f0016a8729b7d5c1f5399`，保存 C02–C07 Windows 修正与 SDK 增量 |
| 应用源码版本 | `0.1.9-test.58+62`；SDK/crate/schema 的独立版本保持，不把应用版本当协议版本 |
| 已发布 Windows 下载 | [test.56 测试预览版](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.56)；本次同步不产生新安装包或 Release |
| 原 Windows 资格分支 | `codex/windows-sdk-qualification-20261003` 保留 `20669f671152972470340a65eac3458dd2f61b4d`，与本次检查点分开 |
| 原始 SDK / 冻结夹具 | 327 个 SDK 输入与 57 个冻结输入字节保持；这是基线兼容证据，完整 SDK26 仍 OPEN |
| 许可与数据兼容 | 第一方代码、SDK 和文档为 AGPL-3.0-only；test.1 是旧数据类型最后兼容测试版，迁移前保存原库及其保护文件 |

## 最新限定验证

以下来自已封存的 Windows x64 Release、离线锁定执行；本次文档同步没有重跑这些测试。

| 范围 | 结果 | 计数限制 |
|---|---|---|
| C07 原 owner / 目录生命周期 | 115 个方法通过，9 组 | 新取消 7、目录 owner 14、原 native 16 已包含在 115 内 |
| 原始 SDK 定向回归 | 42 个方法通过，5 组 | 保留 frozen-region 的 14 项过滤及 reader 的 1 项过滤；child helpers 不另加方法 |
| 网络回归 | 100 个方法通过，11 组 | 失败、忽略、过滤均为 0；不与 C04/C05 的历史重跑相加 |
| 当前限定格式 / 编译 | 5 个文件的格式检查通过；严格 library Rustc 通过 | `io_jobs.rs` 的 17 个既有格式差异仍保留，不称整库格式清洁 |
| 整库 Clippy | 失败，exit 101 | 10 处既有诊断、0 处本轮 owned 诊断；整库 lint 仍开放 |
| 本地交接包 | 两个基线的实际恢复得到相同源码树，全部 3,180 成员校验通过 | 包是源码增量及证据交接，不是可安装产品或 Release |

详见 [Windows 复验](../reports/reconstruction-2026-10-05/windows-sdk-revalidation.md)、[目录 owner 实测](../reports/reconstruction-2026-10-05/directory-owner-sdk.md)、[目录与 blob](PLUGIN_DIRECTORY_BLOB_SDK.md)及[本次文档同步](../reports/reconstruction-2026-10-05/documentation-sync.md)。C07 封存树 `dacac682a9e341a4931508021a1c815929f42dea` 是恢复树，不是提交；本次文档更新后的提交身份以分支历史为准。

## 已推进的 SDK 能力

- changes 元数据具备独立 discovery、严格消费者和有限内容更新来源；能力发现不授予执行权限。
- 独立 Rust / C / C++17 WebSocket 消息、SSE 事件库及源码分发已完成限定资格；数据解码不产生网络授权。
- 独立目录观察、blob 分段校验库已完成有限编解码与跨语言一致性验证。blob 的 `VerifiedBytes` 不等于 Store 的耐久提交。
- Windows 目录捕获、分页、结束、取消、未读交付及 idle 清理接入原 `IoWorker`，保持原 Manager、HostRuntime、IoBinding、时钟和预算。
- 目录查询、编码、取消谓词及真实资源 drop 留在短时钟检查之外；额度在 root / broker / lease 真正释放后归还，累计费用不退。Unknown 不自动重放。

## 下一阶段与冻结门槛

| 顺序 | 工作 | 当前状态 |
|---|---|---|
| C08 | 原工作线程生成目录会话的新鲜随机秘密；失败关闭、取消、预算和持有期零化 | 设计已审，代码与测试尚未执行；旧显式 secret 接口仍由可信调用者负责 |
| G04 后续 | picker / 祖先来源证明、工作台任务入口、独立目录 request/schema 和 feature/import/helper profile 协商 | OPEN；bare File 仅证明对象，公开 FileList 仍 Unsupported |
| 文件完整能力 | blob 耐久 backend/history、upload、watch、rename及恢复矩阵 | OPEN；conditional Replace 仍 Unsupported，不退化为无条件覆盖 |
| SDK26 其余门槛 | 异步组合、长期 changes/cursor、完整账户与网络恢复、第三方安装批准、平台矩阵及生产 UI | OPEN，按完整 26 项要求审计；不缩小为当前已通过的子集 |

生产 protected owner、真实 session、StorageIoWorker、普通用户 token、GUI、账户/TLS/公开服务与其他平台仍需对最终候选分别验收。本轮普通合成 Store 与临时目录的 PASS 不能替代这些资格。Linux 云端证据保持其来源，不能当作 Windows 或全平台通过。

## 阅读与证据规则

默认 README 为中文，另有八种语言入口。[文档导航](DOCUMENTATION_INDEX.md)列出维护入口与历史／冻结范围。状态、已实现接口、实际执行、生产资格及发布状态分别记录；历史报告中的“下一项”“未提交”“未推送”按当时日期理解。原始失败、过滤、Unknown 与未运行项保留，不用后来的通过覆盖。当前开发仍是重构测试线，满足完整门槛后才考虑 SDK 冻结与 0.2.0。
