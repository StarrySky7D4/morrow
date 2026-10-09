# v33 待办明确决定与外观偏好保护验证

2026-10-09，分支 `codex/ArkTsUI`，实现基线 `ada95e0f119b3d3c3273b8e5bf6da031d3b10c10`。本轮为开发检查点，完整 Flutter/Windows 功能对齐仍 **OPEN**；交付只限专用分支，不并入 `main`。推送完成必须以实际远端分支读回确认，本文中的构建结果不是远端发布回执。

[最新发布前独立复核](release-review-publish.md)结论为 **PASS_SCOPED_BRANCH_ONLY_PUBLICATION**，无本轮限定交付阻断项，`releaseEligible=false`。新171输入、九语fixture/隔离copy/live模式及未变的源/副本/SDK/HAP/ABC/四原生库核对见[结构化记录](release-review-publish.json)。[初次审查](release-review.md)与[原结构化记录](release-review.json)保留历史；两份审查均不替代下面明确未取得的设备与完整产品资格。

## 实际产品行为

详情和现有卡编辑器对 completion=2 的旧版待确认任务显示 `help_outline`，并提供“确认已完成 / 确认未完成”两个方向及对应菜单；正常0/1继续使用实际 Checkbox bool 回调。决定绑定原卡/source/revision、唯一实际 TaskId、原文本/状态、页面或编辑器归属、前台与输入 epoch。未完整捕获的原输入、脏编辑、退役/恢复、文件/粘贴及已有业务 Unknown 均阻止新请求。真实 raw flush 后再次核对 owner 与快照，原请求一旦派发就保持原 operation/CAS/flag/wire。

task_toggle 的每次响应必须仍属于实际发送 owner；迟到 owner 变化不能装入 cards 或清除原 pending。成功需要完整 current DTO、严格 committed 和正规范 u64 receipt，不能通过 JS Number 丢失 revision 精度。receipt 等于 current revision 时核实际原TaskId及决定；历史 receipt 小于 current revision 时接纳 Native 最新视图，允许后续任务变化或移除；未来 receipt、坏 DTO 或含糊效果保留原 Unknown。显式重试只发送原 literal，并建立本次接收边界；不会生成另一个决定。

Index 已通过实际 ArkData Preferences 接入 `AppearancePreferences`，原 namespace/key 为 `studio-appearance` / `appearance-v1`。只有成功 hasSync 明确不存在才允许默认基线；空串、非 string、坏记录或读失败进入禁止写入状态。完整候选、已确认 current、原 raw、观测值与已派发 Unknown literal 分开；保留未知 JSON 值的原字节片段，包括大整数与嵌套值，已知字段格式可以规范化，不能称整个 JSON 全文逐字不变。

外观保存要求完整校验、原 raw preflight、put、flush 和 exact readback 全部确认。旧 ACK 不能覆盖更新的滑块/字体/颜色/语言预览。相同 namespace 在同进程跨页面实例共享真实 live-call 屏障：dispose 或后台不伪造已发 Promise 的取消，新页等待旧 read/put/flush/readback 实际终止，随后 Unknown 必须显式“重新读取并核对”。恢复不 put、不默认替换、不自动重放；最新预览保留，另行“保存当前外观”才可以确认。

三条任务设计文案来自真实 Flutter ARB，现有九语 UiStrings 原字节未改。字体导入本轮只有[实际源/SDK设计审计](font-import-source-design.md)，31项来源包含可复用的 Windows Rust `FontPreference` 和 API26 全局 checked font loader 的官方本地示例；字体描述符/真实选择/私有原件/SFNT/注册预算/恢复尚未接入产品。

## 最终实际来源与主机执行

| 最终产品源 | 字节 | SHA256 |
| --- | ---: | --- |
| Index.ets | 356113 | `57A9E5AA56C647C9635BF23483A69DA359867A60C1606F89F1A0348E1765C4A8` |
| AppearancePreferences.ets | 22653 | `849ABCE141EFB3656DB058A13BB8240E5BFF04D001FE8D51558EEBDE6DDF41E5` |
| UiStrings.ets，未改 | 775172 | `17801DD05B1AFD9684F452B0E5B736FFBA7C5520FB5413A8B6D002216CB1A576` |

