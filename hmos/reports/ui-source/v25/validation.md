# v25 分支源码与构建交付

2026-10-09。基于 `08fcea98cd1a99decee099ff91754ff0137ba971`，仅交付 `codex/ArkTsUI`，不合并或推送主线。应用版本保持 `0.1.0-hmos-dev.19` / `1000019`；v25 是验证记录编号。完整 Flutter/Windows 对齐仍 **OPEN**。

## 本次实现

Index 明确保存建立完整真实 raw publication，核同一 editor lease、input epoch 与完整输入后暂停父 writer，再接持久原请求 prepare/issue、原生注册 save/inspect literal 和实际业务协调器。原请求列表与分段恢复只读；Unknown 只允许明确核对或重试固定原请求，已知提交事实不因坏 DTO、刷新失败或 owner 重绑清除。旧 `submit(create/edit)` 入口拒绝，普通 TaskId 操作仍独立。

已确认自己的准确 S1 后，完整未变输入经真实视图撤销边界后关闭原草稿；较新完整 S2/S3 由独立 typed business handoff 接为 source0 子草稿。新 native 使用真实历史业务 Card 的完整 source/revision，固定 plan、first child、retirement、close 操作分别核对；先确认当前子 writer 全部最新 raw，再条件退休父草稿。incoming15/outgoing16 与旧 rawfork13/14 独立，普通 child save 保完整 lineage/pin。配额不放宽。

重启后 planned/closed 请求只读取 native 原 handoff/retirement/close literal，返回显式核对入口，不生成另一套操作。首次 prepare 明确未写入且无任何 Unknown、pending 或 known commit 时，只释放该新提案，让完整草稿可再次明确保存；未知 prepare 不能套用此释放。

## 实际检查

完整实际 ETS/tool 最终 **857/857 PASS，0 FAIL /0 skipped /0 cancelled，16,931.4367ms**；34 个 suite 文件、116 个模型/页面/tool/fixture 输入前后字节一致，见 [结果](models-result.json)、[完整日志](models-final-tests.log)、[前输入](models-inputs-before.json)和 [后输入](models-inputs-after.json)。日志 SHA256 `85E9105580F08D3A141FE28BA35BDDBF39BA956B4675707EDA0467C5012C0772`。

首次全量运行 **855 tests /807 PASS /48 FAIL /0 skipped** 保留为 [stage1 原日志](stage1-models-final-tests.log)：旧 clipboard/rawfork harness 缺少新增真实 guard，旧 field/todos 检查还调用已拒绝的 legacy 保存入口。迁移保持原 field28/Todo16 检查的输入/IME/Unknown/晚到输入边界，另加 100 行正界和 continued_todos 100/101 同 own root 正反，不跳过失败用例。迁移确实发现新保存路径遗漏原始 LF 总行数上限；生产补回 create/continued_todos 的 >100 行拒绝，完整 raw 先确认保留，不创建 intent 或业务保存。新业务23、field28、todos18 的组合69和模型97均包含于最后全量857，不重复累计。

| 检查 | 结果与准确范围 |
| --- | --- |
| 实际 Rust 库与附件二进制 | **177 library +3 binary PASS**，13 条条件测试默认 ignored；[原日志](rust-tests.log)。全命令随后 rustdoc 遇到 `E0463`，原 exit1 保留；顺序重编默认图后 [文档检查](rust-doc-final-tests.log) exit0 /0 doctests，未改源码 |
| 新实际 Store 故障边界 | **48 个子进程崩溃边界 PASS**，13.81s；独立事务的固定 plan、child、retirement、exact/handoff close 重核，见 [最终日志](editor-handoff-crash-final-tests.log) |
| 实际 Store DTO 导出 | **PASS /0.57s**，重现 91,081B /`9AF7CCAA…` 完整双 case fixture；[最终日志](editor-handoff-dto-final-tests.log)、[原 DTO](editor-handoff-store-fixture.json) |
| Session/Handoff/Draft/Fork 模型 | SDK 限定修复后 **97/97 PASS /0 skipped**，11 输入前后一致；[修复后结果](editor-business-handoff-sdk-repair-result.json)。该子集包含于最后全量结果，不重复累计 |
| 双 ABI 原生构建 | locked/offline release **PASS**，完整 274 个 native 输入构建前后及采用时一致；[输入与 archives](native-build-inputs.json)、[ARM64](native-arm64-build.log)、[x64](native-x64-build.log) |
| 最终完整产品 API26 | **SUCCESS /15.363s，34 tasks 全部执行**；314 复制输入、368 编译来源输入前后字节一致；[日志](hap-build.log)、[复制清单](source-copy-manifest.json)、[仓库输入](build-inputs.json)。tool 检查输入由模型清单单独记录，不充作 SDK 编译输入 |
| 包内 native | 两架构 `libmorrow.so` /`libc++_shared.so` 四项大小和 SHA256 与构建输出一致；[记录](native-package-check.json) |

