# G0 真实产品图与剩余资格：011

2026-10-01。G0-C05/C06/C07/C08 仍为 **blocked**；真实 native/Wasm 产品图仍为 **0/2**。这份补充记录当前外部 companion 的实测事实，不修改旧 G0 台账，不把资格 probe 或 Morrow Windows 主产品的成功扩展成 Codex/CC Switch 整体资格。

## 当前来源与工具

实际 companion 是 `<外部 companion 源码目录>/morrow-codex`。仓库内 `companions/morrow-codex` 是旧 mirror，本次未用它作产品来源。原固定 Codex 为 `44fe510ce3ee61c8ef623adcbf89b901c73ddd61`，CC Switch 为 `846de29c13ac4d65f164db8c15dd5fd58e29f972`。本轮完整文件盘点没有重新认证下载来源；其用途是绑定当前本机字节与已交接的 004 patch。

新增只读工具 [m03_g0_graph_011_check.py](../tool/m03_g0_graph_011_check.py) 检查当前 graph 合同、目标 std 可用性、清单/锁/feature/patch、全锁 vendor 覆盖和源码差异。`--metadata` 仅执行真实产品入口的 `cargo metadata --offline --locked --filter-platform x86_64-pc-windows-msvc`；不编译、不准备锁、不下载、不修改 companion。每次输出必须是独立新目录；个人账号、Cargo/Git 配置、编译 wrapper 不从环境继承。

资格工具返回 2，报告保留两条实际 Cargo 返回码 101。metadata 成功也不会令该工具宣称 G0 或产品 build 通过。

## G0-C05/C06：真实入口与闭包

| 对象 | 当前真实入口 | 实测事实 |
|---|---|---|
| Codex native | `upstream/p02-integration-004/codex-work/codex-rs/Cargo.toml`，`cli/Cargo.toml` 中 `codex-cli` 的 `codex` bin | 产品 workspace 存在；锁、Git patch 和默认 features 未被 004 probe 替代 |
| 004 probe | `qualification/p02-integration-004/Cargo.toml`，`p02-integration-probe` bin | 显式开启 `morrow-p02-restricted-qualification`；是拒绝路径资格程序 |
| CC Switch 原产品 | `upstream/p02-source-batch-001/cc-switch-source/src-tauri/Cargo.toml` | Tauri 原产品；`src/lib.rs:350` 构建 Tauri app；不是 portable slice |
| native/Wasm plugin 图 | `tools/build-contract.json` | 两图 `status=not_implemented`、`manifest=null`、`lock=null`；除 preflight 外各 stage 未实现 |

Codex 原源码 8697 文件、patched 源码 8700 文件，差异恰为 12 个路径。12 个当前 patch 输入摘要及 693 行交接 patch 的 SHA-256 均匹配 004 handoff。叶子 symlink 只记录 link 文本，不读取指向目标；目录 link/reparse 被拒绝。两次真实 metadata 的原 manifest/lock 前后相同，完整 Codex 原树、patched 树和 CC Switch 原树在 metadata 前后也相同。

| 锁 | 总 package | registry | local | Git | 当前 002 vendor 缺失的 registry package/version |
|---|---:|---:|---:|---:|---:|
| Codex 产品 workspace | 1471 | 1297 | 158 | 16 | 236 |
| 004 probe | 1117 | 1013 | 104 | 0 | 0 |
| CC Switch 原产品 | 750 | 749 | 1 | 0 | 405 |

这是 **全锁** 的 package/version 与 `.cargo-checksum.json` package checksum 盘点；已找到的 checksum 无不匹配。它既不是 Windows 选定 target 的依赖闭包，也不是 vendor 所有文件的重新哈希。不能把 236/405 全部归为 Windows 编译必需，更不能把 probe 的 0 缺失解释成产品可编译。

004 Cargo home 的 config 只把 crates.io 替换为 `out/p02-exec-store-002/vendor`。产品 Codex 保留 crossterm/tungstenite 等 Git patch；probe 顶层的本地 fork/patch 不自动作用于产品 workspace。独立新 Cargo home 读取同一 vendor 后：

- Codex 产品 metadata 返回 101：固定 crossterm `efa177859fd9623d57b9fe7ae9bf491ae1ac6ec4` 无离线 checkout。
- CC Switch metadata 返回 101：锁要求 `anyhow 1.0.102`，vendor 只有 `1.0.103`；未替换锁版本来绕过失败。
- 本机 Windows 和 `wasm32-unknown-unknown` 的 std 存在。目标安装本身不填补 Wasm 产品清单与源码 slice 的缺失。

