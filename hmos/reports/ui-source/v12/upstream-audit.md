# dev.12 上游版本与附件设计审计

2026-10-05 10:54 UTC / 18:54 Asia/Shanghai。工作区根目录为 `C:/Users/Administrator/Desktop/CodeXProjext/morrow`。本轮只读检查参考工作树、源码、来源声明和源线程 compact snapshot，只新增本报告；没有更新远端引用、向源线程发送消息、修改参考源码、操作设备或提交/推送。三个交付文档此前已由单独授权更新，本报告阶段未再修改它们。

## 本地版本快照

| 工作树 | 分支 | HEAD | Flutter pubspec 版本 | 最近提交时间 |
| --- | --- | --- | --- | --- |
| `build/io-safety-refactor` | `codex/m03-stream-revocation-backpressure` | `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` | `0.1.9-test.57+61` | 2026-10-01 20:33:39 +08:00 |
| `build/win-cloud-20261005` | `codex/windows-sdk-convergence-20261005` | `772466177fe589cee53bc633e69f411c34610104` | `0.1.9-test.58+62` | 2026-10-05 12:40:26 +08:00 |
| `build/windows-sdk-reconstruction` | `codex/windows-sdk-qualification-20261003` | `20669f671152972470340a65eac3458dd2f61b4d` | `0.1.9-test.58+62` | 2026-10-04 23:59:22 +08:00 |

三个 HEAD 与本轮前次检查相同。所查 `pubspec.yaml`、主界面、附件/内容目录、草稿 binding/session/workspace、原草稿/暂存/schema 工作文件没有未提交修改。这里的“最新”指本地活跃参考工作树，不代表已查询远端或源线程未来提交。

HMOS 当前 `AppScope/app.json5` 为 `0.1.0-hmos-dev.12`，versionCode `1000012`。开发适配源码与参考快照应分别记录，不能仅凭版本号声称 Flutter 功能等价。

## 所查 Git blobs

以下路径分别相对三个参考工作树；每行 blob 在三者均相同。所查附件 UI 没有因 test.57→test.58 出现设计变化。

| 源路径 | 三工作树共同 Git blob |
| --- | --- |
| `lib/main.dart` | `2e00f41251c3215f0b6d886712c86adfdf2bb6e6` |
| `lib/attachments/attachment.dart` | `751b45933d25bfcd32b779885c104eea67c87ecc` |
| `lib/attachments/attachment_view.dart` | `76be11b131c15d8c4c101d15f6d9568400eca3c8` |
| `lib/attachments/clipboard_import.dart` | `f23ff12b4446ac0e984ea04aab8424be21a018ab` |
| `lib/attachments/office_clipboard.dart` | `24f54bee34f7a1902a0cf45811e1d35cd07d66a8` |
| `lib/attachments/file_access_native.dart` | `3116024784acf5e9b1a93cd2a5306fc0a93acbbe` |
| `lib/content/rich_content.dart` | `3c53039a1a863ea92d34d8b1e5ec4f03a4938044` |
| `lib/content/idea_markdown.dart` | `165a6f789755db519ca567ca847ce961f9cc1add` |
| `lib/plugins/editor_draft_binding.dart` | `66f483cfbc0307bc61d6d82f11bf334e208ca3b2` |
| `lib/plugins/editor_draft_session.dart` | `bc29015a1f61d93a5129031e669c0634fc7a09a4` |
| `lib/plugins/editor_draft_workspace.dart` | `20c97efc7d54ad9fd1dae647ad8d6af6d47ba677` |
| `workbench_host/schemas/editor_draft.proto` | `73e89a22ad83995ec066e6429ecba7033d609fc9` |
| `workbench_host/src/editor_draft/model.rs` | `5a1c409b23315ec44c930669f7d6145d3ce333be` |
| `workbench_host/src/editor_draft.rs` | `edbaf83383d09d1559062a4da9aa4e1e09077974` |
| `workbench_host/schemas/editor_draft_staging.proto` | `8b0aa7a53c3fd44aeaf6b5af1ff36f75cd8a6084` |
| `workbench_host/src/editor_draft_staging.rs` | `5bbd3d7fed9f7170c845752344de8d5a8043fc5a` |
| `workbench_host/src/versioned_record.rs` | `24c7b527f25338b26a2f022037b4ac4bcc1d6be4` |

文件 SHA256 与 Git blob 不是同一种比较。例：io-safety 的 `main.dart` 原文件 SHA256 为 `6203B7465F320DC8463DFACB5387EA8B437EC3EC1458CCA7E215E44B8F65CB1E`，将 CRLF 统一为 LF 后为 `2CB2A519E31AC982D3A8638EB7DE95FE63D5421ED3D1B6ACDA507CD142169F06`，与另两个工作树原文件一致。三者 working blob 也均匹配上述 HEAD blob，不能把此换行差异报告为 UI 漂移。

