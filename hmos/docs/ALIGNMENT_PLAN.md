# 持续追平 Windows 工作台

目标：持续跟进 `01a085bd-7a94-7f93-8a1f-1ecf417f5ee3`，直至 HMOS 追平 Windows 侧 UI 与大部分实现。独立 `hmos/` 工程、Rust 复用、ArkUI/NDK 路线不变；只推送 `codex/ArkTsUI`，不合并主线。

这是目标的验收清单，不把单轮构建或小功能交付当成最终完成。当前 HMOS dev.13 在 dev.12 URI 附件导入、原 durable staging、草稿 pin 和按修订导出基础上，接入 Markdown 内嵌图片、图片读取/解码状态、缩放/受限平移/复位及编辑器附件行导出。Rust 主机 72 项、实际 ArkTS 模型 163 项与 API 26 HAP 构建通过；最终包 x64 模拟器已验中文名称引用的图片、按钮 125% 放大/100% 复位和未发布 pin 的导出字节一致。测试卡保存后的表格图片、缺失/远程提示与重启无新增草稿通过；查询观察中断后核对现有卡，未重复保存。移除图片后保存前即更新为缺失提示；显式保存/重启保持同一卡身份、原始 Markdown 不变、零附件且无剩余 raw journal。原生双架构库复用 dev.12 字节，真实手势/损坏解码/GIF 未验收，完整范围见 [dev.13 验证记录](../reports/ui-source/v13/validation.md)。

保留 dev.11 阅读详情/卡片菜单/五页持久顺序、dev.10 Markdown/授权粘贴、dev.9 raw journal 和 dev.4–8 的待办、材质、边框修复、响应布局、懒加载瀑布流与 Rust 查询。dev.12 的 Windows 主机范围见 [附件主机验证](../reports/ui-source/v12/attachment-host-review.md)，该版 x64 原生附件/FD 与实际 UI 闭环另记 [dev.12 验证记录](../reports/ui-source/v12/validation.md)，旧包证据不能充作本轮回归。真机、生产保护和完整功能对齐仍开放。

2026-10-05 dev.12 只读版本跟进：原参照工作树 `build/io-safety-refactor` 为 `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` / `0.1.9-test.57+61`；较新 Windows 汇合工作树 `build/win-cloud-20261005` 为 `772466177fe589cee53bc633e69f411c34610104` / `0.1.9-test.58+62`，另有 `build/windows-sdk-reconstruction` 的 `20669f671152972470340a65eac3458dd2f61b4d` / 同应用版本。三者所查主界面、附件、富文本、草稿和暂存源码 Git blobs 相同，相关工作文件 clean；文件换行差异不代表源码功能变化。功能源线程当前仍在构建 Windows 测试组和示例插件宿主流程。具体排序语义和平台边界见 [dev.11 上游审计](../reports/ui-source/v11/upstream-audit.md)及 [原生交互审计](../reports/ui-source/v11/platform-audit.md)；源任务编译进展不等于 HMOS 或普通工作台已具备相同资格。

2026-10-07 dev.13 再次只读检查：Windows 汇合 HEAD 为 `e83cdf1d3506b001d07ffceaaa9f98b688bcc150`，另外两个参考 HEAD 与上述历史观察相同。三个工作树所查 17 个 UI/附件/Markdown/草稿/暂存源码 Git blobs 仍一致，相关工作文件没有未提交修改；不扩大为整个 SDK 或工作树资格。功能线程当时仍在修复 Windows pipe/ConPTY 错误传播，最终结果未取得。观察时间、源码位置与边界见 [dev.13 上游审计](../reports/ui-source/v13/upstream-audit.md)和 [平台审计](../reports/ui-source/v13/platform-audit.md)。

附件 UI 可直接对照 Windows 工作树 `lib/main.dart:5439` 的导入/空标题补齐、`:5498` 的正文/预览、`:5596` 的富剪贴板插入和 `:5857` 的附件行；`lib/attachments/attachment_view.dart:41` 提供类型图标、扩展名、大小、点击预览/默认打开、导出和移除，`:141` 提供音视频播放与图片缩放；`lib/content/idea_markdown.dart:61` 按名称/位置/资产 ID 解析 `attachment:` 图片。`lib/attachments/clipboard_import.dart`、`office_clipboard.dart`、`file_access_native.dart` 是后续平台设计来源。`main.dart` 未直接实例化 `EditorDraftBinding` / `EditorDraftSession` / `EditorDraftWorkspace`，其原专项测试资格不能继承到 HMOS raw journal。

dev.8 已接入原 `query_plan_v2` 的分页过滤、稳定排序和归并，复用已冻结且与上述参照逐字节相同的 `query_v2` / codec / TaskId / 卡片业务块，替换正常视图的 ArkTS 内存筛选。查询调度新增来源单独记录；不全量覆盖 `shared/reference.json`。该轮 Rust 12 项、纯 ArkTS 模型 10 项、OHOS x64 原生 13 项检查通过；该版最终包 440 vp 的查询 UI、重启读取与单列四卡遍历通过。证据及未闭合资格见 [dev.8 验证记录](../reports/ui-source/v8/validation.md)。

