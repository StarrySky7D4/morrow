# Morrow HMOS

Rust + ArkUI 的鸿蒙迁移工程，独立保存在本目录。当前交付 **0.1.0-hmos-dev.11 开发预览**，尚未与 Flutter 功能等价，不能替代正式资料库。

2026-09-27 跟进：按 Flutter `versioned_task_panel.dart` 接入待办重命名、上下移动、批量完成与移除确认。共享 Rust TaskId 模块与当日参照一致；通过主机测试、双架构构建和 x64 模拟器验证。见 [dev.4 验证记录](reports/ui-source/v4/validation.md)。

已提供实际 HAP、Rust OHOS 双架构构建、ArkTS 界面、ArkUI NDK 原生外观预览、N-API 异步桥与共享核心事务。已在 Pura X View / HarmonyOS API 26 x86_64 模拟器安装、启动、建卡保存；另有设备侧 Rust 自检。

2026-09-27 dev.5：接入七种面板风格、独立深度及完整材质跟随关系；修复公共边缘层百分比尺寸造成的错位/贯穿框线。最终安装包的设备截图与此前诊断图分开保存，见 [dev.5 验证记录](reports/ui-source/v5/validation.md)。持续追平目标见 [对齐计划](docs/ALIGNMENT_PLAN.md)。

2026-09-27 dev.6：修复短内容被滚动容器居中造成的大块留白，按 Flutter 固定 14 vp 列间距排列卡片；增加侧栏收起/展开、五页滚动位置保留和设置返回恢复，并支持遵循系统旋转锁定的自适应方向。880 vp 竖屏与 1488 vp 横屏的实际设备证据见 [dev.6 布局验证](reports/ui-source/v6/validation.md)。

## UI 源码对齐

2026-10-05 dev.11：按 Flutter 的阅读流程，点击卡片先打开 Markdown 详情，再显式进入现有草稿编辑器。详情提供分类、阶段、独立 TaskId 勾选、收藏、复制和删除确认；卡片正文长按与菜单提供查看、编辑、复制、转项目、组件设置和前后移动。五个页面各自持久保存自定义顺序，筛选移动保持隐藏卡片的位置；独立拖动柄通过当前来源、页面/查询条件、视图修订和一次性令牌检查。完整拖动边缘自动滚动、鼠标/键盘上下文手势和详情内完整任务编辑仍需补齐。见 [dev.11 验证记录](reports/ui-source/v11/validation.md)。

2026-10-05 dev.10：正文预览改用 Rust CommonMark 解析投影，原生显示行内强调、删除线、嵌套列表、引用、完整代码块和横向表格，链接由用户点击打开，远程图片点击后读取。原始 HTML 只显示为文字。粘贴入口接入鸿蒙授权 PasteButton，按字段和 UTF-16 选区插入完整文字，正文 TSV 复用原 Rust capture 转为 Markdown 表格；超限与异步目标变化拒绝插入，原文字保留。完整富文本、Office 和附件导入仍待接入。见 [dev.10 验证记录](reports/ui-source/v10/validation.md)。

2026-10-05 dev.9：按原 `editor_draft` schema/model 接通文字草稿私有事务日志，保留完整来源、世代 CAS 和原操作历史回执。新卡草稿不创建业务卡；关闭时确认最新文字再保留，提供草稿列表、恢复和确认弃稿入口。仅查看未编辑的已有卡片不占用草稿槽。正文宽度达到 Flutter 的 590 vp 条件时编辑/预览并排，实验字段改为多行。真实验证和限制见 [dev.9 验证记录](reports/ui-source/v9/validation.md)。原始候选串可保留为文字，但普通控件不能重建原 IME 会话。

2026-10-05 dev.8：接入原 Rust 查询计划和 `query_v2`，覆盖标题、正文、假设、结论与附件元数据名称搜索，分批筛选、UTF-16 标题排序与稳定收藏排序；ArkTS 查询去抖并丢弃过期结果，切换条件立即清除旧结果，同页保存保留最近确认的成员并刷新修订。仍限定 256 张开发库；默认顺序是源协议的反向 ID 顺序，不是创建时间。实际验证与设备环境边界见 [dev.8 验证记录](reports/ui-source/v8/validation.md)。附件查询元数据的测试不代表附件导入已接通。

2026-09-27 dev.7：卡片区改为单滚动视口中的原生懒加载瀑布流，按页面/卡片身份维护视图，按修订刷新内容；排序、过滤及卡片高度变化同步使位置缓存失效，避免错位或重叠。16 张长短交错记录通过设备上的收藏、正文增减、排序、空结果及完整遍历检查。见 [dev.7 验证记录](reports/ui-source/v7/validation.md)。这不解除现有 Rust 查询的 256 张限制，也不是内存峰值或真机验收。

已按活跃 Flutter 的 main.dart / appearance.dart 补齐紫灰玻璃工作台、专用分类卡片、外观与材质子页、色盘、字体、日常清单、音乐空状态和独立编辑弹窗，并实际渲染源码参照图与鸿蒙截图比对。详见 [源码对应和验证](docs/UI_DESIGN_DEV3.md)，可查看 [并排截图](reports/ui-source/compare.html)。这不是完整 UI 等价声明。

## 使用与构建

使用 DevEco Studio 打开本目录。默认包名 `dev.morrow.hmos`，不覆盖 Flutter 应用。启动后打开独立的本地试验工作区。卡片保存于应用沙箱的 `hmos-development.sqlite`，不读取现有桌面资料。

