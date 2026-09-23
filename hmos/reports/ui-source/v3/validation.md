# dev.3 UI 验证记录

日期：2026-09-23。开发包：`dev.morrow.hmos` / `0.1.0-hmos-dev.3`。当前最终 HAP 哈希见 `../../build-manifest.json`。

| 检查 | 结果与证据 |
|---|---|
| ArkTS + C++ / 双架构 HAP 构建 | `hap-build.log`，BUILD SUCCESSFUL；不是发布签名或双架构运行验收 |
| 原 Flutter UI 源码截图测试 | `flutter-render.log`，1 passed；440×676 手机及 1280×900 桌面；未编辑原 Flutter 源码 |
| 最终 HAP 覆盖安装与运行 | HDC install / aa start 成功；Pura X View API26 x86_64；`final/` 截图来自最后构建安装 |
| 项目专用卡片 | 收件箱 `HMOSHMOS-native-check` 转项目，修订 9，原已完成待办保留；项目阶段仍为计划中；`final/projects.png` |
| 实验卡片及筛选 | 新建 `HMOS-UI-lab`；正文预览后保存成功；修改阶段“验证中”，同名筛选下仍显示；`lab-save.txt`、`lab-stage-filter.txt`、`final/lab.png` |
| 独立材质 | 仅首页引导改粉色和 0 圆角，其他卡片不受影响；`hero-override.png`；之后恢复默认并应用，`final/preferences.xml` 中 hero.enabled=false |
| 滑块即时反馈 | 修正 Builder 值参数后数值与控件同步，`slider-live.txt` 显示标签 58% 和 Slider 58 |
| 液体玻璃交互 | 修正装饰层根容器的 hitTest；液体模式下可打开组件材质列表及详情并应用 |
| 颜色 | 色轮触控产生 #ff43c3，应用到材质后实际可见；颜色草稿与材质应用分开，非法十六进制禁用应用 |
| 字体 | 选择并应用 HarmonyOS Sans；模拟器字体目录没有列出额外字体，已提供 SDK 默认字体选项；重启偏好包含该值。未验证额外字体文件导入 |
| 语言重启 | `en-before-restart.txt` / `en-after-restart.txt`：强制结束并重新启动后首页保持英文；`preferences-restart.xml` 中 locale=en。结束时恢复中文 |
| 日常清单、外观重启 | 勾选“给自己倒一杯水”，最终覆盖安装后 `final/preferences.xml` 保留；圆角 20、浅色磨砂、独立材质默认，过程设置保留为截图证据 |
| 上游快照一致性 | `../../reference-drift.json`：5 个 preferences / services 相关文件发生变化，未合入；本轮未修改共享 Rust 快照 |

`final/` 是最终包页面截图。外层 `v3/*.png` 是过程中抓取的交互证据，部分早于最后的图标、字体列表及顶端对齐修正。两端使用不同真实测试数据、系统字体和系统栏；不把截图比较视为逐像素等价。

未重跑与此次 UI 无关的全套 Rust 测试。历史 Rust 验证不冒充本次新测试。折射 shader、完整 Markdown / 附件 / 音乐、字体文件导入、完整九语言提示、原全部动效、宽屏 HMOS 设备和 ARM64 真机仍未验收，详见 [UI 补齐记录](../../../docs/UI_DESIGN_DEV3.md)。
