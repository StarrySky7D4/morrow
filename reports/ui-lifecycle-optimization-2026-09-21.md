# 工作台 UI 生命周期优化 · 2026-09-21

基线：`ddd9cc8eec224af51f4e58654c9b297332147d96`，已发布的 `0.1.9-test.54+58`。用户提供的分析基于 test.53；本轮在 test.54 开库优化之上修改 Flutter 视图生命周期，不改变 Rust 存储格式、插件 ABI 或 SDK 冻结状态。当前为本地修改，未推送、未发布、未更改版本号。

## 实现

### 工作台身份与惰性视口

- 概览、收件箱、小项目、实验室、收藏使用同一个 `WorkspaceViewport`，顶部和尾部区域为 Sliver，卡片采用按视口构建的瀑布流，预取距离 300 px。双列仍按可用宽度启用。
- 移除由搜索、排序、完整结果 ID 序列驱动的整组 AnimatedSwitcher。每张卡片以 `(页面, 卡片 ID)` 标识直接子节点，提供 `findChildIndexCallback`；当前模型在每个页面只出现一次同 ID 卡片。未来允许重复放置时须替换为放置 ID。
- 页面改变只保留一个当前页面树并淡入，不堆积退出中的页面。保存最多五个页面的滚动偏移，不永久保活全部卡片或页面。
- 同页查询等待期间隐藏、停用并保留当前虚拟化卡片 State；只有宿主确认的结果可以重新显示。失败、空结果、资料库会话替换会释放旧卡片。排序、删除与撤销不强制重建未变化的可见条目。
- 新依赖 `flutter_staggered_grid_view 0.7.0`；针对该版本重排时列信息保留而 layoutOffset 失效的问题，增加局部 RenderSliver 适配，未修改依赖缓存。MIT 许可及第三方声明已加入打包目录。

### 有限视图与缓存

- 顶层设置第一次打开后才创建，最多保留一个设置树。隐藏时禁用焦点、交互及 Ticker；重新打开恢复焦点，Escape 返回仍有效。内存压力只回收隐藏且已退出动画的设置树，不销毁当前可见编辑器。
- 普通一次性编辑路由仍按原有应用／取消语义释放，没有缓存已弹出的 Route 或临时修改。
- QueryCoordinator 最多保留八个已完成查询的操作引用。A→B→A 重新调用宿主交付原操作结果，由宿主检查当前授权；不会直接把本地 ID 当作当前可读结果，也不会伪造一次新的查询计算。内容代次、后端变化、终止错误和显式失效均有清理路径。
- 非 Rust 的本地／演示投影最多保留八组、合计 20,000 个卡片引用；保存和会话变化使其失效，空搜索不再拼接全部文本。每帧只计算一次投影。此缓存不是 Rust 权威内容的替代。
- 插件目录按后端弱引用保存，最多 256 条／2 MiB 元数据。返回页面始终先读取实时第一页，修订与第一页完整指纹相同才复用后续页；手动刷新及修改后的加载仍完整读取。所有操作仍传修订与包摘要给宿主核验。
- 目录缓存不承诺探测后续页插件包的带外磁盘变化；需要手动完整刷新更新展示，执行时仍由宿主验证实际包。没有把目录快照当作授权。

### 展示与任务生命周期

- HTTP 的目标、请求头、正文、超时、选区、方法及正文格式作为每个后端的草稿恢复。端点选择必须重新加载并核验，恢复不创建或重新发送 HTTP 任务。
- HTTP、服务状态及 TLS 展示计时器在页面被覆盖／TickerMode 隐藏时停止，返回时同步一次状态；后台业务任务、Unknown 与原操作 ID 沿用现有会话。
- 插件 UI 仍在关闭页面时释放。异步关闭失败的 transport 保留在对应后端的待释放集合，新操作必须先完成释放，旧后端的迟到失败不会污染新后端。不宣称已提供任意插件表单的持久草稿或无限会话保活。
- 内存压力清空可重建的本地投影、已完成查询引用及插件目录快照；未确认业务、HTTP 草稿和当前编辑状态不因此丢弃。

## 验证范围

`workspace_lifecycle_test.dart` 覆盖 300／1,000／10,000 张可变高度卡片、20 次滚动、跨单双列断点、稳定 State 重排、删除／撤销、快速往返滚动恢复、查询等待隐藏与授权失败清理、会话更换、设置保活及模拟内存压力。

查询协调器测试验证八项上限、内容代次失效、宿主响应前无结果、终止失败后新操作与过期响应隔离。真实 Rust 集成新增 A→B→A 重用查询、禁用插件后拒绝旧结果交付，使用自建临时资料库，不打开用户原库。

