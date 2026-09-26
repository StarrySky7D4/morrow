# 开发分支同步与本地 Windows 构建

日期：2026-09-25。应用 `0.1.9-test.56+60`。

## 同步范围

`codex/io-safety-refactor` 从 `d3321cd60596679e7d92729dab7e33aa6c0093a7` 快进到 `d9c043191a400df972832d398b80cb73dfc51f56`，纳入主线 13 个提交，无冲突。保留原有 IO SDK 祖先提交。此次本地同步及修正未推送，不创建标签或 Release。

同步前的三个 Core 文件只存在索引/检出状态标记，Git 规范化后的 blob 与 HEAD 相同，没有覆盖未提交的功能代码。其他工作树及用户资料库未改动。

## 文档

- `docs/PLUGIN_SYSTEM_STATUS.md`：更新 test.56、Windows 交互、Web 本地执行及受限主题包进展，保留七类 SDK 开放项。
- `docs/DEVELOPMENT_BOARD.md`：追加当前基线和优先级，旧条目明确作为历史记录。
- `docs/WEB_PARITY.md`：修正版本与已接入能力，将旧阶段缺口和当前状态分开，记录 CI 失败及实际部署边界。

## 本地构建修正

1. `build_i18n.py --check` 最初报告 ARB 过期；重新生成后规范化 Git 内容无差异，实际原因是 Windows CRLF 检出与生成器 LF 字节检查不一致。`.gitattributes` 对生成器校验的文本固定 LF，未修改翻译语义。22 个产物检查和七项语言目录测试通过。
2. 默认打包器拒绝重新编译得到的同版本不同字节工作台 guest，保护规则保留。`tool/build_workbench_bundle.ps1` 增加显式保留原包参数，必须同时提供路径和精确 SHA-256；先读取并验证快照，已有 bundle 与其不同则拒绝覆盖。仍编译当前 guest 和宿主，但安装已验证的兼容包。缺失/错误哈希、与已有 bundle 不同的原包三种拒绝检查通过。
3. CMake 使用 Windows PowerShell，MSBuild 环境未能解析 `Get-FileHash`；脚本使用内置 SHA256 API，避免依赖模块自动加载。

本机保留包来自之前 test.55 发布暂存目录，与同步前的本地 bundle 字节一致：

- 路径：`dist/test55-release/staging/morrow-0.1.9-test.55-rust-workbench-windows/plugins/workbench.morrowplugin`
- 大小：878357 字节。
- SHA-256：`c71dc27866378894d5288053238fef9e0ef15f6b578449279bc681e7fe8b560e`。

此包不冒充 Web 部署所用的 `b28aea5...` 原包，也未声称与远端 test.56 ZIP 全部字节一致。当前源码编译的新 guest 留在 `build/first-party-plugins`，没有改包版本或自动替换用户已批准的同版本插件。

## 复现

在本工作树使用 PowerShell：

```powershell
python -X utf8 tool/build_i18n.py --generate
python -X utf8 tool/build_i18n.py --check
python -X utf8 tool/generate_workbench_client.py --check
flutter pub get --offline
$env:MORROW_RETAINED_WORKBENCH_PACKAGE = (Resolve-Path 'dist/test55-release/staging/morrow-0.1.9-test.55-rust-workbench-windows/plugins/workbench.morrowplugin').Path
$env:MORROW_RETAINED_WORKBENCH_SHA256 = 'c71dc27866378894d5288053238fef9e0ef15f6b578449279bc681e7fe8b560e'
flutter build windows --release --no-pub --target lib/main.dart
```

上述兼容包是本机保留的历史产物，不随源码仓库分发。其他工作树应提供经核验的自身兼容包及其摘要；未提供保留参数时仍按原规则构建和打包，字节变化需要正常升级包版本。运行时继续执行完整包/权限校验。

## 验证结果

- Windows x64 Release 构建通过，文件及产品版本均为 `0.1.9-test.56+60`。当前 guest 与 Rust 宿主均完成 release 编译，最终安装兼容原包。
- 43 项 Flutter/真实宿主组合通过，无跳过；覆盖瀑布流、排序、tips、即时刷新、组件颜色、主题管理与业务授权、记录/偏好持久化及保护文件恢复。
- 最终成品目录中的宿主与插件组合再次通过四项原生集成；它们是上述测试的产物复验，不作为新增覆盖相加。
- 最终程序使用本轮独立新库运行自检，退出码 0；原生合成 API、无声 WAV 解码与播放时钟、seek/互斥/恢复不自动播放、Rust 工作台渲染均通过，无 Flutter 错误。合成 API 自检不等于桌面像素效果验收。
- 语言 22 个生成产物、工作台协议生成检查、七项语言目录测试、三项原包拒绝检查通过。构建保留五项 Rust 未使用函数警告，不宣称全仓库无告警。

运行入口：`build/windows/x64/runner/Release/morrow_studio.exe`，须保留同目录 DLL、data、plugins 和宿主。EXE SHA-256：`2ec39c2ca52e8d18cb64da24dc7ac2019eccba59979627a5cca89f69cbff49f2`。

日志目录：`build/sync-test56-20260925/`，关键文件为 `flutter-build-verified.log`、`regression.log`、`final-bundle-tests.log`、`release-self-check.md`、`release-self-check.png`、`i18n-tests.log` 和 `bundle-rejection-tests.log`。失败的首次构建日志也保留，用于说明修正原因。

## 未完成边界

本轮未重新执行 Web/Android 构建、线上部署、全量 SDK 或公网互操作验收。Web CI 36115685547 的 `net::ERR_ABORTED` 仍需单独定位；不能以 Windows 构建通过替代浏览器门禁。完整 UI autosave、IO/服务 SDK 冻结与跨平台能力矩阵继续开放。

## 补充：主题插件编译

前述 Windows 构建没有编译独立中秋主题，本节补齐该交付。

- 执行 `tool/build_mid_autumn_theme.ps1`，编译 Rust Wasm 和独立包。
- 首次本地 1.0.0 重建与已发布原件字节不同；对嵌入 JSON 固定 LF 后仍有差异，不冒充可复现的发布原件。为避免同版本摘要冲突，将主题清单、Cargo/lock、打包器与脚本版本一致更新至 **1.0.1**。界面效果、插画和权限不变；应用版本仍为 test.56+60。
- 交付：`dist/plugins/morrow-mid-autumn-1.0.1.morrowplugin`，658,949 字节；SHA-256 `34230d83e4fb0d5883630973a6261e275cf0ca63a4d37e61f6b6d307b5dfacdd`。同目录 `.sha256` 为校验文件。
- 最终 Windows 成品宿主上两项真实集成通过：新包独立导入/启用/恢复/卸载；从固定的已发布 1.0.0 原件升级至 1.0.1，重启读回新摘要，业务插件授权与启用状态不变。
- 日志：`build/sync-test56-20260925/theme-build-1.0.1-final.log`、`theme-1.0.1-final-native-tests.log`。早期重建包留在本轮 build 证据目录，未作为正式 1.0.0 输出交付。
- 没有预装到用户资料库，也没有上传或创建 Release；1.0.1 的浏览器验收未在本轮执行。