[发布前完整模型](models-publish-result.json)：**1291/1291 PASS，0 fail/skip/cancel**，53测试文件、171项实际输入前后一致，48700.7493ms；[实际日志](models-publish-tests.log) SHA256 `23A0667310BCF38D43056079D8A0A2EB57F20B009D1C582FDC21BD1481685479`。最新冻结来源见[执行前](models-publish-inputs-before.json)和[执行后](models-publish-inputs-after.json)，已纳入新任务九语固定fixture与既有 `flutter-markdown-uri.json` golden。执行实际 ETS/tool/Index/组件方法及 Store 产生的 DTO；provider、AVPlayer、存储服务、生命周期送达与 rasterizer 仍是受控边界，不是设备播放/持久性/像素或完整 parity 验收。

[初次完整模型](models-final-result.json)1291项/169输入、51731.0527ms及[原日志](models-final-tests.log) `AE9AA11882ABE2731DF73FAA7527EEB764F9ABB9C87A5A48A56D9597D175B7E0` 保留为历史。发布前发现新九语检查默认依赖本地ignored Flutter build目录，随后修复工具并补齐fixture输入库存；旧169项冻结不代表这些较新工具/fixture资格。产品Index、AppearancePreferences、UiStrings、SDK来源与HAP均未变，本次重新执行更新工具后的模型，SDK沿用同一冻结来源与HAP的已有实际构建结果。

以下专项属于限定来源证明，不能与全量数字重复相加：

- [Task源执行](task-pending-decisions-validation.md)：18项新Task测试与108项既有business/history/recovery/field/todo/raw-fork测试合计126/126 PASS；52输入、51实际repository reads前后精确。该专项的Task冻结Index为 `619592C0…86664`；随后外观接入改变整个Index哈希，最终全量另验新字节。测试执行真实按钮/Checkbox/menu回调和Workbench队列，Native回执、生命周期和确认框送达为明确受控seam。
- [外观模型](appearance-preferences-a2-result.json)：61/61 PASS，7输入/4参考 exact。此前56项通过时，独立审查另发现跨页旧flush覆盖新值的P1；[旧失败复现](appearance-preferences-independent-review.md)保留，不改写为通过。修复后的[独立审查](appearance-preferences-independent-fixed-review.md)在最终模型执行61项及额外6个跨实例live-call交接向量，全部限定PASS。
- [实际Index接入](index-appearance-validation.md)：32/32 PASS，10输入/5参考 exact；执行真实字段、methods、lifecycle及状态/按钮回调。只逆转声明的外观patch就逐字重现Task冻结基线，证明Task/business/music/paint等保留；ignored本机基线只用于该专项资格，不是产品发布输入。完整全量不依赖此基线。
- [任务九语检查可复现性](task-decision-i18n-portable-validation.md)：27条期望直接取自18份实际Flutter parts/assembled ARB，fixture保留提取前后来源身份，未以UiStrings反向生成。默认只读仓库固定fixture与实际产品；[无build隔离复制a2](task-decision-i18n-pinned-clean-a2-result.json)12/12 PASS，7仓库输入前后精确、3实际Node进程/13次文件读取/外部0，执行后仍无build目录。另[显式live a1](task-decision-i18n-pinned-live-a1-result.json)12/12 PASS，25输入含18份真实ARB，源缺失/漂移直接失败，不能静默改用fixture。两组与最新全量不重复累计；只有这12项取得无build隔离复制资格，未运行Git clone，也未证明全53测试文件在干净clone运行。[早期九语检查](task-decision-i18n-validation.md)及旧a1/a2保留为历史；实际窄屏长度、glyph和读屏仍未验。

## 完整 API26 与 Native 复用

[完整产品构建](dev33-preferences-tasks-a1-sdk-result.json)：**PASS /SUCCESS /36.467s**，34/34 tasks执行、0 up-to-date；[实际日志](dev33-preferences-tasks-a1-sdk-build.log) SHA256 `497BB606D17FCEA562CAFB44AC9C47ADA91934D3F16F80085A15F818ACF07792`。使用最终完整产品入口，实际 Task/AppearancePreferences 严格编译，保留平台能力和异常处理警告，不声称所有设备支持。

[325项复制身份](dev33-preferences-tasks-a1-source-copy.json)、[368项仓库来源](dev33-preferences-tasks-a1-repository-inputs.json)与[283项Native复用](dev33-preferences-tasks-a1-native-reuse.json)分别记录本次构建来源，独立逐字核对通过。Rust/C++来源与v29完全一致，ARM64静态库58,110,214B / `DD86DF956795BC04CDF98BC3184625C6FAD9D0382E041E86DEAF99C44028744A`，x64静态库56,515,718B / `E9C66A4974D32CD9B6C68559504F56EBA7D10CC27F1DB3CCFDC31B4FDDA4BF1E`。本轮没有新Rust构建/测试资格。

