# Windows 服务与工作台集成验收

2026-09-21；起点 `5b0dcac`，版本仍为 `0.1.9-test.52+56`。本轮修改 Flutter 管理界面及测试，没有修改 Rust 所有权、数据库、授权契约或 SDK。

## 发现与修正

完整 Windows 应用路径发现三个此前隔离面板测试没有覆盖的问题：

1. 插件目录在初次配置读取之后到达，运行面板没有重新绑定元数据，已批准服务仍不可选。现在按目录签名变化合并只读刷新；共享读取成功后继续绑定，读取失败保留阻塞并等待显式刷新。目录更新不替换原选择、不自动启动。两个新增用例在修正前均复现 `Expected 1 / Actual 0`。
2. 窄屏设置滚动位置与折叠项共用 PageStorage 槽，重挂时把 double 当成 bool 读取。现在隔离插件条目和高级选项的临时展示状态，同时避免内部文本框／插件表单反向读取折叠布尔值。折叠项重挂采用默认收起状态，原设置页滚动缓存保留。
3. 宽窄布局过渡中，新面板挂载触发共享会话通知，旧面板在构建期间调用 setState。`SessionViewState` 仅合并并延后界面重建通知；所有权、命令及凭据清理仍遵循原时序。

## 真实应用范围

`integration_test/service_workbench_test.dart` 使用 Windows 原生 Flutter 运行器、完整 MorrowApp、RustStudioStorage 和既有真实 Rust 宿主，在独立临时库／回环端口执行：

- 从服务面板选择批准配置、明确启动有限运行，并完成认证 HTTP 请求。
- 返回内容页面，打开原 Rust 编辑会话；调用实际 capture 转换器将表格文本转为 Markdown，经 UI 输入并提交卡片。
- 重开设置、切换中文，确认语言落库及原任务 key／submission 未变，再完成第二次 HTTP 请求。
- 调整窗口到宽屏，明确停止、观察实际回收并确认；关闭原宿主后重开同一库，核对卡片和语言。

没有调用生产默认资料目录。测试路径缺少必要环境变量时直接失败，不静默跳过。测试用服务 guest 是已有两请求 WAT 夹具，工作台业务由原 Rust guest 执行；这不是任意第三方 API 的验收。

## 验证与产物

- 六文件组合回归 **93 项通过，0 跳过**：service_run_manager、service_manager、plugin_library、http_task_manager、service_run_session、i18n_workbench。
- Windows 完整应用集成最终 **1 项通过，0 跳过**，包含 Markdown 预览及缩放后查询完成断言；见 `build/service-window-qualified.log` 和 `build/service-window-qualified/result.json`。这是同一用例迭代后的最终结果，不累计先前运行次数。
- 九文件 Flutter 静态分析通过。单独 dart analyze 曾在分析服务关闭时遇到本机 perf 文件清理异常，随后 Flutter 分析正常结束。
- Windows Release 构建成功，入口 `lib/main_rust.dart`；输出 `build/windows/x64/runner/Release/`。Debug 目录包含测试运行器，不是发布包。

日志：`build/service-window-panel-red.log`、`build/service-window-regression.log`、`build/service-window-analyze.log`、`build/service-window-release-build.log`。产物与详细模型回执保留在忽略的 build 目录。

复现时将 `MORROW_WORKBENCH_HOST`、`MORROW_WORKBENCH_PACKAGE`、`MORROW_SERVICE_RUN_PACKAGER`、`MORROW_SERVICE_RUN_PACKAGE` 设置为已有真实宿主、内置包、服务夹具打包器和 bootstrap 包的绝对路径，并将 `MORROW_WINDOW_TEST_OUTPUT` 指向新的独立输出目录；随后执行 `flutter test integration_test/service_workbench_test.dart -d windows --no-pub --reporter expanded`。构建命令为 `flutter build windows --release --no-pub --target lib/main_rust.dart`。

| Release 文件 | SHA-256 |
| --- | --- |
| morrow_studio.exe | `95d5dcb22fb1f860aa51397dda0b8649ffd02a15338c2673815b9e66731ac6c0` |
| data/app.so | `9824413fef9ad906c936b387453a55c88c1dd78802a50d0a96ca74f48686324d` |
| morrow-workbench-host.exe | `489d954afc49a68e0a48741cd7329e51735725d46ff51a0351f690b076a9d3ac` |
| plugins/workbench.morrowplugin | `d068b1d8e87e219a74b5446fc14d71c4fabaeddc72977e8f5e3c76d5a052ce1f` |

exe 是原生壳，其摘要未变；本轮 Flutter 修正体现在新的 app.so，必须使用整个 Release 目录。

## 辅助编码与审核

通过已安装 SubagentBridge Unified CLI 实际调用 `glm-5.3-flash / max` 两次。只发送必要接口和限定源码段，主代理负责验收设计、应用、修正、回归与本地整合。

- `task_66801bff3896d4c2f829a332`：测试辅助函数；候选有非法 Finder、过期 API 且缺截图函数，只保留可用等待思路，主代理重写错误部分。
- `task_7dacbdfbd467027fa943bab7`：目录异步加载修补；采用签名变化和延后合并刷新方向，移除轮询等待、未消费状态及失败后多余重试，补后端代次守卫。

插件两次分别记录输入/输出 362/597、1210/1324 tokens，均为单请求。这是插件记账，不能据此声称整体会话节省比例。没有供应商配额购买、远端推送或发布。

## 限制与下一步

本次是真实 Windows 窗口内的 Flutter 框架输入注入；截图由 Flutter 渲染树生成，capture 使用固定转换输入。它不证明系统鼠标键盘、系统剪贴板、系统窗口截图或人工视觉验收。Computer Use 技能要求的 node_repl 入口本会话不可用，没有另建桌面自动化客户端绕过。

下一步补系统输入／真实剪贴板与更完整编辑流程，再推进慢回调停止、其它维护故障与外部效果核对、长 IO 可暂停和大帧分段、应用服务 TLS／出站资源。完整文件系统、跨重启 Unknown 核对、新三语言 IO SDK 及各平台资格仍未完成。
