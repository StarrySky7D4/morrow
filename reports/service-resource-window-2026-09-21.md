# Windows 服务资源发现与撤销验收

日期：2026-09-21。生产源码基线 `6c063dc`；本轮增加测试、夹具和文档，没有修改生产授权策略。版本仍为 `0.1.9-test.52+56`。

## 实际窗口结果

`integration_test/service_resources_window_test.dart` 使用完整 MorrowApp、原 RustWorkbench 宿主、原库和实际编译的 Rust/Wasm 服务插件，在独立临时目录/回环端口验证两条流程：

1. 800×820 窗口内选择有限服务和一个出站端点，明确启动。外部请求体带错误端点/凭据引用，且伪造资源目录头；插件从宿主目录发现实际允许资源，真实 HTTP 返回预期内容。相同服务请求重放不增加外发次数。
2. 第二个请求由实际测试服务器接收并暂停响应。一路通过端点管理按钮停用批准，另一路通过服务面板直接停止。两路都在服务器尚未释放响应时回收原 worker、监听和 Store；保留原任务身份，明确确认后重开同一库核对端点和语言。

两个最终用例均只收到两次实际出站请求。直接停止场景保留启用的持久端点；停用场景验证同一引用、修订恰增一和 disabled=true，并在重开后再次核对。取消允许连接关闭或非成功 HTTP 响应，超时仍使测试失败；没有把超时转换成取消成功。验证不证明远端效果回滚，仅证明原拥有者完成回收及取消结果不被交付为成功。

当前授权策略在批准变更时保守撤销全部恢复的入站/出站授权，因此停用端点会自动回收整个服务。最初验证代码在交接期间读取业务状态，得到 commandSubmit 的 StaleTask。修正为先观察真正回收再读取原库；不是绕开拒绝、自动重发修改或宣称单端点撤销已隔离。下一项是按依赖资源精确撤销，同时保留原 Store 协调锁、撤销先于修改、回滚不复活权限等约束。

## 验证及构建

- 新 Windows 实际窗口集成 **2 项通过，0 跳过**：`build/service-resource-window-final-v2.log`。逐项回执和 Flutter 渲染图在 `build/service-resource-window-6c063dc/{revoke,stop}/`。
- 既有完整工作台窗口回归 **1 项通过**：服务启动、内容编辑/Markdown 预览、语言切换、宽窄窗口、停止及原库重开；日志 `build/service-resource-legacy-window.log`。覆盖共享夹具的原默认分支。
- 五文件 Dart 组合 **71 项通过，0 跳过**：service_run_control、service_run_session、service_endpoint_catalog、service_run_manager、service_run_transport_native；日志 `build/service-resource-window-regression.log`。管道替身测试不等于窗口/网络实效证据。
- 三个 Dart 文件分析通过；Rust fixture 打包器严格 Clippy、Rust/Dart 格式和 diff 检查通过。
- Rust 原生宿主、内置工作台包与实际服务 guest 已构建；**完整 Windows Release 构建成功**，入口 `lib/main_rust.dart`。输出 `build/windows/x64/runner/Release/`，日志 `build/service-resource-windows-release.log`。Debug 目录为测试运行器。

合计 74 项本轮相关测试，不累计上轮 79 项原生测试。未新增系统鼠标键盘、系统剪贴板、OS 截图或第三方公网提供者验收；窗口输入为 Flutter 框架注入，图片是 Flutter 渲染树捕获。测试插件依赖 core codec，不代表公共 Rust/C/C++ IO SDK 已稳定。

| Release 文件 | SHA-256 |
| --- | --- |
| morrow_studio.exe | `95d5dcb22fb1f860aa51397dda0b8649ffd02a15338c2673815b9e66731ac6c0` |
| data/app.so | `b344375ba6abf2c2c0f405a1ffc00f4d6d8b7c5aebb1bce0786db139fe3ee902` |
| morrow-workbench-host.exe | `7a547b96bde40cb74afd27386c99aae9316057e8d51b76325672ce285a80f459` |
| plugins/workbench.morrowplugin | `d068b1d8e87e219a74b5446fc14d71c4fabaeddc72977e8f5e3c76d5a052ce1f` |

必须使用完整 Release 目录。机器可读回执在 `build/service-resource-window-6c063dc/build-receipt.json`。版本未递增，没有制作发布 ZIP、推送或发布 Release。

## 夹具与辅助编码

新增 `package_service_resources_fixture`，有界读取实际 guest，写入显式资源 feature 与 IO 声明，使用 create-new 写包及两个不同操作身份的调用帧。共享 RealServiceFixture 增加可选实际资源插件路径，旧 WAT 命名空间流程保留。复现变量和命令见[夹具说明](../workbench_host/tests/fixtures/service-outbound/README.md)。

窗口辅助器在异步元数据扩展布局时有限重新定位，确认当前目标可命中后才点击。服务器回调只记录请求，断言在测试主流程执行，避免 Flutter guarded pump 与后台 expect 冲突。上述修正属于测试设施，没有掩盖产品异常。

SubagentBridge GLM/max `task_a3dbfe2d191fba5e6795fbd7` 提供打包器草稿；主代理纠正不存在的 Manifest 字段、预算类型、错误包写法及自设模块大小上限。DeepSeek/max `task_75ce5c02ac897a1fe6baba6c` 审核断言，采纳实时持久修订核对；未采纳将 TimeoutException 当作成功取消的建议。两任务完成并释放，执行成功不替代独立验证。