| 范围 | 当前状态 | 达标证据 / 后续工作 |
|---|---|---|
| 首页、导航、分类卡片、收藏、编辑器、响应布局 | dev.8 新包已验 440 vp 单列、744 vp 两列的查询、四卡间距/遍历/重排；dev.7 保留 16 张交错高度设备回归，dev.6 保留 880/1488 vp 面板与位置恢复。完整响应矩阵仍未追平 | 真实 Flutter 源码与 HMOS 在手机/平板宽度、浅/深主题逐页对照；列表、弹窗、键盘、返回流程实际可用；大库内存/帧时与隐藏节点资源资格不能用可访问性节点数替代 |
| 七种风格、玻璃模式、深度、色盘、组件材质跟随 | dev.5 已接基础面板/深度/材质跟随；错位边缘已修复 | 渲染参照、七风格交叉玻璃模式、主题切换、控件与过渡、跟随/循环/取消/保存/重启测试；不能只显示风格名称 |
| 字体、语言、背景、窗口行为 | 系统字体/部分九语/内置纹理已接；文件导入与完整文案缺失 | 字体和背景选择器、持久 URI/授权、动态文案、可访问性与宽屏实测 |
| 卡片、TaskId、分类/阶段、回收站 | dev.11 接阅读详情、显式编辑、复制、长按/菜单及持久前后移动和独立拖动柄；dev.4 完整 TaskId 编辑保留 | 补任务迁移、详情完整任务操作、拖动边缘自动滚动和鼠标/键盘交互矩阵；保持未知字段、CAS、原操作回执、删除时间规则 |
| 查询、排序、大库加载 | dev.8 已接原 Rust 计划、单次完整快照、完整属性搜索与稳定排序；去抖/待发合并/过期回包丢弃，真实失败与空结果分开 | 仍限定 256 张；需 UI 响应分页、大库内存/帧时和生产 guest/捕获资格。旧“最近添加”为反向 ID 顺序，不能宣称为创建时间排序 |
| 正式 Rust 宿主和存储会话 | 未接；当前为独立未封存开发库 | 提取平台会话接口，HUKS、稳定身份、单库所有者、审计/备份/恢复契约；禁止绕过原宿主非 Windows 拒绝规则 |
| 持久草稿、S1/S2、未知结果核对 | dev.12 在原 schema/model、固定来源/raw journal 上接选中资产、pin、计费、consumed import 清理和持久 import 核对；SAME 已提交业务操作可跨草稿推进/弃稿精确核对 | 草稿确认与业务提交分开；仅来源 0/2/3，仍拒绝 1/4、前驱捕获/父子交接。继续接业务提案跨进程原请求恢复、captured S1/S2 续写重基；主机通过不提供生产保护资格 |
| 附件、剪贴板、Markdown/富文本、导入导出 | dev.13 补 Markdown 当前附件图片解析、段落/表格共享核验读取、读取/解码状态、缩放/受限平移/复位、编辑器附件行导出和已登记预览失败清理的显式重试；dev.12 URI/durable import/journal/pin/一致性保存与按修订导出、dev.10 授权纯文字/TSV 粘贴保留 | 各版证据分列；本轮 x64 图片、按钮缩放/复位、原 pin 导出、保存后表格图片与重启只读已验，真实手势/损坏解码/GIF 和实际清理故障仍待验收。单次一个文件、64 MiB 准备与含 pin 草稿预算不变；活动内嵌图另限 8 个/64 MiB，未登记读取失败目录只尽力清理、不在重试账本内；账本不扫描死亡进程遗留的全部物理缓存。补音视频控制、文件默认打开、多选与空标题补齐、HTML/Office/剪贴板图片文件及全篇连续选择 |
| 音乐、歌词、解密 | 空状态 | 复用 Rust 解密模块，播放器、播放列表、歌词、文件权限、后台/中断恢复实测 |
| 插件包、动态 UI、HTTP/服务/文件任务 | 未接运行期宿主 | 跟进 Windows 实现与合约，接 Wasm 执行/权限/资源预算/服务与任务控制；区分 Windows 平台实现和共享业务块 |
| 分发与资格 | dev.13 未签名 HAP 构建、既有 x64 模拟器运行；复用 dev.12 双架构原生库，无真机与签名 | 最终包运行、进程中断、前后台、拒权、损坏、空间耗尽和并发；记录签名/ARM64 真机等外部条件 |

每一轮更新源码观察与实际功能证据，保护 `shared/reference.json` 的冻结来源。上游在途代码不得未经审查直接覆盖。新版本截图、测试日志和构建输入哈希分别记录，旧图不能被当成新包验收。

远端推送失败时保留本地提交；必须读取远端分支确认是否收到，再决定是否重试。其他任务对 `main` 的更新不属于本任务操作，不重置、不覆盖。
