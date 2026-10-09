# v33 发布工具修正后的独立审查补充

2026-10-09，结论：**PASS_SCOPED_BRANCH_ONLY_PUBLICATION**。当前分支交付以本补充及 [release-review-publish.json](release-review-publish.json) 为准，`branchOnlyPublishable=true`、阻断项为空；仅限 `codex/ArkTsUI`，不授权合并 `main`。完整产品验收 **OPEN**，`releaseEligible=false`。

原 [release-review.json](release-review.json) 和 [说明](release-review.md) 保持原字节，分别为 `D4F1C0A5…29CDA`、`933F3D74…04362`。旧169输入执行与其审查保留为历史，不能用来证明修正后的发布工具。产品源码、SDK构建和不可变HAP未改变，原产品与包审查事实仍有效。

## 发布前发现与修正

原默认任务九语测试直接读取未跟踪的本机 `build/win-cloud-20261005`，新源码checkout缺少该目录时会在收集测试时失败。新固定fixture直接提取真实Flutter九语的三个设计key，共27条文案；来源是18份parts/assembled ARB，不是从UiStrings反向生成期望。

本审查再次直接读取18份实际ARB，逐项核对其字节、SHA256、parts/assembled以及fixture文案，全部一致；提取前后来源清单相同。fixture为10,493字节，SHA256 `462B23ED8E32CE23D88F06F602AF560172D8AC84FE19BA7922D4684163DB7F61`。

默认任务检查与有界生成器检查只读取仓库fixture和实际产品源，并继续执行真实UiStrings查找。读取当前Flutter worktree是另一个显式live模式，严格核对18份来源及27条文案；缺失或漂移会失败，不会静默切回固定样本。完整历史全集生成器不是此次可移植性范围，也未运行。

独立审查还发现既有 `markdown-model.test.cjs` 会读取 `tool/fixtures/flutter-markdown-uri.json`。最终runner将两份实际fixture JSON均纳入来源冻结，**实际输入数为171**，没有为了原预估170而漏掉读入数据。

## 当前执行证据

[models-publish-result.json](models-publish-result.json)：**1291/1291 PASS**，53测试文件，fail/cancelled/skipped/todo均0，48,700.7493ms。171项输入执行前后完全相同，且本审查与当前实际文件逐项核对全部一致。实际日志141,078字节，SHA256 `23A0667310BCF38D43056079D8A0A2EB57F20B009D1C582FDC21BD1481685479`；独立读取的尾部计数与报告一致。新旧两个1291结果不相加，专项12项也不另加到完整总数。

[隔离默认检查](task-decision-i18n-pinned-clean-a2-result.json)：12/12 PASS，7份仓库输入。独立核对源清单、真实隔离副本、guard及日志hash；该树执行前后没有build目录。guard经NODE_OPTIONS继承到子生成器，真实3个Node进程的审计文件共13次读取，均在隔离树内，外部读取0。日志SHA256 `21E5732222323941351965BF5AD66EF43A211CC1E46F091235092DE175B1DD9B`，实际Node v24.19.0执行文件hash也吻合。这是12项的隔离复制树证明，**不是实际Git克隆或全部53文件的隔离运行证明**。

[显式当前源检查](task-decision-i18n-pinned-live-a1-result.json)：12/12 PASS，25项输入，即7份仓库源与18份实际ARB，前后及当前字节全部一致；显式live模式核对27项实际翻译查找。日志SHA256 `C89C465B506F11873661D3352977EB10407D42DDF307A62C8A64CEC162C1C9C5`。默认与live资格分别记录，没有把当前18份外部参照当作默认测试依赖。

## 产品与包资格保持原范围

再次核对 **368仓库构建输入、325复制输入、283 Native输入**及当前build-manifest，均精确匹配；两个v29静态库、production和构建副本仍一致。旧SDK日志仍为实际34任务全执行、36.467秒SUCCESS，本次工具修正没有新增SDK或Rust构建资格。

不可变dev23 HAP仍为 **31,564,598字节**，SHA256 `39F2E8A11E9C03A5AECF475539AE864BCA0609D97F48ABFFF4F65DD266545F88`。其字节与原独立读取Zip/11项准确ABC记录的包完全相同；另再次核对22个当前emit输出和4个当前strip原生库输出，均匹配原包证明。包仍unsigned、未安装。

新Task/Preferences真实设备、ArkData持久性/重启/断电、任务持久intent、音乐真实picker/grant/声音、字体导入产品、全界面框线/像素矩阵、ARM64/签名/protected及完整Flutter/Windows对齐均保持 **OPEN/NOT_RUN**。旧dev22画面不授予新dev23功能资格。最终暂存、提交、远端分支和main未变化证明由Root核对；本审查不是推送回执。
