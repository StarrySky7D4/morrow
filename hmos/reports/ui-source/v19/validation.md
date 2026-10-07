# dev.19 Unicode16 字段计数与完整粘贴交付

2026-10-07。交付范围为 `codex/ArkTsUI` 的源码、模型和构建，完整 Windows/Flutter 对齐目标仍 **OPEN**。最终 **0.1.0-hmos-dev.19 /1000019** 未签名 debug HAP，**28,164,478 bytes**，SHA-256 `F7A913980CF3213536820727A677EEC04EA169FE2B65CE16E305F352D14BE4EC`，归档 `.build/artifacts/dev19/entry-default-unsigned.hap`。完整构建输入 **305项 disk/staged 均PASS**并匹配该包。**dev.19/dev.18 均未安装，当前设备仍 dev.17；本轮新包设备验收 NOT_RUN**。

## 本轮行为与来源

实际 Flutter `main.dart` 的粘贴先按 UTF-16 selection 替换，再检查**完整未来字串**的 `Characters` 长度。`CardTipsEditor → TipListEditor(maxTextLength:1000)` 的待办上限包含所有行和换行，不是每行各1000。三份所查参考工作树源码 Git blobs 一致且相关路径 clean；准确 HEAD、blob和实际 SDK formatter 规则见 [字段源码审计](editor-field-policy-source-audit.md)。

本轮原生固定 `unicode-segmentation = "=1.12.0"` 的 **Unicode 16.0.0**，对齐真实安装的 Flutter `characters 1.4.1`。缓存的1.13.3使用Unicode17，未用于此固定对照。新增纯计算 `native.editorField` N-API worker，不打开Store、不写草稿、不取得capture ticket；完整原字串不trim、normalize、replace或truncate。孤立surrogate在编码前明确拒绝，合法emoji pair、有意U+FFFD、空白/换行保留；Dart可以保存无效UTF-16，此严格编码边界仍不同。

| 字段 | 完整 grapheme 上限 |
| --- | ---: |
| title | 60 |
| todos（完整传入字串；当前UI仅待添加单条输入） | 1000 |
| hypothesis | 5000 |
| conclusion | 10000 |
| description | 20000 |

`EditorFieldPolicy` 检查回执的字段、固定limit、Unicode版本、完整UTF-16/UTF-8长度与count；超限返回完整计数供UI显示，保存准入明确失败。当前HMOS `taskText/editorValues.todos` 是待添加单条的pending input，已有任务另在 `current().tasks`；虽然policy完整检查传入字串及其换行，**尚未复现Flutter全部已有行+待添加行聚合1000、newline草稿映射和行间选区模型**，不能将单条500→1000调整称为全部待办容量等价。编辑控件移除UTF-16 `maxLength`，增加异步计数；超限完整输入保留。raw journal与IME按原结构/字节边界记录，不新增业务字数截断；业务保存与粘贴单独拒绝活跃composing、超限及无效编码。selection/base/extent/affinity/composing仍是UTF-16坐标，不能用grapheme offset替代。

真实Flutter默认 `LengthLimitingTextInputFormatter` 会按Characters截短，平台决定立即执行或composition结束后执行。本轮HMOS保留超限原字串，并阻止粘贴/业务保存，**直接输入控件行为尚不等价**；这不能描述为完整Flutter输入/IME复现。原始草稿恢复普通文字也不重建系统IME会话。

## 完整未来粘贴与已确认 pin

粘贴先冻结原输入、选区、editor/card/draft身份与input epoch，替换选区后异步检查prefix + inserted + suffix的完整文字；图片placeholder预检使用固定长度有效资产ID，在任何confirm/import前完成。每次await后重读owner、generation、selected/import slots与原选区，输入或选择绕回同值也不能让旧epoch继续。

原件仍按独立operation逐项durable import并确认pin；最后使用实际资产ID重建完整正文并再次检查后才提交文字。晚结果、目标变化或最后检查失败保留已确认pin，停止后续项，正文不伪装插入成功。Unknown保留准确原请求，不自动重放；未发spool与已经发出且未确认的来源分别处理。真实Index方法与实际ETS协调器有主机模型覆盖，未据此宣称真实系统剪贴板或设备IME通过。

业务检查还冻结完整raw values、命令、编辑器/card/task身份及input epoch；最后raw flush期间的输入/选区/类别/owner/生命周期变化阻止旧请求发出。独立review复现待办重命名成功回执按trimmed base text关闭编辑器、隐藏迟到IME候选的问题；已改为仅在原owner、epoch、完整 `samePasteTarget` 与无composition仍一致时消费该次输入。普通重命名仍正常关闭，迟到候选/同字串往返保留编辑器；Unknown保留原序列化请求与冻结snapshot，明确 `not_committed` 只清理提交状态。最终实际Index SHA-256 `9B897AC560A05A91D05007C4706E9B96576FC24329AE037B3E7C8CB8A5B6C85A`，范围及复现/修复证据见 [Index集成审计](index-editor-field-integration-audit.md)。

