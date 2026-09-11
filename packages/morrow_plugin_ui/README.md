# Morrow 插件界面渲染包

0.1.9-test.10 的内部 Flutter 包。将经过校验的声明式表单渲染为宿主控件；第三方作者仍使用 C／C++／Rust，首期不需要 Dart guest，也不接入 TS／JS guest。

```dart
import 'package:flutter/material.dart';
import 'package:morrow_plugin_ui/morrow_plugin_ui.dart';

Widget buildForm(UiDocumentModel document, Object viewIdentity,
    ValueChanged<UiIntent> handleIntent) => PluginForm(
  document: document,
  viewIdentity: viewIdentity,
  onIntent: handleIntent,
);
```

调用环境需要 Flutter Material 的本地化与有限宽度约束，通常放入 MaterialApp 的页面／面板。`viewIdentity` 由可信宿主分配，必须在视图或 generation 变化时更换；同一视图普通重建时保持稳定。原生 `UiDocument.decode` 的返回值可直接传入。

- 提供行列、文本、按钮、输入框和开关；窄屏行自动折行，正文按宿主主题映射三种颜色角色。根使用透明 Material，控件装饰继承宿主 Theme。
- 文本编辑、输入法组合、选区和焦点保留在 Flutter。只有确认后的合法文本发出 `UiIntent`；UTF-8 超限提交整体恢复，不截断 Unicode。
- 相同节点值的主题／布局／快照更新保留本地编辑；显式文本／上限更新覆盖输入但不伪造用户事件。新视图清空旧局部状态。持久草稿、冲突与恢复策略由宿主接入层负责。
- `UiIntent` 是本地类型化数据，携带 node、action 和事件值，不是插件授权、核心提交或保存回执。包不分配协议序号、不创建任务、不访问 Store。

`ui_models.dart` 与 Cap’n Proto 绑定解耦，允许普通 Flutter Web JavaScript 编译。浏览器示例使用从独立 Rust 二进制样本离线生成的类型化测试模型；没有将消息协议改成 JSON。完整应用级 JS／Wasm 消息桥接尚待实现。

## 本地验证

先分别在 `packages/morrow_core_client`、本包和 `example` 解析锁定依赖，再在仓库根运行：

```powershell
pwsh -File tool/verify_plugin_ui.ps1 -Web -Python python
pwsh -File tool/verify_plugin_renderer.ps1 -Web
```

原生控件测试覆盖输入法、字节限制、主题与草稿保留、窄屏、失效回调及语义标签。实际控件编辑编码成 Cap’n Proto 后由 Rust Session 验证。`-Web` 构建独立预览，使用本地 CanvasKit，通过真实 Chrome 点击、键盘输入和开关操作检查事件，保存截图。

本地截图测试使用系统字体，默认 Windows `msyh.ttc` 和 `seguiemj.ttf`；可以用 `MORROW_UI_TEST_FONT`／`MORROW_UI_TEST_EMOJI_FONT` 指定可读字体。Web 验证服务器仅监听回环地址，从指定本机文件提供测试字体，仓库和构建产物不复制这些字体；example 需要此测试服务器，不是可独立发布的产品页面。其他平台需提供适当字体并另行验收。

见 [界面接入说明](../../docs/PLUGIN_UI_RENDERER.md) 和 [验证记录](../../reports/plugin-ui-renderer-validation.md)。三语言 UI 构造器、包 UI 注册、实际插件事件桥接、主应用接入及持久草稿尚未完成。