真实 native build 的最小后续入口应针对 `codex-cli --bin codex`，固定 Windows target、真实产品 features、独立 target 目录与 digest-bound lock/patch；必须先补固定 Git 和选定闭包的离线依赖。若改依赖，应同时满足固定原 Codex AGENTS 的 Bazel 锁约束。本轮未做该准备或 build。

## G0-C07/C08：成功生命周期与权限架构

004 当前源码和 `receipts/p02-integration-004/bypass-audit.md` 仍明确限定：restricted feature 默认为关闭，正常产品默认分支保留。109 个断言的历史证据是同一 probe 中被选中的拒绝/fixture 场景，不是成功 IPC、socket、OS child 或 durable writer 的资格。

| 当前源码边界 | 仍需真实成功/失败/释放证据 |
|---|---|
| `core/src/client.rs:build_api_transport`、`connect_websocket` | 同一真实 kit 的授权 transport；HTTP/SSE/WS 成功、redirect/auth 策略、reconnect/fallback/cancel 的终态和释放；注入先于原 redirect rejection 的语义差异必须解决 |
| `core/src/realtime_conversation.rs`、直接 API/HTTP client | 统一 runtime authority/backend 所有权；缺注入拒绝不能证明全进程网络被截获 |
| `thread-store/src/live_thread.rs` 与 `thread-store/src/local/mod.rs` | ThreadManager 注入生产 store；writer acquire、append/durable、pending metadata/事务、取消 acquisition、显式 shutdown/discard 与真实 writer release |
| `exec-server/src/local_process.rs`、`core/src/exec.rs`、shell snapshot | 受权真实 child/PTY/IO；wait/Exited/Closed、输出 drain、撤销、进程树和 fd 释放；直接 LocalProcess/较早 shell capture 仍可绕过 Core 的资格 guard |
| `LiveThreadInitGuard` | Drop 中异步清理不等于远端 writer 已释放；本机 Arc 强计数归零不能替代真实资源释放回执 |

这些是本机实现与架构前置，不能列为“等硬件/跨平台就能通过”。必须选定可信 IPC/backend 的 product 所有权，接入真实网络、存储和执行服务，再用同一当前候选重跑双方成功、拒绝、撤销、取消和关闭。Morrow 010 的 Windows Flutter → supervisor → 原 host → Wasmi 成功可继续作为其自身范围的证据；它不更改这些 Codex 入口的资格状态。

## 可以在本机推进的下一片

1. 为产品 workspace 确定固定依赖准备及目标 closure；补全独立原生产品 graph 清单、锁和失败回执。网络依赖获取/安装需要单独授权，本轮没有执行。
2. 为 CC Switch 选定真实纯逻辑 slice 与接口。一个明确的最小来源是 `src-tauri/src/model_capabilities.rs`（291 行，仅依赖 `serde_json::Value`，含 5 个原测试），而不是整个 Tauri crate。建议后续新增 `portable/cc-switch-pure/Cargo.toml`、`Cargo.lock`、`src/lib.rs`、`src/model_capabilities.rs`、`tests/source_parity.rs` 和 digest-bound source receipt；复制源模块字节并保留声明优先与 Unknown 语义，再定义实际产品使用的 JSON ABI。
3. 该 slice 的原锁 `serde_json=1.0.149` 与 probe 相同，但传递版本不同：原 CC Switch `memchr=2.8.0`/`zmij=1.0.21`，probe 为 `2.8.1`/`1.0.19`。必须选择、审查并锁定真实 slice closure；不能静默复用 probe lock。仅生成空 wasm、rlib 或一个未被产品调用的纯模块均不能把 Wasm 图计为通过。本轮仅给出精确候选范围，未创建假目标。
4. 明确 Codex runtime authority、credential、writer、exec 所有权及产品接入路径，之后才能验收 C07/C08。

## 当前环境无法继承的资格

Linux/macOS/Android/Web 需要各自构建和运行证据；本机 Windows 结果不传递。真断电、同用户恶意进程隔离、硬件/设备特性和真实账号 auth recovery 也没有被本轮证明。真实账号、外部服务、系统配置与平台权限测试需专门范围和适当环境。

Unknown 原操作保持待核对，不自动重放。before-proof 故障保持保守拒绝。本轮没有强迫非零 OS 短写、关机、启动新服务、自动启动、CI、安装、提权系统权限或发布操作。

证据入口：[011 follow-up](../reports/codex-morrow-v1.1/g0-followup-011-2026-10-01.md)。旧 `joint/g0-checklist.json` 和 round6 review 只用于合同/历史溯源，没有改写或扩充其历史通过结论。
