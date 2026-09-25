# 组件调色与 Windows 崩溃修复

## 交付

修复版客户端：`build/mid-autumn-v3.1-windows/Release/morrow_studio.exe`，保留整个 Release 目录。先保存并退出旧客户端，再启动此版本。主题包仍为 `dist/plugins/morrow-mid-autumn-1.0.0.morrowplugin`，无需重装，SHA-256 仍为 `B9C8DD591EF88E2D14BE4EE7F4B402FF5D6563530618F71CBAF1E4D1B61D9751`。

## 问题与修复

用户最终确认点击调色弹窗的「应用颜色」就退出。Windows Application 日志中，v3 客户端在 2026-09-25 13:43 的故障为 `flutter_windows.dll + 0xf5d3`，异常代码 `0xc0000005` / `0xc000041d`。使用同版引擎本地 PDB 定位到 `FlutterPlatformNodeDelegateWindows::HitTestSync`，不是 Rust 插件保存异常。

- 默认主题模式也隐藏组件调色及继承颜色按钮；主题接管组件颜色，保留原自定义颜色供停用主题后恢复。透明度、玻璃和轮廓仍可叠加，完整材质覆盖时继续隐藏整组无效设置。
- 调色弹窗依据实时主题状态撤销编辑，保存回调本身也拒绝过期操作。关闭主题不会重新激活旧弹窗的提交资格。
- 「应用颜色」不再先改输入框、触发一次实时预览然后关闭，而是只返回校验后的最终颜色。调用方等到弹窗反向动画和 overlay 移除完成后再更新组件。
- 设置页/调色弹窗的透明度动画保留语义树至路由完成移除，避免在零透明度帧提前拆掉节点。
- 组件编辑控件保留稳定的语义容器，在可编辑状态切换时作为完整子树重建。草稿在页面 State 中保留，避免 slider/focus 节点的增量重挂载造成 Windows AXTree 孤立或待补节点。未关闭无障碍支持，未修改全局 Flutter SDK 或引擎 DLL。

上游也记录了类似的 Windows AXTree 不完整更新问题：[Flutter #182444](https://github.com/flutter/flutter/issues/182444)、[相关尚未合并的修复分析 #190344](https://github.com/flutter/flutter/pull/190344)。这些是诊断参考；本次采用应用内节点生命周期修复，没有宣称该上游问题就是唯一根因。

## 验证

- 35 项不同的 Flutter 回归最终通过：组件调色与过期回调、原全局配色、主题/插件兼容、设置导航、响应式草稿/重载、原材质与风格流程。主回归日志 `build/component-color-final-tests.log` 中 34 项通过，1 项遇到已有测试把 `NeumorphicChoiceChip` 当作 `ChoiceChip` 的旧类型假设；改为检查其真实 ChoiceChip 子控件后，该文件 2 项通过，见 `build/component-color-responsive-final.log`。不重复计数。
- 静态分析 7 个相关文件通过：`build/component-color-analyze.log`。
- 真实 Windows Release、普通 WidgetsFlutterBinding、同一持续窗口中的流程通过：连续 3 次应用颜色、保存组件；在调色弹窗打开时启用主题，验证提交禁用；完整覆盖隐藏保存；卸载主题效果后恢复控件和原颜色。
- 上述过程中，原生 MSAA 辅助进程对本测试进程执行 **45,765 次**命中查询，均成功；进程退出码 0，最终 stderr 无 Flutter 引擎 AXTree 错误。证据：`build/component-color-live5-{report.txt,msaa.log,stderr.log,exit.txt}`。仅添加按钮隐藏/提交保护的前几轮仍出现 AXTree 错误，未作为最终资格结果。
- 新的交付客户端使用全新测试库和真实已安装主题登记运行工作台自检：原生窗口、工作台渲染、音频实际解码/时钟、进度跳转、互斥及禁止自动播放通过，退出码 0。见 `build/mid-autumn-v3.1-qualification/check.md`、`stderr.log`、`exit-code.txt`。

原生复现工具源码：`tool/component_color_windows_smoke.dart`、`tool/windows_accessibility_probe.cpp`。使用 MSVC 编译后者（链接 ole32、oleaut32、oleacc、user32、shell32，GUI 子系统），Flutter assemble 将前者编译为独立 Windows release 资源。运行时指定 `MORROW_ACCESSIBILITY_PROBE`、`MORROW_ACCESSIBILITY_LOG`、`MORROW_COLOR_TEST_REPORT` 三个绝对路径，工作目录为项目根目录。除 Dart 检查结果外，必须检查进程退出码和 stderr 中是否出现 `AXTree` / `[ERROR:flutter/`；只看 PASS 文字不足以确认原生资格。

## 构建边界

交付 AOT SHA-256：`10001C7CD04E4128A30DA82C73A0E38F2431AE43D62CB794507B9D07BC46D6DE`。引擎仍为 v3 相同的 `61B77BC881F5C57AF5F3EEAC4A96FFD9FD87E35B77565F44C678DE105C92C2ED`。基于 v3 目录的 runner、宿主和 native 依赖组合重新编译的 Flutter UI，输出独立 v3.1 目录；不是新安装程序。v3 目录和用户真实资料库未覆盖。

最初尝试标准 Windows integration_test 构建时，遇到既有 business guest 同版本字节校验及同时进行的 Debug 构建 PDB 冲突，随后改用独立 assemble 输出和既有同版 native 依赖；没有绕过 guest 的版本保护。
