# test.33：默认工作台实例池接入

日期：2026-09-13。应用 `0.1.9-test.33+38`，审计／工作台宿主 test.33；核心 crate 仍为 test.22、内容格式 6，运行时／SDK crate 仍为 test.11，默认工作台 guest 仍为 test.13，随包清单 test.33。第一方 AGPL-3.0-only。仅本地开发，无推送、标签、Release 或上传。

## 实际应用接入

默认 Windows 工作台不再独立持有 ManagedInstance，而由绑定同一 HostRuntime／Manager 的 Pool 和 Session 管理根连接。初始化从当前批准状态启动，普通内容、捕获和设置服务任务通过 Pool.run_task；短期对象授权／撤销、持久提交和在线表单均绑定实际根连接，没有可变连接交给 guest。

Pool.run_task 维护实例前后状态，错误 handler／注册输入上限先拒绝，不把宿主请求错误归为 guest 故障。真实陷阱、任务协议错误及执行限额停止该根；普通业务失败不会隔离健康实例。失效后的纯转换输出不再交付，真实命令响应仍保留；提交后发生陷阱不会回滚内容，也不自动重试任务。

在线表单沿用已有同步 InlineUi 执行机制，由工作台观察实际执行失败并停止同一根、关闭表单和维护池。普通命令造成根失效后也关闭已有表单，显示只读与显式重新启用提示。错误用户事件、业务拒绝和外部超时不据此隔离健康根。配置仍先检查 revision／摘要，旧请求不关闭当前会话；显式启用建立新根和新表单代次。

finish 先关闭在线表单和池连接，再尝试待封存审计写入，即使关闭失败也不省略封存尝试。结束会话后原内容、附件读取和导出仍可用；再次打开工作台从持久批准状态建立新会话。Drop 仅尽力清理实例，不能替代 finish 的封存结果。

本阶段是默认工作台根生命周期接入，不等于默认界面已支持多包安装、依赖选择或图结果提案。没有改变持久格式、运行期契约、guest ABI 或获批 unsafe 范围。

## 验证与独立审查

- 最终 Runtime 全特性 **188 项通过**，全目标严格 Clippy 通过，日志 `build/test33-runtime.log`。新增普通任务 10 项覆盖真实任务字节、真实 Summary 回执、按根撤权、跨 Host／Manager／Session 拒绝、业务失败、错误 handler／超输入、真实 dispatch 时钟撤权、陷阱／协议／fuel 故障，以及提交后陷阱不回滚。普通根和共享提供者即使包 ID 相同，也以真实连接隔离故障。
- 新增实际宿主集成 **6 项通过**：真实 Rust 内容、附件、在线表单与 CAS；停用持久及旧代次；finish 幂等、只读和重开；真实普通 Trap、无 completion TaskProtocol、UI Trap。最终宿主全量 **24 项通过**，全目标严格 Clippy 通过，日志 `build/test33-host-final.log`；其中包含这 6 项，并非另加 6 项计数。
- 核心包与依赖四组 **44 项通过**，对应严格 Clippy 通过。新增同一 Registry 在先前解析成功后，根／必需提供者包被篡改、换为另一合法包或移除时，每次新解析仍拒绝；只有恢复精确原字节才重新成功，批准修订与持久状态不变。
- 最终真实 Flutter／宿主集成 **5 项通过**；工作台包重新构建校验、Windows Release 编译成功。没有 UI 源码变更，未把旧 Flutter analyze 结果作为本轮验证。
- 最终真实 Rust A／M／B 实例池资格再次通过，仍验证双根共享、关闭一个根、停止叶节点、旧提案拒绝、新绑定重新授权、仅显式写入及最终资源归零。Wasm 文件与 SHA-256 沿用 [test.32记录](test.32-instance-pool.md)，本轮实际读取这三个文件执行。

三个子代理分别完成默认宿主接线、普通任务测试、宿主独立测试与审查。审查暴露的在线表单故障漏处理已修正并实测；后续补齐普通任务失败后的表单关闭与只读提示。

## 持续写入与校验开销

接入发现每个普通任务前后维护状态时，会多次读取、解压和校验整个包。Registry.resolve_enabled 的同一次遍历先加载根，返回前又重复加载相同根。已仅消除这次重复读取，复用同次遍历中验证过的不可变 Package；不跨调用缓存，不依赖 mtime，也不移除必需边检查。每次新解析及调用前后的维护仍重新验证磁盘内容。这不是原子磁盘快照，也不保证返回后外部文件不再变化。

本轮优化前 debug 审计组 3 项通过，耗时 476.30 秒；优化后同组 3 项通过，耗时 222.32 秒。组中耗时主要来自 1100 次创建并穿过自动封存容量边界。两次运行有其他编译／测试并行，不是严格控制变量的性能基准；只能结合代码确认重复加载已减少，不能将耗时比宣传为普遍提速。debug 仍明显慢于接入实例池前的历史记录，每任务全包解析／校验开销保留为后续性能工作。

最终 Release 单独运行 `continuous_edits_cross_old_capacity_and_close_seals_the_tail`，1100 次创建、自动封存与退出尾段封存全部通过，测试耗时 **13.98 秒**，编译时间不计入。这不是六平台吞吐量、交互延迟或十万卡片规模验收。日志分别为 `build/test33-host-baseline.log`、`build/test33-host-final.log`、`build/test33-release-writes.log`。

## Windows 最终产物

实际应用版本 `0.1.9-test.33+38`，退出码 0，合成资料库活动登记存在。证据目录：`build/workbench-host/test33-final-4d76f367e1b740049135f2f96cae722a/`。

实际 Release 自检通过 Rust 工作台渲染、原生背景 blur 0／1／12／40 API 接受和关闭、静音 WAV 解码与时钟、跳转、声音互斥和恢复不自动播放。背景项为 API 检查，未重做桌面像素对比。应用运行须保留整个 Release 目录。

| 产物 | SHA-256 |
| --- | --- |
| `build/windows/x64/runner/Release/morrow_studio.exe` | `5f5e1c4ce0b75449709fd2a5cd4b8bb3a6a2fadc41bca183a62b08d5157d987e` |
| `build/windows/x64/runner/Release/morrow-workbench-host.exe` | `2a210300763c913bc1f9e89a185409908e4a3e705a8140d5274f77c889d1eb2f` |
| 包内 `plugins/workbench.morrowplugin` | `b4d58618094ce684dc4388d531a74594d8c4d9ad8510572ab27f66dc3dbd078c` |

## 复现与后续门槛

```powershell
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --all-features --target-dir build/dependency-parent
$env:MORROW_WORKBENCH_WASM = Join-Path (Get-Location).Path 'build/first-party-plugins/wasm32-unknown-unknown/release/morrow_workbench_plugin.wasm'
cargo test --offline --locked --manifest-path workbench_host/Cargo.toml --target-dir build/host-pool-review
cargo test --offline --locked --manifest-path workbench_host/Cargo.toml --release --target-dir build/workbench-host --test auditing continuous_edits_cross_old_capacity_and_close_seals_the_tail -- --exact
```

继续推进默认多包协作与恢复 UI、跨进程任务、异步图调度／完整等待环、服务接口、持久恢复预算、完整协作审计和隔离重放、其他平台接入以及规模性能。M3–M7 与 0.2.0 退出门槛保持未完成，不能将根会话接入等同于完整插件系统完成。
