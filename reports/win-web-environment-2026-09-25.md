# Windows / Web 开发环境与兼容性修复

Windows 开发者模式已获用户明确授权并启用，Flutter 插件符号链接创建及依赖安装成功。

## 源码修复

- 工作台 Cap'n Proto 生成器复用核心客户端的条件导出方案：原生版本保留反射元数据；Web 版本去掉不适用于 JavaScript 数字精度的可选反射信息。完整 schema ID 以十六进制字符串保存，并提供 BigInt 读取入口。修复写在生成器中，重新生成不会丢失。
- 8 份原生绑定的主体与修复前生成结果一致，协议摘要、布局和写入代码不变。
- 浏览器回归发现上游有符号整数 getter 的默认值 XOR 在 JavaScript 中返回无符号结果。Web 生成代码对 Int8/Int16/Int32 getter 恢复相应位宽的符号；已覆盖本项目 Int32 的最小值、最大值、-1 与 0，保留原生实现。
- Windows runner 单独启用 C++20，让 C++/WinRT 使用标准协程头，解决 MSVC 14.51 对旧 experimental/coroutine 的 STL1011 编译错误。

## 本机工具修复

沿用 Flutter 3.44.4 / Dart 3.12.2、Rust 1.96.0、Visual Studio Build Tools 2026 与 Windows SDK 10.0.26100.0。补齐 Cap'n Proto 1.4.0、LLVM 22.1.7、Chrome for Testing 154.0.8037.57、Rust wasm32-unknown-unknown 目标与 wasm-bindgen-cli 0.2.128；复用 Python 3.14.6，更新用户 PATH 和 Chrome 路径。

工具放在 `C:\Users\19825\Tools\MorrowDevTools`，wasm-bindgen 按现有脚本要求放在 `build/tools/wasm-bindgen/bin`。当前仓库设置 `core.autocrlf=input`，语言生成资源已恢复预期 LF 字节。

Flutter 3.44.4 的本机 Chrome 测试服务器将 URL 转成 Windows 文件路径后，再按正斜杠前缀匹配，导致 CanvasKit 返回 404。已将本机 SDK 的 `flutter_web_platform.dart` 中该表达式改为 `request.url.path`，重新生成 Flutter 工具快照。原文件备份在 `build/environment-backups/flutter_web_platform.dart.original`，旧快照保留为 `C:\flutter\bin\cache\flutter_tools.snapshot.before-canvas-path-fix`。该补丁仅存在于本机 SDK，不包含在项目源码中；更换 SDK 时需复查上游是否已修复。

## 验证

- `flutter build windows --release --no-pub` 通过。
- `flutter build web --no-pub --no-web-resources-cdn` 通过。
- 原生平台绑定及草稿、导入、交接、提案、版本化内容协议回归：37 项通过。
- Chrome 平台绑定回归：5 项通过，含精确 schema ID 和有符号 Int32 边界值。
- 元数据适配器 Python 测试：4 项通过。
- 工作台、核心客户端生成一致性检查通过；语言资源 22 项校验通过。
- 绑定静态分析仅有 `identity.dart` 中既有的 3 条常量命名提示；Windows 构建仍有既有 Rust 未使用函数警告。

构建产物：`build/windows/x64/runner/Release/morrow_studio.exe` 与 `build/web`。本次验证编译和相关协议行为，未进行完整应用 UI 验收，也不代表 Windows 插件宿主功能已迁移至 Web。
