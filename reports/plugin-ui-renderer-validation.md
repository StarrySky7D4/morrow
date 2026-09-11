# Flutter 插件界面渲染验证

日期：2026-09-11；Windows x64，Flutter 3.44.0／Dart 3.12.0，Chrome/152.0.7977.83；应用基线 0.1.9-test.10，UI 契约 v1。基础渲染包未接入主应用资料库与插件管理入口。

## 实际通过

- `tool/verify_plugin_renderer.ps1 -Web` 最终完整运行成功：重建 Rust 文档并核对样本 SHA-256、核对离线生成的 Web 模型、Flutter 包与独立示例分析、8 项原生控件测试、Rust 检查真实控件事件、Flutter Web release 构建和实际 Chrome 交互。
- 控件测试覆盖 900／320 像素宽度和 2 倍文字缩放、主题改变时的本地草稿／选区／焦点保留、显式字段快照覆盖不发事件、模拟输入法组合结束、UTF-8 上限与控制字符、删除／禁用／旧视图回调和语义标签。
- 原生实际 TextField 输入“从 Dart 编辑🌈”后，可信测试适配器将 UiIntent 编成 UiEvent。Rust Session 接受原字节，核对 UInt64 最大 generation、中文／emoji、序号与视图；重放及关闭后的事件拒绝。
- Chrome 实际加载普通 Flutter JS／CanvasKit 页面，在 360×640 下由 CDP 点击输入控件、发送选择全部按键和 Unicode 文本输入，再点击置顶与应用。独立检查收到 `editText|title|浏览器编辑|false`、`setToggle|pinned||true`、`activate|apply||false`，不是调用控制器直接赋值。
- 已查看原生浅色窄屏、深色宽屏及实际浏览器截图；中文、emoji（原生样本）、输入框、开关和按钮可见，无所测尺寸的溢出。
- 共享模型拆分后，`tool/verify_plugin_ui.ps1 -Web` 的客户端分析、15 项 Dart 测试、原生消息探针、Dart/Wasm Chrome 探针和 Rust 接纳均再次通过。修改共享浏览器工具后另行执行 `node tool/test_core_browser.mjs --ui` 和 Rust `check-web`，仍通过。
- Rust `cargo clippy ... --example ui_vectors -- -D warnings` 通过，格式与差异空白检查通过。

完整日志：`build/ui-renderer/renderer-verification.log`；二进制链回归日志：`build/ui-renderer/protocol-verification.log`。

## 产物

以下 SHA-256 为本次本地实际文件。截图受字体、引擎和环境影响，不用其固定摘要替代后续视觉检查。

| 文件 | 字节 | SHA-256 |
| --- | ---: | --- |
| `build/ui-renderer/light-narrow.png` | 10870 | `85a88eaf7ac0e849921bee0a5b9ca582b086ceafd43893cca04580c6c4ae9cec` |
| `build/ui-renderer/dark-wide.png` | 11955 | `55bde204cbc1786e73d06f993bf1b23ee0af0c07deb233b5c81035007239db3c` |
| `build/ui-renderer/browser-narrow.png` | 11123 | `e95b36abdd383afe09b99dffdcb5f95ec1ec899346c35cd4a158ef4003d6cc7e` |
| `build/ui-protocol/widget-event.capnp` | 176 | `1e2a7c412a548b29a45e08b6e4b6553ccd9bfa1d78eae9f0cf18f3775af8b6a6` |

## 调试发现与复现条件

普通 Dart JS 不能直接编译既有 Cap’n Proto 生成器中的部分精确 64 位反射字面量。渲染器改用独立 `ui_models.dart`；原生与 Dart/Wasm codec 保留全 UInt64。Web 示例的类型化模型由独立 Rust 二进制离线解码生成，并由脚本检查一致性，未改运行期协议。

Flutter 3.44 的 Windows Chrome 单元测试服务器为 CanvasKit 返回 404，因此改用正常构建的静态页面和本地资源；未修改全局 SDK。外部默认字体未加载时，初始页面文字与布局不可用；验证页面显式加载本机字体后正常。测试服务器只提供指定字体文件，字体没有复制到仓库或发布构建。原生与 Web 可通过 `MORROW_UI_TEST_FONT`／`MORROW_UI_TEST_EMOJI_FONT` 指定本机字体；默认 Windows 雅黑及 Segoe UI Emoji。

## 范围限制与下一步

原生控件与 Rust 事件验证使用可信测试适配器；浏览器渲染和 Dart/Wasm 二进制消息分别验证，未完成应用级 JS 控件与 Wasm 核心桥接。没有实际三语言 guest UI 输出、包 UI 扩展点注册、任务事件确认／背压、权限绑定、持久草稿与正式内容提交。

输入法组合通过控件测试模拟，未完成物理输入法、读屏、Android／iOS／macOS／Linux 实机或其他浏览器验证；主题测试不是主应用色彩罗盘、玻璃、侧栏动效的整合证明。Web example 需要本地测试字体路由，不是可独立部署的产品页面。本轮没有主应用全平台打包、远端推送或 Release。

后续顺序见 [渲染接入边界](../docs/PLUGIN_UI_RENDERER.md)。M6-06 仅推进基础渲染，整个插件系统仍在建设。
