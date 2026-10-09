# v25 Handoff：实际 SDK 诊断后的限定修复

2026-10-09。Root 首次完整 API26 产品 CompileArkTS 未通过；本代理仅修复其 Handoff model 明确诊断，不改变 wire、协调器合同、页面或 native，不执行 SDK / 设备 / Git。

原 `hap-build.log` SHA256 `5425241824459DF534C8FED90A1670C6F1886DA67C3D5C1C22E5769359DEF033` 的三项 owned 错误以及原输入 SHA / bytes 已保存到 `editor-business-handoff-sdk-diagnostics.json`：line258 将 `EditorIntentProof` 赋给不同 nominal class，line493 使用受限 `Object.assign` 与 untyped literal。

修复为将五个既有 intent proof 字段逐项赋值给 `DraftBusinessLink.intent` 的 typed instance，并将五个既有 source0 scope 字段逐项赋值给明确 `DraftScope` 实例。值、身份、完整 source、request/retirement/close literal、发请求顺序和所有效果归属均不变。

当前重新冻结 `EditorBusinessHandoff.ets`：40823 bytes，SHA256 `8F68B983FB4747360A07B59BC80B0397ABE9CFF8990C7F4950F2280C71F2570F`。其它原 10 个 model/test/actual Store fixture 输入不变。修复后同一四组实际 ETS 测试 **97/97 PASS / 0 FAIL / 0 SKIP / 8663.9329 ms**，全部 11 输入运行前后 bytes/SHA 相同。

新证据使用独立 `editor-business-handoff-sdk-repair-inputs-before.json`、`editor-business-handoff-sdk-repair-inputs-after.json`、`editor-business-handoff-sdk-repair-result.json` 和 `editor-business-handoff-sdk-repair-tests.log`；没有覆盖第一次 97 PASS 的记录。新 log SHA256 `6CC03CEB74BF44E66427E237FEF4C90E8DA2782D277BEFF7FA1460BF9B671EC9`。

这次本地实际模型检查不能证明完整 ArkTS SDK 已修复成功。Root 需以更新输入做实际产品重建；原 SDK 失败及其其它页面诊断不因本报告消失。
