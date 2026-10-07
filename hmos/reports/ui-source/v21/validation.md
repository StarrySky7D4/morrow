# 编辑器生命周期、草稿接续与 Add 焦点修复检查点

2026-10-07。独立 `hmos/` 工程，应用身份仍 **0.1.0-hmos-dev.19 /1000019**，同步专用分支 `codex/ArkTsUI`，不合入 main。本次是源码、模型与API26构建检查点，**新包未安装，设备验收NOT_RUN**；推送完成以根代理远端核对为准。完整 Flutter/Windows 目标 **OPEN**。

## 本轮实际范围

Index 将每次挂载的编辑器视图归属冻结到 SDK 回调；文字、选区、焦点及任务重命名再核原视图/任务身份。真实组件生命周期撤销后，旧回调不能写替换编辑器。这是应用接纳边界，**不是 SDK 输入队列排空证明**。

raw fork 已接入：等待父草稿已有写入明确结束后冻结准确父 proof，保持采集；首子回执通过后，在同一同步步骤切换 writer并复制当前完整 raw，确认子最新值再发固定条件父退休操作。保持原 source kind/revision/完整bytes和原业务冲突；未加入的暂存附件须先处理，Unknown只显式核对原请求，不改身份重放。

手动确认放弃先结束视图接纳，再确认完整 raw及准确草稿清理；保留关闭只关闭已确认保留的输入。保存成功关闭仅允许原owner、输入代次及完整当前值仍与提交相符；关闭期间有较晚完整输入则重新挂载并接续保留，采集不完整或Unknown停止清理。尚未交付的系统事件不因此获得完整持久化资格。

Add 自动焦点等待同owner/行实例有效布局，先 reveal再有界重试，捕获不可见错误；用户转焦、失效身份、disable/disappear撤销过期意图。旧行 area回调不能删除新实例的布局测量；焦点失败不回滚已创建行或原文字。首轮78项与修复前两个探针保留于 [焦点审查](focus-review.md)，最终修复结果另列下表。

本轮 **Rust未改**，复用 `b4860df4` 接续基础检查点已核对的双ABI库。该历史 native/Store/fault证据见 [原生接续验证](../v20/retirement/validation.md)，不计为本轮新运行。严格 `editor_save`、历史业务结果接口、业务来源重基及完整V2多行后继投影仍仅 [设计](business-continuation-design.md)，没有新增后端dispatch/proto。旧 [生命周期审计](editor-lifecycle-audit.md)记载接入前事实和SDK限制，不能把旧缺陷位置直接当作最终源码。

## 本轮验证

| 范围 | 结果 | 证据 |
| --- | --- | --- |
| Add焦点修复完整目标模型 | **82/82 PASS /0 FAIL /0 SKIP**，1171.12ms | [最终日志](todos-focus-final-tests.log) |
| Index实际方法/lease/fork及既有准入回归 | **66/66 PASS /0 FAIL /0 SKIP**，3881.5057ms | [最终日志](index-lease-fork-final-tests.log) |
| 全部实际ETS/工具/Index模型 | **718/718 PASS /0 FAIL /0 SKIP**，9023.7326ms，已包含上述目标组，不能相加当新增通过数 | [完整日志](models-final-tests.log) |
| API26产品HAP | **SUCCESS /14.348s**；CompileArkTS7.374s，33tasks/17执行/16复用 | [构建日志](hap-build.log) |
| 双ABI复用 | **PASS**：264原生source与两archive/采用库逐字节核对，依据b486；没有本轮Rust测试或原生重建 | [复用核对](native-reuse-check.json) |
| 包内native | **4/4 PASS**：两ABI `libmorrow.so`/`libc++_shared.so`全字节匹配strip输出，并与b486包对应项一致 | [包内核对](native-package-check.json) |
| 构建输入disk | **322项 before/after身份PASS，disk PASS** | [冻结清单](build-inputs.json)、[disk核对](build-manifest-disk.log) |
| 构建输入staged | **322项 PASS**，与构建冻结字节及最终HAP匹配 | [暂存核对](build-manifest-staged.log) |
| 新包安装/焦点、fork、关闭设备资格 | **NOT_RUN** | 设备仍旧476901包 |

模型使用受控SDK/provider/时间及实际方法提取，不代替真实生命周期或IME。先前78项日志、两个焦点失败探针和Index初期失败保留，不覆盖为最终通过；最终源码与日志的范围必须匹配。包版本名相同不能合并不同HAP资格。

最终未签名debug HAP **28,796,777字节**，SHA256 **5B62B7F3889810940DB46ED6817728F811CCD75FA096CA21317AF25504EA0992**，归档 `.build/artifacts/dev21-ui-checkpoint/entry-default-unsigned.hap`。本地构建包不提交到Git，不据构建成功填写安装/签名资格。完整构建输入见 [当前manifest](../../../build-manifest.json)。

最终Index SHA256 `B0A53C29BEB0F7A54A2D6DFB3DB0F6C74CABF316996BF46198F906CE97AC37E9`；`EditorTodos.ets` SHA256 `6ABAA19A360487AEAB67F34E5F53EF966B1D3566A05B42377A9AF2901E2AAD18`；`EditorViewLease.ets` SHA256 `4F3172C6C8DADEC3019A36F0FA4EF82CFA1D1B8A9232F16F5A0CAE4FAD870730`。这些是源码字节身份，不授予运行时资格。

## 旧包实际设备事实

既有 x64 API26模拟器仍运行 `eeca59f8` UI集成包，HAP SHA256 **4769015976A302099D4CDC6ABB49DE3F7A836AE3559FED1AD6F56A677F6C460E**；该包不含本轮修复和fork接线。

实际点击Add后焦点错误 **150003**导致崩溃，栈为 `requestFocus → EditorTodos.ets:122:75`，见 [原故障日志](device-todos/add-row-jscrash.log)。公开新卡草稿 `HMOS-todos-20261007-D` 随后实际恢复标题与正文 `Public Flutter todo UI fixture: 汉字 🧪 é.`，见 [恢复截图](device-todos/own-todo-crash-recovery-v21-restored.png)；保留关闭后首页显示“草稿已保留，可从草稿入口继续”，见 [保留后截图](device-todos/own-todo-preserve-before-fix-v21-after.png)。[原动作记录](device-todos/progress-clipboard.json)保留失败/未知阶段，不重写为通过。

这些证据限定为旧包的一次崩溃及原文字恢复/保留，**不证明新焦点修复已生效**，也不证明多行编辑/拖动/业务保存重启、fork pin继承、手动弃稿、准确关闭、SDK队列排空或任意IME恢复。签名、ARM64真机、HUKS/保护存储、准确业务来源接续及完整Flutter/Windows对齐继续 **OPEN**。
