# 2026-10-05 dev.10 Markdown 与粘贴来源审计

本报告只读核对实际源码、工作树 HEAD、Flutter 应用版本、Git blob 和 SHA-256；未构建、操作设备或复验 Windows 测试。上游工作树仍在推进，以下是本次观察值。

| 活跃工作树 | HEAD | `pubspec.yaml` 应用版本 |
|---|---|---|
| `build/io-safety-refactor` | `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` | `0.1.9-test.57+61` |
| `build/win-cloud-20261005` | `772466177fe589cee53bc633e69f411c34610104` | `0.1.9-test.58+62` |
| `build/windows-sdk-reconstruction` | `20669f671152972470340a65eac3458dd2f61b4d` | `0.1.9-test.58+62` |

三个工作树的下列 Git blob 均相同，相关源码和 pubspec 本次未发现已跟踪修改：

| 源码 | Git blob |
|---|---|
| `lib/main.dart` | `2e00f41251c3215f0b6d886712c86adfdf2bb6e6` |
| `lib/content/idea_markdown.dart` | `165a6f789755db519ca567ca847ce961f9cc1add` |
| `lib/content/rich_content.dart` | `3c53039a1a863ea92d34d8b1e5ec4f03a4938044` |
| `plugins/workbench/src/capture.rs` | `8c50094af0a350449d0f848ed656c32dc8654685` |

对应设计参照为：`idea_markdown.dart:11` 的可选择 Markdown、标题/代码/引用/表格样式、安全链接及远程图片显式读取；`rich_content.dart:37` 的链接规则和 `260` 的 TSV 表格转换；`main.dart:5498` 的 `_bodyEditor` 预览、590 宽度分栏条件；`main.dart:5596` 的 `insertPaste` 按焦点选择字段、替换当前选区、检查字段长度及粘贴结果提示。HMOS 使用原生 ArkUI 显示 DTO 和独立异步预览协调，属于设计及行为适配，不是 Flutter widget 代码直接复用，也不继承原测试资格。

以下为 `io-safety-refactor` 当前文件原始字节 SHA-256：

| 文件 | SHA-256 |
|---|---|
| `lib/main.dart` | `6203B7465F320DC8463DFACB5387EA8B437EC3EC1458CCA7E215E44B8F65CB1E` |
| `lib/content/idea_markdown.dart` | `816AC4C1B3A04149A51C5A1453C2F5F17B5E0DD750A74D9A27571CE5F68BA9E0` |
| `lib/content/rich_content.dart` | `BAD8A530EF678B453CBDB1E1F9A0FE33B712FE44CB0D0C25A4D3A08B7428D4E9` |
| `plugins/workbench/src/capture.rs` | `89D199F3A831B489D047C56F23CE520F451E37B4F163CB2F699B4F91F30B4BCF` |

`hmos/shared/plugins/workbench/src/capture.rs` 与上述原文件逐字节相同，SHA-256 同为 `89D199F3…F30B4BCF`；dev.10 `hmos/rust/src/markdown.rs` 调用其中 `capture::plain`（原文件第 71 行）和 `capture::safe_link`（第 37 行）。另外两个工作树的 capture 原始字节 SHA-256 为 `55BFED8EAC946545F0F31602641F55CDABB64B19EFDEF7945EBB6F6E2F01A010`；本次确认差异仅为 CRLF/LF，统一 LF 后内容相同，Git blob 也相同。没有覆盖 HMOS 原冻结来源清单。

Markdown 解析使用本轮新增的 `pulldown-cmark` 适配，输出标题、段落、引用/列表层级、代码、分隔线、表格、强调、删除线、链接及图片 alt。原始 HTML 作为文字展示，不执行 HTML。URL 复用原 safe_link 后额外拒绝控制字符及有歧义的目的地址。解析和纯文字粘贴转换为只读动作，不访问 URL、附件或业务 Store，不产生 captured S1/S2 证明。TSV 转换前检查完整行列预算，超限拒绝整次转换，避免原 plain/table 的历史 take() 行为截断内容。

当前粘贴读取系统授权 PasteButton 下的纯文字条目，冻结目标字段与 UTF-16 选区，拒绝候选输入、剪贴板读取期间变化、目标文本/选区变化和长度超限。正文可将 TSV 转为 Markdown 表格。完整 HTML/Office 富文本转换、图片与文件附件导入、持久化附件别名和附件恢复尚不具备；有 HTML 的条目只使用其可读纯文字。远程图片只有用户点击读取后才创建 Image，附件 URI 不转为外部路径。

预览按块提供选择/复制；连续跨段落全篇选择尚不具备。表格横向滚动，单元格内图片同样通过显式读取入口显示，附件 URI 仍仅呈现未导入提示。宽屏按正文可用宽度分栏，窄屏切换为预览；真实控件光标、候选输入、图片读取及两种宽度下的最终显示仍需实际 HMOS 产物验证。本报告不宣布完整 UI/功能追平或生产存储资格。
