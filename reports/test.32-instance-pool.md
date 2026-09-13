# test.32：自动激活、共享实例与显式恢复

日期：2026-09-13。应用 `0.1.9-test.32+37`，审计／工作台宿主 test.32；核心 crate 仍为 test.22、内容格式 6，运行时／SDK crate 仍为 test.11，默认工作台 guest 仍为 test.13，随包清单 test.32。第一方 AGPL-3.0-only。仅本地开发，无推送、标签、Release 或上传。

## 实现与边界

Manager 新增只读激活计划，先检查当前 revision，再解析已批准必需闭包与宿主明确选入的可选 slot，固定包摘要和必需边。最多 64 个包，按必需提供者先于消费者排序；没有开启插件、修改批准或恢复旧对象权限的副作用。

Pool 绑定实际 HostRuntime 与首次成功激活的 Manager。每个会话使用独立根连接，非根提供者在池中共享。启动失败释放本次部分创建的连接；关闭一个根不会停止仍被其他会话使用的提供者。Session Drop 立即撤销根，后续 maintain 清理宿主记录；Pool Drop 无法替外部持有的 HostRuntime 断开记录，宿主结束前须显式 close_all。

多层执行器向内部监督路径记录实际故障节点的连接身份。Wasm trap、任务协议错误和 guest 执行限额会停止该实例，必需消费者随之失效；可选子树故障不直接停止独立根。外部取消、业务失败或图策略超限不据此隔离健康实例。执行期间的直接撤权在 run 返回前完成传播与清理。

显式 restart 重新检查当前批准图，创建新的会话和连接，不恢复旧对象 grant，不重放任务，也不自动提交。默认每条恢复链最多 3 次尝试、相邻尝试间隔 1 个宿主时钟单位，硬上限 8 次；准备失败计入已接纳尝试，旧 revision 请求不消耗预算。恢复成功后继续保留原尝试计数。默认／硬上限均为 16 个会话记录、64 个提供者；失效但仍被持有的恢复句柄占用会话额度。

这里的激活是准备真实代码和建立 Ready 连接，不是启动独立 OS 进程或后台 Worker。默认工作台尚未接入实例池。接口、释放责任与限制见 [实例池设计](../docs/PLUGIN_INSTANCE_POOL.md)。没有扩大已批准的 shared_memory.rs unsafe 范围。

## 最终验证

- 全特性 Runtime 全量 **178 项通过**，日志 `build/test32-runtime.log`；包含原有 152 项、激活计划 8 项、实例池 18 项。最终全特性／全目标严格 Clippy 通过。
- 激活计划覆盖必需 diamond 顺序、显式可选闭包与可选环、64 包允许／65 包拒绝、revision 优先、旧计划快照及只读无副作用。
- 实例池覆盖共享与独立授权、Drop／close／Pool Drop 撤权、同 Host 不同 Manager 拒绝、外部 Host 容量耗尽后的部分启动清理、必需和可选失效、实际叶／根 Wasm 陷阱、恢复次数和冷却、新对象授权、旧结果不可恢复、取消及图策略拒绝、外部已断开记录与执行期间直接撤权。
- 宿主 **18 项通过**；Flutter／真实宿主集成 **5 项通过**。工作台包重新构建并验证，Windows Release 编译成功。本轮没有 UI 源码改动，未重复完整 Flutter UI 套件或把旧 analyze 结果当作本轮证据。
- 子代理独立审查、测试和实际三包资格与主代理生产实现并行完成。最终集成后由主代理再次运行完整 Runtime 和真实三包资格。

审查修复了四个问题：同 Host 不同 Manager 原先可能混用池；共享提供者故障原先可能误停同名包的独立根；外部已断开的实例原先可能阻塞维护；仅在执行陷阱后维护原先会延迟直接撤权的消费者清理。上述情形均有最终回归。开发期间的缺失新入口、借用冲突和可折叠 if 警告已修复，没有以放宽 unsafe 或隐藏该警告绕过检查。

