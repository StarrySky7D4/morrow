# Windows 启动响应修正（2026-09-21）

## 本轮修改

- `main_rust.dart` 先显示轻量、可拖动/关闭的启动窗口，再异步打开宿主、读取偏好与内容库。首帧绘制与后端准备重叠进行。
- 启动卡片复用工作台的玻璃材质，让部分 GPU 首次绘制成本与后端准备重叠，缩短正式工作台的首绘等待。
- `WorkbenchLoading` 只显示品牌与本地化进度，不持有存储或插件接口，不创建可编辑的空工作台。数据成功恢复后才挂载正式工作台，启动失败仍进入原恢复界面；清理子进程的错误不覆盖最初的打开失败。
- `Catalog::install` 对已存在的不可变插件包先执行原有完整读取、类型和摘要校验，校验成功直接复用。移除每次启动都进行的临时包写入、磁盘同步和已存在文件发布尝试。新增包仍原子发布；损坏或其他摘要的现有文件仍拒绝使用，既不覆盖也不绕过校验。保留授权、启用和升级规则。
- 增加显式 `--startup-check=<report>` 诊断：必须指定数据目录，记录首帧与真正工作台帧，完成后关闭测试进程。只有此模式记录诊断文件；正式启动不写额外时序日志。渲染错误会使诊断失败。

## 测量

同一 Windows 设备上的独立 Release 进程，各 3 次；从已有隔离诊断库再做只读 SQLite 一致性复制，包含 1 张卡片、1 个附件。不读取或修改用户原库。重启的是进程，未清空操作系统文件/驱动缓存，不能称为冷机测试。

| 指标 | 修改前 | 修改后 |
| --- | ---: | ---: |
| 从创建进程到窗口可见 | 1.105–1.260 s | 0.442–0.444 s |
| 窗口可见中位数 | 1.215 s | 0.443 s |
| Dart 入口到首次绘制完成 | 0.908–0.939 s（正式工作台） | 0.234–0.239 s（启动界面） |
| Dart 入口到正式工作台绘制完成 | 0.908–0.939 s | 0.816–0.823 s |

窗口响应中位数减少约 64%；正式工作台首帧（Dart 入口起）中位数从 0.928 s 降至 0.818 s，减少约 12%。**窗口可见和内容就绪是两项独立指标，启动卡片不代表已经可以编辑内容。** 初版平面启动提示只提前显示窗口，正式工作台耗时未明显变化；复用真实玻璃绘制后得到上述进一步改善。宿主准备仍约 0.44 s；省去重复插件包写盘没有在这个小样本中表现为显著的宿主就绪时间下降。

窗口可见通过 Win32 枚举本次启动的 PID 观测（约 10 ms 采样）；正式工作台帧通过 `MorrowApp.onFirstFrame` 对齐实际 frame number，再记录引擎 Raster 完成时间，避免把加载动画或本地化尚未就绪的帧算作完成。`process_total_ms` 包含时序回调批量上报和子进程收尾，不能作为启动就绪指标。

证据：`build/startup-native-before.json`、`build/startup-window-before.json`、`build/startup-window-parallel/summary.json`、`build/startup-window-material/summary.json`（最终保留方案）。重现工具：`tool/profile_windows_startup.py`，需显式传入 exe、隔离 fixture 和全新输出目录。

后续性能边界：卡片仍逐项读取，附件仍在当前会话中由宿主重新校验导出；大量内容/附件与首次工作台的 GPU 绘制成本尚未在本轮重构。没有使用未经验证的持久附件缓存、跳过审计或提前启用插件来换取启动速度。本轮不代表大内容库或其他设备/平台的性能保证。

## 验证与构建

- 7 项 Flutter 回归通过：七语言窄窗启动界面、加载切换后的内容/主题保留且无占位保存、就绪通知一次、恢复与设置返回行为。日志：`build/startup-flutter-tests.log`。
- 21 项 Rust 包/登记回归通过：幂等安装、完整内容和摘要校验、篡改不覆盖、并发发布、失效授权及升级规则。日志：`build/startup-package-tests.log`。
- 最终 Release 宿主/插件的 2 项真实进程集成通过：持久化内容与偏好、密钥缺失和原密钥恢复。日志：`build/startup-native-regression.log`。
- 相关 6 文件静态分析和 `git diff --check` 通过。日志：`build/startup-final-analyze.log`、`build/startup-diff-check.log`。
- Windows Release 已完整重建，日志：`build/startup-final-build.log`。输出：`build/windows-corners/x64/runner/Release/morrow_studio.exe`，须保留整个 Release 目录。

版本仍为 `0.1.9-test.52+56`。未调用 SubagentBridge 或其他子代理，未推送/发布；SDK 接口与冻结条件不变。

最终重建产物追加一次冒烟通过（退出码 0）：窗口可见 0.431 s，正式工作台首帧 0.833 s（Dart 入口起）。证据：build/startup-final-material-smoke/summary.json。四个交付文件的 SHA-256 已刷新至 build/startup-artifact-hashes.json，Release 内宿主与插件包均与对应构建输出逐字节摘要一致。