## 转换输出与独立预算

Rust Markdown/clipboard及ETS回执验证移除旧 **20,000 UTF-16** 代理限制，使用同一Unicode16的 **20,000完整graphemes**。20,000个`e + combining acute`可形成40,000 UTF-16 units，完整转换接受；多一个完整grapheme明确失败。转换成功回执新增 `paste_grapheme_count/paste_utf16_length/paste_utf8_length/unicode_version`，绑定最终完整 `paste_text`；缺失metadata、错误版本/长度或无效Unicode拒绝，不走旧回执fallback。错误回执保持空正文/图片/SHA和三个count=0。

| 独立边界 | 本轮状态 |
| --- | --- |
| field worker request JSON/原text | 各512KiB UTF-8，不因一个grapheme很长而放宽 |
| Markdown输入/完整converter输出/serialized converter reply | 各512KiB；同时适用完整grapheme≤20000，JSON还包含warnings和图片metadata |
| Markdown阅读投影 | 累计Strings256KiB、links64KiB、JSON512KiB及既有结构预算不变 |
| 冻结共享业务 | title16KiB、V2 body/properties aggregate64KiB；单任务非空且≤2048 UTF-8 bytes，CardRecord/event/Store预算独立 |
| raw draft journal | 单字段512KiB、完整protobuf4MiB和UTF-16 selection/composing结构预算保持 |
| 来源plain/HTML/XML与本地RTF | 沿用dev.18的2Mi/8Mi UTF-16，byte envelope为6Mi+3/24Mi+3，严格原文编码、完整SHA和FD不改 |
| spool/Store import与草稿pin | 总64MiB/20spool槽及sidecar等既有配额保持，不等价于Flutter普通附件200MiB |

因此1000个中文todo可以通过grapheme规则，却仍超过共享单任务2048 bytes；字段计数 `ok` 不能被当作业务持久化成功。正式Flutter Studio plugin capture仍统一2Mi UTF-16/64KiB serialized request，RTF8Mi仅对齐 `plugin==null` 本地分支，不存在异常后自动fallback。RTF Unicode string继续使用明确的UTF8+BOM serialization，raw bytes不重写；严格损坏编码/NUL/RTF有意U+FFFD等dev.18差异保持。详情见 [原生字段与转换审计](field-native-audit.md)。

本节**取代dev.18交付时字段及转换输出按UTF-16代理计数的差距**；待添加单条输入由500提高到1000，完整Flutter多行todos聚合/映射仍开放。历史 [dev.18验证](../v18/validation.md)保留当轮事实，不将未齐的待办行模型、控件输入、共享字节预算或设备资格改写为完成。

## Fresh 验证

| 范围 | 当前结果 | 证据 |
| --- | --- | --- |
| 完整实际ETS模型 | **535/535 PASS、0 fail/skip**；fresh执行全部 `tool/*.test.cjs`，包含下列子集和独立工具模型 | [log](arkts-final-model-tests.log) |
| 实际字段相关生产ETS与Index方法 | **277/277 PASS**；EditorFieldPolicy16、EditorPaste12、ClipboardPaste14、ClipboardFiles70、AttachmentFiles97、ClipboardInput55、Index集成13；native/provider/FS为明确模拟 | [policy](editor-field-policy-model-tests.log)、[paste](editor-paste-model-tests.log)、[协调器](clipboard-paste-model-tests.log)、[files](clipboard-files-model-tests.log)、[既有文件](attachment-files-model-tests.log)、[input](clipboard-input-model-tests.log)、[Index](index-clipboard-integration-tests.log) |
| 最终实际Index字段/业务/重命名独立review | **33/33 PASS**；20字段/业务集成+13剪贴板集成，后13与上行重叠；不额外累加到完整535 | [log](index-editor-field-integration-tests.log)、[audit](index-editor-field-integration-audit.md) |
| 完整Windows Rust library suite | **106 PASS、0 FAIL、3条件比较默认ignored**，88.96s；ignored不算通过 | [log](field-native-rust-tests.log) |
| 真实Flutter Characters捕获 | **1 PASS**，1,093完整split corpus +105完整五字段边界，共**1,198**来源身份 | [Flutter log](field-characters-flutter-tests.log)、[完整身份](field-characters-reference.json)、[meta](field-characters-reference.json.meta.json) |
| 三项条件比较在final Rust显式重跑 | **3 PASS、0 FAIL**，9.48s；1198字段身份、既有五个完整Flutter输出及六个完整容量身份 | [log](field-native-flutter-final-compare.log) |
| API26 C++ syntax与locked/offline双ABI staticlib | **ARM64/x64 PASS**；14.56s/8.50s，均定义新增C导出 | [ARM64 syntax](field-bridge-arm64-syntax.log)、[x64 syntax](field-bridge-x64-syntax.log)、[ARM64 build](field-native-arm64-build.log)、[x64 build](field-native-x64-build.log)、[symbols](field-native-archive-symbols.log) |
| 原生输入/candidate/保留dev.18/实际链接库/Flutter及依赖来源 | **PASS**；256repository inputs、2候选、2保留dev.18与2production archives，全范围来源/固定Unicode依赖hash核对 | [manifest](field-native-inputs.json)、[final check](field-native-input-final-check.log)、[production check](field-native-input-production-check.log) |
| 最后review前API26完整HAP | **SUCCESS /14.963s**；此候选不代替最后review后完整包 | [log](hap-release-build.log) |
| 最后review后最终API26完整HAP | **SUCCESS /11.302s**；28,164,478 bytes，完整SHA如上，含最后重命名IME修复 | [log](hap-final-build.log) |
| 最终HAP四个native条目 | **PASS**：四条全部完整解压核验SHA/长度；双ABI `libmorrow.so`准确匹配SDK stripped packagingfiles，均不同于dev.18；两份libc++与dev.18字节相同 | [comparison](native-package-comparison.json)、[check](native-package-check.log) |
| 完整HAP input disk/staged | **305 inputs，disk/staged 均PASS**，准确匹配最终F7A91398…包；与256 native输入清单分别记录 | [disk](build-manifest-disk.log)、[staged](build-manifest-staged.log) |
| 独立API26真实多指tester | **main HAP SUCCESS /6.581s，test HAP SUCCESS /9.073s，10/10工具模型PASS**；无产品源码/testhook变更 | [main build](image-gesture-tester-main-build.log)、[test build](image-gesture-tester-ohosTest-build.log)、[tool tests](image-gesture-tester-model-tests.log) |
| dev.19字段输入/IME/系统粘贴/业务保存/图片手势与tester安装运行 | **NOT_RUN**；dev.19/dev.18均未安装，当前设备仍dev.17 | 不用旧包运行或主机结果替代 |