## 实际 Rust 三包资格

`qualify_instance_pool` 使用已有独立 Rust A／M／B Wasm 文件，实际执行 `abc` → `A[M[B:CBA]]`。两个根会话共享中间和叶提供者；关闭第一个根后，第二个仍能执行并提交。保留另一份未提交结果后停止叶实例，旧结果在维护前即失效，维护后旧根会话不可用。

显式恢复建立新根／中间／叶绑定，包清单、批准锁和 Manager revision 未变。旧提案仍拒绝；新根没有旧 ReadContent／EditContent 对象授权，须明确重新授予。内容只有种子和两次显式修改，无自动任务重放。结束时池和共享对象用量归零，重开 Store 内容一致。最终源码实测结果为 **PASS_SCOPED**，不是 OS 进程崩溃恢复证明。

复用的实际 Wasm：

| 模块 | 路径 | SHA-256 |
| --- | --- | --- |
| Rust 根 A | `build/chain-guests/wasm32-unknown-unknown/release/morrow_example_dependency_caller.wasm` | `8d27dde13213dd33df242a5ad048cc500aac644140d2071004532ed80732920e` |
| Rust 中间 M | `build/graph-guests/wasm32-unknown-unknown/release/morrow_example_graph_middle.wasm` | `cd4ea09383df499ccab6e0a6568d7b14f0b4f99bb68a99532920377a0ef0c0f1` |
| Rust 叶 B | `build/chain-guests/wasm32-unknown-unknown/release/morrow_example_chain_provider.wasm` | `1a53d73ecc1e6720f47893c3d0fcecd544133762254689cedcecfa5073ef9c91` |

## Windows 最终产物

实际 Release 程序产品版本为 `0.1.9-test.32+37`，退出码 0，独立合成资料库的活动库登记存在。证据目录：`build/workbench-host/test32-final-94a9d103a9e4437aa30fd8e5c08266e9/`。

自检通过实际 Rust 工作台渲染、Windows 原生背景 blur 0／1／12／40 API 接受与关闭、静音 WAV 解码与时钟、跳转、播放互斥和恢复不自动播放。背景检查是 API 验证，没有进行桌面像素对比，也未证明默认 UI 已接入共享池。

| 产物 | SHA-256 |
| --- | --- |
| `build/windows/x64/runner/Release/morrow_studio.exe` | `40021f9589efd4201276bf058d5efcb3690ff42e25bfa0ff5079a2bb07d50c27` |
| 包内及 `build/workbench-host/release/morrow-workbench-host.exe` | `c9fdf2b5a934bd3834725cc9f274b8c6021b4a464d34952fe03ec0b3db94169f` |
| 包内 `plugins/workbench.morrowplugin` | `cda52318760446e69d739ac09938ddea9102a20112ec0f65114a500d275224a9` |

运行应用须保留完整 Release 目录，不能单独复制主 exe。

## 复现与剩余任务

```powershell
cargo clippy --offline --locked --manifest-path plugin_runtime/Cargo.toml --all-features --all-targets --target-dir build/dependency-parent -- -D warnings
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --all-features --target-dir build/dependency-parent
cargo run --offline --locked --manifest-path plugin_runtime/Cargo.toml --features packages --example qualify_instance_pool --target-dir build/pool-qualification -- build/chain-guests/wasm32-unknown-unknown/release/morrow_example_dependency_caller.wasm build/graph-guests/wasm32-unknown-unknown/release/morrow_example_graph_middle.wasm build/chain-guests/wasm32-unknown-unknown/release/morrow_example_chain_provider.wasm
```

OS 进程崩溃检测、跨进程任务运行、异步多 Worker 图与等待环、后台重启调度、持久恢复预算、默认多包 UI、通用服务接口、完整协作审计／证据封存和隔离重放、其他平台接入仍待完成。M3–M7 与 0.2.0 退出门槛继续保持未完成，不以本阶段测试数量替代系统验收。
