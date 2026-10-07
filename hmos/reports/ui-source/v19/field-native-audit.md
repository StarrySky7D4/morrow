# dev.19 原生字段计数与转换边界审计

记录时间：2026-10-07。此 worker 的最终范围是 Rust、C++ NAPI、native DTS、实际主机测试和独立双 ABI 候选归档；未操作设备、Git、Index 或 HAP 构建，未修改冻结 `hmos/shared`。本轮原生 source/build-qualified；dev19 设备 **NOT_RUN**，完整 Flutter 功能等价仍 **OPEN**。最终整包／分支结果见 root 的独立交付记录。

## 真实参考与 Unicode 版本

实际 Flutter 参考工作树为 `build/io-safety-refactor`，HEAD `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420`。最终 fresh 核对 `lib`、`pubspec.yaml`、`pubspec.lock` 没有本轮修改。

`lib/main.dart:5596–5630` 的真实粘贴路径先在 UTF-16 selection 上替换，再对完整未来字串使用 `value.characters.length`：title 60、todos 1000、hypothesis 5000、conclusion 10000、description 20000。description 的图片 Markdown 追加也检查完整未来正文。`CardTipsEditor` 传给 `TipListEditor` 的 1000 上限为包含行间换行的总量；`tip_list_editor.dart:222–232` 给单行控件的余额扣除其他行 Characters 和行间换行，不能把它误解为每行都各有 1000。

