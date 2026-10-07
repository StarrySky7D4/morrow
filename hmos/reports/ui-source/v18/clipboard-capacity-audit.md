# dev.18 剪贴板来源容量审计

Fresh 记录：`2026-10-07T05:32:30.168842+00:00`。范围为真实 Flutter 来源、HMOS SDK/文件准备边界及主机实际 ETS 模型；完整 Flutter 功能等价仍 **OPEN**。不将已安装 dev.17 的系统 plain/TSV 观察计作 dev.18 新包、富格式或原生 FD 验收。

## 参照身份

| 工作树 | Fresh HEAD | 实读范围状态 |
| --- | --- | --- |
| `build/io-safety-refactor` | `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` | 所查六路径 clean |
| `build/win-cloud-20261005` | `e83cdf1d3506b001d07ffceaaa9f98b688bcc150` | 所查六路径 clean |
| `build/windows-sdk-reconstruction` | `20669f671152972470340a65eac3458dd2f61b4d` | 所查六路径 clean |

三者以下六个 Git blobs 完全一致。实读工作文件规范化 CRLF/LF 后逐字节相同；原字节 SHA 会因换行不同而变化，因此表中列 Git blob 身份，不把换行误记为设计漂移。不扩大为整个工作树、SDK 或主任务资格。

| 实读路径 | 三参考共同 Git blob |
| --- | --- |
| `lib/main.dart` | `2e00f41251c3215f0b6d886712c86adfdf2bb6e6` |
| `lib/attachments/clipboard_import.dart` | `f23ff12b4446ac0e984ea04aab8424be21a018ab` |
| `lib/attachments/office_clipboard.dart` | `24f54bee34f7a1902a0cf45811e1d35cd07d66a8` |
| `lib/content/rich_content.dart` | `3c53039a1a863ea92d34d8b1e5ec4f03a4938044` |
| `lib/plugins/capture_native.dart` | `7d5f158bea809e4e07f1d0f1ca4a031a15101746` |
| `lib/plugins/studio_native.dart` | `82d2b1064ef8467cdf64164d3b799338cb66ab1f` |

## Flutter 实际限制及调用链

`clipboard_import.dart:165` 在系统记录上先检查 plain `text.length <= 2*1024*1024`；Dart String.length 是 UTF-16 单元数。`rich_content.dart:50` 的 HTML、`:214` 的 Spreadsheet XML 也各限 2 Mi UTF-16 单元。文件和原始格式附件另按 `IdeaAttachment.maxSize`、累计 200 MiB 与最多 20 个检查（`clipboard_import.dart:129-140`），不是 2 MiB 转换字节上限。

RTF 必须区分两条可达路径：`clipboard_import.dart:107-110` 的 `convertRtf` 仅在 **plugin==null** 时调用 `rtfToPlainText`，后者在 `rich_content.dart:276` 限 **8 Mi UTF-16 单元**。RTF-only 系统记录在 `clipboard_import.dart:262`、Windows Office RTF-only 在 `:315` 调用该闭包。`main.dart:5608` 实际调用 `readPaste(reader, _plugin, l)`，`:5281` 的 `_plugin` 来自可选 editor/plugin；该本地路径有真实入口，不能因此说正式原生工作台总使用无插件路径。

plugin 非空时直接调用 `plugin.capture('rtf', text)`，`studio_native.dart:17-30` 转到 `captureWithPlugin`；`capture_native.dart:22` 对所有格式统一限 **2 Mi UTF-16 单元**，`:143` 还检查序列化插件请求 **64 KiB**。调用失败后，系统记录外层 `clipboard_import.dart:270` 或 Office 外层 `:323` 只保留原件/告警，没有转而调用 8 Mi 的异常 fallback。所查三个参考 `lib/` 均未找到 `_toRtfCaptured` 或 `_normalize` 这两个函数名。

因此本轮 HMOS 的 RTF 8 Mi 档覆盖 Flutter 可选本地转换路径；它不是正式 RustStudioPlugin 8 Mi 来源或相同插件消息容量的等价证明。来源准入、格式结构预算、最终字段容量和捕获票据是独立边界。

## HMOS 两档准入与原件

| 请求 format | decoded 原文 UTF-16 上限 | 原始 bytes 传输包络 |
| --- | ---: | ---: |
| plain / html / xml | 2,097,152 | 6,291,459 = 3*N+3 |
| rtf | 8,388,608 | 25,165,827 = 3*N+3 |

合法 UTF-8 每 UTF-16 单元至多三个字节，加三个 BOM 字节；BOM UTF-16 的 `2*N+2` 也在此包络内。字节包络不代替 decoded 原文单元计数：SDK plain string 先按 UTF-16 检查，再编码；plain ArrayBuffer 仅按 byte envelope 准入，由 native 严格解码后检查单位。`AttachmentFiles.clipboardRequest` 按 format 选包络，native 在核验完整原件 SHA 后按同档计真实原文单位，超限明确失败，不能截断后返回成功。