插件与 HTTP 用例验证目录请求数量、修订变化、手动刷新、后端切换、关闭失败、草稿恢复、隐藏后停止轮询、恢复后不重复启动任务。

完成记录：

- Flutter 全量测试：367 通过、65 因环境条件跳过；真实宿主集成另行启用，3 项通过。最后补充隐藏卡片的键盘焦点隔离后，17 项工作台／窗口回归通过（与全量重叠，不累加）。
- 本次修改的 18 个 Dart 文件限定静态分析无问题，焦点补丁涉及的两个文件复查无问题；`git diff --check` 通过。没有把限定检查描述为全仓库零诊断。
- Windows Profile 真实进程测量通过。驱动有既有的 integration_test plugin 未检测提示，但测试退出成功，应用主动写出的 FrameTiming／RSS JSON 已保存；不依赖驱动的 reportData 汇报。
- 最终 Windows Release 的配置与 CMake INSTALL 构建成功；随后对该目录的最终宿主／插件再次运行 3 项原生集成，全部通过。可执行入口为 `build/windows-corners/x64/runner/Release/morrow_studio.exe`，Dart AOT 位于同目录 `data/app.so`；必须保留完整目录。版本仍为 `0.1.9-test.54+58`，不是新的已发布测试版。

```powershell
flutter build windows --release --no-pub --target lib/main_rust.dart --config-only
cmake --build build/windows-corners/x64 --config Release --target INSTALL
```

### 最终 Profile 测量

窗口物理尺寸 1264×681，磨砂材质、内存库、可变文字高度卡片。每组八轮「概览→收件箱→项目→实验室→概览」，共 32 次切换；每组约 166 帧。各规模初始挂载 2 张，切换后挂载 1–4 张。另有 Widget 回归验证长列表滚动后的挂载数量小于 30。

| 卡片总数 | UI p95 / 最大 ms | UI >16.7 ms | Raster p95 / 最大 ms | Raster >16.7 ms | 八轮进程 RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| 300 | 9.795 / 13.914 | 0 | 6.024 / 30.785 | 2 | 160.2–169.5 |
| 1,000 | 9.455 / 13.800 | 0 | 4.707 / 10.371 | 0 | 178.0–180.2 |
| 10,000 | 15.651 / 25.868 | 6 | 4.074 / 18.257 | 1 | 203.0–209.4 |

[原始 JSON](ui-lifecycle-windows-profile-2026-09-21.json) 同时记录 8.3 ms 预算、逐轮 RSS 与挂载数。`initial_settled_ms` 包含测试 pumpAndSettle 和界面动画，不能当作冷启动／首次可交互耗时。

这些结果证明可见视图数量不再随全部卡片数增长，不证明万卡所有帧达标，也没有以 test.53 或 test.54 原版进行相同夹具的 AB 对照；不能据此声称整体快了某个百分比。几次开发过程测量的峰值有波动，本表保留最终完整代码实测值。小规模首次阶段 RSS 仍上升，八轮不足以判定长期稳定或无泄漏。

复现：

```powershell
$env:MORROW_PROFILE_OUTPUT = Join-Path (Get-Location) 'build/ui-lifecycle/windows-profile.json'
flutter drive -d windows --profile --no-pub --driver tool/settings_profile_driver.dart --target integration_test/workspace_lifecycle_profile_test.dart
```

## 后续边界

1. 本轮控制了页面数、卡片视口与缓存条目；尚未实现统一的 Dart／原生／GPU 资源计费、租约管理和长时间压力验收。
2. 主工作台只挂载当前页；跨页不承诺保留所有卡片 State，保留的是滚动信息与有界数据引用。大库首次本地筛选、ID 映射及全库统计仍有线性工作；数据库分页与持久化索引需要另行推进。
3. 可见性策略覆盖路由与 TickerMode，不等于所有平台的应用进入后台、视频解码器或任意插件定时器都已统一暂停。
4. Windows Profile 使用内存资料库与文本卡片；不是冷开库基准，不代表真实媒体／插件／网络负载、移动端、Web 或 120 Hz 全面达标。短周期总进程 RSS 不等于原生／GPU 泄漏已排除。
5. 未增加全量预热或整页截图；玻璃材质实现保持原有路径。后续应针对剩余 Raster 峰值单独采样，再决定滤镜共享或绘制层优化。

参考 API：[惰性列表重排映射](https://api.flutter.dev/flutter/widgets/SliverChildBuilderDelegate/findChildIndexCallback.html)、[SliverMasonryGrid](https://pub.dev/documentation/flutter_staggered_grid_view/latest/flutter_staggered_grid_view/SliverMasonryGrid-class.html)、[TickerMode](https://api.flutter.dev/flutter/widgets/TickerMode-class.html)。
