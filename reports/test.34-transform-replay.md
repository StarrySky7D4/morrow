# test.34：纯转换历史证据与隔离重放

日期：2026-09-13。应用 `0.1.9-test.34+39`，审计／工作台宿主 test.34；核心 crate 仍为 test.22、内容格式 6，运行时／SDK crate 仍为 test.11，默认工作台 guest 仍为 test.13，随包清单 test.34。第一方 AGPL-3.0-only。本轮仅本地开发，无推送、标签、Release 或上传。

## 实现

新增固定 task_evidence.proto 和 Protobuf 原字节＋LZ4 容器。证据包括实际包 archive、原 Invocation、实际预算、后端 profile、Runner 保留的原 completion、退出码／故障、调用数和剩余 fuel。未知可选字段保留在原始 PB 中；校验使用外部固定的原 PB 摘要，不通过解析后重编码验证。

Pool.record_transform 从当前批准的真实根会话实际执行并捕获，不接受外部 TaskReport 伪装实际捕获。入口复用实例池的身份校验、取消和故障收敛，拒绝内容命令及依赖声明／动态依赖任务。证据不含可恢复的权限或生产句柄。

独立 replay 接口只运行嵌入字节，每次新建 no-WASI Runner，无 HostRuntime／Store／凭据或路径回调。核心 exchange 尝试始终拒绝，忽略错误后完成也不能变为成功。精确比较结果原字节、退出码／故障、调用数和剩余 fuel，预算不得超过可信调用者策略。后端 profile 不同、外部取消／超时／撤权等不支持的确定性重放条件明确拒绝。

新增只读独立工具 `morrow-transform-replay`。必须提供原 PB SHA-256，退出 0 为 MATCH，2 为 MISMATCH，1 为拒绝；不打印输入和输出正文。

此容器有完整性而没有来源签名。格式层 encode 可以编码调用者提供的观察，不能将任意文件及自行提供的 hash 当成宿主执行证明。默认工作台尚未自动存证，内容提交／审计签名原子关联、宿主投影及依赖图材料继续推进。完整接口和下一步设计见 [纯任务证据](../docs/PLUGIN_TASK_EVIDENCE.md)。

## 验证

- 最终 Runtime 全特性 **201 项通过**，全目标严格 Clippy 通过。日志 `build/test34-runtime.log`；包括既有 188 项和新增真实 WAT 重放 13 项。
- 13 项覆盖原 Invocation 逐字节校验、修改固定输入产生真实 Trap、业务失败、完成后 Trap、无 completion、执行额度、忽略 exchange 错误仍失败；成功观察的输出／exit／fuel／调用数被修改，即使重新获得合法格式 hash 也返回不匹配；后端／外部结果／预算拒绝；原包目录失效仍能离线执行但旧 Session 不复活；跨 Manager／Session 与真实撤权不能捕获。
- core 证据模块 **10 项通过**，对应库／测试严格 Clippy 通过。覆盖原字节与未知字段保留、重复字段／线类型／组／长度／压缩上限、错误摘要与缺包、成功与故障结果约束、非页整数实际内存预算及依赖拒绝；4 MiB 模块、128 KiB Invocation 和 64 KiB 输出容量通过。该核心库 `wasm32-unknown-unknown` 编译检查通过，仅证明格式代码编译，未声称浏览器应用或 Wasm 重放运行通过。
- 最终工作台宿主 **Release 全量 24 项通过**，日志 `build/test34-host.log`，包含 1100 次持续创建与审计封存。Flutter／真实宿主集成 **5 项通过**；默认工作台包构建与校验通过，Windows Release 应用构建成功。没有 UI 源码变更，未重复完整 Flutter UI 套件或把旧 analyze 当作本轮结果。
- 三个子代理分别完成格式与恶意输入测试、独立运行时审查测试、真实 Rust SDK 资格。主代理实现捕获／独立 Runner／CLI，并在集成后执行全量 Runtime、宿主、CLI 和 Windows 验证。

## 真实 Rust SDK 与独立进程

实际构建 `sdk/examples/rust-transform` 到 `build/replay-guests/wasm32-unknown-unknown/release/morrow_example_transform.wasm`，模块 SHA-256 为 `2b79ee7512d36d3b31c7712eebb7389bf9ddd202428086e43027a8e73868d874`。

