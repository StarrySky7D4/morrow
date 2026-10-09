# v26 分支基础更新与验证

2026-10-09。基于 `19582cc1fc66dca96cb979753f35d579112081ef`，仅更新 `codex/ArkTsUI`，不并入主线。应用版本保持 **0.1.0-hmos-dev.19 /1000019**，v26 是记录编号。完整 Flutter/Windows 对齐仍 **OPEN**。

## 本次源码

严格 `current_v2` 以真实当前全文 source、完整 raw publication 和原 Core CAS 编辑标题/正文/假设/结论及附件；LF todos 与 continuation 均为空。保留当前真实 TaskId、完成状态、顺序、退役身份及 nested/outer/migration 未知字段，不投影旧 own LF 基线；分类/阶段必须匹配当前 source 并保留其原始 bytes，收藏独立。元数据或 TaskId 修改后的新 source 可用于新正文请求，签发后 source 再变仍冲突，不能自动重基。

`draft_read_history` 只接受准确 card/draft/operation/generation 与空其余字段，读不可变原记录并带真实当前 generation/active flags，核完整响应预算，不创建、恢复活跃或授予写权限。Handoff 模型允许无 parent writer 时读注册 S2 历史，核真实当前完整子记录后打开实际当前 writer，再继续固定退役/关闭步骤；确认原退役历史只解除同一退役 Unknown，独立 close Unknown 保留。

真实原生 draft ACK 省略可选 `intent_next_after`；模型 summary 仅允许 omitted 或显式空串，拒 null/非空，read/discovery 仍严格要求空串。该修正不更换固定 plan、proof、effect 或原 wire。

## 最终检查

| 检查 | 结果和范围 |
| --- | --- |
| 全量实际 ETS/tool | **869/869 PASS，0 fail/skip/cancel**，24,178.5627ms；34 suite 文件，120 项仓库模型/页面/tool/fixture 输入前后字节一致；[结果](models-result.json)、[日志](models-final-tests.log)、[仓库输入](models-repository-inputs.json) |
| 独立模型子集 | 5 suite **132/132 PASS**，17 inputs（16 模型/fixture + Index）前后一致且 matches owner freeze；[审计](source-parity-and-recovery-audit.md)、[日志](independent-model-review-final.log)。包含于全量，不重复累计 |
| Rust 默认检查 | **187 library PASS /0 fail /16 ignored /89.09s**，附件 **3 PASS /5.65s**，self-check0/doc0 exit0；[完整日志](rust-tests-final.log)。所有7改动文件前后一致 |
| 新实际故障边界 | **7 Core 子进程崩溃向量 PASS**（1 条精确 fault-injection 测试，1.78s）；固定原 wire 重试恰一提交且保任务；[日志](current-v2-crash-final.log)。旧48 handoff事务未改，本轮未复跑 |
| 两份真实 Store DTO | current_v2 **38806B /50F9037B…**，明确改变标题及正文、保留任务；history **75959B /4191624D…**，原 S1/不同 S2 父/推进 S3 子/退役和closed原literal。各精确导出1 PASS；[原生审计](editor-current-reopen-native-audit.md)、[结构结果](editor-current-reopen-validation.json) |
| 双 ABI 原生 | locked/offline release **PASS**，277 项完整 native 来源构建前后及采用时一致；[清单](native-build-inputs.json)、[ARM64](native-arm64-build.log)、[x64](native-x64-build.log)。最终 retry1 对未变 production 使用 Cargo 增量结果，归档与首次编译相同 |
| 最终完整产品 SDK | **SUCCESS /27.403s**，34/34 tasks 全部执行；314 复制输入、373 仓库输入构建前后字节一致；[日志](hap-build.log)、[复制清单](source-copy-manifest.json)、[构建输入](build-inputs.json) |
| 包内四项原生库 | SHA256/字节与最终构建输出一致，**PASS**；[记录](native-package-check.json) |

全量模型 runner 的宽清单还观察了 1600 项 ignored tool dependencies/历史 tester 输出，前后一致但不充作编译或实际测试读取输入，且不提交这些文件；[划分说明](models-input-partition.json)。最终完整日志 SHA256 `52511893632B9DC98F4DB71F41DE6EDCDBE50C9CE41303BC5F7AEA625815782C`。

最终未签名 HAP：ignored `.build/artifacts/dev26-recovery-foundation-retry2/entry-default-unsigned.hap`，**30,198,963 bytes**，SHA256 **`90C3171DE4110A4574F8B178A70133F8CA024AC4C21DBBD66D5BF6F1624AFD4D`**；**未安装，设备验收 NOT_RUN**。见 [artifact](artifact.json) 与 [当前构建记录](../../build-manifest.json)。

## 保留的中间结果

原 native 编译已成功，随后新 current_v2 测试改为明确不同的真实标题/正文，旧 DTO/测试日志与输入按 before-body-change/stage1 保存，最终以新冻结清单和 fresh retry1 归档核对。[保留映射](stage1-retained-native.json)。宽 filter DTO 导出一次夹带旧 export 缺少其变量而整体失败，保留 [失败日志](current-v2-dto-final.log)，最终两项均用 --exact 成功。

模型早期调用漏 hooks 与首次 close DTO 字段顺序假设的失败保留在 stage1/initial-diagnostics；最终 current-child 的真实 DTO 只核固定退休，closed DTO 核原已注册 close 的逐字节恢复。首次新 close 和其 Unknown 重试用既有实际源码/controlled transport 检查，不能借另一排序的收据宣称原 wire 成功。

首个完整 SDK **SUCCESS /14.356s**，但不含随后发现的可选 cursor 解析修正；其 **30,198,887B /C950B3C4…** 包、输入、日志均保留为 sdk-stage1，见 [映射](sdk-stage1-retained.json)。最终为全新 retry2 完整产品副本，采用修后 Handoff `FC2878C8…`。不覆盖旧结果、不用中间包证明新源码。

## 尚未完成

**Index 保持 v25 源码**（`1AB29F6A…`），v25 原业务保存/接续已接；本轮新增无父/退役父/推进子草稿页面重启恢复、关闭后最新 source 的 current_v2 重开尚未接入，body_kind 未实现。模型只读恢复不是页面恢复完成，也不证明旧 own LF 全文编辑或完整任务文本语义对齐。

新原生/UI 的设备输入/保存/接续/关闭/进程重启闭环、真实渲染问题复核、ARM64真机、签名、HUKS/protected storage/audit、完整 SDK 未交付输入保全、连续选择、富内容/权限/空间耗尽、插件/网络/备份及完整 Flutter/Windows 产品验收仍 OPEN。host/Store/SDK 构建和旧B62 UI候选不能代替这些资格。
