# 新拟态控件打磨与跨版本迁移工具

日期：2026-09-23。开发工作树 `codex/io-safety-refactor`，应用仍为 `0.1.9-test.54+58`，内置包 `0.1.9-test.54.4`。本轮不提交、推送或发布，也不迁移真实用户库。

## 设计参考与实现

参考 [Design.dev 新拟态工具与说明](https://design.dev/tools/neumorphism/) 和 [Quackit 新拟态按钮示例](https://www.quackit.com/html/templates/buttons/neumorphic_button.cfm)，采用一致的左上光源，将控件按用途分为凸起表面和凹陷区域。保留文字、颜色、焦点和禁用提示，选中状态不单凭阴影表达。实现使用项目自己的 Flutter 绘制代码。

- 普通按钮轻微凸起，按下或选中后下沉，经过平面中点缓动；减少动态效果时直接收敛。
- 输入框、搜索框、滑轨、开关轨道为凹槽；滑块与开关拇指维持凸起。导航选中项、材质与背景选项使用较浅内阴影。
- 音乐信息区、播放列表当前曲目使用内嵌效果；每日任务选中标记同步变化。主面板的外阴影减轻，避免大块浮雕挤占层次。
- 通用按钮、输入、下拉输入和复选框主题统一接入；组件设置中的开关与选项使用相同绘制基础。Apple 自适应开关仍保留原生 Cupertino 绘制。
- 风格列表默认收起，保留当前风格名称；点击展开，选定后收起。折叠时停用指针、焦点和隐藏语义。
- 光学层保持子节点身份；样式变化不重建编辑控制器。透明材质不增加整块填色，内阴影只在边缘绘制。

## 迁移范围

新增独立 Windows 命令行工具 `morrow-migrate-library.exe`，随 Windows 构建进入应用目录。支持旧版 JSON 和受保护 SQLite 内容库；默认预检，显式 apply 后写入不存在的新目录。使用现有宿主、审计与真实内置插件执行迁移，不旁路内容事务。

新内容库自定义字体从 `<库目录>/fonts/<SHA256>.sfnt` 优先恢复；旧全局字体目录仍作为兼容回退，字体大小、格式和摘要检查保持有效。

[迁移使用说明](../docs/MIGRATE_LIBRARY.md)列出接受格式、用法和边界。当前接受 JSON version 1（SharedPreferences 包装及裸快照）与受保护 MORR SQLite schema 5–21；不是按产品版本号盲目推断，也不支持未知未来格式降级。

JSON 的旧时间文本和旧附件 ID 只保留于原始 JSON 副本；附件在新库重新导入。插件管理器旁置目录不会自动成为目标库活动配置。待决编辑/设置记录会阻止自动迁移，包括尚不能独立证明结束的旧 revision 1 提案。托管库切换后的字体目录仍可能依赖全局位置，建议按说明用显式 `--data-directory` 打开新库。

SQLite 预检先在源审计锁下复制 DB/WAL/SHM/密钥，再打开临时副本，避免 SQLite 在源目录创建侧车文件。复制前后核对字节与文件集合。迁移核验逐卡来源、历史操作回执、设置和媒体；失败目标保留 `MIGRATION_INCOMPLETE.txt`，应用和工具拒绝将其作为正常库继续使用。修复再次迁移时媒体重定位操作号与旧回执冲突的问题，操作号由确定的新偏好摘要派生。

## 验证记录

- UI/设置/字体/音乐/风格组合 35 项通过；玻璃过渡及液体玻璃交互 9 项通过，共 44 项。最终控件与主界面 11 项复跑通过，不重复计数。
- 14 个本轮 Dart 源码/测试目标静态检查无问题；九语言 22 个生成产物一致性检查通过。
- 主界面截图 `build/ui-style-preview/neumorphism.png`；浅色/深色控件画廊 `test/goldens/neumorphic_controls_light.png`、`neumorphic_controls_dark.png`。已检查文字、图标、选中凹陷与滑轨；修复截图发现的内凹填色遮字、缺失图标及暗色开关对比度问题。
- 迁移 CLI 8 项集成测试通过，无跳过：双格式、源文件不变、活跃源拒绝、未知字段/缺失附件拒绝、失败标记、实际 TaskId Wasm，以及本地媒体/字体连续迁移两次和历史操作保留。日志 `build/review-migration-cli.log`。
- Windows Release 构建成功，`morrow-migrate-library.exe` 随 `morrow_studio.exe`、宿主和插件包一起安装；日志 `build/review-neumorphism-migration-windows.log`。
- 最终 Release 宿主的 Flutter 客户端/托管库重开回归 4 项通过，无跳过；日志 `build/review-neumorphism-migration-native.log`。
- 最终随包迁移程序通过帮助、预检、旧 JSON 导入、第二次 SQLite 迁移的独立执行验证，自动找到随包插件，原 JSON 字节未变；证据 `build/review-migration-release-smoke.json`。

正式测试合计 **56 项**（44 UI + 8 迁移 + 4 成品宿主），不重复计入重跑和成品 smoke。没有运行真实用户资料迁移，没有新增 Android/Web 构建，没有进行 GPU 帧性能、长期资源或其他平台验收。

源码与本地构建校验清单见 `reports/deep-review-build-2026-09-23.json`。原有编辑器 autosave、长期历史维护、SDK 冻结和跨平台资格开放项仍继续；本轮完成不表示整个重构完成。

截图是 Flutter widget 渲染，只用于布局与视觉检查，不作为 GPU 帧性能或设备验收。
