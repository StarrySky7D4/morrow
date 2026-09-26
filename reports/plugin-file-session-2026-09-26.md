# 文件任务会话、增量校验与原生选择器接线

2026-09-26，基于本地 `b630e86557f6b158f2921547626493bbbb6cd8f4`。从已校验的 Google Drive 源码与增量 bundle 恢复；主线基准仍为 `d9c0431`，应用版本保持 `0.1.9-test.56+60`。未使用 CI/Actions、推送或发布。

## 完成范围

新增不依赖 Flutter 的 `FileTaskSession`，按原 backend 对保留当前尝试、已消费进度及最多五条摘要历史。页面离开只移除观察者；已明确提交的校验继续，重新进入不会创建新任务。历史不保存路径或原始字节；当前会话只保留最多 4096 字节预览，不拼接完整文件。

显式启动后领取 Captured，按精确 offset 请求最多 64 KiB 的块，核对长度、EOF 和增量 SHA-256，收到 Finished 才标记文件校验完成。空文件直接校验空摘要。存储的实际退出、维护失败、修复与确认独立展示，摘要通过不能代替 join 或清理成功。默认观察预算为 35 秒，循环在调用边界检查，不承诺中断单个未返回的底层调用。

丢启动回执只用原 submission 的状态恢复；丢块请求、结束请求或一次读取回执保持 Unknown，不重发。取消先改变会话代际，迟到字节不再交付；取消后的修复／确认仍需实际退出。丢确认回执通过全局状态确认 Local，不再次确认旧 key。已知任务始终核对 task key 与 submission，外来任务不得接管。

原生 IO 设置页新增系统文件选择、已批准 FileRead 插件／handler、捕获上限、期限、读取校验、状态刷新、取消、修复、确认及摘要预览。路径只传给原 worker，选择器不读取文件内容。原生能力需 backend 显式声明；浏览器通道不显示路径选择入口。目录变更或 backend 更换后拒绝迟到选择，当前已提交请求保持原身份。HTTP 面板监听文件会话，在文件尝试未解决时停止领取和操作该任务。

## 本轮验证

| 检查 | 结果和范围 |
| --- | --- |
| 独立 Dart 测试 | 28 通过、无跳过：新增会话 21，既有客户端 7 |
| 会话异常路径 | 错位、提前／缺失 EOF、空／过大块、错误摘要／结果种类／捕获长度；丢启动／块／读取／结束／确认回执；外来任务、取消迟到块、意外退出、维护修复、观察期限、页面观察者脱离 |
| 客户端跨语言帧 | 读取仓库保存的上一轮 Rust／C／C++ 真实回执，独立核对原始内容摘要；本轮没有重新运行 Rust 生产者 |
| 定向 Dart 分析 | 两个测试目标及可达纯 Dart 依赖通过；不包含 Flutter 页面完整类型分析 |
| UI 集成文件 | 源码审查与 Dart 格式解析通过；根目录缺少 flutter_lints，保留警告 |
| 冻结原件 | 17 个 transport 固定文件摘要保持；无 schema、guest、Wasm 或原包改写 |

证据保存在 [本轮日志](evidence/plugin-file-session-2026-09-26/)。使用独立 Dart 3.12.0 和原锁定依赖；本轮未修改 Rust，不把历史 runtime 415／Workbench 64 等计入本轮通过数。

## 待验收范围

系统选择器与 Flutter 页面已经源码接线，尚未完成 Flutter 完整类型分析、widget 测试或真实窗口操作。此前 Flutter 启动被自动审批以间接云实例元数据访问拒绝，本轮没有重试该路径。Windows 真实选择、选择期间切库、页面重开、取消、故障回执、修复／退出及 DPAPI/TLS 组合验收继续开放。

新增文件专用文案暂为中文／英文回退，通用状态复用既有多语言文本；完整九语言校验待补齐。不提供跨进程恢复、持久文件证据、目录或写入；路径身份仍从 worker 实际 open 时确定，不保证选择时刻快照或 symlink/junction 根约束。Linux 生产受保护存储限制保持。

下一步以完整 Flutter 检查和 Windows 文件任务产品路径验收为门槛，再补齐本地化。当前可作为可恢复开发检查点，不能宣布插件底座跨平台整体收尾。

## 复验

在 `packages/morrow_core_client` 中使用独立 Dart 执行 `dart pub get`，然后运行：

```sh
MORROW_FILE_WIRE_FIXTURES=../../reports/evidence/plugin-file-wire-2026-09-26/rust-frames dart test --reporter expanded test/file_task_session_test.dart test/file_task_client_test.dart
dart analyze test/file_task_session_test.dart test/file_task_client_test.dart
```

在仓库根目录执行 `python3 tool/plugin_transport_baseline.py verify`。Google Drive 检查点同时保存完整 tracked 源码 ZIP、需要主线基准的累计增量 bundle、累计证据 ZIP、此报告、恢复说明及 SHA-256 清单。
