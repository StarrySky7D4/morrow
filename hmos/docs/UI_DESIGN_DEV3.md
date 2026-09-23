# Flutter 源码 UI 对齐 · dev.3 · 2026-09-23

设计依据为 `build/io-safety-refactor` 的实际 Flutter 工作树。运行原 `MorrowApp` 生成参照图，HMOS 截图来自 API26 / x86_64 模拟器；没有改动原 Flutter 源码。查看 [多页面并排对照](../reports/ui-source/compare.html)。

## 页面与控件

| Flutter 来源 | HMOS 实现 | 已补齐内容 |
|---|---|---|
| `main.dart` 工作台 / navigation / header | `Index.ets` | 紫灰色工作台、首页插画、搜索、手机菜单、侧栏、快捷记录与底部提示；保留 760 / 1050 宽度断点 |
| `pages/workspace_pages.dart` | SpecializedCard / PageIntro / pageFilters | 收件箱整理及转项目；项目阶段、进度、三态任务数量、前三项待办；实验编号、假设、观察；收藏横向卡片；对应分区筛选 |
| `appearance.dart` / `liquid_glass.dart` | Appearance / GlassRim / PaperBackdrop | 磨砂、超透、液体玻璃渐变与边缘高光；浅色 / 深色 / 自定义色调；灰度、明暗、圆角；默认、纯色、纹理、透明画布选项 |
| `settings_surface.dart` | Settings | 同一背景上的设置页、返回、920 最大宽度、可滚动单列分区；子页切换回到顶部 |
| `component_material_page.dart` | MaterialList / MaterialEditor | 组件及逐卡材质列表、自定义开关、继承主题、模糊、透明度、圆角、颜色；应用、取消、恢复草稿默认值；独立作用域保存 |
| `color_compass.dart` | `ColorWheel.ets` / ColorSettings | 232 大小 HSV 色盘、明度、色样、十六进制输入、校验、取消 / 应用；HMOS 使用设置子页承载，尚非原模态呈现 |
| `fonts/font_settings.dart` | FontSettings | 字体输入、系统字体选择、预览、应用及重置；系统目录无可枚举字体时提供 SDK 默认 HarmonyOS Sans；文件导入禁用并说明 |
| `main.dart` 日常清单、引用；`music/music_panel.dart` | DailyPanel / Quote / MusicPanel | 三项小清单、引用、随身听空状态与播放列表展开；音乐导入 / 播放未接通，不模拟播放成功 |
| StudioDialog / NewIdeaDialog | Editor / BodyPreview | 独立编辑弹窗、眉题、固定操作区、正文预览入口；基础 Markdown 标题、列表、引用、代码块和段落预览；预览不修改原文 |
| `morrow_i18n` 九份 ARB | `UiStrings.ets` | 复用无参数原文目录、界面语言选择和保存；业务协议值保持不变、用户正文不翻译。平台新增提示和部分动态文案仍有中文回退，尚非完整九语验收 |
| Flutter MaterialIcons | `rawfile/materialicons.otf` | 复用原图标字体及许可；主题 / 分区切换时图标响应更新 |
| 外观材质示例 | `bridge.cpp` + ContentSlot | 真实 ArkUI NDK Text/Column，随模式和深浅色更新、卸载释放；主体导航和编辑控件为 ArkTS |

## 验证

- 原 Flutter widget 截图测试 **1 项通过**，包含手机首页、编辑器、收件箱、项目、实验、收藏、设置、字体，以及桌面工作台。手机逻辑尺寸 440 × 676，输出三倍图。
- dev.3 HAP 包含 ARM64 / x86_64，编译成功；已覆盖安装并运行于 Pura X View / API26 x86_64 模拟器。
- 收件箱测试卡转项目后保留原 `Check-native-task`；项目卡显示 `1 完成 / 0 未完成 / 0 待确认`，阶段仍为“计划中”，没有把任务完成等同于阶段完成。
- 新建 `HMOS-UI-lab`，切换正文预览再保存，回读标题、正文和实验分类；修改到“验证中”并筛选成功。
- 独立首页引导材质改为粉色及 0 圆角，其余卡片保持原材质；已测试应用及恢复默认。过程截图为 `v3/hero-override.png`。
- 修正液体高光覆盖层拦截点击，以及 Builder 值参数导致滑块标签不刷新的问题。修正后可在液体模式进入设置子页，滑块值与标签同步。
- 色盘选色、应用到材质；系统默认字体选择、应用；切换 English 后强制结束并重启，界面继续使用英文。设备 Preferences 证据含字体、语言和独立材质。
- 日常清单勾选、外观参数和卡片均在后续覆盖安装 / 重启后保留。

最终包和输入哈希见 [build-manifest.json](../reports/build-manifest.json)。最终包截图集中在 `reports/ui-source/v3/final/`；`v3` 其他文件是过程验证，不冒充同一最终二进制的截图。验证清单见 [validation.md](../reports/ui-source/v3/validation.md)。

## 仍需平台适配的差距

液体玻璃采用 ArkUI 模糊、渐变及静态边缘高光；**没有实现原 Flutter backdrop 折射 shader、指针跟随光照或逐像素等价**。环形插画采用同样椭圆几何，但渐变合成不同。移动端透明画布作用于应用内部，不代表系统桌面透视。

附件、富文本剪贴板、完整 Markdown 表格 / 行内样式 / 附件预览、字体文件导入、背景图片 / 视频、音乐播放与歌词仍待接通。正式插件、网络服务、内容保护与备份设置依赖尚未移植的宿主能力，按原 Flutter 的能力条件不展示可用管理界面。没有伪造授权、备份或播放结果。

宽屏分支已编译，Flutter 桌面参照已渲染；HMOS 宽屏设备、软键盘各尺寸、完整动效和无障碍矩阵尚未验收。未做 ARM64 真机或发布签名验证。

卡片默认排序仍为“默认顺序”，因为当前 Rust 投影没有创建时间；不伪造最近添加时间。当前数据为独立开发试验库，正式存储、持久编辑恢复等边界见 [PARITY.md](PARITY.md)。

主任务仍在推进。本轮共享快照检查发现 `preferences.proto`、`studio.capnp`、`preferences.rs`、`services.rs` 和偏好测试五处上游差异；此轮 UI 改动未覆盖已验证的 Rust 快照，尚未把这些增量合入 HMOS。实际检查结果保存在 `reports` 中。
