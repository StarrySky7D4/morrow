# v22 恢复门禁修复与严格业务基础检查点

2026-10-07。此次按用户要求同步专用分支 `codex/ArkTsUI`，不合入 main。应用身份继续 **`0.1.0-hmos-dev.19 /1000019`**，v22 是证据目录，不是升版。完整 Flutter/HMOS/Windows 对齐目标仍 **OPEN**。

本轮有三个独立证据范围：f9基础加两项UI修复的已安装候选；新的Rust/ETS严格业务源码与主机/隔离SDK验证；根代理随后构建的最终产品/native检查点。不能把第一个候选的设备结果算给包含新业务后端的最终包。v21及更早报告保持各轮交付时事实，本页更新当前状态，不追改历史。

## 实现范围

Index 的 `draftRestoreInput` 原为普通private变量，恢复时设true、稍后清false不会触发ArkUI更新，真实设备诊断确认 Todo owner/revision/current正确但 `editingEnabled=false`。本轮将其改为 `@State`，使恢复后的输入门禁更新。EditorViewLease 的 `mountedOwner` 同样改为 `@State`，owner尚未挂载时不构建content；现有冻结view owner、callback围栏、rawfork及手动放弃/保留/exact成功关闭合同保持。单独lease State改动的候选仍不能Add，之后定位到恢复标志；不能将第一候选写成已解决。

新development `editor_save` / `editor_commit_inspect` 与 `continued_todos` 已实现：完整原 Submission 字符串一次冻结，publication准确历史Write/proto SHA与全部raw字段/无composition/所选pins绑定，准确Core command及历史Card/receipt重建，当前Card只作live诊断。50003紧凑标记绑定完整原wire/publication摘要，保留50001及未知字段。owned全文todos按真实Dart splitLF/trim/去空/首次去重规则，一次Core事务提交完整字段/任务/附件；相同label保留准确旧TaskId/completion/未知Task，移除ID退休，新label确定性新ID，重命名采用legacy删除+新增规则。最终Properties与完整转义请求/回复预算不放宽。

独立 `EditorBusiness.ets` 负责prepare/restore、save/inspect各原wire明确重试、历史资格和完整source/pub/wire摘要、恒定root+latest6字段基线。owner/inputepoch分别核，历史事实不因view替换失效，消费旧输入仍须完整sameValues及原epoch。原弱历史只可`legacy_semantic_only`，不是全部旧edit兼容，也不能提升强资格。此模块**尚未被Index使用**；新native路由虽已有源码，产品当前保存路径没有调用它。

**业务成功后的source0子草稿、祖先/pin权限交接、条件父退休与Index全流程尚未实现。** 现kind1 rawfork保原source冲突不构成可继续保存S2的业务基线；development marker/既有rawfork不等于protected S1/S2权限或系统输入队列排空。完整原业务wire的产品跨进程持久恢复也仍OPEN。

## 已安装UI候选与实际设备观察

候选只包含冻结 `f9c395a7cc7b1d3b0707b52f241565b758114614` 的产品UI加上述两项状态修复，复制旧b486 native；并行新后端源明确排除。其 [artifact](lease/final-artifact.json) 与 [完整源拷贝清单](lease/final-source-copy-manifest.json) 单独记录311输入；API26构建 [SUCCESS /16.921s](lease/final-product-build.log)。

| 候选身份/观察 | 实际结论 | 证据 |
| --- | --- | --- |
| unsigned HAP | **28,799,011 bytes / SHA256 `96C0AB68BCB513251C56AC9E61543594762693BEFE280E31E24552C50679CA28`** | `.build/artifacts/dev22-lease-final-checkpoint/entry-default-unsigned.hap`，`lease/final-artifact.json` |
| API26/x86_64安装、启动与版本 | **PASS**，dev19/1000019；不是ARM64真机/正式签名 | [安装记录](device-final/installation.json)、`device-final/bundle-after-install.json` |
| 公开D草稿恢复 | **有限PASS**，标题`HMOS-todos-20261007-D`、原公开正文恢复 | [截图](device-final/final-restore-D-v22-restored.png)、对应json/raw/capture及progress |
| Add第一条空行 | **有限PASS**，1/100、一条可交互空行、0/1000显示 | [前](device-final/final-add-first-D-v22-before.png) / [后](device-final/final-add-first-D-v22-after.png)、`device-final/progress-clipboard.json`阶段PASS |
| 首次输入`first 汉字 🧪 é.` | **FAILED_OR_UNKNOWN**，准确visible-input断言得到undefined；没有输入/业务保存/重启闭环通过结论 | [原capture](device-final/final-input-first-D-v22-input-17.capture.json)、[PNG](device-final/final-input-first-D-v22-input-17.png)，progress保留原错误 |
| 后续只读当前状态 | 编辑器已关闭，显示“已确认的完整输入已保留，可从草稿入口继续。”；不能据toast证明上述新文字准确写入 | [当前状态](device-final/final-inspect-D-v22-current.png)、对应json及progress |
| 后续独立恢复D | **有限PASS**，原输入没有重放，实际待办行恢复`first 汉字 🧪 é.`；业务提交0 | [新恢复截图](device-final/final-recover-after-input-D-v22-seek-13.png)、`final-recover-after-input-D-v22`阶段；独立于原失败输入阶段 |
| 再点击保留关闭 | **FAILED_OR_UNKNOWN**，标题控件仍在，编辑器未关闭；提示“当前事件尚未完整捕获，原事件已保留；草稿清理已停止。”，草稿仍保留 | [原after截图](device-final/final-preserve-before-final-D-v22-after.png)、`final-preserve-before-final-D-v22`原失败，不能写完整关闭PASS |

