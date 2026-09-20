# 慢拥有者回调期间的服务停止

2026-09-21；基线 `628fe2d`，版本仍为 `0.1.9-test.52+56`。本轮只修改测试与方案，不改变生产协议、SDK 或构建。

## 实际验证

`network_node/tests/managed_service_owned.rs` 新增一个测试入口，覆盖正常返回和回调 panic 两种情况。使用真实 Store、Manager、IoWorker、ServiceHost 和回环监听器。测试专用 CommandOwner 在临时目录追加一个字节并 sync_all，随后发出已执行信号、等待显式门闩释放；不是用等待时间猜测操作已经开始。

第一条命令已执行并阻塞后，再排队第二条命令，随后请求 host 和 listener 停止：

- 停止调用在 500 ms 门槛内返回，新命令被 Closed 拒绝。
- 已开始命令读取为 Unknown；排队命令为 Closed 且 started 为 false。
- 等待真实监听器 join 后端口不能连接，但原 owner 仍不能回收，维护／析构次数保持 0。
- 取消一个真实 pending 的 shutdown 观察 Future，不丢失后续回收入口。
- 显式释放回调后才取得原 owner；正常与 panic 分别保留 Ok 和 Unavailable 执行结果，断连与维护独立成功。
- 临时效果文件始终只有 `[1]`，没有回滚、重放或排队命令的第二次写入；原 owner 只在 exit 被丢弃后析构一次。

此测试验证的是可信拥有者命令回调，**不是 guest 中任意同步外部函数的强制中断**，也没有证明完整主应用／真实第三方 API 的慢请求全部合格。原监听和存储回收经过实际 join，不把超时当成结束证据。

## 验证命令与结果

新增入口独立 **1 项通过**；完整 `managed_service_owned` **9 项通过，0 失败，0 ignored**，包含上述入口。两个路径在一个入口内，不重复计数。

```powershell
cargo test --manifest-path network_node/Cargo.toml --target-dir ../integrate-track-a/build/io-codec --features plugin-adapter --test managed_service_owned
cargo clippy --manifest-path network_node/Cargo.toml --target-dir ../integrate-track-a/build/io-codec --features plugin-adapter --test managed_service_owned -- -D warnings
rustfmt --edition 2024 --check network_node/tests/managed_service_owned.rs
```

严格 Clippy 与改动文件格式检查通过。全依赖 fmt 检查曾发现 `core/tests/evidence_chunks.rs` 等已有格式差异，未据此格式化无关文件。日志为 `build/slow-owner-native.log`、`build/slow-owner-regression.log`、`build/slow-owner-clippy.log`。独立运行出现的 synthetic panic 是主动测试的已捕获分支，不是未处理的宿主崩溃。

## 辅助编码与下一步

通过 SubagentBridge 实际调用 `glm-5.3-flash / max` 一次，任务 `task_8e4ee765e84c3dad461c70da`；插件记录输入 655、输出 918 tokens。候选包含不存在的 API、缺少 mut、重复解包和重复释放门闩，主代理按真实接口修正后编译验证。模型没有直接改写仓库或推送。

停止语义已有本轮证据，但同步回调仍占用唯一执行线程。`network_node/src/managed_http.rs` 虽然周期检查撤权／取消，仍通过 `Handle::block_on` 等待传输；这不等于普通业务可以越过长 IO 执行。下一编码项转为[可暂停 IO 原型与门槛](../docs/PLUGIN_SUSPENDABLE_IO_PLAN.md)，其它故障、系统输入、文件系统、新 IO SDK 与全平台工作保持开放。未重新累计历史窗口测试、未重建生产产物、未推送或发布。
