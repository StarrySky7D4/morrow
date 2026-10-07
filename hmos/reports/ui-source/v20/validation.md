# dev20 源码检查点验证

2026-10-07。本次只同步 `codex/ArkTsUI`，不合入main。完整Flutter/Windows目标仍 **OPEN**。新输入formatter、多行待办组件和新卡待办原子创建已具备源代码及限定验证，**Index尚未接入新组件/格式器**，不能称UI对齐完成。

## 产品包与来源

AppScope仍为 **0.1.0-hmos-dev.19 /1000019**。最终检查点包为 **28,225,993 bytes**，SHA256 **36151E624FD61297436E979F9AA54D0682CDA8DF9B9F02F162AA0EF4CC07C3C5**，归档 `hmos/.build/artifacts/dev20-source-checkpoint-final/entry-default-unsigned.hap`；未签名debug、**未安装**。这与发布并安装过的dev19归档 **F7A91398… /28,164,478 bytes** 是不同包，不能仅凭相同版本号互相替代。

Root采用冻结的dev20-final双ABI库，生产复制hash一致；产品完整API26构建先 **11.574s SUCCESS**，回执修复后 **6.746s SUCCESS**，独立SDK查出组件命名冲突并修复、最终源冻结后再构建 **8.145s SUCCESS**。四个native ZIP条目完整解压，双ABI libmorrow.so与实际SDK stripped packaging文件逐字节hash/长度一致，均不同于已发布dev19；两份libc++未变。见 [最终构建](hap-checkpoint-sdk-reviewed-build.log)、[库采用](input-native-production-check.log)、[native包核对](native-package-comparison.json)。前一4DF6EE2F…候选保留于独立 `dev20-source-checkpoint`归档，[原输入清单](checkpoint-initial-build-manifest.json)和[原native核对](native-package-initial-comparison.json)不替代最终36151E62…交付。

[全源清单](checkpoint-build-manifest.json)包含 **313项**原输入和新源的完整SHA，global `reports/build-manifest.json`指向本记录。包含未在Index采用的组件/策略及Rust测试源，清单的字节一致性不表示每个文件已在产品入口运行。全源disk/staged核对另记 [disk](build-manifest-disk.log)、[staged](build-manifest-staged.log)。旧dev19归档及v19当轮验证保持原状。

## 实际验证范围

| 范围 | Fresh结果 | 限制与证据 |
| --- | --- | --- |
| 完整实际ETS/工具模型 | **617/617 PASS，0 fail/skip** | 命名修复后fresh全部tool/*.test.cjs；平台/worker为明确模拟；[log](arkts-checkpoint-final-model-tests.log) |
| 新输入回执策略 | **15/15 PASS**，计入上行 | 完整哈希/old-new echo、正/零额度、UTF16/UTF8、filter-finalize、owner/stop/迟到结果与独立预算；[log](editor-input-policy-model-tests.log)。初测fixture错误使用TextValue的原默认affinity/selection，记录保留于[initial](editor-input-policy-initial-tests.log)，改为明确fixture默认后通过 |
| 新待办模型与真实组件方法 | **55/55 PASS**，计入完整617 | 含零额度retained完整metadata回归；声明式UI、真实IME与拖拽滚动尚未设备验收；[audit](todos-source-audit.md)、[log](editor-todos-model-tests.log) |
| 完整Rust library suite | **123 PASS，0 FAIL，6 default ignored** | ignored本身不是通过；[log](input-native-final-rust-tests.log) |
| 真实Flutter/Dart条件比较 | **5 PASS**；285完整编辑值一致、12明确wire预算差异，1198字段身份、50待办整理身份、5转换输出、6容量身份 | 保留formatter与controller/IME不同资格；[log](input-native-final-all-flutter-compare.log)、[audit](editor-input-native-audit.md) |
| 新卡待办事务与Store退出 | focused **9 PASS**；第6个默认ignored以独立fault-injection测试明确运行PASS，7个真实子进程退出边界 | 同create事务none/all、原receipt核对、重启/后续mutations及不同raw同投影拒绝；release库不带fault-injection；[audit](create-todos-audit.md)、[crash](create-todos-store-crash-tests.log) |
| 双ABI最终原生来源 | **PASS**：261 repository native inputs、2 final库、保留dev19与formatter-only候选、实际Flutter/依赖/fixture | 不复用旧formatter-only库冒充待办业务；[manifest](input-native-inputs.json)、[check](input-native-input-check.log) |
| dormant ETS 独立真实SDK编译 | **API26 SUCCESS 10.754s**；307复制源+5生成harness完整hash核对PASS | 显式入口实例化真实组件/Policy/Hash/NAPI契约；独立bundle dev.morrow.hmos.checkpointsdk，26,464,308 bytes /B28AF8BF…；首轮9.740s失败与框架命名冲突修复均保留。未运行；[audit](sdk-smoke-audit.md)、[log](sdk-smoke-final-build.log) |
| 独立图片多指工具 | **22模型 PASS**；main/test HAP API26构建PASS | 未安装/注入，不构成图片像素手势通过；[audit](image-device-audit.md) |

最后交付复核另对261项native源执行disk/采用库/外部对照核验和Git staged字节核验，见 [最终disk核对](input-native-root-final-check.log)、[staged核对](input-native-staged-check.log)。暂存空白检查时曾改动三份已纳入hash的报告脚本EOF，已恢复准确原hash、允许原EOF保留并重新全量核对PASS；中间drift日志保留，不改写构建来源。

## 已发布dev19的后续设备记录

设备 `127.0.0.1:5555` 使用冻结的 **F7A91398… dev19**。成功安装日志、保存/fresh bundle读回1000019绑定对应归档；bm不提供设备安装字节hash。PowerShell解析bundle因metadata/metaData大小写冲突失败发生在安装已成功之后；随后以Node原文解析只读核对版本，**没有重复安装**。

升级前在dev17对已有C fixture取消旧导出窗口并保留草稿；安装dev19后实际观察草稿2，恢复同一 `HMOS-clipboard-20261007-C` raw行、原Markdown表格及同一个20B TSV pin，标题原生计数25/60。本轮仅观察/恢复，未提交新卡业务保存，也未产生新的导出字节闭环。原阶段journal及UI dump/PNG分别保存在 [升级记录](device-migration/progress-clipboard.json)与 [dev19观察](device19-current/progress-clipboard.json)。

Root本次重新查看实际[首页PNG](device19-current/dev20-launch-installed19-stable.png)及[草稿PNG](device19-current/dev20-read-own-c-draft19-restored.png)：可见草稿数量、字符计数、表格与TSV附件，所观测首页未见贯穿未知框线。该限定图像观察不证明所有页面渲染，更不证明新检查点UI、图片双指或IME通过。capture标记REQUIRED是工具要求人工复核，人工复核范围由本段记录；没有修改原capture日志。

## 仍待完成

Index完整newline/owner/epoch/IME接入及新卡发送冻结todos、业务等待与原raw保留分开、行间选区/拖拽/焦点真实控件、Windows直接输入与ArkUI composition差异继续推进。新检查点设备测试 **NOT_RUN**；HUKS/正式捕获与存储、签名、ARM64真机及完整Flutter/Windows资格仍 **OPEN**。原TaskId详情命令保留，已有V2 edit明确拒绝非空legacy todos，不能把新卡development adapter称为Flutter V1/V2兼容完成。

本报告不预报push成功；root另行核对提交范围及远端专用分支SHA/main状态，不创建release tag或合并主线。