input-first中实际Back退出编辑器，后续read-only观察/提示和独立准确文字恢复分别记录；恢复证明此次已持久保留该原文，却不能覆盖原驱动FAILED_OR_UNKNOWN或宣称整套IME/元数据/业务保存闭环通过。随后保留关闭因未完整capture而停止，当前保持open editor和草稿，未知事件不被丢弃；这也是未闭合的实际产品流程。旧eeca包150003焦点崩溃、5B62阶段恢复后Add禁用及lease-only候选/诊断证据均保留。真实 [诊断](lease/actual-owner-diagnostic.log) 记录 `enabled=false/current=true`，临时console不进入最终UI源码。

上述候选**不含本轮新editor_business native/ETS基础**；本轮最终产品包的安装/设备验收为 **NOT_RUN**，不会借相同dev19版本号或上述设备结果提升资格。

## 新基础的fresh有界验证

| 范围 | 结果 | 原始证据与边界 |
| --- | --- | --- |
| 完整默认Rust | **158 PASS /0FAIL /9 default ignored**，155 library+3附件binary；81.45s+5.51s | [完整日志](editor-business-full-rust-tests.log)；ignored未混入PASS，不能将旧条件对照计为本轮默认运行 |
| 新业务专项 | **14PASS /0FAIL /2 default ignored**，2.12s；包含在完整Rust内，不累加总数 | [专项日志](editor-business-rust-tests.log) |
| 实际Store进程失去结果 | **1PASS /14真实故障向量**，3.37s；create/continued各七个事务边界 | [故障日志](editor-business-store-crash-tests.log)；host fault-injection subprocess，不是设备crash |
| 实际Store DTO导出 | **1PASS**，fresh create/continued原wire、准确native DraftRecord和完整reply | [日志](editor-business-store-fixture-tests.log)、[fixture](editor-business-store-fixture.json) |
| 实际Dart/Characters | **50完整normalization identities**，capture exit0；Rust独立fresh条件比较 **1PASS** | [capture](create-todos-dart-capture.log)、[reference](create-todos-reference.json)、[compare](create-todos-dart-compare-tests.log)；不是已有V2 TaskId UI等价 |
| 新实际ETS协调器 | **23/23PASS /0skip /3157.9661ms**，含真实Store DTO解析/摘要 | [最终日志](editor-business-model-final-tests.log)；受控receiver与平台provider，不是native transport/设备 |
| 原stage1 ETS | **22/22PASS /0skip /3143.2051ms** | [保留日志](editor-business-model-stage1-tests.log)，原测试身份不覆盖 |
| 恢复/lease相关实际ETS | lease **16/16PASS**；Index相关67/67PASS | `lease/lease-model-tests.log`、`lease/index-final-model-tests.log`；不替代SDK真实声明式时序与设备输入 |
| 独立新业务API26 SDK | **SUCCESS /11.654s**，实际import/prepare/restore/public API/type refs | [构建](business-sdk/sdk-build.log)、[manifest](business-sdk/business-sdk-source-copy-manifest.json)；311copies/5wrappers、7model/typeDeps前后hashPASS、0live-source差异 |

完整默认Rust的九项条件测试各有自己的入口。此轮单独fresh的新业务crash、DTOexport和Dart50比较上表分别记录；其它历史Flutter/原draft条件证据不冒充此轮重新执行。上述新23项及旧模型均包含在下面根代理fresh全部742项里，不相加制造总数。

