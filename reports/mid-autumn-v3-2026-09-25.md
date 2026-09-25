# 中秋主题第三版：标准独立插件

组件调色修复版已交付，客户端请使用 v3.1；主题文件保持 1.0.0 不变。见 [修复与验证记录](component-color-fix-2026-09-25.md)。

## 交付

- 独立插件：`dist/plugins/morrow-mid-autumn-1.0.0.morrowplugin`，661,049 字节，SHA-256 `B9C8DD591EF88E2D14BE4EE7F4B402FF5D6563530618F71CBAF1E4D1B61D9751`。
- 配套 Windows 客户端：`build/mid-autumn-v3-windows/Release/morrow_studio.exe`。保留整目录，先保存退出旧窗口再启动。
- 真实 Windows 窗口截图：`build/mid-autumn-v3-qualification/check.png`。
- 浅/深色 widget 截图：`build/mid-autumn-v3-preview/white.png`、`dark.png`。
- 构建证据：`build/mid-autumn-v3-build-result.json`；独立插件构建脚本 `tool/build_mid_autumn_theme.ps1` 已实际运行，重复构建包哈希一致。

这是现有插件系统接受的标准 Wasm 插件包，既不只是外置 JSON，也没有建立另一套安装列表。导入后默认停用；启用、停用、升级、移除均走既有插件登记与权限流程。旧版内置主题 SharedPreferences 状态不会隐式导入或启用插件。

使用说明在 `plugins/mid_autumn/README.md`。首次升级需要在插件管理导入此文件并启用；旧预览没有通用主题协议，需要本次客户端适配。

## 设计参考

本轮于 2026-09-25 检索第一方作品页与案例说明；部分 Behance 直接访问返回 403，使用搜索索引的作品说明与图片结果作参考，没有下载或复用第三方设计图片。

