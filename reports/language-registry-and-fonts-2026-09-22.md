# 外置语言配置与自定义字体

基线：`0.1.9-test.54+58`，叠加本地 UI 生命周期优化。本轮不变更版本号，不提交、推送或发布。

## 已实现

- `packages/morrow_i18n/languages.json` 统一语言代码、自称、源语言及回退语言。生成器派生 Dart 清单、Rust 宿主允许列表、Flutter 配置、编译消息与摘要固定的 PB/LZ4 资源，消除多处语言白名单维护。
- 保留九种现有语言，为字体设置补齐九语言文案。新增语言的源文件和操作步骤见 [语言与字体说明](../docs/LANGUAGES_AND_FONTS.md)。这是构建时扩展，尚未提供任意语言包的运行时安装。
- 空间外观增加独立字体设置页，支持系统字体族名称、TTF／OTF 导入、即时应用和恢复默认。通用主题与标题栏继承选择；代码区保留等宽字体。
- 字体元数据通过 Rust 原资料库的独立宿主记录保存，支持修订比较、操作身份重试、重开恢复；字体二进制保存在应用本地文件／Web IndexedDB，以 SHA-256 引用，不存入常规偏好消息、不上传。
- 导入文件最大 20 MiB，检查 SFNT 类型、表目录范围及恢复摘要；每进程最多 8 个导入字体和 64 MiB 累计源文件，失败的引擎注册也计入预算。该预算不代表全部解码器或 GPU 内存。
- 切换字体沿既有稳定根节点更新，不改变编辑会话身份。丢失字体文件时保留元数据并回退默认显示。

## 验证

| 检查 | 结果与范围 |
| --- | --- |
| Flutter 常规回归 | 370 通过、65 因额外环境未配置而跳过；不将跳过算作通过 |
| 最终字体／语言定向回归 | 8 通过，包含保存、重开、重置、界面状态及 IME／选区保持 |
| i18n 包测试 | 4 通过 |
| Python 资源生成器 | 7 通过；临时仓库只增加意大利语配置和 ARB 即生成相应 Dart／Rust 清单及资源，重复代码被拒绝 |
| 资源／私有协议生成一致性 | 22 项语言产物及宿主 Dart 绑定检查通过 |
| Rust 宿主 lib | 85 通过，提供原有三份 guest Wasm 测试夹具；初次漏设夹具环境变量的失败不计作产品回归 |
| Rust UI 偏好定向 | 3 通过；字体修订冲突、同操作重试、无插件保存、隐藏系统记录、原库重开等 |
| Windows Profile 字体引擎 | 两个独立进程阶段通过：实际 Roboto TTF 导入／注册及本地文件恢复，再验证根主题与设置页显示；测试夹具调用文件导入 API，未人工操作系统文件选择器 |
| Dart 相关文件分析 | 8 个分析目标，无问题 |
| Web Release | 构建通过，未完成浏览器字体导入的运行验收 |
| Windows Release 与真实宿主集成 | Release 构建通过，3 项真实宿主集成通过，包含字体选择保存、无插件保存及资料库重开 |

Profile 驱动报告 `integration_test plugin was not detected` 警告，但两阶段框架测试及驱动均成功退出；不把这个驱动当成 Android instrumentation／XCTest 验收。日志位于 `build/font-preview/runtime-{import,restore}.log`，其余验证日志以 `build/font-` 命名。Windows 产物位于 `build/windows-corners/x64/runner/Release/`，应用与宿主已配套重建。

## 边界

私有宿主 Cap'n Proto 协议新增字体读写命令并更新身份摘要，应用与宿主必须配套；插件公共 ABI／SDK 未修改。字体文件本地保存，不随内容库跨设备自动迁移；重置不删除文件。暂不支持 TTC／WOFF、自动枚举系统字体及字体家族的多字重组合。

未新增语言的母语审校，未进行 Android／iOS／macOS／Linux 真机验收。本轮之前生成的 Android APK 不包含这些新增功能。

运行字体注册沿用 Flutter 的 [FontLoader](https://api.flutter.dev/flutter/services/FontLoader-class.html)，不引入另一套字体渲染引擎。
