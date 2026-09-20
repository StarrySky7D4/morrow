# Package 执行状态与 worker 的逐 import 驱动

2026-09-21；基线 `4d88041`。应用版本仍为 `0.1.9-test.52+56`；本轮为源码改造，未重建 Windows 发布产物、未推送或发布版本。

## 接入结果

`PreparedPackage` 新增内部 `FrameExecution`，持有自有的 Runner continuation、内容路由模式和既有协议错误状态；没有 package、输入、宿主或回调借用。package 与原输入变量释放后，执行仍可恢复。

真实 `io_jobs::run_job` 已从“一次调用 package 并长期持有路由闭包”改为显式驱动每次 import：读取待处理调用及原 token，在本次路由内借用宿主，然后恢复同一 guest 调用。取消、额度、资源授权与路由代码的原顺序保持不变。

最终完成帧的绑定、服务响应解码、字节收费、持久完成和 Unknown 处理仍留在原 worker，没有建立第二套校验或存储路径。移除了只供这一内部路径使用的 `run_service_frame` 与 RefCell 回调适配；外部公开 API、Schema 和 SDK 不变。

固定资源/文件适配器同样使用这套状态，但禁止的核心 import 会逐个返回 `-1`，不会泄漏给 IO 回调。任何一次禁止的核心调用都会使最终结果保持 TaskProtocol，并清除完成内容，保留原来优先于后来取消的错误语义。

## 新增验证

四项直接覆盖包层边界的测试通过：

1. 连续两次禁止的核心调用后再发 IO：IO 回调仅执行一次，guest 总调用数为三，最终 TaskProtocol 且无完成内容。
2. 释放 package 与原输入后，服务执行仍按 Core、Core、IO 顺序恢复，保留原输入并正常完成。
3. 已拒绝核心调用后，在 IO 暂停期取消，原 TaskProtocol 不被覆盖为取消成功。
4. 普通未声明 IO 的包不能进入服务 frame，guest 不运行且不消耗 fuel。

`build/frame-worker-unit.log` 为这四项测试的单独日志；全量验证结果见下方。它们验证内部状态边界，不代表 HTTP 等待期间的应用命令并发已实现。

## 全量回归

运行时含故障注入的全量回归 **489 项通过，0 失败**；两个 ignored 入口是由父测试带隔离数据库及故障点启动的子进程测试，父测试均通过。该数量包含新增四项与冻结 C/C++/Rust 原包验证，不重复累计前轮结果。严格 Clippy 通过。

真实网络链路另 **33 项通过**：受管 HTTP 18 项、内容服务 6 项、原 owner 服务停止/回收 9 项。

```powershell
cargo test --manifest-path plugin_runtime/Cargo.toml --target-dir ../integrate-track-a/build/io-codec --features fault-injection
cargo clippy --manifest-path plugin_runtime/Cargo.toml --target-dir ../integrate-track-a/build/io-codec --features fault-injection --lib --tests -- -D warnings
cargo test --manifest-path network_node/Cargo.toml --target-dir ../integrate-track-a/build/io-codec --features plugin-adapter --test managed_http --test managed_service_owned --test managed_content_service
rustfmt --edition 2024 --config skip_children=true --check plugin_runtime/src/package.rs plugin_runtime/src/package/frame.rs plugin_runtime/src/package/frame/tests.rs plugin_runtime/src/io_jobs.rs
```

日志：`build/frame-worker-regression.log`、`build/frame-worker-clippy.log`、`build/frame-worker-network.log`；初次定向检查为 `build/frame-worker-initial.log`。本轮没有浏览器、真实系统输入或应用新构建验收。

## 辅助编码与主审核

GLM max 提供包执行状态候选，DeepSeek max 编写连续拒绝核心调用的测试。主代理修正候选并接入 worker，补充其余三项边界测试和回归验证。

| 任务 | 模型 | 插件记账输入/输出 tokens |
| --- | --- | --- |
| `task_a21161f5830040b4633723d0` 初稿 | GLM max | 817 / 1204 |
| `task_08744cfbe9429995a1e5e569` 修订稿 | GLM max | 542 / 1399 |
| `task_30e783c5a3458eef31fd696b` 连续核心调用拒绝测试 | DeepSeek max | 342 / 148 |

审核修正了错误的类型路径、缺失的泛型、虚构的 callback 方法、只拒绝第一个 Core 的路径，以及在 release 构建中静默忽略内部错误的写法。采用循环拒绝，并从第一次借用中克隆 token；不能在可能并发取消的第二次 pending 查询上 unwrap。候选中的“允许 Core 路由”也不被当作授予内容权限。

## 下一步

当前 import 路由仍同步调用原 `BrokerRouter`，HTTP 的 `block_on` 仍会占用 worker。下一步需要把 `RouteContext` 和 HTTP 执行拆成受管等待任务，让 worker 保存当前 frame/任务核验状态，在等待期间处理原 owner 命令，完成后回原 owner 保存观察、复核权限并恢复 guest。

停止、撤权、截止、观察 Future 被取消及实际后台退出必须分别处理；等待和未读结果的累计额度不能重置，后台未退出不能报告回收。没有完成这条接线与真实慢 HTTP 验收之前，仍不能声明工作台已支持并行处理网络等待与普通业务。