## 原 schema/model 与来源声明

检查 `hmos/rust/editor-draft-reference.json` 四个 source_files 和 `hmos/rust/editor-draft-staging-reference.json` 三个 source_files：source_worktree/source_head 与上述 io-safety HEAD 匹配，七项 SHA256 声明全部匹配实际原文件。下表为去重后的六个源输入。

| io-safety 原文件 | 当前原文件 SHA256 / 声明值 | 副本或复用方式 |
| --- | --- | --- |
| `workbench_host/schemas/editor_draft.proto` | `8F80DB114BCC480D9553AE60916170B1982C9CDE0D0452753A35B491279FDDDC` | `hmos/rust/editor-draft-model/schemas/editor_draft.proto` 字节一致 |
| `workbench_host/src/editor_draft/model.rs` | `D11632512506F357A73112568A6E8856AA74E87500B91709C87083A50DAC6C8E` | `hmos/rust/editor-draft-model/src/lib.rs` 字节一致，另有 wrapper |
| `workbench_host/src/editor_draft.rs` | `B4ADAFD9EDCA7ECBA9E4DD3A23963F226E3009233914D62C16630A1C7AC0DA28` | HMOS 适配 journal、history、pin、计费/清理与 publication；非逐字复制 |
| `workbench_host/src/versioned_record.rs` | `6A0131915BAC966FF745DA5580A91256A918D0C10B246B9E7C4120E972385E8A` | HMOS 草稿来源解码/附件绑定适配 |
| `workbench_host/schemas/editor_draft_staging.proto` | `5FD6183551224232AA90ADB6F7D90E26317E2B4A294F155732668A8B9EA4E981` | `hmos/rust/editor-draft-staging-model/schemas/editor_draft_staging.proto` 字节一致，独立 prost wrapper |
| `workbench_host/src/editor_draft_staging.rs` | `4DE975878809AD9FCB969693768165BC55AD0090FDD8971DAA64ABB7215B2C99` | HMOS 适配原状态机/身份/事务/历史/保留与清理；非逐字复制 |

检查时 HMOS `rust/src/editor_draft.rs` SHA256 为 `49410A0892F3673716552FDB5C8BFE4A9188BF21109C31F850E32B349F43A500`，`rust/src/editor_draft_staging.rs` 为 `0FEBB0C9F8DF31EC104D473FAC9B89C9F2CAF4EFD4DA6715B25C9FD2DEEF0E87`。它们与原宿主不同是已声明适配，不是 schema/model 副本失配。schema/model 的形状校验不提供捕获或业务提交权限。

审计发现并报告了主草稿来源清单残留的 dev.9“全部附件/consumed imports 拒绝”阶段说明。root 已修正，随后只读复核：`editor-draft-reference.json` 现为 `phase: raw-text-durable-journal-with-attachment-pins`，允许 origin 0/2/3 的 pin 和 consumed imports，边界继续拒绝 captured 1/4、predecessor/parent/retirement lineage；四项源哈希仍匹配。该旧说明已经修正，不再列为当前缺口。staging-reference 为 `durable-editor-imports-with-selected-draft-pins`，其保护、捕获、平台和测试范围边界保持明确。本报告没有修改两个来源清单。

## Flutter 附件设计定位与 HMOS 差距

下列 Flutter 定位相对 `build/win-cloud-20261005`，另两个工作树同 blob；HMOS 定位相对 `hmos`。

