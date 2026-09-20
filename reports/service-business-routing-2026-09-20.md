# 服务期间业务自动路由验证

日期：2026-09-20。结论：**PASS_SCOPED**。实现基线为 `f135dd17502ea06c2ee310ca95677ab62398c859`；版本保持 `0.1.9-test.52+56`，冻结 SDK 与发布状态不变。

## 实现范围

`RustWorkbench` 将已观察到的服务运行期间普通业务调用接入原拥有者命令通道。业务队列保持调用顺序，传输队列只等待单次管道响应，因此命令 Pending 期间仍可观察状态或请求停止。实际取得传输槽时选择并固定原 task，覆盖先请求启动、启动回执尚未到达就发起业务的情况。

启动结果不明时阻止业务继续，等待显式状态核对；停止、回收和确认观察保持原任务关联。已提交命令遇到超时、损坏回复、Unknown 或任务更替时不重发、不转到新服务、不回退本地执行。`ServiceCommandFailure` 保留 task、submission、可用时的 command 和终态，供后续显式核对。当前没有自动恢复提交回执或自动重新执行。

路由开始后，Starting 等待、提交、轮询、读取以及控制请求的实际排队、flush 和响应等待共享截止时间。传输超时或写入结果不明时封闭管道，以 EOF 请求清理并等待原进程真实退出；移除该传输超时路径的强制 kill。关闭阻止后续轮询，不等待整条业务队列；清理状态不覆盖最初的传输错误。

服务运行中的调度状态观察不再把业务界面错误设为只读；实际回收后恢复原生本地诊断。业务错误与 QueryFailure 分类保持原语义。类型化敏感结果拥有解码数据后擦除内层帧；返回借用 Reader 时显式转移原缓冲区寿命。损坏的已消费回执保留原命令身份并报告结果不明。

内层完整请求限制为 64 KiB，本地私有请求仍为 128 KiB。实查现有上传块已经是 32 KiB，本轮没有修改块大小；其他超过 64 KiB 的完整业务帧明确拒绝，尚未完成所有大请求的分段适配。发送暂存、构建器段和拥有的敏感回执按原清理边界处理。

## 验证结果

最终 Dart 组合回归：**69 通过，0 失败，0 跳过**。本地日志：`build/service-business-regression-final.log`。包含：

- `test/host_request_test.dart`
- `test/service_run_control_test.dart`
- `test/service_run_transport_native_test.dart`
- `test/service_control_test.dart`
- `test/io_task_control_test.dart`
- `test/service_session_test.dart`
- `test/workbench_close_native_test.dart`
- `test/service_business_routing_native_test.dart`
- `test/service_business_routing_real_native_test.dart`

新增受控管道测试共 6 项，以实际 Python 子进程读写管道，但不执行 Rust 业务。覆盖延迟启动回执、Pending 时的状态/停止、只读提示隔离、Reader 寿命、查询与业务错误、Unknown 身份和不重发、超限帧、关闭后停止轮询、损坏内层回执以及认证令牌清理。

新增真实 Windows 原生集成测试 1 项，使用 debug Rust 宿主、原 Rust 工作台 guest 和有限服务 WAT 测试包。在两个实际 loopback HTTP 请求之间，通过普通公开业务方法创建、编辑、读取卡片及保存语言；验证运行中 IO 观察不阻断写入，并完成停止、实际回收、确认、关闭及重开后的数据读取。独立运行日志：`build/service-run-routing/real-routing-test.log`。该 WAT 是测试夹具，不代表完整 Rust 服务插件或 SDK 使用资格。

Rust 宿主和新夹具打包 example 使用锁定依赖离线构建通过：`cargo build --manifest-path workbench_host/Cargo.toml --bin morrow-workbench-host --example package_service_run_fixture --locked --offline`。example 的 Clippy `-D warnings` 通过，日志：`build/service-business-fixture-clippy.log`。本轮未重跑此前完整 Rust 测试集，不将历史结果计为本轮执行。

受影响的 7 个手写 Dart 文件分析无问题，日志：`build/service-business-analyze-verified.log`。Dart 格式检查、example 的 rustfmt 检查及 `git diff --check` 通过。`python tool/verify_plugin_sdk_baseline.py` 校验 36 个固定文件、13 对原 Wasm/包通过；`python tool/generate_workbench_client.py --check` 通过。未修改 schema、生成协议或冻结 SDK 包。

## 验证中修正与限制

首轮组合测试有 2 项关闭测试失败：传输错误被清理中的“正在关闭”覆盖。恢复原错误优先级后，关闭专项 3 项及最终组合 69 项全部通过。真实测试最初把单作业 max_calls 设为 64，与夹具声明的 4 不符；修正测试参数，没有扩大运行时准入。

受控 Python 夹具曾有一次退出码 1 且未留 traceback 的间歇失败。增加 traceback 留存后，专项重复运行和最终组合未再复现；不能据此宣称长期并发稳定性已经验收。

本轮未构建完整 Flutter Windows GUI、安装包，也未执行其他平台、设备或发布验收。当前服务仍限已批准的单个 loopback HTTP 有限运行，保留每服务 8 个未消费命令句柄与 512 项提交历史；没有自动续租、新 TLS 或出站资源接线。

## 后续切片

先完成 Flutter 服务运行页面，明确有限预算、当前身份、绑定/监听/回收状态和停止、修复、确认操作，再验证实际页面与内容、插件 UI、capture 并用及故障交互。随后推进大请求显式分段、可暂停长 IO、TLS 和出站资源。文件系统、Unknown 持久核对、三语言 SDK 和全平台资格保持原验收门槛；IO-D2b/IO-E2 尚未整体完成。
