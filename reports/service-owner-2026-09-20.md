# 服务完整所有者回收验证

基线 `8563f07279c48c583962eac6a50d8783f4ba1601`；隔离分支 `codex/io-safety-refactor`；应用版本保持 `0.1.9-test.52+56`。**PASS_SCOPED**：原生服务适配器保留完整 HostOwner，Windows 真实 Storage 的原库、保护租约和审计身份可在实际 worker 退出后回收。本轮不宣称常驻服务或主应用监听入口完成。

## 实现与边界

- ServiceHost 与 ManagedRoute 支持完整拥有者；克隆共享原 worker，不重开库、不复制实例或批准。`new_owned` 拒绝选项时交还原 worker。
- 请求停止与实际回收分开。`try_reclaim` 一次交还完整 WorkerExit；`shutdown_owned` 等待真实线程结束，取消等待后仍可用保留句柄回收。执行、断连、封存结果分别保留。
- 监听监督任务与 worker 生命周期分别处理。关闭 socket 不代表执行结束，最后一个 ServiceHost Drop 只请求停止，不使用强杀或阻塞析构。
- 旧 HostRuntime 专用构造和 bind/bind_tls 保持源码兼容；包含 Storage 的调用改用 owned 入口。没有新增 unsafe，没有修改 SDK 原件或协议。

## 验证

| 检查 | 结果 | 本地证据 |
| --- | --- | --- |
| 工作台 Rust Release 全特性回归 | 174 passed / 0 failed / 0 ignored，含3项新 Storage 测试 | `build/service-owner-host-full.log` |
| 网络节点 Release plugin-adapter 回归 | 107 passed / 0 failed / 0 ignored，含7项新所有权测试；文档编译另行复验通过 | `build/service-owner-network-final.log`、`build/service-owner-network-doc.log` |
| 两个 crate 的所有目标严格 Clippy | 均通过，`-D warnings` | `build/service-owner-host-clippy.log`、`build/service-owner-network-clippy.log` |
| 冻结 SDK 基线 | 36个固定文件、13对原 Wasm/包通过 | `tool/verify_plugin_sdk_baseline.py` |
| 修改文件格式及差异检查 | 通过 | rustfmt / git diff --check |

计数按顶层测试目标汇总，未重复计算审计恢复子进程的内部输出。新增10项已包含在上述总数中。

七项网络测试覆盖：取消与一次回收、取消等待后再回收、构造拒绝保留原 worker、端口冲突、监听显式关闭/Drop、最后句柄 Drop 与延迟维护、旧空路由调用的类型推断兼容。维护门闩实际保持5200毫秒，证明新回收接口不会在旧五秒界限提前结束；这不是长期运行租约的证明。

三项 Windows Storage 测试使用受保护的原库：实际认证 loopback HTTP 请求执行并持久化 Observed；监听停止时原库、Registry 和签名身份保护锁仍占用；回收后保持原绑定和身份，审计验证并重开原库。另覆盖封存注入失败后保留完整 owner、显式修复而不重放业务，以及端口绑定失败的完整回收。测试使用受控短时钟，没有证明服务超过旧30秒窗口可运行。

## 失败及修正记录

独立复审发现把旧 bind 泛型化会使 `vec![]` 调用失去类型推断。已保留旧签名，新增 bind_owned/bind_tls_owned，并加入兼容回归。

最终网络测试的107项执行测试全部通过，但当时同时构建工作台全特性版本，共享 Cargo 目录中的未带哈希 core rlib 被替换，导致 rustdoc 的 E0460/E0463 依赖产物不一致。原失败日志保留；工作台完成后顺序重建网络文档，编译通过，文档测试0项。随后两个 crate 的严格 Clippy 均通过。未把首次失败记录为完整成功，也未通过删改测试规避问题。

## 剩余工作

旧 IO v1 仍限制最长30秒绑定；服务持有 Storage 时工作台仍可能 Busy。下一阶段按 [实施方案](../docs/PLUGIN_SERVICE_RUNTIME_PLAN.md) 分离运行租约与每请求预算，移交完整 WorkbenchState，再实现有界命令调度和可暂停 IO。仅添加串行队列不等于 UI 已能在长请求期间响应。

主应用启动/停止入口、常驻真实入站验收、Unknown 证据核对、文件系统后端、完整三语言 IO SDK 与跨平台资格仍未完成。本轮没有 Dart/UI 改动，未重复构建 Flutter。仅本地提交，没有推送、发布或关机；内置子代理参与实现和复审，未实际调用 DeepSeek。