| Flutter 源码与现有行为 | dev.12 HMOS 实现及剩余差距 |
| --- | --- |
| `lib/main.dart:5439`，循环导入最多 20 个文件，空标题用文件名补齐；`:5781` 粘贴/导入工具行 | `model/AttachmentFiles.ets:257` + `pages/Index.ets:591` 使用实际文件选择器 URI 流，单次选一个；取消不创建草稿 owner。多选及自动填空标题未对齐 |
| `lib/main.dart:5498`，正文编辑/Markdown 预览；`:5596` 按目标字段粘贴，随后导入剪贴板文件，纯图片可插入 attachment URI | 标准 Markdown、授权纯文字/TSV 粘贴保留；文件选择器导入已接。`Index.ets:377` 仍只读取可读文字，明确提示富文本样式、图片/文件未从剪贴板导入 |
| `lib/attachments/attachment_view.dart:41`，类型图标、扩展名、大小、点击预览/默认打开、导出、可选移除 | `Index.ets:2172` 保存附件提供图片/GIF 预览及导出；`:2267` 编辑器附件提供选择/移除与导入核对。通用文件默认打开、编辑器附件行导出及完整图标/提示设计尚未对齐 |
| `lib/attachments/attachment_view.dart:141`，音视频 MediaKit 控件，图片 InteractiveViewer 缩放与读取/解码错误提示 | `Index.ets:716` 限图片/GIF，`:2561` 用核验后的私有临时文件 Image 预览。音视频播放、图片缩放及解码失败 UI 未接 |
| `lib/content/idea_markdown.dart:61`，attachment 图片按名称/位置/资产 ID 匹配选中附件；远程图片显式点击读取 | `pages/MarkdownPreview.ets:25` 仅允许 http/https 图片读取；attachment URI 仍显示“图片未导入”，尚未接选中资产解析与核验后的内嵌读取 |
| `lib/attachments/clipboard_import.dart:153`，文件 URI、HTML 原件及 Markdown 转换、图片等多格式；`:277` + `office_clipboard.dart` 提供 Windows Office 补充读取 | HTML/Office/RTF、剪贴板图片/文件和捕获票据仍未接；本轮普通文件导入不提供这些资格 |
| `lib/attachments/file_access_native.dart:20` 导出；`:33` 普通文件外部打开，脚本/可执行类型转为仅导出 | HMOS 按源修订或草稿世代核验完整字节后经保存选择器导出；暂不提供通用文件外部打开 |

优先补齐 `attachment:` 内嵌图片，以及音视频/通用文件打开与编辑器附件行导出，可直接复用本轮已核验的流式读取通路。多选、空标题和图片交互设计随后对齐。完整富剪贴板需独立绑定捕获与资产来源，不能仅把 HTML 显示出来就称完成。

`main.dart` 没有直接实例化 `EditorDraftBinding`、`EditorDraftSession` 或 `EditorDraftWorkspace`；它使用 WorkbenchEditorSupport/WorkbenchEditorSession 等接口。不能把独立 binding/session/workspace 的原测试资格转移为此 HMOS 编辑器已具备原 captured S1/S2 生产流程。

当前 HMOS 可复用原 durable import 的 Pending/Ready/Retired/Pruned、私有 journal、附件 pin、SHA256/长度核验和 consumed cleanup；允许 source 0、durable import 2、same-draft previous pin 3。origin 1/4、captured predecessor、parent/retirement lineage 仍明确拒绝。16 活动草稿、256 累计身份与含 pin 字节的 64 MiB 活动预算、64 MiB 准备缓存预算仍有效，与 Flutter 普通附件 200 MiB 上限不等价；删除当前引用不承诺历史 blob 即时回收。

生产 HUKS/库身份/租约/审计封存、正式业务原请求跨进程 Unknown、captured S1/S2、跨段连续全篇选择、完整宽屏矩阵、签名/ARM64 真机等边界未因本轮解除。[附件主机验证](attachment-host-review.md)记录已通过的限定主机范围；[dev.12 验证记录](validation.md)由 root 单列最终设备、平台与构建证据，本报告没有操作或核验设备。

## 源线程 compact snapshot

只调用只读 `wait_threads(timeoutMs: 0)`，未发送消息；多目标调用先返回环境线程的 inactive 状态，随后以功能线程单目标取得更新。

| 源线程 | 本次状态与最近进度 | Cursor / revision |
| --- | --- | --- |
| `01a085bd-7a94-7f93-8a1f-1ecf417f5ee3` | active；latest turn `01a10bb1-330e-75d0-b491-c5f5a4d5ed91` 为 inProgress。最近说明：待推送 C08–C10 的目录授权、选择链与插件请求接口增量；C08/C09 有阶段验证，C10 修复后测试尚未重跑，先补验证及回归再提交，完整 SDK 未完成项保留 | `11781d34-6efe-4262-8ec6-e839f8da2203:13` / 13 |
| `01a0cc0a-76a5-7973-a6c8-eb7566100932` | thread notLoaded，latest turn `01a0cc30-8cf7-7cf0-9a60-fa166099e671` completed。cursor 未变化；此前最终结论为 CLI 1.3.0-stable 项目创建/API26/未签名 HAP 构建可用，未验证应用安装运行、签名和发布；代码检查文件数 0 不能证明有效检查 | `3331ad88-1a98-4bb5-88f6-adb7bd334e4e:1` / 1 |

功能线程此前 cursor 12 的 Windows 测试组/示例插件构建进度已被上述 revision 13 更新。线程进度与已落本地参考 HEAD 分开记录，不能把 inProgress 任务或源平台阶段验证视为 HMOS 完成证据。
