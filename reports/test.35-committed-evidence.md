# test.35：任务证据与内容原子提交

日期：2026-09-13。应用 `0.1.9-test.35+40`，审计／工作台宿主 test.35；核心 crate 仍为 test.22，内容存储格式由 6 升至 7；运行时／SDK crate 仍为 test.11，默认工作台 guest 仍为 test.13，随包清单 test.35。第一方 AGPL-3.0-only。仅本地开发，无推送、标签、Release 或上传。

## 实现与边界

新增宿主 `create_content_with_evidence`、`edit_content_with_evidence` 和带实时 guard 的编辑入口。实际内容、操作回执、原始 Commit、待封存事件、证据原件及有序引用由同一 SQLite 事务写入；最后授权检查仍在提交之前。新入口保留原有包权限上限、连接身份、范围、撤权及到期检查，历史证据不恢复旧授权。

Commit 的 tag 10 保存有序原始证据 PB 的 SHA-256。无证据仍编码为原有 schema 1，字节保持不变；有证据使用 schema 2。原件按摘要去重，但重复引用的顺序和位置保留。幂等重试必须匹配原命令、全部有序摘要和原始证据；不能用重新执行后变化的观察替代旧操作的证据。

新表 `task_evidence` 与 `operation_evidence` 随格式 6→7 原子迁移。格式 4／5 经已有迁移阶段进入 6，再进入 7；每个阶段原子提交，不宣称整个多阶段迁移只有一个事务。审计只读入口接受 5／6／7。旧核心不支持格式 7，不提供降级写入。签名校验仍覆盖原始 Commit 字节，而不解析重编码。

每个操作最多 16 个证据位置，原始 PB 加容器合计最多 64 MiB，重复位置仍计入预算。读取检查有序位置、摘要、容器上限及原件，超大 BLOB 在 SQL 查询中先限长。完整性检查拒绝缺失、篡改、多余和孤立原件；新提交不能悄悄收编既有孤立原件。快照先在固定读取事务中验证完整性，再创建目标。

`Store::operation_evidence(card_id, operation_id)` 是宿主本地历史读取接口；错卡、缺失操作或非内容操作均返回 NotFound，没有新增 guest 任意读取接口。默认工作台尚未自动调用任务捕获与这些提交入口；此阶段完成核心原子接口和真实 Rust 资格链路，未完成默认 UI 自动存证、宿主投影重建或完整协作重放。

## 验证

- 核心 Release 全量、启用 `fault-injection`：225 项通过，`--list` 交叉核对 225；全目标严格 Clippy 通过。日志 `build/test35-core.log`、`build/test35-core-list.log`。随后审查新增一项超大操作记录测试；最终 `operation_evidence` 整组 14 项通过（13 个实质场景、1 个子进程入口），对应严格 Clippy 通过。14 项包含先前全量里的 13 项，不能重复相加。
- 新组覆盖创建／编辑真实子进程在写证据后、提交前、提交后崩溃，6→7 迁移前后崩溃，晚到撤权／到期、幂等与有序重复引用、缺失／篡改、16 项及实际 64 MiB 预算、Commit schema 非法矩阵、旧编码不变和孤立证据拒绝。
- 审计 Release 全量、启用 `fault-injection`：57 项通过，独立 `--list` 也是 57；全目标严格 Clippy 通过。日志 `build/test35-audit.log`、`build/test35-audit-list.log`。未重复计入嵌套子进程输出。
- Runtime 全特性 201 项通过，独立清单为 9 个 unit 与 192 个 integration；全目标严格 Clippy 通过。日志 `build/test35-runtime.log`、`build/test35-runtime-list.log`。测试仍覆盖限定 `shared_memory.rs` 的 Windows 只读映射边界，本轮没有扩展 unsafe 范围。
- 工作台宿主 Release 全量 24 项通过，其中持续 1100 次创建与审计封存所在组耗时 14.01 秒；不是严格性能基准。真实 Flutter／宿主集成 5 项通过。日志 `build/test35-host.log`、`build/test35-flutter.log`。没有 UI 源码变更，未重复完整 UI 套件。
- 核心普通 `wasm32-unknown-unknown` 和 `web-storage` 配置均编译通过；这不是浏览器运行、Web 数据库迁移或其他平台验收。

