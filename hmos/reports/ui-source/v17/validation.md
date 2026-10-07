# dev.17 富内容粘贴分支交付

2026-10-07。本轮按用户要求交付 `codex/ArkTsUI` 的源码和构建，不新增设备流程。完整 Windows/Flutter 对齐目标 **OPEN**；系统富内容剪贴板与 Office 提供者的实际设备验收 **NOT_RUN**。

## 新实现及确认范围

成功 PasteButton 授权后读取 SystemPasteboard 的 getData/getValidTypes/changeCount，最多 20 条记录。真实系统记录产生一次性来源，获取 plain/HTML/RTF/Spreadsheet XML、原始二进制图/文件、PixelMap 和合法系统文件 URI；纯文字 URI 不授予文件读取能力，原 URI 不入 picker grant、不持久化或跨读取复用。来源绑定 owner 与快照，迟到读取/编码失效并释放资源；真正系统授权仍可能打不开文件，显示明确失败。

原件 bytes 保存到现有 spool，RTF 原始编码不经 UTF-8 改写；PixelMap 使用受剩余预算限制的 PNG packToData。原件、内嵌图片、sidecar 共用全局 64 MiB / 20 槽。Rust 在已授权 FD 上核验完整原件 SHA，复用 capture 转换 HTML/RTF/Spreadsheet XML/TSV；内嵌图片单独提取，二进制不经 JSON。先完整检查文字、字段和槽位，再顺序沿用原 durable import 状态机；确认 pin 后才将临时引用映射为持久 asset ID，并插入同一字段/选区/IME/owner 快照。未发请求的 spool 可清理，已发 Unknown 保留准确原请求供核对，不自动重放。

正文优先富格式；其他字段有 plain 时优先 plain，否则取富格式文字投影。HTML 失败可用 plain fallback 并保留原件/警告；plain 的 TSV 转换有差异才保存 TSV 原件。DOCX/XLSX/OLE 提供的原始二进制可保存，不等于已转换其包内容或完整 Office 捕获实现。

## 本轮证据

| 检查 | 结果 | 证据 |
| --- | --- | --- |
| ClipboardInput 实际 ETS 模型 | 36/36 PASS，SDK/provider 模拟 | [log](clipboard-input-model-tests.log) |
| ClipboardFiles 实际 ETS 模型 | 46/46 PASS，FS/native 模拟 | [log](clipboard-files-model-tests.log) |
| 既有 AttachmentFiles 模型 | 97/97 PASS | [log](attachment-files-model-tests.log) |
| 完整实际 ETS 模型 | 402/402 PASS | [log](arkts-final-model-tests.log) |
| Rust Windows library | 91 PASS，0 fail；默认未跑的条件对照另列 | [log](clipboard-rust-tests.log) |
| 实际 Flutter 对照 | 1 Flutter test 捕获五组完整输出；单独 Rust 比较 1 PASS，五组逐项相同 | [Flutter](clipboard-flutter-tests.log)、[比较](clipboard-flutter-rust-compare.log) |
| OHOS ARM64/x64 release staticlib、双架构 C++ SDK syntax | PASS | [ARM64](clipboard-arm64-build.log)、[x64](clipboard-x64-build.log)、[C++](clipboard-cpp-syntax.log) |
| API 26 最终 HAP | PASS，9.467 s，未签名 debug | [最终构建](hap-release-build.log) |
| 版本 / HAP 字节数 / SHA-256 / 归档 | 0.1.0-hmos-dev.17 / 1000017；27,815,034 bytes；`D9DECC46BB0E953BB56A4CDC5DBBB71D0B863E6380C273CC677B73C36689C03F` | `.build/artifacts/dev17/entry-default-unsigned.hap` |
| 原生构建输入/最终 HAP 原生条目 | 252 原生输入与双静态库逐项核对；HAP 四个条目完整解压字节比较，双 ABI libmorrow 新编译而改变，两份 libc++ 与 dev.16 相同 | [原生来源审计](clipboard-native-audit.md)、[HAP 完整字节比较](native-package-comparison.json) |
| 全 HAP 输入 disk / staged 核对 | 301 inputs，disk 与 staged 均 PASS，匹配最终 D9DECC46… 包 | [工作文件](build-manifest-disk.log)、[暂存索引](build-manifest-staged.log) |
| dev.17 安装、系统富剪贴板/Office、原件/图片保存重启/预览导出移除 | NOT_RUN | 本轮不新增设备流程 |

首次 SDK 构建保留 [失败记录](hap-first-build.log)：catch 任意类型重抛和闭包结果被推断 never。最小修复为明确 Error 转交和持有异步结果的类型对象；三组相关模型 fresh 36+46+97 通过。最终构建以表中 fresh 结果为准，不能将首轮失败删除或用模型替代构建。

Rust 解析与来源边界详见 [原生审计](clipboard-native-audit.md)和 [精确输入](clipboard-native-inputs.json)。五个 Flutter 对照是本地转换样例，不是 Office 提供者、NAPI FD 设备运行或系统剪贴板矩阵。dev.16 A97122… 包后续 MP4 准备/播放暂停/横屏/seek/Back 限定 PASS 另记 [后续媒体记录](../v16/follow-up-validation.md)；Home 后仍在应用内，后台 NOT_QUALIFIED，旧包观察不计 dev.17 验收。

## 保持开放的差距

原生转换上限 **2 MiB 原始 bytes**，Flutter 限额按 2 Mi UTF-16 单元；大于原生限额的富格式原件可按附件额度保存，但转换会明确失败/警告，不能称容量等价。20 条系统记录可能展开多个原件和图片，仍需剩余 20 槽及含 sidecar 的 64 MiB 全局预算，且草稿 pin 预算不放宽；Flutter 普通附件 200 MiB 未对齐。字段 UTF-16 与 Flutter grapheme 的差距、待办 500 对 1000、IME/affinity、全篇连续选择仍在。

完整 HTML/RTF/Office/图片/文件提供者互操作、2 MiB 边界、失败/权限变化/后台/进程终止、Unknown 持久核对、pin 保存重启/导出移除须在真实系统验收。HUKS、未封存开发 Store、captured S1/S2/捕获票据和父子交接、正式业务跨进程 Unknown、256 张卡限制、插件/网络/音乐/完整语言字体、签名及 ARM64 真机仍开放。

## 分支交付边界

范围仅 `codex/ArkTsUI`。准确 push/readback 将另存 `.build/delivery/dev17/branch-delivery.json`（ignored，由主代理完成）；此报告不预先声称推送成功，不包含自身提交 SHA。无 main 合并/推送、发布 tag 或签名分发操作。
