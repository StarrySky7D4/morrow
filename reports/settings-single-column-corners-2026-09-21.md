# 设置单列与窗体圆角（2026-09-21）

## 本轮修改

- 主设置“展开完整设置”移动到标题行最右侧，保留原图标、提示与打开行为。
- 主设置、插件、IO 和组件设置共用的 `SettingsSections` 改为单列、横向铺满，区块间距 20；宽屏不自动分栏，缩放不重建草稿。选项按钮内部仍可换行。
- Flutter 在合成画布、标题栏与边框后统一抗锯齿裁剪，避免重叠边缘色缝。原生整数 HRGN 仅作为留出 2 个物理像素余量的命中区域，避免截断 Flutter 的边缘覆盖。
- 原生磨砂窗口使用独立圆角几何裁剪，与画布半径、DPI、最大化状态及顶部偏移一致，防止磨砂填满透明圆角。几何更新失败时释放磨砂层。

几何裁剪接口参考 [Microsoft CompositionGeometricClip](https://learn.microsoft.com/en-us/uwp/api/windows.ui.composition.compositiongeometricclip?view=winrt-26100)。

## 验证

- 16 项相关 Flutter 测试通过：按钮右对齐，700/1540 宽度单列与区块间距，320/700/1540 组件草稿保留，完整设置开关，外观与圆角裁剪策略。
- 6 个相关 Dart 文件分析通过；`git diff --check` 通过。
- Windows Release 构建成功，独立输出到 `build/windows-corners/x64/runner/Release`。原 Release 窗口正在运行并占用可执行文件，因此没有覆盖或关闭该窗口。新预览的宿主和内置插件 SHA-256 与当前配套输出一致。
- 新 Release 的 `--canvas-check` 实际屏幕合成检查通过。100% 缩放下，半径 8/20/32 的四角均存在部分覆盖像素，分别为 `[9,9,9,7]`、`[28,28,28,20]`、`[43,43,43,35]`；圆角之外分别检查 12/212/648 个像素，磨砂无越界。零不透明度、渐进低强度磨砂、标题栏、前景清晰、染色与透明恢复也通过。
- 像素探针只在显式资格模式下读取自建背景和应用覆盖的四个小区域；使用临时位图采样并释放 GDI 对象，不打开内容库。
- Windows 集成：原生形状与附件回归 2 项通过，包含调整大小、最大化与恢复；设置视觉流程 1 项通过，覆盖宽屏单列、深色组件编辑、透明画布和窄屏设置。两文件连续启动时第二个窗口曾遇到 Flutter 调试连接失败，单独重跑视觉流程通过。已检查生成的主设置和展开页图像，位于 `build/single-column-visual`。

日志：`build/single-column-tests.log`、`build/single-column-windows-tests.log`、`build/single-column-visual-test.log`、`build/corners-native-pixels.txt`、`build/corners-preview-build.log`。

本轮没有修改 SDK、协议或版本号，没有推送或发布，也没有调用 SubagentBridge。实测像素证据限当前 Windows 和 100% 缩放；未据此声明多屏混合 DPI 或 GPU 性能验收。
