# 设置标题、切换性能与九语言界面（2026-09-21）

## 本轮交付

- “空间外观”标题恢复左对齐，保留两端设置图标和展开按钮的对称位置。长语言的不透明度标题及滑杆两端说明允许换行，避免侧栏溢出。
- 组件设置列表改为按可见区域创建条目，不再一次构建内容库中的所有卡片。普通磨砂/超透材质直接使用模糊滤镜，不加载液体折射着色器，也不响应无意义的光源悬停更新；液体材质及其过渡仍保留。
- 新增俄语、法语、德语、西班牙语、日语、韩语、葡萄牙语。每种 885 条，共 6,195 条新增译文，覆盖主界面、外观、导入、恢复和插件/网络权限管理。保留原中文、英语，共九种语言。
- 语言选择器显示各语言原生名称。启动参数、Flutter 偏好、Dart 原生客户端和 Rust 内容库验证同步接入新增语言；保存沿用原操作身份与修订机制，无第二份语言数据库。
- 继续使用严格 ARB 源文件、官方 Flutter ICU 生成和有摘要校验的 PB/LZ4 资源。九语言键集合、类型化占位符必须一致。支持列表固定英语优先，避免新增德语后改变原先“不支持的系统语言”回退；显式 `L10n.forLocale` 的未知语言与无 delegate 回退仍为中文。

## 切换性能实测

Windows Profile 模式、1180×820 窗口、MemoryStorage 中 300 张卡片，预热后执行六轮“组件列表 → 编辑 → 返回 → 返回”，共 24 次路由变化：

| 指标 | 修改前 | 修改后 |
| --- | ---: | ---: |
| UI 帧 p95 | 3.984 ms | 1.869 ms |
| UI 帧最大值 | 82.210 ms | 6.362 ms |
| UI 超过 16.7 ms 的帧 | 6 | 0 |
| Raster 帧 p95 | 6.762 ms | 5.130 ms |
| Raster 帧最大值 | 10.254 ms | 8.579 ms |

两次分别采集 370、377 帧；帧数量受窗口调度影响。数据来自当前 Windows 设备和限定流程，不是所有硬件、IO 服务页面或用户内容库的性能保证。驱动：`integration_test/settings_navigation_profile_test.dart`；证据：`build/settings-profile-large-before.json`、`build/settings-profile-large-after.json`。使用 `flutter drive --profile`，不是不受支持的 `flutter test --profile`。

## 验证

- 39 项 Flutter 组合回归通过：标题位置、设置宽度、返回与过渡、300 卡片列表末项编辑/保存及滚动位置、九语言宽度 390/800/1050/1440、语言持久化、查询身份稳定、编辑文本/IME/选区/滚动保持等。日志：`build/settings-i18n-tests-final.log`。
- 随后扩展色盘和无效媒体 URL 的九语言草稿保持，相关 11 项回归通过（与上面组合重叠，不相加）；日志：`build/i18n-drafts-tests.log`。
- 语言包 4 项、Python 资源编译器 6 项及生成一致性通过；验证实际九语言资源加载、俄语 1/2/5/21 复数、占位符、篡改拒绝、语言回退。日志：`build/i18n-package-tests-final.log`。
- Rust 原库偏好 2 项通过，包括七种新增语言逐个保存、相同操作重试、关闭/重开原库恢复。最终 Release 宿主及插件的 Dart/Rust 真实进程集成 2 项通过，验证新增语言客户端写入路径、原有内容和偏好保存、保护文件恢复；日志：`build/locale-host-tests.log`、`build/settings-i18n-native-tests.log`。
- 七语言 Windows Profile 实际窗口流程通过：1180 宽屏、390 窄屏及组件编辑，生成 21 张 Flutter 渲染截图；检查俄语宽屏、法语/德语窄屏和韩语组件页。输出：`build/settings-languages-captures`，日志：`build/settings-languages-window.log`。Driver 输出已有 integration_test 插件探测警告，但实际应用断言、截图生成及 driver 最终结果均通过；不宣称物理鼠标或操作系统截图验收。
- 本轮相关 13 文件分析、额外草稿测试文件分析、i18n 包分析通过。全仓分析另有 3 条既有 info（`service_business_routing_native.dart` 两处大括号及 `service_frame_real_native_test.dart` 的 print），不计为本轮清零。`git diff --check` 通过。

## Windows 预览

`lib/main_rust.dart` 完整 Release 构建成功，输出：

`build/windows-corners/x64/runner/Release/morrow_studio.exe`

请保留整个 Release 目录。九种打包语言资源逐字节等于生成源资源，配套 Rust 宿主和插件与构建源产物一致。日志：`build/settings-i18n-config.log`、`build/settings-i18n-build.log`；完整摘要：`build/settings-i18n-artifact-hashes.json`。

- `data/app.so` SHA-256：`6f8a9b8bfdc459ced717ead30b720ec2371349b1a0507e7ab14f39d4e8a8fa78`
- `morrow-workbench-host.exe` SHA-256：`3a3f710892ca4cc7d6aa21f12a058ad6cd7362b252206c73498d6c584214facb`
- `plugins/workbench.morrowplugin` SHA-256：`ee271fe12a70ed9ab0cadcca7822ab52df3247b192a79aff0b9465b78469ca61`

本轮由主代理直接编写译文与代码，未调用 SubagentBridge 或其他子代理。译文尚未经过母语使用者审校；未做 Android/iOS/Web 的本轮构建或设备验收。测试只使用内存或临时内容库，不修改用户原库；版本仍为 `0.1.9-test.52+56`，未推送或发布。SDK 冻结和其余 IO 工作的门槛不因界面与本地化通过而改变。
