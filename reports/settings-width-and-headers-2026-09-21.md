# 设置宽屏排版与统一标题栏（2026-09-21）

## 修改

- 主设置标题行的设置图标和展开按钮使用等宽 40 像素区域；两端对齐，避免副标题的弹性布局让按钮内缩。图标中心处于同一水平线，左右内边距相等。
- 保持设置页单列，画布仍覆盖整个窗口；主设置、插件、IO、组件列表及组件编辑页的内容统一居中，最大宽度 920 逻辑像素。窄屏留白 12，较宽窗口留白至少 24，区块仍保持 20 像素间距。
- 完整主设置和子页共用 `SettingsPageHeader`：返回按钮、18 像素半粗标题、相同留白和主题文字颜色。移除子页 AppBar 的玻璃层，不再将组件材质用于页标题；保留原共享背景画布。
- 完整主设置保留“返回工作台”提示及原退出回调，子页返回仍由 Navigator 管理，Escape、系统返回、页面过渡和编辑草稿不变。

## 验证

- 33 项 Flutter 组合通过：主设置、组件材质、插件/IO 管理及返回动效；1180/1540 宽度下验证图标中心对称，390/700/1540/2560 宽度下验证居中、单列和宽度上限。
- 8 个相关文件 Flutter 分析通过；直接 Dart 分析首次遇到本机 perf 临时目录清理错误，改用 Flutter 分析命令成功。无源码分析告警。
- Windows 设置视觉流程通过：1540 宽屏主设置、1180 工作台与深色组件页、透明组件编辑和 390 窄屏。检查实际生成图像，输出位于 `build/settings-polish-visual`。
- Windows Release 已按 `lib/main_rust.dart` 重建到 `build/windows-corners/x64/runner/Release`，包含更新后的 AOT、宿主和插件。配套宿主与插件摘要核对一致，构建日志为 `build/settings-polish-build.log`。

日志：`build/settings-polish-tests.log`、`build/settings-polish-analyze.log`、`build/settings-polish-windows.log`。本轮不修改原生圆角实现、SDK、协议或版本号，未调用 SubagentBridge，未推送/发布。