第一完整 SDK 构建在 **22.515s /26 executed tasks** 因 Handoff 两处 nominal/type/标准库限制和 Index owner-hook nominal typing 失败；见 [原日志](stage1-hap-build.log)、[原输入](stage1-build-inputs.json)、[原复制清单](stage1-source-copy-manifest.json)。修复为明确 typed 字段/owner hooks 赋值，合同和原 wire 不变；[限定修复说明](editor-business-handoff-sdk-repair-audit.md)。`retry1` 曾 SUCCESS /16.452s，随后补回原始行数门禁，所以其 `4E00DBBB…` 包属于历史中间结果，见 [stage2 保留映射](stage2-retained-artifact.json)和 [日志](stage2-hap-build.log)。最终以全新 `retry2` 隔离副本重建，旧副本、失败和成功中间包均保留。

新 archives：

| ABI | 字节 | SHA256 |
| --- | ---: | --- |
| arm64-v8a | 57,038,980 | `5C0C78E43A1660E803ACE768EFC19890029096A75EE51FE64D7E221A4105F90E` |
| x86_64 | 55,454,248 | `2559EBF39EB706B29611523998EECDF9E1700326208969C1E347EA0808D7EA2D` |

完整最终 HAP 为 ignored `.build/artifacts/dev25-business-handoff-checkpoint/entry-default-unsigned.hap`，**30,173,833B**，SHA256 **`9590D87DE2F83CE736F1BE0ED2D7CD8FA7C3A127373F53AB9F7A2A2C2288B686`**。**未签名、未安装，设备验收 NOT_RUN**。见 [artifact](artifact.json)与 [当前 build-manifest](../../build-manifest.json)。旧 B62 UI-only 设备恢复/keep 证据不能替代新 native 与新保存流程运行。

## 范围与剩余工作

模型受控 reply、field worker、lease detach 证明实际方法与队列路径，不证明 Store、IME 或 SDK 回调排空；Store 故障检查不证明 OHOS runtime 或不同进程全局配额原子性。SDK/包 hash 不证明 rendered UI、签名、设备实际保存或重启。native plan/child/parent/close 是各自 CAS 的独立事务，多对象不会被宣称为一个原子事务。

父草稿缺失/已退休或 child 当前代次已推进的页面重启接续仍 **OPEN**：模型支持完整 current child 校验后的 `openCurrentChild`，Index 未接该路径。关闭后卡片重开并继续 owned todos、favorite/category/TaskId 变化后的准确新 revision 全文编辑也未完成。issued 明确业务拒绝不能被当作 prepared 取消；原请求归属保留，替换/放弃合同仍需后续实现。

新 native 的真实多行输入、附件 lineage、保存/接续/关闭/重启设备闭环、ARM64 真机、HUKS/protected storage/audit、全篇连续选择与 SDK 未交付事件保全、完整富内容/手势/拒权/空间耗尽、插件/网络/备份及完整 Flutter/Windows 产品验收仍 OPEN。详见 [native 审计](editor-handoff-native-audit.md)、[模型审计](editor-business-handoff-model-audit.md)与 [上游和 Index 审查](upstream-and-integration-review.md)。
