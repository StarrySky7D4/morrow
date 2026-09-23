> 本文为 dev.2 历史记录；当前状态见 [dev.3 UI 补齐记录](UI_DESIGN_DEV3.md)。

# Flutter 源码 UI 对齐 · 2026-09-23

本轮修正此前 HMOS 自行设计的绿色纵向表单。设计依据是 **build/io-safety-refactor 实际工作树源码**，并直接运行 MorrowApp 生成参照图；不是凭旧展示图重新设计。源码文件哈希见 `../reports/ui-source/flutter-reference.json`。

## 源码对应

| Flutter 来源 | HMOS 实现 | 本轮状态 |
|---|---|---|
| `lib/appearance.dart` Palette / Glass | `Index.ets` ink/muted/accent/glass/round | 采用原浅色/深色颜色、76% 默认透明度、22 blur、圆角比例；ArkUI 模糊与 Flutter 的合成不完全相同 |
| `lib/main.dart` build / header / navigation | Header / Navigation / build | <760 菜单搜索、>=760 侧栏、>=1050 外观面板，12/24 外边距；窄屏运行验证，宽屏分支仅编译 |
| greeting / hero / collectionHeader / ideaCard / quickCapture | 同名或对应 Builder | 恢复原首页结构、字号、间距、214 高度、卡片、过滤器和快捷记录 |
| AmbientPainter / OrbPainter | `StudioArtwork.ets` | 原三处光晕、五条曲线、48 个倾斜椭圆几何；SweepGradient 用分段渐变近似，非逐像素等价 |
| `lib/appearance.dart` StudioDialog；`main.dart` NewIdeaDialog | Editor | 独立遮罩、图标/眉题/标题/副标题、可滚动正文、固定保存区；不再常驻首页下方 |
| `pages/workspace_pages.dart` pageIntro | PageIntro / statCount | 原分区说明、统计布局；专用项目/实验卡片的完整布局仍待移植 |
| `little_tips.dart` FooterOverlay | Workspace 底部提示 | 使用原提示、圆点和 lyrics 图标；无音乐功能冒充 |
| Flutter MaterialIcons 资源 | rawfile/materialicons.otf | 直接复用同一图标字体，保留 materialicons_license.txt |
| 外观预览 | C++ renderPreview + ArkUI NativeNode | 真实 NDK Text/Column 挂载、释放及深浅色更新；主界面控件仍为 ArkTS |

## 验证

- 从当前 Flutter MorrowApp 运行本地 widget 截图测试，**1 项通过**。渲染手机逻辑尺寸 440×676 和桌面 1280×900，3 倍图像；使用原 Windows 字体作参考，HMOS 使用系统字体。
- HAP 双架构打包成功；最终 dev.2 在 API26 x86_64 Pura X View 模拟器覆盖安装、启动，保留已有试验库。
- 新建 `Flutter-source-UI-check` 保存后关闭编辑器；重启后搜索仍能读到该卡片。数据是真实试验数据，不是界面固定示例。
- 未保存编辑点击关闭出现确认；“继续编辑”保留标题输入并成功保存。
- 卡片收藏按钮与“仅收藏”筛选验证通过。
- 原待办 `Check-native-task` 从 ✓ 切换到 ○ 再切回 ✓，修订 7 / 8；对应布局记录保存在报告目录。
- 外观页浅色→深色，NDK 预览文字同步更新；返回主页可见深色样式。
- 最终截图为 `hmos-home.png`、`hmos-editor.png`、`hmos-settings.png`。`*-v1/v2` 和带 `dark` 的图为本轮过程证据，不冒充最终逐像素验收。

## 已知差距

超透/液体玻璃 shader、主题自定义、字体导入、多语言、完整设置页、附件/Markdown 预览、项目/实验专用卡片与原动效尚未完整复现。未接通的编辑工具显示禁用状态并说明，不能据此视为功能已迁移。外观值目前只在本次会话保留。

当前投影不提供创建时间，所以排序默认标为“默认顺序”，不把按记录 ID 的返回顺序标作“最近添加”；已提供本地标题/收藏优先排序。卡片底部显示真实阶段，未伪造时间。输入上限保留当前 HMOS 适配器约束，并非 Flutter 的 60/20000。

本轮没有修改 Flutter 业务源码或共享 Rust 快照，没有重跑与 UI 无关的全部 Rust 回归。模拟器运行与编译通过不等于 ARM64 真机、发布签名或完整 UI/功能等价验收。

## 截图

- [直接从 Flutter 源码渲染的手机首页](../reports/ui-source/flutter-mobile.png)
- [鸿蒙模拟器首页](../reports/ui-source/hmos-home.png)
- [Flutter 编辑器](../reports/ui-source/flutter-editor.png) / [鸿蒙编辑器](../reports/ui-source/hmos-editor.png)
- [鸿蒙外观设置](../reports/ui-source/hmos-settings.png)
- [并排查看](../reports/ui-source/compare.html)