| 案例 | 可借鉴的方法 | 本轮应用 |
| --- | --- | --- |
| [Runam Mid-Autumn Packaging Design 2023 — Kris Nguyen](https://www.behance.net/gallery/176721221/Runam-Mid-Autumn-Packaging-Design-2023) | 建筑、灯笼与多层场景组织 | 将单个几何月亮扩展为亭台、石桥、水岸与远山，建立前中后景 |
| [MoonSong — 造物起异](https://www.behance.net/gallery/178282875/MoonSong-Mid-Autumn-Festival-packaging-) | 围绕月光、音乐、诗意建立完整叙事 | 聚焦月夜庭园意境，用月光和水面倒影串联场景 |
| [Qi-Cha Mooncake Packaging — Shan May](https://www.behance.net/gallery/128122627/Qi-Cha-Mooncake-Packaging-?l=194) | 节日元素与整体配色协调 | 保持黛绿、月白、桂金的有限配色，玉兔作为局部焦点 |
| [Tōng Bǎo Mooncake Packaging](https://loupandthesky.com/projects/tong-bao/) | 中秋故事与团聚体验 | 桂花、灯影和庭园共同表达节日，而非堆叠孤立符号 |
| [Fluent 2 Color](https://fluent2.microsoft.design/color) | 中性色层次与节制的强调色 | 插画集中在首页卡片，任务区仍保持清晰阅读与操作层级 |

采用内置 imagegen 生成原创《月满庭》，加入桂花簇、细致屋瓦与窗棂、石桥、灯笼、山雾、水纹和玉兔；保留内容左侧阅读空间。不是照搬参考作品。源图保存在 `plugins/mid_autumn/artwork/moonlit-garden-source.png`；交付 WebP 同尺寸 1536 × 1024、563,932 字节。最终完整提示词和工具模式见 `plugins/mid_autumn/artwork/PROMPT.md`。

检查浅、深两套实际界面后调整画幅，保留完整月亮与玉兔，图案不遮挡文字和主要操作。窄屏采用弱化装饰，辅助技术忽略纯装饰内容。

## 行为与实现

- 移除 Flutter 内置中秋 asset 与特定主题画师；主题清单、两套色彩、首页短句与图片均来自独立 Wasm 包。客户端保留通用渲染器，读取内容有长度、图片尺寸与摘要限制。
- `Package::is_ui_theme` 依据标准声明识别主题。Registry 的 `set_enabled` 在一次持久化快照中切换互斥主题，Manager 先撤销相关旧实例；不改变业务插件的启用、授权和实例。
- 主题是配色/装饰层，风格是结构/质感层。基础风格一直读取 `surfaces.visualStyle`，风格切换不再停用主题。
- 插件管理 UI 沿用导入确认、启用、停用、卸载和错误恢复；主题条目隐藏不面向用户的原始转换工具。风格列表只展示已安装主题的选择快捷入口。
- 自动隐藏被覆盖的全局配色选项；完整材质覆盖时还隐藏玻璃、组件材质/圆角与背景选项。已打开的组件页保留状态并动态显示说明。关闭覆盖/停用主题后恢复控件；不会改写原基础设置。
- 原有工作台包版本冲突兼容修复继续保留；业务 guest 的字节和版本没有改变。

## 验证

共 101 项不重复自动化测试通过，另有独立运行的打包、准备与实际 Windows 检查：

- 54 项 Flutter 界面与回归：主题安装状态、互斥与重启、损坏描述/图像/登记变化、存储失败、300 px 双倍字号、风格叠加、失效控件隐藏与恢复、原偏好与编辑草稿、插件管理实际按钮、业务插件表单连续性，以及既有插件列表、风格、组件设置、透明/液体玻璃与音乐控制。
- 4 项 Flutter—Rust 集成：真实独立主题包的检查/导入/启用、分段图像校验、关闭并重启宿主后的恢复、停用和卸载；原业务权限、内容/偏好、保护密钥及歌词功能回归。
- 11 项 Registry 测试，包括主题切换只增加一次登记修订、拒绝过期操作、重启互斥保持、业务批准不变。
- 12 项运行时测试，包括旧主题实例撤销、过期操作不撤销当前主题、业务实例继续有效，以及不确定写入场景。
- 20 项宿主插件目录/控制测试，覆盖原插件导入执行、启停、会话、缺包/损坏、升级/降级与授权。
- 本轮 Dart 静态分析、共享 Rust Wasm 编译检查和 `git diff --check` 通过；既有 Rust 未使用代码警告保留。

日志：`build/mid-autumn-v3-{regression,native-tests,native-final,registry-tests,manager-tests,host-tests,analyze,wasm-check}.log`。早期一次回归命令包含不存在的测试文件名，已纠正；最终回归日志为 54 项全部通过。

实际 Windows Release 使用全新测试内容库和本轮导入的主题登记启动，创建/读取内容、收藏/待办操作与界面渲染通过；启用主题时实际解码静音 WAV、推进播放时钟、拖动进度、音频互斥和不自动播放检查通过，退出码 0，登记文件前后 SHA-256 一致。截图与明细：`build/mid-autumn-v3-qualification/check.png`、`check.md`。未用用户真实内容做测试，也未强制关闭用户正在运行的窗口。

## Windows 构建范围

UI 使用 Flutter assemble 独立生成 AOT 与资源，未携带内置主题 JSON。原 Windows runner、引擎和原生依赖复用原 Release，引擎与当前 Flutter SHA-256 已核对一致；Rust 宿主/迁移工具为本轮源码构建。业务 guest 保持原字节。预览是经过组合验证的独立目录，并非重新制作安装程序。

原 Release EXE 保持 `F21A5FFBE3530008BDA2A35860E89F2A8FD5683188768ED8D3A96A11E0D75BCF`。现有安装和真实资料库未覆盖。无法保证操作系统对话框、第三方硬编码绘制和所有用户媒体格式都能受主题控制或已逐项验证。
