# UI 契约与 Rust／Dart 双向验证

日期：2026-09-11；Windows x64，应用基线 0.1.9-test.10。独立 UI 契约 v1；内容、任务、guest ABI 与主资料库均未切换。

## 实际链路

Rust 生成包含普通文本、输入框、开关和按钮的中文表单；Dart 原生进程读取并验证，独立构造编辑事件。Rust Session 接受 Dart 事件并核对中文／emoji、UInt64 最大 generation；重放及关闭后的事件拒绝。

Chrome/152.0.7977.83 中实际运行 Dart/Wasm，读取 Rust 表单并编码事件；浏览器测试器导出事件原字节，原生 Rust 再验证其内容、身份、序号与关闭边界。未通过浮点数传递 UInt64。浏览器导出的字节与原生输出均单独记录，不能以浏览器中的 PASS 字符串替代 Rust 检查。

## 测试与范围

- 核心原有 109 项默认回归、7 项故障恢复、严格格式／Clippy、自检和 wasm32 构建通过；UI 新增 4 项测试通过。
- UI 用例覆盖树形／叶节点父引用、重复 ID、自环、深度 8／9、节点 128／129、字段混用、输入上限、逐字节截断、schema 损坏、展开文本预算、跨视图／代次／修订、重复及越序事件、禁用控件和关闭视图。
- Dart 原有 10 项回归及新增 5 项 UI 测试通过（完整运行先通过 14 项，追加展开预算用例后单独重跑全部 5 项 UI 测试）；客户端分析无问题。不可变表单、同源 Rust 样本、异常树与消息、UInt64 和展开文本限制均覆盖。
- UI 样本由 Rust 重建并与提交样本逐字节摘要比对；Dart 原生及真实浏览器分别生成事件，由 Rust 读取核验。
- core-web 的实际 wasm32 配置检查通过。未进行 Flutter 控件渲染、三语言 Wasm guest UI 输出、实际插件事件调度或其他平台 UI 资格验证。

日志位于 build/ui-protocol：core-verification.log、verification.log、bounds-verification.log、dart-bounds-verification.log。

## 消息产物

以下文件位于 build/ui-protocol，SHA-256 是实际原字节摘要。

| 消息 | 字节 | SHA-256 |
| --- | ---: | --- |
| document.capnp | 592 | `77a2345d8f199ce100431ae4812f5ea37a727159256d02fb5ec34d7ac333b4df` |
| expected-event.capnp | 176 | `1e2a7c412a548b29a45e08b6e4b6553ccd9bfa1d78eae9f0cf18f3775af8b6a6` |
| dart-event.capnp | 176 | `1e2a7c412a548b29a45e08b6e4b6553ccd9bfa1d78eae9f0cf18f3775af8b6a6` |
| browser-event.capnp | 176 | `1e2a7c412a548b29a45e08b6e4b6553ccd9bfa1d78eae9f0cf18f3775af8b6a6` |

## 尚未完成

当前 Session 仅接纳待路由事件，不授予权限、不写内容、不管理持久草稿；尚未绑定实际插件连接。UI 构造 SDK、包扩展点、Flutter 渲染、事件任务桥接与生产序号确认仍需实现。主应用资料未迁移；未推送、未发布 Release。
