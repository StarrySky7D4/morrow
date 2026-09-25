# 中秋主题第二版：设计与插件兼容修复

> 本记录保留第二版历史。第三版已改为标准独立 `.morrowplugin`，支持风格叠加、主题互斥和无效控件隐藏；新入口与插件文件见 `reports/mid-autumn-v3-2026-09-25.md`。

## 设计参考与应用

2026-09-25 检索并阅读以下第一方资料。参考设计方法；没有复制其中图片或下载商用素材。

- [VINC：香港丽晶酒店 2025 月饼包装设计](https://www.vincdesign.com/zh-hant/portfolio/香港麗晶酒店-高級月餅包裝設計-2025/)：从其现代几何、水波、月光、玉兔和绿金配色的设计说明中提取节日意象。
- [Fluent 2：Color](https://fluent2.microsoft.design/color)：用中性色建立阅读层次，减少装饰色占比；操作强调与状态语义保持清晰。
- [Fluent 2：Material](https://fluent2.microsoft.design/material) 与 [Windows：Layering](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/layering)：区分背景、内容表面与临时浮层，材质变化应有明确用途。

落实为月白纸感、黛绿表面和少量桂金；移除重复的大月亮与满屏星点，在首页用一个月洞窗容纳远山、月光倒影、玉兔与桂枝。导航、卡片、输入与按钮继续保持 Morrow 原有布局。浅色正文/次级文字/强调色对背景与表面，以及深色对应组合，均通过 4.5:1 对比度测试；自选透明背景或材质仍由用户配置决定实际对比度。

默认仅统一色彩与氛围装饰，保留玻璃、液体玻璃、组件圆角、基础风格及自选媒体背景。新增「覆盖材质与背景」开关，开启后临时使用统一磨砂表面与主题背景。卸载重置开关并恢复基础配置；旧版已保存状态没有这个字段时按关闭处理。

截图为 Flutter widget 渲染，已检查两套配色、层次、装饰与内容布局：`build/mid-autumn-v2-preview/white.png`、`dark.png`。

## 工作台不可用：已复现的根因

上一版预览复用了旧 Rust 宿主与旧业务包。空资料库启动成功，但没有覆盖用户资料库已经选择另一份同版/更新插件包的情形。旧宿主初始化时无条件选择随应用业务包，触发 `RevisionConflict`，整个插件管理器初始化失败，工作台因而只读、插件显示不可用。

只复制用户的插件目录和登记文件到一个全新、独立资料库，原宿主复现：`writable=false`，`available=false`，提示 `RevisionConflict`。没有修改真实资料库、插件登记或授权。完整内容副本受身份锁保护，本次尊重该锁，复现不使用该内容副本。

修复在 `initialize_manager`：当随应用包与已选 digest 不同且版本不高于已选版本时，从本地登记目录读取并校验已选的精确包，再沿用该包。登记版本、digest、授权和启用状态不变；显式禁用仍然保持禁用。确有新版本时沿用原升级/审批规则。登记损坏或已选包缺失时仍报告问题，不以别的包冒充。没有放宽 Registry 的降级检查，也没有给旧 guest 换版本号。

同一份登记副本在新宿主上验证：`writable=true`、`enabled=true`、`approved=true`、`available=true`、`warning=null`。歌词时间解析和播放切换也通过。前后日志：`build/mid-autumn-v2-repro.log`、`build/mid-autumn-v2-repro-fixed.log`。

临时完整资料库副本 `build/mid-autumn-v2-library-copy/` 的自动清理被执行工具安全策略阻止（仅返回 `blocked by policy`，没有更具体原因）。该目录仍包含本机诊断复制的数据及密钥，不在交付 Release 中，不应对外分享；原资料库没有被删除或改写。

## 回归验证

- 11 项主题测试：生命周期、窄屏双倍字号、持久化失败、浅深色对比度、组件液体玻璃与独立圆角、覆盖开关、原偏好与草稿保留、外部插件实时表单会话连续性。
- 32 项既有外观回归：原风格、滑块、复选框、实验控件、风格切换、透明与液体玻璃。
- 10 项 Rust 插件控制测试：同版不同 digest、旧应用打开新登记、原禁用/授权保留、内容可写、UI 会话，以及原有损坏登记、恢复和权限场景。
- 3 项 Flutter—原生 Rust 集成测试：真实 guest、数据/偏好持久化、权限重新验证、保护密钥恢复；另有 1 项实际登记副本验证。
- 共享 Rust 浏览器代码 `cargo check --target wasm32-unknown-unknown --lib` 通过（既有警告保留）。
- 本轮 Dart 源码与新增测试静态分析无问题；`git diff --check` 通过。

日志在 `build/mid-autumn-v2-{theme-tests,regression,rust,native-integration,wasm-check}.log`。

## 独立 Windows 预览

入口：`build/mid-autumn-v2-windows/Release/morrow_studio.exe`。须保留整个 Release 目录。先保存并退出正在运行的旧预览，再打开此入口使用原资料库；没有强制关闭用户窗口。

UI 通过 Flutter `assemble release_bundle_windows-x64_assets` 编译；复用原 Windows runner、引擎与原生依赖，Flutter 引擎 SHA-256 已核对一致。替换为本轮源码编译的 Rust Release 宿主和迁移工具；业务 guest 保持原 Release 字节不变。完整 guest 重打包原有的版本一致性检查没有绕过。此预览是组合验证包，不是完整安装程序重建。

构建来源和校验值：`build/mid-autumn-v2-build-result.json`。原 Release EXE 与现有安装保持不变。需要重新启动修复版才能使用新的宿主逻辑；仅在旧窗口切换主题不会替换正在运行的宿主进程。

实际 Windows Release 资格检查已通过，退出码 0。使用新的空内容库加既有插件登记副本，实际完成创建、收藏、待办操作与内容读取；检查前后登记文件 SHA-256 相同。主题保持启用，原有超透材质与画布效果共存。真实窗口截图与日志在 `build/mid-autumn-v2-qualification-installed/check.png`、`check.md`。

随身听实际解码静音 WAV、推进播放时钟、跳转进度、因背景音频阻止而暂停，以及恢复后不自动播放，全部通过。原版和新版本的旧检查都在固定 500 ms 等待后读到时长 2 秒、进度 0；已将资格检查调整为最多等待 5 秒的实际进度事件，避免把设备启动延迟直接视为失败。没有修改正常播放逻辑。用户现有音乐清单和全部音频格式未逐项测试。
