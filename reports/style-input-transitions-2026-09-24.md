# 七种风格的输入边框连续过渡

日期：2026-09-24。工作树 `codex/io-safety-refactor`，基线 `e89f4ff`，应用保持 `0.1.9-test.54+58`，内置插件保持 `.54.5`。本轮不提交、推送或发布。

## 发现与修复

上一轮五种实验风格之间已有边框插值，但新拟态与它们互切、回到扁平时会落入 Flutter `OutlineInputBorder.lerpFrom`，丢失额外凹槽；新拟态浅深主题切换仍通过离散 dark 布尔量在中点换色。扩展像素测试先复现失败：新拟态浅色到纸感浅色的起始帧 alpha 差异/原图 alpha 总量约 0.958，阈值 0.02。原始失败日志 `build/style-input-all-baseline.log`。

新增 `StyledInputBorder` 与固定大小 `InputRelief`，统一七种风格的凹槽权重、明暗参数、圆角、描边与标签间距插值。扁平的正式主题也采用相同基类，避免目标边框先吞掉源边框的自定义绘制。新拟态和实验边框沿用各自端点表现；混合中间值仍可作为新过渡起点，最多保留新拟态/黏土/器物三种凹槽参数，没有嵌套旧边框、保存 Widget 或引入额外图层。

统一凹槽的浮动标签缺口：遵循本机 Flutter 的 gapStart 坐标及 gapPadding，在 LTR 与 RTL 均避让标签。普通 Material 输入仍拥有焦点、IME、选区与文本状态。没有修改存储、协议、字体或风格默认选择。

## 验证

- `build/style-input-final-tests.log`：36 项定向回归全部通过，无跳过，包括原有两张新拟态金图、控件交互、风格保存重开、透明画布、表面光照与收起行为。
- 输入像素检查覆盖七种风格 × 两种主题共 14 个端点，182 条有向路径的起始、结束与中间透明区域；另外检查新拟态明暗中点、跨风格中途反向以及复制/缩放后的像素保持。
- 实际 `TextField` 在连续 14 次风格/主题切换期间保留同一 State、焦点、完整 TextEditingValue、IME 组合区与选区。
- 新拟态/黏土/器物三种凹槽 × 两种文字方向，实际像素确认浮动标签缺口清空，原无缺口参考非空。
- `build/style-input-final-analyze.log`：本轮五个 Dart 源码/测试目标无静态问题；定向 `git diff --check` 通过。原有金图未改写。
- Windows Release `INSTALL` 构建退出 0，日志 `build/style-input-windows.log`；正式入口仍为 `lib/main_rust.dart`。输出 `build/windows-corners/x64/runner/Release/morrow_studio.exe`，运行需保留同目录依赖。最终源码、日志和产物 SHA-256 见配套 JSON。

## 边界与下一步

这是输入边框视觉连续性与状态保持修复，不是 GPU 加速数据；没有新增 Profile、Android/Web 或人工物理输入验收。任意第三方显式自绘边框不自动接入本基类。首次 Raster 峰值、按钮/滑轨全路径过渡、插件自绘控件仍需继续验证。

同时补齐上一轮历史提交证明的成品包四项原生验证，并完成其独立报告；不把这四项计入本轮 36 项 UI 回归。正式编辑器自动保存、工作台 owner 接线、SDK 与全平台资格继续开放。