资格示例通过实际 Pool 捕获二进制逆序和 ASCII 输入限制业务失败；零卡片、零事件，池用量归零。随后销毁全部 HostRuntime／Manager／Pool 并删除合成 DB 和包目录，再解码证据、重放，两项精确匹配。此后主代理实际运行独立 Release CLI，两份证据均 MATCH，错误全零摘要返回 Integrity／退出 1。工具不查询原注册表，不恢复旧授权。

| 证据文件 | 原始 PB SHA-256：用于 CLI 固定摘要 |
| --- | --- |
| `build/replay-qualification/rust-transform.morrowevidence` | `1a2494382abed7b52af8596372d0edc1e1d9802f77a1b16351cc691c6bb7d9b5` |
| `build/replay-qualification/rust-transform.failure.morrowevidence` | `7549003bed7b4c00b02efdf10f1df65a4c45a001822d6e0f274af91a9b411137` |

压缩容器文件 SHA-256 分别为 `1de0e68a5043b81585de600c7216052bc5e6f7f12b09d3e11d3989d7588bfded`、`e22c01b09b49169c619eee5876c5e0f83a91bf9b9e05137efc90c729199bfd15`，不能把这两个文件摘要当成 CLI 的原 PB 参数。

独立工具 `build/replay-tool/release/morrow-transform-replay.exe` SHA-256：`b5b27f76b111d9c4616d9a53a0cf4631afd0940565130ece5edf856415cd5bab`。

## 最终 Windows 应用

实际版本 `0.1.9-test.34+39`，自检退出 0，合成资料库活动登记存在。证据目录 `build/workbench-host/test34-final-c02287e28b134904943518db7d8975a0/`。

实际程序通过 Rust 工作台渲染、原生背景 blur 0／1／12／40 API 与关闭、静音 WAV 解码及播放时钟、跳转、播放互斥和恢复不自动播放。背景项是 API 检查，没有桌面像素对比，也不证明默认工作台已开始持久存储任务证据。运行应用须保留整个 Release 目录。

| 包内产物 | SHA-256 |
| --- | --- |
| `build/windows/x64/runner/Release/morrow_studio.exe` | `6e23ba85da17daca932f9d9b8a725bfeca744bec9cdab81a599fc703c40c385e` |
| `build/windows/x64/runner/Release/morrow-workbench-host.exe` | `9d017c7eb7d196e1340f3b1b9bde721d0baa1133fb57e47894c03c5ef21b33c0` |
| `build/windows/x64/runner/Release/plugins/workbench.morrowplugin` | `4c5623fd8f67aedfb6e23f6dd120627e1b3238851e9161dc5cde87b0398aaf46` |

## 复现及剩余门槛

```powershell
cargo test --offline --locked --manifest-path core/Cargo.toml --test task_evidence --target-dir build/evidence-core
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --all-features --target-dir build/dependency-parent
cargo build --offline --locked --manifest-path sdk/examples/rust-transform/Cargo.toml --target wasm32-unknown-unknown --release --target-dir build/replay-guests
cargo run --offline --locked --manifest-path plugin_runtime/Cargo.toml --features packages --example qualify_transform_replay --target-dir build/replay-qualification -- build/replay-guests/wasm32-unknown-unknown/release/morrow_example_transform.wasm build/replay-qualification/new-capture.morrowevidence
build/replay-tool/release/morrow-transform-replay.exe build/replay-qualification/rust-transform.morrowevidence 1a2494382abed7b52af8596372d0edc1e1d9802f77a1b16351cc691c6bb7d9b5
```

资格示例以 create_new 写文件，重复执行须选择新输出文件名。新的任务内容或包变化会产生新的原始 PB 摘要，应使用当次实际输出。

下一步将捕获证据保留到现有 Store::apply 的原子提交中，关联内容、操作、原始 Commit 与待封存事件，并扩展完整性检查／快照／导出。多层 A/B 图与宿主响应录制、投影重建、保留及清理状态、配额／崩溃恢复、默认多包 UI、异步调度与全平台验收仍未完成。M3–M7 与 0.2.0 门槛保持未完成，本阶段为纯任务 Windows 原生后端 PASS_SCOPED。