独立SDK只生成隔离bundle `dev.morrow.hmos.editorbusinesssdk` unsigned HAP，**25,774,084 bytes /SHA `8A09E79FEE2A57BFFA1A0BF505DCE54FB68504D5831B0532B87B5B59BACAE621`**，runtime/device/installation **NOT_RUN**。它的现CPP/native拷贝仅供编译，不能作为新native采用资格。新脚本/entry/manifest与产品build-inputs隔离；两个wrapper的准确before/after bytes/hash已登记，其余三个生成项原样保留。

详细接口/错误边界见 [native审计](editor-business-native-audit.md)、[ETS审计](editor-business-model-audit.md)、[独立review](editor-business-review.md)。各自明确Windows Store、controlled ETS、SDK与设备资格，未声称protected授权或完整Flutter验收。

## 本轮最终产品/原生交付

| 项目 | 当前状态 |
| --- | --- |
| 根代理fresh ARM64 release | **PASS /5.28s /56,104,946 bytes**；SHA `F3815F306E96618F8389B17E59679CDD1E1E477BC1324BE34943B579DBF2DAA4`，`native-arm64-build.log` |
| 根代理fresh x86_64 release | **PASS /4.90s /54,511,556 bytes**；SHA `5ADCB7BAD62417868DD6A2D03699B9D69C3267DE1072B8534E0926FE30DE6B41`，`native-x64-build.log` |
| 完整原生输入与两库采用 | **267 repository inputs /2 archives /2 production archives PASS**；`native-build-inputs.json`、[采用核对](native-adoption-check.json)，新Rust来源包含在清单 |
| 原生包内四项字节核对 | **4/4PASS**，两ABI的libmorrow.so与libc++_shared.so匹配SDK stripped实际打包文件；[包内核对](native-package-check.json)，不是直接拿未strip文件比较 |
| 最终完整ETS全部模型 | **742/742PASS /0fail /0skip /7365.4536ms**；[fresh日志](models-final-tests.log)，SHA `78C0D200443C115431DDFE4E81FD98C6770FA995FB3B9AF19D0F779F884857B7` |
| 最终产品API26 HAP | **SUCCESS /9.653s**，CompileArkTS6.231s；33tasks，20execute/13reuse；[构建日志](hap-build.log) |
| 最终产品不可变artifact | `.build/artifacts/dev22-business-checkpoint/entry-default-unsigned.hap`；**29,101,587 bytes /SHA256 `41688FDAF9BA7F33D2F6AD14CAB52AB85BCA6D517CADE905C42603F78927D4A6`** |
| 最终整包source manifest before/after/disk | **334 inputs PASS**，build前后完整源字节一致，disk核对匹配上述HAP；[输入清单](build-inputs.json)、[disk日志](build-manifest-disk.log) |
| 最终staged输入核对 | **334 inputs PASS**，暂存字节与构建冻结源码及最终HAP一致；[实际核对日志](build-manifest-staged.log) |
| 最终产品签名/安装/设备/ARM64真机 | **NOT_RUN** |

根代理提供的最终结果已逐项读取对应日志/manifest核对，最终暂存输入核对已实际通过；候选/独立SDK数字没有用于填产品身份。文档不填自身commitSHA，推送仍由根代理执行，只有确认专用远端分支后才报告完成；不合入main。

最终产品source身份：Index `34DDECA5A375328ADB1A7EC53DE7F8E5D1A90804CD8634FAA85672D7A588160C`；EditorViewLease `96624FE1F3ADF1528BF188F0C7FFFBE838454833499E3EDA1BAB48E428B4CFF3`；EditorBusiness `6469D1F2AF4E2DA6B9066B52917E658BC13ECECB4EF2A4E6B92FBB4AD0D996CF`。source、scope与版本身份独立登记，新后端未接Index不因已打入产品native而消失。

## 仍开放

准确多行文字输入、选择/IME/formatter、全文行间连续选择、任务高度/拖拽/保存重启与关闭竞态设备矩阵；strict业务Index接线、准确历史source0子草稿/pin祖先权限及S2/S3实际再次业务保存；产品在途原wire跨进程Unknown恢复、protected capture/生产宿主/HUKS/签名/ARM64真机；附件200MiB与更紧独立业务预算差距、完整富剪贴板/Office/媒体/图片手势/插件网络/设置等既有未闭合项。保持全目标 **OPEN**，不以版本号、本地计数或Add空行代替完成。