```powershell
# 在本目录执行；首次 ohpm 依赖安装可用 devecocli build 或 Studio 同步。
./scripts/build-rust.ps1 -Test
./scripts/build-rust.ps1 -Abi arm64-v8a
./scripts/build-rust.ps1 -Abi x86_64 -Runner
./scripts/build-hap.ps1
./scripts/check-reference.ps1
```

环境：Rust 标准库 `aarch64-unknown-linux-ohos`、`x86_64-unknown-linux-ohos`；API 26 SDK；Cap'n Proto 1.4.0。脚本提供本机已验证路径，`build-rust.ps1 -Sdk` 与 `build-hap.ps1 -Studio` 可覆盖 SDK/Studio 位置。Cap'n Proto 可执行文件须在 PATH，脚本也加入本机已部署目录。

HAP 输出：`entry/build/default/outputs/default/entry-default-unsigned.hap`。当前模拟器接受该调试安装；**不是已签名发布包，也不是 ARM64 真机验收**。构建脚本关闭本工程 hvigor daemon，避开当前全局 daemon 锁问题，不删除全局缓存或停止其他构建。

## 已接通范围

- 卡片标题、正文、假设、结论，新建和修改；类别、阶段、收藏、搜索和删除视图。
- V2 TaskId 待办新增、独立勾选、重命名、上下移动、确认移除；同名任务不会联动。批量勾选经确认后与当前阶段在同一事务提交，普通排序/重命名保持阶段和完成状态。
- 直接调用现有 `cards_v2` / `tasks_v2`，通过 `morrow-core::HostRuntime` 的对象授权、版本化 CAS 与原操作幂等事务保存。
- 开发库查询从单次 WAL 快照读取完整属性，复用 `query_plan_v2` 的 128 项 / 64 KiB 帧预算、分页过滤和排序归并；回收站保持独立撤销视图。此本地调用没有生产 guest 执行、查询捕获或审计权限证据。
- 删除后仅允许 **8 秒内撤销**，沿用共享核心的规则，不提供永久恢复承诺。
- `u64` 修订和时间戳跨 ArkTS 边界使用十进制字符串；完整源记录随修改请求绑定，不用界面投影覆盖正式内容。
- 数据库操作在 N-API worker 执行；ArkTS 串行提交。回复未确认保留同一序列化请求；正式提交后回读错误仍保留核对状态。编辑输入不会被迟到的保存回复覆盖。

## 尚未完成的关键边界

当前使用原核心的 **未封存试验 Store**，并非生产 `Workbench` 的安全降级。现有正式宿主 `storage.rs` 在非 Windows 平台明确拒绝打开；此检查保持原样。Harmony HUKS 密钥保护、库身份、单所有者租约、审计封存、恢复与备份还需适配并单独验收。

试验库限定 256 张卡片，保留核心默认事件预算（1024 条 / 64 MiB），没有绕过队列上限或静默清除历史。达到容量会停止写入。系统自动备份关闭，避免普通文件备份冒充 SQLite 一致性快照。

dev.9 已持久保留确认过的原始文字草稿，最多 16 份活动草稿、256 个累计身份与 64 MiB 活动预算，沿用事件队列上限。原 schema/model 字节保持一致；附件 pin、前驱捕获证据和父子交接明确拒绝。普通控件重启后以普通文字恢复候选串，不能重建 IME 组合会话；方向/affinity 也不能从 Harmony 的起止选区事件反推。较新草稿遇到已变化的业务来源会保留并显示冲突，不隐式覆盖或重基。

**正式业务在途请求、未确认草稿/清理请求仍只在进程内保留；尚未接原 captured S1/S2 交接及正式提交跨进程 Unknown 恢复。** 突然终止只能恢复此前已确认的草稿代次。已成功提交的卡片可重启读回。附件、插件运行/管理、网络服务、TLS、媒体/歌词、字体文件导入、完整九语文案、完整宽屏矩阵和备份恢复仍待补齐。外观与日常清单保存于 Preferences；草稿使用 Rust 核心事务日志，详见 [功能与复用清单](docs/PARITY.md)。

## 跟随主任务

功能参照：[检查项目并完成更名](codex://threads/01a085bd-7a94-7f93-8a1f-1ecf417f5ee3)。环境参照：[安装 Deveco CLI](codex://threads/01a0cc0a-76a5-7973-a6c8-eb7566100932)。

初始快照来自 `../build/io-safety-refactor`，HEAD `ddd9cc8eec224af51f4e58654c9b297332147d96`，版本 `0.1.9-test.54+58`，包含未提交增量。2026-10-05 核对该参照为 `925fb8ca` / test.57；较新 Windows 汇合工作树为 `77246617` / test.58。冻结快照未整体替换；当前有 50 个共享路径变化、72 个新增路径，文件 IO、通道、生产监督/恢复和 SDK 仍需独立审查。四个查询/卡片/TaskId 业务块哈希仍一致；查询调度和文字草稿模型分别固定来源。见 [上游审计](reports/ui-source/v9/upstream-audit.md)。

`shared/reference.json` 固定 228 个共享文件的 SHA-256，包括 V2 新增源码。`check-reference.ps1` 比较真实工作树与快照并写入差异报告；不自动覆盖正在使用的源码。后续同步必须审查差异、更新功能清单、重跑 Rust/HAP/设备验证。没有创建定时任务或向原任务发送消息。

共享源码保留原 AGPL-3.0-only 许可与 `shared/LICENSE`；快照不是另起一套业务规则。ArkTS/C++/Rust 适配代码属于本目录；原 Flutter 与活跃工作树未修改。

