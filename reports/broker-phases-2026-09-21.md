# Broker 认领、执行与提交阶段拆分

2026-09-21；实现基线 `9bf199f`。应用版本仍为 `0.1.9-test.52+56`，未重建 Windows 发布产物，未推送或发布版本。

## 已实现

`io_execution::Broker` 的原同步入口现在由三个内部阶段组成：

1. **认领**：验证原身份，唯一认领内存记录，核对已保存的请求原件，提交持久发送边界。失败仍按原语义处理，提交回执不明不能进入 backend。
2. **执行**：不可复制的 `DispatchTicket` 自有请求、限额、身份与 reservation；执行前再次检查存活条件，最多调用一次 backend，产出自有的 `DispatchObservation`。这一阶段没有 HostRuntime、Store、Manager 或实例借用。
3. **提交与交付**：结果先核对原 broker、宿主和连接的结构身份，再回原宿主保存原件和 Observed 状态；之后才复核当前交付权限。撤权可以阻止交付，但不能抹掉已取得的观察。

原有四次存活/时钟检查保持原顺序；没有给回调增加 `Copy` 约束。等待及未读结果持有原 reservation，即使 registry 已退役也不会提前释放；清理只移除匹配的 live 记录。发送边界后的失联、弃置或失败继续保持 Unknown，不允许自动重发。

类型和分阶段方法均保持模块私有；公开 `dispatch` 仍同步串接三阶段。本轮没有新增公开 SDK 或 guest 契约，也没有新增 unsafe。

## 验证范围

七项新增测试直接覆盖实际 broker 的阶段边界：

- 将认领结果移入线程，实际 backend 在门闩后等待时，原 HostRuntime 完成内容事务；线程结束后回原 owner 提交，关闭并重开原库验证卡片。
- 实例在结果 Ready 后撤权：仍保存响应原件和 Observed，拒绝 payload 交付。
- 认领后弃置、实际执行后丢失结果：均保留 Unknown，不能再次执行或采样新的派发时钟。
- 错误 broker、宿主或连接：不写入原件、不采样时钟，不把观察提交到错误目标。
- registry 已退役但结果尚未读取：原任务额度仍被占用，处理完原件后才释放。
- 认领后、执行前撤权：不进入 backend，已提交的发送边界仍保留 Unknown。

阶段线程测试使用受控合成 backend；它证明实际 broker 执行状态能脱离原宿主借用，不能替代应用内慢 HTTP 与普通工作台命令并行的验收。

相关运行时/故障注入回归 **117 项通过，0 失败**，包含新增七项。两个 `ignored` 入口是供父测试启动的隔离子进程，父测试 `crash_after_committed_claim_reopens_unknown_without_executing_again` 和 `send_boundary_process_exit_never_resends_and_reconciles` 均通过。严格 Clippy 通过。

网络链路回归另 **33 项通过**：受管 HTTP 18 项、内容服务 6 项、原 owner 服务停止/回收 9 项；这些仍使用公开同步驱动。

```powershell
cargo test --manifest-path plugin_runtime/Cargo.toml --target-dir ../integrate-track-a/build/io-codec --features fault-injection --lib --test io_execution --test io_execution_gates --test io_jobs_brokered --test managed_file_io --test http_io --test service_io --test service_run_broker_budget
cargo clippy --manifest-path plugin_runtime/Cargo.toml --target-dir ../integrate-track-a/build/io-codec --features fault-injection --lib --test io_execution --test io_execution_gates --test io_jobs_brokered --test managed_file_io --test http_io --test service_io --test service_run_broker_budget -- -D warnings
cargo test --manifest-path network_node/Cargo.toml --target-dir ../integrate-track-a/build/io-codec --features plugin-adapter --test managed_http --test managed_service_owned --test managed_content_service
rustfmt --edition 2024 --config skip_children=true --check plugin_runtime/src/io_execution.rs plugin_runtime/src/io_execution/phase_tests.rs
```

日志：`build/broker-phases-regression.log`、`build/broker-phases-clippy.log`、`build/broker-phases-network.log`；新增测试单独验证为 `build/broker-phases-unit.log`。没有把前轮 476 项回归重复计入本轮。

## 辅助编码与审核

GLM max 提供主要阶段拆分和两个测试候选；DeepSeek max 提供弃置测试和补充审查。主代理负责接口设计、候选修正、另外四项边界测试和集成验证。

| 任务 | 模型 | 插件记账输入/输出 tokens |
| --- | --- | --- |
| `task_e2d358f9b22e1520677e199f` 阶段代码 | GLM max | 1642 / 1837 |
| `task_f6cce48ae81f63cfc490d49e` 测试候选 | GLM max | 573 / 690 |
| `task_226db83aee9d35591da817ac` 弃置测试 | DeepSeek max | 463 / 254 |
| `task_b58f6fe4ca764c0296fab776` 补充审查 | DeepSeek max | 2461 / 824 |

候选中的虚构 Registry/锁定义、非 Copy binding 移动和扩大可见性已移除；测试中的错误元组顺序、门闩位置、身份类型及枚举假设已修正。主代理新增测试也修正了未存原件应返回 EvidenceUnavailable 的假设；Store 不允许不同 subject 复用同一 operation ID，未伪造这种持久状态来凑测试。模型任务成功不等于代码正确，审查意见以实际语义及本地结果复核。

## 下一项

将私有阶段对接 `RouteContext`、HTTP 等待任务和原 worker 的调度：受控任务必须保留原服务权限、取消、截止、累计额度和未读结果资源，并在返回原 owner 后才能持久提交和恢复 guest。随后接入 package 完成帧验证及拥有者命令调度，证明慢网络期间真正的工作台业务可继续完成。

当前 HTTP 适配器的 `block_on` 和公开同步驱动仍在；S2/S3 调度、应用 TLS/出站资源、完整文件系统、持久 Unknown 核对、三语言新 IO SDK 与全平台资格仍未完成。
