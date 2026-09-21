# 设置页首开卡顿修正（2026-09-21）

## 结果与范围

设置导航入口采用无水波纹反馈，保留悬停、焦点与按下高亮。组件列表、组件编辑、插件管理与 IO 设置共用 220 ms 淡入淡出，不再使用 Material 默认缩放快照、延迟显现及仅进入方向的遮罩。工作台背景画布持续显示；进入途中可以按返回，路由结果、草稿和选区继续保留。保留路由语义边界与系统减少动画设置。

改动集中于 `lib/settings_surface.dart`、`lib/main.dart`、`lib/component_material_page.dart` 和 `lib/plugins/plugin_library.dart`。未改变玻璃材质、不透明度、内容库协议或 SDK。普通按钮继续沿用原点击效果，只调整设置导航入口。

## 原因与诊断

上一轮仅测试预热后的连续切换，遗漏了首次点击入口的 GPU 编译成本。本轮把组件列表首开、编辑首开、编辑返回、列表返回分别采样。

Windows Skia 完整追踪中，首开出现约 19 ms 的 `FillRRectOp / shader_compile`，其中驱动链接约 17 ms。只更换页面过渡仍有约 22 ms 的绘制峰值；直接调用同一导航回调而不触发按钮水波纹时，列表首开降至 5.544 ms。最终调整入口反馈后，实际 `tester.tap` 同样消除了这一主要峰值。

尝试过离屏图形预热与玻璃填充改写，未见稳定收益，已撤回；最终没有新增启动预热或提前加载插件/页面。没有通过绕过点击或丢弃冷启动帧来生成最终成绩。

## Windows Profile 测量

本机 1180×820、内存内容库 300 张卡片；每次采集在新进程中完成。使用 `flutter drive --profile`，计时运行关闭 Skia 追踪，也不并行执行编译或单元测试。下表为各阶段 Raster 最大帧耗时：

| 场景 | 磨砂修改前 | 磨砂最终复测 | 液体玻璃补测 |
| --- | ---: | ---: | ---: |
| 列表首开 | 24.776 ms | 4.864 ms | 8.812 ms |
| 编辑首开 | 23.752 ms | 15.394 ms | 19.882 ms |
| 编辑返回 | 5.712 ms | 9.526 ms | 9.517 ms |
| 列表返回 | 6.080 ms | 6.847 ms | 6.817 ms |

- 磨砂最终四阶段 UI 与 Raster 均无超过 16.7 ms 的帧。随后六轮共 24 次路由变化，351 帧中 UI 最大 4.970 ms、Raster 最大 8.829 ms，均无超预算帧。
- 液体玻璃补测的编辑首开仍有 1 帧 19.882 ms；六轮连续切换中另有 1 帧 21.211 ms（355 帧）。因此不宣称液体玻璃已全程满足 60 Hz 帧预算。其余阶段及 UI 线程未出现超预算帧。
- 液体玻璃补测在补齐 Material 等效路由语义边界前完成；最终磨砂复测和单元回归包含这一调整。它不改变绘制或材质实现。
- 这些是本机 Flutter 窗口的限定流程，非全部显卡、鼠标实操、插件 IO 服务加载或用户原库的性能保证。插件和 IO 设置已接入相同路由与入口反馈，但未单独测量真实服务负载下的冷启动。

证据：`build/settings-cold-before.json`、`build/settings-direct-navigation.json`（仅诊断）、`build/settings-navigation-feedback.json`、`build/settings-cold-frosted-final.json`、`build/settings-cold-liquid-final.json`。完整 Skia 诊断：`build/settings-first-trace-full.json.timeline.json`。带追踪结果只用于原因分析，不作为上述帧耗时成绩。

驱动 `integration_test/settings_navigation_profile_test.dart` 默认采集实际测试点击；`MORROW_PROFILE_DIRECT=1` 仅供隔离入口反馈，`MORROW_PROFILE_TRACE=1` 配合 `--trace-skia --endless-trace-buffer` 留存完整追踪；`MORROW_PROFILE_GLASS=liquid` 切换材质。最终输出记录材质、输入模式和是否追踪，避免混用诊断结果。

## 回归与预览

- 39 项 Flutter 组合回归通过：过渡中返回、精确结果与草稿/选区、减少动画、共享画布、响应式设置、组件编辑/保存、插件设置，以及四种背景的液体效果。日志：`build/settings-cold-tests-final.log`。
- 对最终 Release 中的 Rust 宿主与插件运行 2 项真实进程集成，内容与外观/语言保存、重开及保护恢复均通过；只使用临时内容库。日志：`build/settings-cold-native-tests.log`。
- 本轮相关 6 文件分析通过。日志：`build/settings-cold-analyze-final.log`。`git diff --check` 通过。
- `lib/main_rust.dart` Windows Release 构建成功。日志：`build/settings-cold-config.log`、`build/settings-cold-build.log`。

输出：`build/windows-corners/x64/runner/Release/morrow_studio.exe`。使用时保留整个 Release 目录。

产物摘要见 `build/settings-cold-artifact-hashes.json`：

- `morrow_studio.exe` SHA-256：`f4e9a5cd4b2bd33b6e156a6a84711e12c4656454bce886e69c141677550c5c39`
- `data/app.so` SHA-256：`605d24821ca171ff272cfe133918895786c879fd427340e55ea5c38584080c46`
- `morrow-workbench-host.exe` SHA-256：`8b1b79977781c8963a6937de42f73e134831308852b3a40ba2620e1e7eade225`
- `plugins/workbench.morrowplugin` SHA-256：`ee271fe12a70ed9ab0cadcca7822ab52df3247b192a79aff0b9465b78469ca61`

未调用 SubagentBridge 或其他子代理，未推送或发布，版本仍为 `0.1.9-test.52+56`。测试使用内存内容库，不修改用户原库。
