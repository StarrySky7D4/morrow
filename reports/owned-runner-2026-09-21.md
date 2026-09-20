# Runner 自有执行状态与旧接口兼容

2026-09-21；改造基线 `67e8162`，应用版本仍为 `0.1.9-test.52+56`。这是实际 Runner 源码改造，未重建 Windows 发布产物、未推送或发布版本。

## 已改变的行为

`plugin_runtime/src/lib.rs` 的 `State` 不再包含宿主回调或输入借用。任务输入、限额、取消状态及有界待处理请求由执行状态持有。核心、依赖和 IO import 在验证内存范围、顺序与累计调用额度后保存请求，由私有 `continuation::Execution` 保留同一 Wasm Store 与 continuation。

现有 `Runner::run*` 在 Store 外调用原有宿主回调，再把结果送回原 import 的返回位置；不重新调用 guest 入口，不重置 fuel 或调用计数。核心/IO 传输失败仍返回 `-1`，依赖调用失败仍是 `TaskProtocol`；空响应、超限响应、任务完成顺序和完成后的 trap/cancellation 继续按原规则处理。

每个恢复结果绑定进程内不可伪造的执行身份与调用序号。外来、重复和旧序号响应不能消费当前 continuation；取消/截止发生在暂停期时，不向 guest 内存写入迟到响应。身份只是内部关联，不是插件授权凭据。

没有新增 unsafe、公开 API、guest Schema 或 SDK 类型。旧 `morrow_io_v1.call` 字节契约保持不变。

## 验证

新增八项直接使用真实 Runner 内部执行状态的测试：

1. 原 Runner 和输入变量释放后仍可继续执行，核心与 IO 两次恢复保留局部变量；同一入口执行一次、最终完成 `done`。
2. 外来、旧序号和重复响应被拒绝；正确响应仍可恢复。
3. 暂停期取消与到期阻止已就绪字节写入，并丢弃 continuation 与任务完成结果。
4. 派发前取消不进入回调；未获回复的任务不能伪造成功完成。
5. 核心/IO 共用同一调用额度，恢复后的无限循环仍耗尽原 fuel，不自动补给或复活。
6. 现有同步驱动按原顺序调用核心/IO，结果与完成内容保持一致。
7. 已发生的边界错误不被随后的取消覆盖，保留原同步路径的错误优先级。
8. 真实 Runner 停在 IO import 时，原 HostRuntime 可提交卡片，恢复后关闭并重开同一数据库验证内容。

第八项测试证明实际执行状态不持有原宿主借用，不是受管 HTTP 等待期间的工作台调度验收。前一轮的 TCP 探针也不能替代这项生产集成门槛。

完整 `plugin_runtime` 回归 **476 项通过，0 失败、0 忽略**（包含本轮八项，不再重复累计初次定向测试）。其中包含冻结 C/C++/Rust 原包的任务、转换、UI 契约及依赖路由检查；基线 loader 校验原有 36 文件、13 包对，不重新构建或重新封签旧包。严格 Clippy 通过。最终审核补回了边界错误的优先级，并以第七项测试约束；其后完整回归与 Clippy 均重新通过。

```powershell
cargo test --manifest-path plugin_runtime/Cargo.toml --target-dir ../integrate-track-a/build/io-codec --features packages
cargo clippy --manifest-path plugin_runtime/Cargo.toml --target-dir ../integrate-track-a/build/io-codec --features packages --lib --tests -- -D warnings
rustfmt --edition 2024 --config skip_children=true --check plugin_runtime/src/lib.rs plugin_runtime/src/continuation.rs plugin_runtime/src/continuation/tests.rs
```

日志：`build/owned-runner-regression.log`、`build/owned-runner-clippy.log`；初次定向与内部状态验证见 `build/owned-runner-initial.log`、`build/owned-runner-unit.log`。

SubagentBridge `glm-5.3-flash / max` 生成双 import 的 WAT 夹具，任务 `task_a4358799525adfd5699d370e`，插件记账输入 362、输出 819 tokens。主代理审核内存区间和返回值检查，编写执行状态与恢复验证并接入旧驱动；未将任务执行成功当作候选正确性证明。

真实网络链路另验 **33 项通过**：受管 HTTP 18 项、内容服务 6 项、原 owner 服务停止/回收 9 项。包含实际回环 HTTP、TLS 根/主机名校验、七种批准方法及原宿主回收；这验证现有同步链路回归，不是新的可暂停网络调度。

```powershell
cargo test --manifest-path network_node/Cargo.toml --target-dir ../integrate-track-a/build/io-codec --features plugin-adapter --test managed_http --test managed_service_owned --test managed_content_service
cargo check --manifest-path plugin_runtime/Cargo.toml --target-dir ../integrate-track-a/build/io-codec --target wasm32-unknown-unknown --no-default-features --lib
```

`wasm32-unknown-unknown` 库目标检查通过，日志 `build/owned-runner-wasm-check.log`；不等于浏览器执行或 Flutter Web 集成。网络回归日志 `build/owned-runner-network.log`。

## 下一步与边界

公开驱动仍同步执行原回调，`BrokerRouter::route` / `RouteContext::dispatch` 和 HTTP 的 `block_on` 仍会占用 worker。本轮没有解决慢可信回调的强制中断、工作台同时处理命令、应用 TLS/出站资源、完整文件系统或新 IO SDK 稳定性。

下一项拆分 `Broker::dispatch_checked`：原 owner 完成唯一认领及持久发送边界；等待任务仅持有有界请求与真实存活 reservation；结果回到原 owner 保存原件、记入观察并复核交付。必须保留“提交回执不明不发送、发送边界后不自动重发、撤权仍可保留观察但不得交付、等待和未读 Ready 期间不释放额度”。

之后把 package 的完成帧/原身份核验和 worker 的取消、服务权限、累计预算、普通命令排序接到这一执行状态，才可以声明受管 IO 等待不再占用原宿主。旧原包通过并不代表这些新调度能力已完成。