[包内检查](dev33-preferences-tasks-a1-package-check.json) **PASS**：十一项指定实际产品模块均列入完整编译入口、生成ts/proto并存在于实际包内ABC记录，分别为八音乐模块、RecessedGlassRelief、AppearancePreferences和Index。四项双ABI `.so` 与本次compiler stripped outputs逐字一致；不声称与旧HAP共享库逐字相同。实际 `ets/modules.abc` 为2,990,540B / `F6E62E4FF945A648EC20AAE4D06B8681588E1F46939DB42A29926E16C313D66B`；模块emit和包内记录不是设备运行资格。

[不可变归档身份](dev33-preferences-tasks-a1-artifact.json)：HAP **31,564,598B**，SHA256 `39F2E8A11E9C03A5AECF475539AE864BCA0609D97F48ABFFF4F65DD266545F88`，版本 **0.1.0-hmos-dev.23 /1000023**，归档 `.build/artifacts/dev33-preferences-tasks-a1/entry-default-unsigned.hap`。包unsigned、**未安装**；实际设备仍是下面单独记录的旧dev22，不得替代新Task/Preferences运行验收，也不是签名发布。

## 本轮设备观察属于 v32 /dev22

[实际安装回执](device/installation-dev22.json)核对已提交v32完整来源、324项验证副本和不可变包 **31,442,946B /06342B5CFB5EDA93493BEDA13B76B6DB9D0AF32C4F95394C280B1B6448E1C4FB**，在既有Pura X View2模拟器/HDC `127.0.0.1:5555` 更新后读回 **dev22/1000022** 并启动。它提供主机HAP身份、安装回执和bundle版本，不是受保护的设备包哈希。首页仍显示14卡、4草稿；本轮没有业务保存、草稿退役或删除动作，不能从显示计数扩大为整个资料库逐字读回资格。

新拟态前后[旧dev21首页](device/dev21-neumorphism-home-b.png)与[新dev22首页](device/dev22-neumorphism-home-b.png)原图对照确认：Hero和首卡可见部分此前额外的内侧raised轮廓消失，搜索仍有设计要求的上左凹陷效果。新拟态设置前后没有同样明显差异，不能据此扩大修复结论。首页/设置七风格PNG、UI树、实际动作及独立图片审查按其访问页面和包身份单独记录，不能认定所有界面未知框线或Flutter完整像素矩阵通过。旧v32报告“未安装dev22”是原当轮事实，本轮新增安装证据不回改它。

本轮实际依次观察新拟态、纸感、黏土、Fluent、粗野主义、工业风和扁平的首页/设置，最终恢复原扁平风格，首页仍14卡/4草稿。具体原图审查见[独立图片报告](device/visual-review.md)及[结构化来源](device/visual-review.json)。粗野主义选中后一处驱动旧断言因设置滚动后标签不在可见树而停止；没有再次派发select，关闭重开后核对真实选择。该驱动失败、后续观察和工具修复证据都保留，不据此判定产品失败，也不扩大为全部页面/所有风格组合通过。

[最终返回首页观察](device/final-observation-a2.json)核对原扁平标签、14卡/4草稿、安装前后实际bundle版本及36项动作回执；显示计数不能替代完整业务资料与草稿全文逐字验证。

此前旧dev21两次DocumentPicker进入Download后关闭而无选中文件的记录保留在[v32验证](../v32/validation.md)；本轮画面观察没有产生新的音乐导入/播放资格，也不能据此确定provider缺陷。公开自生成WAV/LRC转移和hash仅是准备，不是授权URI/解码/声音证明。

## 尚未闭合的资格

**v33/dev23新Task与Preferences设备验收NOT_RUN**。真实待确认TaskId的两向按钮/菜单、普通勾选、窄屏/glyph、disabled/Unknown状态、原请求前后台和重启，真实ArkData写入顺序/flush/readback/重启/断电均未验。Task当前pending为同进程原wire保留，未实现任务持久intent journal；外观共享屏障仅同进程/同namespace，未提供跨进程CAS或崩溃journal。

全页面/风格/材质/浅深色/宽度/覆盖的像素矩阵、第二外侧光阴影和父容器裁剪、高对比度/减少动画/光学折射仍 **OPEN**。真实音乐picker/grant、OHOS完整持久化、codec/声音/seek/EOF/后台/重启/部分清理，在线歌词、解密、metadata/封面、字体/背景原件产品、GC/protected、HUKS/正式宿主、ARM64运行、签名和完整Flutter/Windows对齐均 **OPEN**。主机测试、API26构建或旧dev22安装不关闭这些资格。
