# 风格化滑块的交互与绘制契约修复

日期：2026-09-24。工作树 `codex/io-safety-refactor`，基线 `e89f4ff`；应用 `0.1.9-test.54+58`、内置插件 `.54.5` 不变。只更新本地源码、测试与 Windows 预览，未提交、推送或发布。

## 问题与改动

新拟态轨道按 `isEnabled` 直接换色，忽略原生 `enableAnimation`；滑块按钮仍渐变，轨道却提前跳到禁用终态。同时，自绘轨道没有处理 `secondaryOffset`，使 Material Slider 的 `secondaryTrackValue` 缓冲段无显示。使用原始绘制接口的两项像素测试均先复现失败，日志 `build/style-slider-baseline.log`。

现在轨道的颜色与凹槽强度跟随启用动画；启用/禁用布尔量继续供原生交互和几何使用，不再跳过颜色动画。轨道遵循有效 SliderTheme 的活动、未活动、缓冲及禁用颜色，默认颜色移至主题层，保留原有启用外观。缓冲段只在当前值前方绘制，遵循 LTR/RTL，裁剪在轨道内；零轨道高度不画阴影或内容。

新拟态与五种实验风格的自绘滑块按钮也遵循有效 `thumbColor`、`disabledThumbColor` 及启用动画。实验按钮的普通描边不再把主题线条原有透明度覆盖成不透明；器物刻线、粗野主义硬描边及其他风格形状保留。

## 验证

- 最终 `build/style-slider-final-tests.log`：**43 项相关回归通过，无跳过**。包含前轮 39 项按压、输入、透明画布、保存重开、金图、状态和收起回归，以及本轮新增四项。
- 轨道像素验证五个动画位置：相同动画位置的像素不因交互布尔值改变而跳变；完整启用与禁用终态确有差异。
- 缓冲段在两种书写方向出现；落后当前值时不绘制。显式传入的绿色缓冲/红色活动颜色得到使用，零高度完全透明。
- 六种非扁平风格 × 五个启用位置，实际像素验证滑块按钮在显式启用/禁用颜色之间插值；取样避开器物握柄刻线。
- 实际 Material Slider 在 LTR/RTL 中保持按下并依次切换七种风格，原 State 与焦点保留；拖动开始/结束各一次，连续值变化正常，方向键遵循文字方向，禁用后鼠标及键盘不再触发数值更改。
- `build/style-slider-final-analyze.log`：两个绘制文件及新增测试无静态问题；定向差异检查通过。本轮未更新金图；已有浅深新拟态金图普通比较通过。
- 已检查本轮生成的黏土浅色、器物深色工作台截图；其他截图仅完成测试渲染，未逐张人工审查。
- Windows Release `INSTALL` 退出 0，日志 `build/style-slider-windows.log`，正式入口 `lib/main_rust.dart`。本地输出 `build/windows-corners/x64/runner/Release/morrow_studio.exe`；保留同目录依赖运行。源码、日志、所检截图及产物摘要见配套 JSON。

## 仍开放

当前 Flutter SliderThemeData 对 thumbShape/trackShape 在动画中点离散切换；本轮保证其交互身份，尚未把几何形状改为连续变形。开关自身的风格切换、首绘 Raster、Android/Web 与物理设备验收仍开放。

缓冲段是组件层支持；现有随身听尚未提供/接入实际缓冲进度，不宣称播放器已展示网络缓冲。没有修改播放器任务、数据持久化或插件协议，也没有新增 GPU 性能结论。正式 autosave owner、SDK 与全平台资格继续按原看板推进。