HTML/RTF/XML 即使超过转换单元/字节包络，原件仍可在现有附件预算内准备；转换失败或 fallback 须明确提示，不能把保留原件称为完成转换。原图、文件 URI 和其他二进制原件不套用这两档文本转换限制。现有同进程/根目录 **64 MiB** 总额、**20 个 spool 槽**、`512 KiB+4096` sidecar 预留、当前草稿 pin/导入额度都不放宽。snapshot 读取本身仍限 20 条系统记录、64 MiB 已读原件总量；展开原件/图片再受剩余额度检查。

输出仍限 20,000 UTF-16 单元，字段 UTF-16/grapheme 与待办 500/1000 差距未由来源容量修复关闭。文件读取、原件 SHA、已有 durable import/pin/Unknown 原请求与 late owner/checkCount 规则保持原状态机；二进制不经 JSON。

## 严格编码边界

SDK 字符串在 TextEncoder 前检查配对 UTF-16 surrogate。孤立高/低 surrogate 或两高组合明确警告“未编码或保存该原件”，不让编码器静默生成 U+FFFD 后冒充原件。有效配对 emoji 及有意 U+FFFD 字符在 SDK 原件层正常保存。系统提供的 Unicode RTF **string** 没有原始 document bytes，本轮明确保存为 **带 UTF-8 BOM 的确定性 serialization**，不是声称取得原始文件编码或原始文档字节。BOM 指明 literal Unicode，避免 `ansicpg1252` 等声明把 UTF-8 literal 误读为 ANSI；RTF ANSI hex 仍由 native 分域处理。SDK 原 string 不额外计这个序列化 BOM 单元，native 移除编码 BOM 后核原文 UTF-16 数；全部三个 BOM 字节计入 snapshot/spool 配额、byte_length、原件 SHA 和 native ReadContract，现有 `3*N+3` 包络不放宽。ArrayBuffer 仍逐字节保留，不加 BOM，也不在 SDK 层重新解码/改写原件；native 解码失败和格式档案保存是不同结果。

Flutter `decodeClipboardText`（`rich_content.dart:377-388`）可从 BOM/启发式 UTF-16 或允许 malformed 的 UTF-8 读取并移除 NUL。HMOS native 保持严格 BOM UTF-8/UTF-16 与 RTF 原始 codepage 路径，不做相同宽松替换；未知/非法编码、NUL 和不支持的 RTF 形式须明确失败，不能宣称完全编码等价。RTF raw 原文 codepage 单元计数及 native 编码证明由本轮原生审计单列；此 ETS 报告不替代它。SDK 原件层及 native plain/HTML/XML 接受有意 U+FFFD；native RTF 共享转换输出仍遇 U+FFFD 即保守拒绝，以防 lossy surrogate replacement。这是明确失败的既有差距，本轮未扩大 escaped-surrogate 解析范围。无 BOM 的 RTF 严格按声明 ANSI/codepage；BOM RTF 的 literal Unicode 与 ANSI hex 分域处理，不能套用 Flutter 的无 BOM UTF-16 启发式、malformed replacement 或去 NUL 行为。

## 本轮 fresh 模型证据

| 范围 | 最终结果 | 证据 |
| --- | --- | --- |
| ClipboardInput 实际 ETS + 模拟 SDK | **55/55 PASS**；首个容量版本45，严格SDK字符串修复新增8，Unicode RTF serialization新增2后fresh重跑55 | [log](clipboard-input-model-tests.log) |
| ClipboardFiles 实际 ETS + 模拟文件/native | **59/59 PASS**；Unicode RTF完整length/SHA与BOM计费新增2 | [log](clipboard-files-model-tests.log) |
| 既有 AttachmentFiles 模型 | **97/97 PASS** | [log](attachment-files-model-tests.log) |
| 容量改动首轮实际 API 26 SDK 编译 | **PASS /18.203 s**；临时包仍dev.17身份/native，且早于最后SDK字符串及RTF serialization修复 | [首轮构建](hap-first-build.log) |
| dev.18 最终原生库/HAP/安装/富格式设备资格 | 本报告不提供此资格；由主代理最终报告分别记录 | 不使用旧包替代 |

模型执行真实 ETS，provider、FS、native 是模拟对象；不是实际 SystemPasteboard、Office 提供者或设备 FD 运行。覆盖 ASCII/CJK/emoji 的精确 2 Mi UTF-16 边界和一单位超限、raw plain 字节包络、四格式 byte preflight 两档边界、decoded-unit 失败保留完整富原件、两种转换包络外的原图/文件、64 MiB 总额不变，以及严格 SDK 字符串三种情况、RTF Unicode/emoji+ansicpg1252 的带 BOM serialization、无 BOM 原始 ANSI ArrayBuffer 保留、完整serialized bytes的length/SHA与3B额外预算。字节 preflight 的虚拟大文件专测 ArkTS 准入，不冒充 native 完整读取或解析证明。

v17 原报告与日志保持历史。本轮未改 Index、全局文档、版本或共享快照，也未操作设备、提交或推送。