三个子代理分别完成存储实现与文档、独立故障测试与审查、真实 Rust 资格与 Runtime 回归；主代理完成提交契约／授权接口、集成修正与 Windows 验证。

## 真实 Rust 任务与签名证据

`plugin_runtime/examples/qualify_committed_evidence.rs` 实际通过 Pool 捕获 Rust SDK 的 bytes.reverse。无 CreateContent 授权时拒绝且不产生内容／事件／原件；获得范围授权后产生唯一 schema 2 提交，引用与原件准确一致，重复提交返回同一回执。合成固定签名覆盖原始 Commit 中的证据引用；关闭全部宿主／实例池、删除原库及插件目录后，从独立只读快照读取证据仍精确重放 MATCH。

最新资格实际执行 PASS_SCOPED，日志 `build/test35-committed-evidence.log`。使用公开合成签名密钥与新建测试目录，没有访问生产资料库或声称完成生产密钥验收。实际 Rust 模块 SHA-256 为 `2b79ee7512d36d3b31c7712eebb7389bf9ddd202428086e43027a8e73868d874`。

```powershell
cargo test --offline --locked --release --manifest-path core/Cargo.toml --features fault-injection --target-dir build/evidence-store-full
cargo test --offline --locked --release --manifest-path audit/Cargo.toml --features fault-injection --target-dir build/evidence-audit
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --all-features --target-dir build/dependency-parent
cargo run --offline --locked --manifest-path plugin_runtime/Cargo.toml --features packages --example qualify_committed_evidence --target-dir build/committed-evidence -- build/replay-guests/wasm32-unknown-unknown/release/morrow_example_transform.wasm
```

## 本轮修正与剩余门槛

旧迁移测试将新建格式 7 库伪装为格式 4／5 时遗漏删除新证据表，导致完整性检查正确拒绝；已修正三份合成测试夹具，未放宽生产迁移验证。测试临时调用私有容器函数的问题改用公开格式构造夹具。新历史读取入口经独立审查补上 SQL CASE 限长；首次编译暴露 usize 不能作为 SQLite ToSql 参数，固定上限转为 i64 后 14 项与严格 Clippy 均通过，并重新构建 Windows 产物。

后续首先将默认工作台的捕获原件保留至内容提交，并设计重试时复用同一观察；随后推进宿主投影与多层依赖图证据、默认多包 UI、全局证据配额／保留／GC、异步调度和故障恢复、独立检查点及六平台实际验收。M3–M7 与 0.2.0 退出门槛仍未完成，本阶段为 Windows 核心提交关联 PASS_SCOPED。

## 最终 Windows 产物

实际应用版本 `0.1.9-test.35+40`，自检退出 0，全新合成资料库活动登记存在。证据目录 `build/workbench-host/test35-final-d97c1685cd2a4a06a5c67d23bc91045f/`；结构化结果 `build/test35-final-verification.json`。最终源码在 SQL 限长修正后重新构建，日志 `build/test35-bundle-final.log` 与 `build/test35-windows-final.log`。

实际 Release 通过 Rust 工作台渲染、原生背景 blur 0／1／12／40 API 与关闭、静音 WAV 解码及时钟、跳转、播放互斥和恢复不自动播放。背景项为 API 验证，没有桌面像素对比。应用运行须保留整个 Release 目录。此自检不证明默认 UI 已自动保存任务证据。

| 最终包内产物 | SHA-256 |
| --- | --- |
| `build/windows/x64/runner/Release/morrow_studio.exe` | `72f0ba9866c28c99066dc3235077ef1e2412d6118238fcd2eab5d5f90d53c546` |
| `build/windows/x64/runner/Release/morrow-workbench-host.exe` | `d2cc0cbb6295e707728e1bf57b70e6bd8ce0477a22fe79a541937267c29cb0ee` |
| `build/windows/x64/runner/Release/plugins/workbench.morrowplugin` | `e8813f140d78dbea64dfd2d742908b7a150158e7178585f6d9ca2eeef6411c1d` |