参考 lock 固定 `characters 1.4.1`；实际安装的 package README／grapheme tables 使用 Unicode 16.0.0。当前缓存另一版本 `unicode-segmentation 1.13.3` 是 Unicode 17，不能用于此固定版本对照。因此新增依赖严格固定 `unicode-segmentation = "=1.12.0"`，实际 crate 的 `UNICODE_VERSION == (16,0,0)` 已由测试核对。该版本的[官方生成 tables 源码](https://raw.githubusercontent.com/unicode-rs/unicode-segmentation/v1.12.0/src/tables.rs)亦声明此版本；Cargo.lock、crate registry checksum 和实际使用的源文件哈希均记录在 `field-native-inputs.json`。没有自行实现分段或按 UTF-16/code point 代理计数。

真实 Flutter 测试通过后捕获 `field-characters-reference.json`：

- 1,093 个安装 Characters 的完整 `splitTests` corpus，实际执行 Flutter 分段结果。
- 105 个完整字段边界：5 字段 × 7 样例（ASCII、中文、emoji、family ZWJ、国旗、combining、Indic）× limit−1／limit／limit+1。
- 每项保持可再生完整 code points／repeat、完整 UTF-8 长度、来源 SHA256、Characters count、UTF-16 长度和接受结果。Rust 逐项重建并核对，1,198 个来源身份全部一致；没有只比较预期计数常量。
- 额外记录 5 种 malformed UTF-16。Dart 字串可保留它们且 Characters 仍能计数；HMOS transport 明确拒绝孤立 surrogate，不静默替换，故不能声称此无效编码范围等价。

## 异步接口与原字串边界

新增 `native.editorField(input:string):Promise<string>`；C 导出为 `morrow_hmos_editor_field(const char*):char*`。请求严格为 `{field,text}`，text 是完整未来原字串。字段白名单和上限：

| field | grapheme limit |
| --- | ---: |
| title | 60 |
| todos | 1000 |
| hypothesis | 5000 |
| conclusion | 10000 |
| description | 20000 |

回执始终包含 `ok,error,field,grapheme_count,utf16_length,utf8_length,limit,unicode_version`，固定 `unicode_version:"16.0.0"`。成功保留完整原字串的三种长度；超限使用 `EditorFieldGraphemeLimit`，保留完整计数供 UI 显示。编码／schema／字节错误的三个 count 为 −1；可识别的合法 requested field 和对应 limit 保留。未知／无法解析的 field 不授予资格。request JSON 与解码后 text 各有独立 UTF-8 512KiB 上限。

JSON 的 text 原 token 先以 `RawValue` 保留，再严格反序列化为 String，拒绝 escaped lone surrogate，避免 serde 或平台转换先替换原字串。C++ 对实际 NAPI request UTF-16 验证成对 surrogate 后才编码 UTF-8；原始 request 含孤立 surrogate 会在 worker 未启动前抛 `NATIVE_NOT_STARTED`。有效 emoji pair、intentional U+FFFD、空白、换行和 JSON escaped NUL 按真实内容计数，不 trim、normalize、truncate 或补完成 IME。

此 worker 是纯计算，不打开 Store、不取得 capture ticket、不修改 draft／selection／composition。既有 async Work 的 request/reply 所有权与 free 保持；本新增操作不接收 FD。selection、affinity 和 composing 仍使用 UTF-16 坐标，不能换成 grapheme offset。完整业务值、owner／epoch 的 await 后重检和 composing 保存规则由 ETS policy／root 集成负责，见 `editor-field-policy-source-audit.md`。

## 富内容／Markdown 输出改正

移除原生 Markdown 和 clipboard 输出旧 `20,000 UTF-16` 代理限制，使用同一固定 Unicode 16 计数函数限制 **20,000 完整 graphemes**；输出不截断。例：20,000 个 `e + combining acute` 为 40,000 UTF-16 units，本轮完整正文被 plain、HTML、RTF 转换和 Markdown 接受；多一个完整 grapheme 明确失败。

converter 成功新增 `paste_grapheme_count,paste_utf16_length,paste_utf8_length,unicode_version`，均由最终完整 `paste_text` 得出；错误保持空正文／图片／SHA，三个 count 为 0，version 固定。ETS agent 已用这些 metadata 验证完整输出，移除自身旧 UTF-16 误限。不能仅以单独插入片段授予字段保存资格；root 必须再次检查 prefix + inserted + suffix 的完整未来五字段。

保留／明确新增的独立预算：

| 边界 | 当前预算／行为 |
| --- | --- |
| field request JSON／原 text | 各 512KiB UTF-8，明确拒绝 |
| Markdown 输入／完整 converter 输出 | 各 512KiB UTF-8；同时完整 grapheme ≤20000 |
| 完整 serialized converter reply | 512KiB，含正文、warnings、图 metadata；不能靠正文长度代替 |
| Markdown reading projection | 既有累计 Strings256KiB、links64KiB、JSON512KiB；结构预算不变 |
| plain／HTML／XML 原件 | 2Mi UTF-16，UTF-8/BOM bytes envelope 6Mi+3 |
| 原始 RTF | 8Mi UTF-16，合法原件编码 bytes envelope 24Mi+3；按真实原件编码 units 核对 |
| HTML/XML/RTF 解析 | 原先 nodes1024、depth40、严格编码、无外部实体执行等保持 |
| 原件／图片与 FD | 总 spool／Store import64MiB、spool20 slots、image10、images64MiB、SHA及 owned FD 规则不变 |

RTF 8Mi 是参考 `readPaste` 的 **plugin == null 本地 fallback** 真实可达分支；正式 Studio plugin capture transport 仍统一 2Mi UTF-16，另有64KiB serialized request。没有 plugin 失败后自动回退到8Mi的调用链，不能把本轮本地转换资格称为 protected capture 等价。SDK Unicode RTF string 保持 dev18 已修正的确定性 UTF-8 BOM serialization，raw bytes 不改写；完整 serialized bytes 参与 SHA／预算。旧 native RTF 输出对 U+FFFD 的保守拒绝仍是明确编码差异，本轮未扩大为 escaped-surrogate parser 修复。

## 后台／草稿预算与 IME 差异

计数 `ok` 仅说明原生 transport 与该字段 grapheme 上限通过，不是后台持久化资格：冻结 core title 上限16KiB UTF-8，V2 body/properties aggregate64KiB，任务单项非空且≤2048 UTF-8 bytes，CardRecord／event／Store 等既有预算独立。1000 个中文 todo 可通过1000 graphemes但达到3000 bytes，仍会被单任务2048 bytes边界拒绝；一个很长 combining cluster 可通过计数但超过 title16KiB。没有修改 frozen backend 或吞掉这些错误。

raw draft journal 的单字段512KiB UTF-8、完整 protobuf4MiB和 UTF-16 selection/composing 结构校验继续独立，原始临时／composing 内容不能因字数 inspection 成功就成为已完成业务输入。原生不改变原字符串或选择。

真实本地 Flutter SDK `LengthLimitingTextInputFormatter` 按 Characters 截短，默认策略会因平台而 enforced 或 composition 结束后截短；其选择偏移仍为 UTF-16。本轮 HMOS 保留原字串、用异步计数拒绝超限粘贴／业务保存的规则，**不声称直接输入自动截短、系统 IME 或设备 UI 已等价**。交互和生命周期验收仍需实际设备，不能从主机通过推导。

## 最终验证及候选库

| 检查 | 最终结果／证据 |
| --- | --- |
| 完整 Windows Rust library suite | 106 PASS、0 FAIL、3 ignored；88.96s，`field-native-rust-tests.log` |
| 真实 Flutter capture | PASS；`field-characters-flutter-tests.log` |
| 最终三个 conditional Flutter comparisons | 3 PASS、0 FAIL；9.48s，`field-native-flutter-final-compare.log`，覆盖1198字段身份、既有5完整输出和6完整容量身份 |
| installed API26 CPP syntax | ARM64／x64 PASS；两个 `field-bridge-*-syntax.log` |
| locked/offline OHOS release staticlib | ARM64 14.56s PASS；x64 8.50s PASS；两个 `field-native-*-build.log` |
| ELF symbol | 两候选均定义 `morrow_hmos_editor_field`；`field-native-archive-symbols.log` |
| native input/hash inventory | 256 repository inputs、2新候选、2保留dev18、实际Flutter来源与固定Unicode dependency全 PASS；`field-native-input-check.log` |
| HAP／dev19设备 | 本 worker未运行 HAP；dev19设备NOT_RUN；由root独立记录 |

最初完整 suite 的两个旧边界测试仍把10,001个emoji当20k UTF-16 overflow，改为20,001后符合新 grapheme 语义并通过。初次失败日志保留为 `field-native-rust-initial.log`；没有删除这些边界检查。8个新 native 测试验证完整五字段边界、ZWJ/flags/combining/Indic、严格 malformed Unicode、FFI/free、独立字节预算、完整 metadata及超旧UTF16代理的转换。三个 ignored 对照依赖实际Flutter捕获，最终已显式运行，不把 fullsuite 的 ignored 计为通过。

| 独立 candidate archive | bytes | SHA256 |
| --- | ---: | --- |
| `hmos/.build/editor-field-native/dev19/arm64-v8a/libmorrow_hmos.a` | 55303046 | `97B6EF7CDD178F6E22179881894AE6A7D5A24E3E60394B51B091DAF1CBA6A937` |
| `hmos/.build/editor-field-native/dev19/x86_64/libmorrow_hmos.a` | 53712046 | `91305282E30C7F649DBDDE5F49041EA520EABA253852E42FA7F0C8EC95E7FAA1` |

候选 build script 只写 `.build/editor-field-native/dev19`，未拷贝到 `cpp/rust`，未覆盖已发布 dev18 证明；dev18 ARM64 hash `8EA7F0446E46BE185519E50357691065724FC6061C37E2B269A9631E97DB8D89`、x64 `E241203FF46BB7C7AAFDF38FDE60EF8119E694525629A7377A585659D9F7FE44` 已 fresh 核对一致。SDK LLVM15不能读Rust LLVM22 bitcode；ELF symbol 检查明确使用 `--no-llvm-bc`，仅证明实际机器对象的导出，完整最终link由root HAP验证。

`field-native-inputs.json` 覆盖所有当前 native Rust模块／测试、Cargo graph lock、schemas、shared来源、bridge/DTS/CMake和相关build/checkscripts的字节数及SHA256。`field-native-input-check.ps1` 还枚举 native source 以拒绝未列入清单的新源；独立共享package的standalone Cargo.lock不是此root Cargo graph输入。root采用库后可运行 `-ProductionArchives` 核对 `cpp/rust` 两份库与候选完全相同；新checkout没有ignored候选时可用 `-SourceOnly`，不会假称archive已核验。Flutter和本地cache额外复核通过显式 `-IncludeFlutterReferences -IncludeExternalDependencies` 完成。
