# 中秋主题插件交付记录

> 此为第一版历史记录。已有插件登记的兼容缺陷已在第二版修复；请使用 `reports/mid-autumn-v2-2026-09-25.md` 中的新预览入口，勿继续使用下方旧预览。

## 实现

- 新增内置声明式主题「中秋 · 月满庭」，默认关闭；加入现有界面风格列表，支持加载并启用、切换到基础风格、再次启用和卸载。
- 月白桂金 / 黛青月金两套配色；矢量圆月、远山、祥云、桂花与玉兔首页装饰。
- 共享 Palette 和 Material Theme 覆盖工作台、标题栏、导航、面板、卡片、按钮、输入框、选择控件、菜单及弹窗；保留原 Widget 身份和交互。
- 独立本地主题状态键 `morrow.ui-theme-plugins.v1`，保存成功后才切换。未改变既有 Rust 偏好协议或内容存储。原始外观设置保持可恢复。
- 加载损坏资源、存储失败时保留基础界面或当前主题，并提供重试提示。完整行为和覆盖边界见 `plugins/mid_autumn/README.md`。

## 验证记录

- 相关测试组合 42 项通过：主题生命周期、损坏资源、保存失败重试、独立本地持久化、配色文字对比度、真实 asset 加载、原偏好不变、搜索输入与编辑器草稿不丢失、切换基础风格和重启恢复；另覆盖既有风格、滑块、复选框、开关、透明材质和过渡。
- 日志：`build/mid-autumn-tests.log`。
- 补充 1 项窄屏（300 px）双倍字号、减少动画场景下的加载/卸载测试通过，累计 43 项；日志 `build/mid-autumn-accessibility.log`，该测试静态分析也通过。
- 本轮变更源文件及测试静态分析无问题：`build/mid-autumn-analyze.log`。
- 实际 Flutter 工作台截图：`build/mid-autumn-preview/white.png`、`build/mid-autumn-preview/dark.png`，已人工查看配色、布局和装饰。这些是 widget 测试渲染，并非运行中的原生窗口截图。
- Windows 使用独立构建目录 `build/mid-autumn-windows/`。构建结果及原版 EXE 校验记录写入 `build/mid-autumn-build-result.json`；构建日志 `build/mid-autumn-windows-build.log`。

## Windows 试用版

入口：`build/mid-autumn-windows/windows/x64/runner/Release/morrow_studio.exe`。须保留整个 Release 目录中的依赖。

完整 `flutter build windows --release --no-pub` 在新 UI 的 AOT 与资源生成后，遇到现有业务 guest 打包检查：`Bundled guest bytes changed without a package version bump`。本次没有改变业务插件版本、替换旧 guest 或取消该检查。试用版采用新生成的 `build/windows/app.so`、`build/flutter_assets`，配合原 `build/windows/x64/runner/Release` 的 Windows runner、引擎、原生依赖、Rust 宿主和原 guest。原引擎与当前 Flutter 引擎 SHA-256 完全一致；打包后的主题 JSON 与源文件 SHA-256 一致。所有文件组装到全新目录，不覆盖旧 Release。

原 EXE SHA-256 保持 `F21A5FFBE3530008BDA2A35860E89F2A8FD5683188768ED8D3A96A11E0D75BCF`；原 guest 和新试用版 guest 字节一致。主题 AOT SHA-256 为 `68C5BF1A5D552A0A2075647CF6E05E708A74E118E978931029F914B14A4D9CDB`。

试用版通过原生 `--startup-check`，使用独立、全新的数据目录。最终退出码 0；实际显示工作台，未记录 Flutter 渲染异常。日志和时间记录：`build/mid-autumn-startup-final/`。初次约 2 秒显示工作台，缓存后复验约 0.6 秒；这里只报告本机启动检查，不作为性能基准。主题加载/卸载的完整交互由上述 widget 测试验证。

未来若需要从头重新发布整个 Rust 包，仍需单独处理既有 guest 字节与包版本一致性问题；本次主题源码没有修改 Rust 业务协议。构建时临时设置的 Flutter 输出目录已恢复。

未启动用户现有工作台或更改其真实偏好。当前 Git 工作区已有的 Web 部署相关改动保持原样。
