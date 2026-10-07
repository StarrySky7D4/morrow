# dev.18 剪贴板来源容量与图片基础手势交付

2026-10-07。按用户要求交付 `codex/ArkTsUI` 的源码、模型和构建；完整 Windows/Flutter 对齐目标 **OPEN**。最终 **0.1.0-hmos-dev.18 /1000018** 未签名 debug HAP，**27,866,016 bytes**，SHA-256 `910F3069B7979E5F8C6CF9E7DC770DF6E342725D493798DDE38F317524DB7DE2`，归档 `.build/artifacts/dev18/entry-default-unsigned.hap`。**dev.18 尚未安装，设备仍运行 dev.17；本轮新包设备验收 NOT_RUN**。

## 本轮行为

plain/HTML/Spreadsheet XML 来源按 **2,097,152 UTF-16 单元**、RTF 按 **8,388,608** 核原文；字节包络分别 **6,291,459 /25,165,827**。SDK plain string 先检查单元数，native 严格解码后检查准确原文；byte envelope不代替语义限制，SHA和FD保持完整原件，超限明确失败，不能截断后成功。RTF8Mi只对齐Flutter可达的 **plugin==null 本地分支**；真实RustStudioPlugin入口仍统一2Mi单元、插件请求64KiB，调用错误不转无插件fallback。三参照HEAD/共同源码哈希与准确调用链见 [SDK来源审计](clipboard-capacity-audit.md)、[原生来源审计](clipboard-native-capacity-audit.md)。

系统UnicodeRTF **string** 保存为明确的 **UTF8+BOM deterministic serialization**；这是系统Unicode flavor的格式档案，不声称取得原始document bytes。literal Unicode与ANSI hex编码域分别处理；全部BOM bytes计入预算/长度/SHA/ReadContract，源单元不额外计序列化BOM。系统 **ArrayBuffer** 原件逐字节保留，不加BOM或重写。SDK孤立surrogate在编码前拒绝；有效emoji/有意U+FFFD在原件层保留。native保持严格损坏编码/NUL/未知RTF失败，区别于Flutter无标记UTF16启发式、malformed replacement和去NUL；RTF输出仍保守拒绝有意U+FFFD，这是明确未齐差距。

沿用dev.17授权快照/一次性来源、原件/内嵌图准备、完整preflight、逐项durable import、确认pin后映射引用与冻结选区插入。64MiB全局/20槽/sidecar及草稿pin预算不变，较大富格式原件可保留但转换失败须告警，原图/文件不误限为文本来源。晚回调、停止和Unknown仍保留准确请求与已确认项，不自动重放。

图片原有基础手势修复移动双指焦点、单指/纯双指pan、pinch接管、tight布局边界和立即反向；end/cancel及token/URI/epoch/geometry/owner/SDK timestamp围栏防旧事件。实际Flutter布局下有效手势缩放1–2.5；按钮和双击复位是既有HMOS补充。**30/30模型通过，最终包真实图片手势 NOT_RUN**；惯性/fling/scale-velocity动画未实现，GIF/损坏图片未验收，见 [图片手势源码审计](image-gesture-source-audit.md)。

## Fresh 检查

| 范围 | 最终结果 | 证据 |
| --- | --- | --- |
| 完整实际 ETS 模型 | **464/464 PASS** | [log](arkts-final-model-tests.log) |
| ClipboardInput / ClipboardFiles / 既有 AttachmentFiles | **55/59/97 PASS**；provider/FS/native模拟 | [input](clipboard-input-model-tests.log)、[files](clipboard-files-model-tests.log)、[既有文件](attachment-files-model-tests.log) |
| 图片实际 ETS 和实际组件原方法 | **30/30 PASS**；非设备手势 | [log](attachment-image-gestures-model-tests.log) |
| Windows Rust完整library suite | **98 PASS，0 fail，2条件比较默认ignored** | [log](clipboard-rust-tests.log) |
| 五个既有实际Flutter完整输出，final Rust单独对照 | **1 PASS，五组完整输出保持一致** | [log](clipboard-five-fixture-rust-compare.log) |
| Fresh实际Flutter容量捕获 + 单独Rust条件比较 | **Flutter 1 PASS，Rust 1 PASS**；六组HTML/XML/无插件RTF边界的完整source身份、长度/SHA、结果及成功输出一致 | [Flutter](clipboard-capacity-flutter-tests.log)、[Rust比较](clipboard-capacity-rust-compare.log)、[完整身份](clipboard-capacity-reference.json) |
| OHOS ARM64/x64 release native | **PASS**，新构建 | [ARM64](clipboard-arm64-build.log)、[x64](clipboard-x64-build.log)、[253原生输入与candidate身份](clipboard-native-inputs.json) |
| 最终 API26 HAP | **SUCCESS /6.782 s**，完整含最后RTF string serialization修复 | [log](hap-release-build.log) |
| 最终 HAP native条目完整字节核对 | **PASS**：四个条目全部完整解压后核验 SHA/长度；双 ABI libmorrow 与 dev.17 不同，两份 libc++ 相同。253 原生输入、2 候选库、2 份保留 dev.17 库及 2 个当前链接输入均核对通过 | [比较](native-package-comparison.json)、[原生输入核对](clipboard-native-input-check.log) |
| 最终 HAP输入 disk/staged核对 | **302 inputs，disk / staged 均 PASS**，完整 HAP 字节匹配上述 910F3069… 包 | [disk](build-manifest-disk.log)、[staged](build-manifest-staged.log) |
| dev.18 安装/富内容与容量设备/图片手势/原件pin保存重启导出移除 | **NOT_RUN** | 当前设备仍dev.17 |

首轮18.203s临时编译与RTF serialization前 **460模型/7.474s** 候选分别保留 [首次构建](hap-first-build.log)、[修复前模型](arkts-before-rtf-string-model-tests.log)、[修复前构建](hap-before-rtf-string-build.log)；它们不代替最终464/6.782s包。模型运行真实ETS，但平台对象是模拟；本地Flutter对照和双架构构建不证明真实SystemPasteboard/Office provider/NAPI FD或ARM64设备资格。新运行的dev.17设备事实见 [独立后续记录](../v17/follow-up-validation.md)：自有控件系统复制的普通文字、TSV 表格转换及 UI 确认 pin 限定通过，导出选择器打开后未完成目的地/字节核验。不追改已发布 [dev.17验证](../v17/validation.md)，也不计为dev.18通过。

## 仍开放的边界

RTF正式插件与本地分支容量/捕获票据不同；损坏编码和RTF有意U+FFFD仍明确拒绝。输出20,000 UTF-16、字段grapheme和todos500vs1000、Flutter200MiB附件容量、完整IME/affinity/全篇连续选择均未齐。完整Office/HTML/RTF/图片/文件provider及格式矩阵、DOCX/XLSX/OLE包内容转换、前后台/拒权/空间耗尽/损坏/并发/进程中断/Unknown持久核对继续验收。

HUKS与未封存开发Store、256张卡限制、captured S1/S2/票据/父子交接、正式业务跨进程Unknown、完整插件/网络/音乐/字体语言/宽屏布局、签名及ARM64真机仍未完成。本轮不以来源容量或图片模型关闭完整目标。

## 分支交付

范围仅 `codex/ArkTsUI`。准确push/readback由主代理另存 `.build/delivery/dev18/branch-delivery.json`（ignored），此报告不预先声称推送PASS，不含自身提交SHA。无main合并/推送、发布tag或签名分发操作。
