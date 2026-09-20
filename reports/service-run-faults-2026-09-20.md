# 有限服务启动诊断与真实故障验证

日期：2026-09-20。结论：**PASS_SCOPED**。基线 `687bb5f6c205ee439d480cee14d9ba2c234e89aa`，应用仍为 `0.1.9-test.52+56`。本轮修改 Dart 控制会话和页面诊断，不修改 Rust 生产实现、私有消息布局或冻结 SDK。

## 启动失败后的操作规则

只有通过原通道契约校验的宿主错误响应才转为 `ServiceRunStartFailure`。摘要不符、格式损坏、成功回复缺失运行状态、管道 EOF 均保持原不确定结果，不据此自动重新启动。

宿主返回错误不证明没有工作线程或清理任务。会话保留原 submission，随后只读观察任务与服务：确认为本地无任务时归档本次错误；存在同身份任务时保持原停止、修复及确认门槛；观察失败或身份不符时继续保留 Unknown，阻止新启动及对陌生任务的控制。后续启动必须由用户明确触发并使用新提交身份。

错误详情限制为 1024 个 Unicode 字符，提供中英文提示，刷新与页面卸载/重挂不清除它；最近 5 次尝试历史保留原身份及错误详情。诊断不是授权，也不是外部效果回滚证明。

## 验证证据

最终 15 文件组合回归 **162 通过、0 失败、0 跳过**，日志 `build/service-run-faults-regression.log`。其中会话 16 项、控制通道 8 项、运行面板 20 项、新增真实故障 8 项；这些子集已包含在 162 中，不另行累计。

真实故障测试使用 Windows Release 的 `morrow-workbench-host.exe`、原 Rust 工作台包以及测试专用有限服务 WAT 包，覆盖：

1. 实际端口占用：绑定失败后收回原内容库，保留原任务直到显式确认。
2. 有限期限：1200ms 运行在约 300ms 时仍存活，至少运行 1100ms 后、15 秒内自行退出；无需停止命令，后续 HTTP 连接失败。
3. 运行中通过普通业务 API 撤销发布批准。
4. 运行中通过普通业务 API 撤销认证。两类撤销均先成功处理真实 HTTP，撤销后拒绝新连接；确认原记录禁用且修订仅增加一次，回执不确定时不重发写入。
5. Registry 修订错误。
6. 服务配置修订错误。
7. 累计运行预算超声明。
8. 工作线程调用额度超声明：实际启动错误仍留下同 submission 清理任务，确认退出前不能重新启动。

每种真实故障均核对原内容库仍可读写；任务确认只在实际退出、回收后进行。独立真实故障运行 **8/8、0 跳过**，日志 `build/service-run-routing/real-service-faults.log`；同一组也在最终组合中再次通过。

受控 Python 管道验证错误分类及不重发，widget 验证 320px 中英文诊断、卸载/重挂、刷新和显式新提交。会话测试另覆盖观察失败、身份更换及需要修复的模拟状态；模拟修复不算真实封存故障验收。

9 个受影响手写 Dart 文件的 Flutter 静态分析通过，日志 `build/service-run-faults-analyze.log`；格式检查零改动。中英文 5 项语言包产物检查、工作台客户端生成检查，以及冻结 SDK 36 个固定文件/13 对原 Wasm 与包完整性检查通过。没有修改 Rust 生产代码，不重复引用历史 Rust 测试数量。

复现真实测试须设置 `MORROW_WORKBENCH_HOST`、`MORROW_WORKBENCH_PACKAGE`、`MORROW_SERVICE_RUN_PACKAGER`、`MORROW_SERVICE_RUN_PACKAGE`，分别指向原生宿主、原工作台包、测试打包工具和 bootstrap 服务夹具；未提供时会跳过。受控管道测试的 `MORROW_CLOSE_TEST_PYTHON` 应为真实 Python 可执行文件，不能使用 WindowsApps 跳转入口。本轮上述参数均已配置，真实测试未跳过。

## Windows 预览构建

`flutter --no-version-check build windows --release --no-pub` 成功，日志 `build/service-run-faults-windows-build.log`。产物在 `build/windows/x64/runner/Release/`，须保留整目录中的 DLL、data 和 plugins。CMake 安装目标仍指向该隔离构建目录。

| 文件 | SHA-256 |
| --- | --- |
| `morrow_studio.exe` | `95d5dcb22fb1f860aa51397dda0b8649ffd02a15338c2673815b9e66731ac6c0` |
| `data/app.so` | `f341f9b21bbbe0c1a8a17edac36f34415e8ed6ebfd2fbbc418bddc3ecd1beb18` |
| `morrow-workbench-host.exe` | `489d954afc49a68e0a48741cd7329e51735725d46ff51a0351f690b076a9d3ac` |
| `plugins/workbench.morrowplugin` | `d068b1d8e87e219a74b5446fc14d71c4fabaeddc72977e8f5e3c76d5a052ce1f` |

构建后的原生宿主和原工作台包与本轮真实测试使用的文件摘要一致；Flutter AOT 已更新。不把构建成功等同真实窗口视觉/输入验收，没有制作安装包、ZIP、签名或 Release。

## 修正与后续任务

纠正前轮路由报告的额度解释：单作业调用额度上限来自包的 `host_calls`（夹具默认 16），不是并发作业数 `max_jobs`（4）。本轮以 64 调用稳定触发真实拒绝，没有调整生产上限来通过测试。

下一顺序：真实窗口与内容/UI/capture 共存 → 真实丢回执、慢回调停止、封存失败修复 → 长 IO 可暂停及超限业务帧分段 → 应用服务 TLS 与出站资源。文件系统、跨重启 Unknown 证据核对、因果链、C/C++/Rust IO SDK 候选与其他平台仍按原验收门槛推进。当前 WAT 服务夹具不证明任意三方 Rust 插件已获完整网络 SDK 支持。IO-D2b/IO-E2 仍部分完成，未推送或发布。