首轮native suite的两项旧边界仍把10,001个emoji当20k UTF-16 overflow；调整为20,001个完整grapheme后通过，失败记录保留 [initial Rust log](field-native-rust-initial.log)。新增C++桥首次仍链接旧dev.18静态库，两个ABI缺少 `morrow_hmos_editor_field`，HAP **FAILED /3.546s**；切换已验证dev.19候选库后14.963s构建成功，最后review及重命名修复后再完整构建11.302s通过。原失败保留于 [before-native-integration](hap-before-native-integration.log)，不删去，也不当作最终包仍失败。

模型运行实际生产ETS或逐字提取的实际Index方法，但平台和worker对象是模拟；Unicode算法由独立真实Flutter/Rust对照证明。双ABI编译和实际SDK语法核对不证明运行中的NAPI、SystemPasteboard、FD、IME或ARM64硬件资格。

## 独立图片多指工具

新bundle **`dev.morrow.hmos.gesturetester`** 的真实API是 `Driver.injectMultiPointerAction(PointerMatrix,speed)`，不是不存在的 `performMultiPointerAction`。测试工具只按root提供的新鲜fixture身份/版本/画布像素边界观察或执行单个probe，不启动/切换产品、创建附件、清理应用数据或设置全局环境。两个无签名HAP构建与10个工具边界模型通过，**未安装、未进行实际双指注入**；跨窗口provider、sandbox截图获取、图片位移/焦点/裁剪/边界反向均待root实际验证。

注入ACK本身不证明渲染正确；工具要求人工核对真实PNG，UI读不到的preview token/pin与archive hash不能冒充读回。工具详尽范围、包hash、逐步执行和Unknown不重放规则见 [双指tester审计](image-multipointer-tester-audit.md)。未来若在冻结dev.18包上验证，必须标为dev.18运行证据，不能因工具报告存于v19就计为dev.19验收。

## 仍开放

Flutter多行todos的newline草稿映射、行间选区和全部已有行+待添加行聚合1000仍未齐。直接输入formatter截短行为、系统IME/composition/affinity与全篇连续选择仍有差异。真实HTML/RTF/Office/图片/文件provider、完整格式/DOCX/XLSX/OLE转换、captured S1/S2与捕获票据、前后台/拒权/空间耗尽/损坏/并发/进程中断/Unknown持久核对和原件pin保存重启导出移除继续验收。图片基础手势保留dev.18源码修复，惯性/fling/scale-velocity未实现，GIF/损坏格式未验。

HUKS、未封存开发Store、256张卡限制、正式业务跨进程Unknown、父子交接、完整插件/网络/音乐/字体语言/宽屏布局、签名及ARM64真机仍未完成。dev.17限定系统普通文字与TSV/确认pin证据仍单列于 [后续记录](../v17/follow-up-validation.md)，不计为dev.19新包资格。完整Windows/Flutter目标 **OPEN**。

## 分支交付

范围仅 **`codex/ArkTsUI`**。准确push/readback由root另存ignored交付记录；本文不预先声称推送PASS，不包含自身提交SHA。无main合并/推送、发布tag或签名分发操作。
