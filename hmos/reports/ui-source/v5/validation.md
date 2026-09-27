# HMOS dev.5 验证记录（2026-09-27）

本轮接入 Flutter 七种面板风格、全局/组件立体深度和组件材质跟随，并修复用户报告的全局错位框线。目标仍是持续追平 Windows UI 与大部分实现，本记录不是完成声明。

## 公共框线缺陷

原公共 Rim 叠层以 `100%` 声明宽高。带 padding 的面板使叠层取得内容宽度，部分不固定高度的容器又让叠层继承过大的高度；结果是导航内部多一条竖线、卡片边线穿过相邻卡片、快捷输入和页脚边线延伸至页面底部。`final-home.png` 是修复前的实际证据，不能用作最终包验收。

修复：每个面板通过 `onAreaChange` 将实际外边界传给其描边层，按组件身份和圆角基值隔离尺寸；绘制限制在这些边界内。外观示例使用自身的 13 vp 圆角基值；材质草稿示例也独立测量。液体边缘在布局尺寸稳定后清空重绘，卸载时取消待执行绘制。

最终 HAP 的 SHA-256 是 `D36E45753D74EBC698A2C9A3E3B34F841DF657D367C88899CE15CBEAAEBBF9DD`。`verified/package.json` 绑定该包；`verified/` 才是最终几何修复后安装包的截图/布局记录。已人工查看：

- `clear-{flat,neumorphism,paper,clay,fluent,brutalist,industrial}.png`：七种风格切换后面板边缘与预览尺寸对齐。
- `clear-home.png`：导航、搜索、引导、两列卡片、快捷输入、页脚不再有穿过内容的偏移轮廓。
- `liquid-editor.png`、`material-preview.png`：弹窗和独立材质预览边缘沿自身面板绘制。
- `liquid-projects.png`、`liquid-recycle.png`：分类页/回收站卡片与摘要的边缘范围正确。
- `light-frosted-settings.png`：切换到浅色/磨砂后没有遗留旧的液体描边。

液体模式与工业风仍按原设计保留其表面边缘；这次修复的是错误尺寸导致的游离框线，没有通过关闭全部风格来隐藏缺陷。验证设备为 Pura X View / API 26 x64，1320×2232 屏幕、当时 880 vp 应用宽度。当前首页短内容居中留白、原控件/按压动画、折射 shader、完整响应布局仍需继续对齐，不代表所有设备/主题组合已验收。

## 材质与风格实现

- 对照活跃 Flutter `appearance.dart` / `visual_style_picker.dart` / `style_depth_slider.dart`：七种半径比例、边缘与阴影，独立 0–200% 深度，零半径保持方角。
- 材质跟随解析整个终端材质；终端未启用/不存在时继承主题。缺失目标可以保留；所有引用环（包括未启用节点）拒绝。独立圆角不再次乘全局风格缩放。
- 编辑使用草稿；取消不生效，应用后才替换并持久保存。源选择器排除会产生环的目标。
- 复用九语 ARB 增量；新的动态说明仍有中文，不能声称完整九语。

`dev5-appearance-tests.log`：实际纯 ArkTS 模型经 SDK TypeScript 编译后运行，6 项通过。覆盖旧设置兼容、零值/整材质跟随、循环拒绝、非法数据、4096 节点链/JSON 恢复与风格半径。它不验证 ArkUI 绘制。

在本轮较早 dev.5 包上完成的交互证据：`follow-draft.*`、`follow-cancelled.txt`、`follow-applied.txt`、`cycle-excluded.txt`、`hero-draft.txt`。搜索框跟随首页引导；取消后保持原设置，应用后显示来源；再编辑引导时搜索框不在候选内。首页引导保存 opacity=40、radius=8、depth=1.49。强停/重启后 Preferences 内容一致，`preferences-{before,after}-restart.xml`；最终包再次打开跟随预览仍显示来源和 149% 深度。测试通过 UI 操作，未直接修改数据库/Preferences 文件。

## 来源与构建边界

`flutter_style_reference_test.dart` 运行真实 MorrowApp（内存测试存储），1 项 Flutter widget test 通过，导出本目录 `flutter-*.png` 七张。它们是白色/磨砂的源实现参照，与 HMOS 深色/绿色强调色并非相同配置的像素差分。测试文件保存在 hmos/tool，没有修改活跃 Flutter 工作树。

`hmos-*.png`、`final-*.png`、`geometry-*.png` 属于此前构建/诊断阶段，保留问题溯源，不当作最终包截图。`dev5-device-styles.log` 是较早版本的七种选择记录；`dev5-surface-device.log` 和 `verified/` 是最终尺寸修复后的记录。

`dev5-hap.log`：ArkTS/C++ HAP 构建成功，最终包安装并启动。dev.5 未修改 Rust，沿用 dev.4 的双架构库与既有验证（不把旧 Rust 检查写成本轮新回归）。包未签名；ARM64 真机未验收。完整构建输入及产物哈希见 `build-manifest.json`。
