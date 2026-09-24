# 风格选择的 Windows Profile 验证与模糊核优化

日期：2026-09-24。应用 `0.1.9-test.54+58`，内置插件 `.54.5`。本轮只更新本地代码和预览，没有提交、推送或发布。

## 场景与证据

新增 `integration_test/style_selection_profile_test.dart`，通过实际设置入口及风格选项的 tester 点击，依次选择新拟态与五种实验风格，再进行包含扁平的第二轮，共 13 次。选择后核对保存值、实际工作台风格和列表收起状态。每个材质在独立进程执行，Profile 模式强制检查；帧采集按 buildStart 时间过滤，在操作前后排空回调，分别保存 UI、Raster、总耗时及 16.7/8.3 ms 计数。

环境：Windows、RTX 4070 Ti、驱动 32.0.15.8157、请求窗口 1180×820、内存库 300 张卡片。没有真实宿主 IO、视频或第三方插件负载。第一轮是本进程首次选择，风格预览已显示，且没有清除驱动缓存，不能称为完整冷启动成绩。测量期间没有并行编译或单元测试。

完整各帧数据、两个原实现基线、两个中间实验和三个最终材质结果见 `style-selection-profile-2026-09-24.json`。原始日志和完整 Skia 追踪在 `build/style-selection-*`；追踪摘要与原文件摘要已归档。

## 诊断与改动

原实现的首次新拟态选择 Raster 最大帧分别为 85.373 和 101.094 ms（复测值精确值以 JSON 为准）；纸感分别为 42.905 和 31.708 ms。单独的带追踪诊断中，新拟态有 18 次 shader_compile，纸感有 8 次，涉及 FillRectOp/CircularRRectOp 等；这证明存在首次绘制编译成本，不能将它全部归因于一个控件。

新拟态、黏土和 Fluent 的动画此前随 depth 改变模糊核半径。最终将核半径固定，保留几何与透明度变化。仅固定模糊核的两个中间实验中，新拟态首次峰值约 47 ms，但像素检查发现接近零的下沉阴影仍过重，因此没有直接采用该中间版本。最终增加下沉光照强度随深度淡入，并保留透明中心及原有明暗插值。

这是限定场景的改进证据，不证明所有着色器编译已消除，也不宣称固定核是所有峰值的唯一原因。没有新增全量启动预热。

## 最终实现结果

| 材质 | 首轮 UI 最大 | 首轮 Raster 最大 | 第二轮 UI 最大 | 第二轮 Raster 最大 | 首轮/第二轮 Raster 超 16.7 ms 帧 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 磨砂 | 13.163 ms | 68.610 ms | 14.698 ms | 13.798 ms | 5 / 0 |
| 超透 | 11.379 ms | 52.199 ms | 11.238 ms | 22.984 ms | 8 / 1 |
| 液体玻璃 | 13.414 ms | 58.058 ms | 11.857 ms | 12.526 ms | 8 / 0 |

三材质共 39 次选择全部通过，最终实现所有采样 UI 帧没有超过 16.7 ms。仍存在首次 Raster 峰值和超透第二轮一帧超预算，不能宣称全程 60 Hz，更不能宣称 120 Hz 达标。超透/液体只有最终实现的本轮测量，没有其改动前对照，不能声称两者性能提升百分比。

## 回归、产物与后续

- 48 项相关 UI 回归全部通过，新增零深度附近阴影像素验证；日志 `build/style-kernel-final-tests.log`。四个源码/测试/采集目标静态分析通过。
- Profile 三场景通过；Windows integration_test 的原生插件提示仍出现，但 flutter drive 通过 VM Service 返回成功，且逐阶段 JSON 完整；不将其记为物理鼠标或移动设备验收。
- 测试后恢复正式 `lib/main_rust.dart` 配置。最终 Windows Release 构建及产物摘要单独记录于 `style-kernel-build-2026-09-24.json`。
- 下一步针对首次绘制的阴影/裁剪/渐变组合继续分项追踪，验证更多主题、快速连续切换、真实插件负载及资源占用。SDK、编辑持久化、跨平台等原看板开放项保持不变。

复现：设置 `MORROW_PROFILE_GLASS=frosted|clear|liquid` 与绝对 `MORROW_PROFILE_OUTPUT` 路径，执行 `flutter drive -d windows --profile --no-pub --driver tool/settings_profile_driver.dart --target integration_test/style_selection_profile_test.dart`。再次运行可加 `--use-application-binary` 指向已构建 Profile exe 的绝对路径。诊断额外设置 `MORROW_PROFILE_TRACE=1`，并使用 `--trace-skia --endless-trace-buffer`；该模式只测前两个风格，带追踪结果不能与普通计时直接比较。
