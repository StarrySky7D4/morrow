# dev20 源码检查点

2026-10-07。本次交付将当前HMOS适配源码同步到专用分支 **codex/ArkTsUI**，不合入或更新main。实际commit/push及远端SHA由root另行核对，本文件不预报推送成功，也不记录自己的commit SHA。**AppScope仍为 `0.1.0-hmos-dev.19 /1000019`；新的EditorTodos、EditorInputPolicy尚未接入Index，当前源码检查点不是dev20完整应用验收。**

## 已实现与实际验证

| 范围 | 当前证据 | 资格边界 |
| --- | --- | --- |
| Rust只读输入formatter、NAPI/类型接口 | 实际Flutter Windows捕获297完整编辑身份：285在native预算内逐字段一致；12为明确request/reply字节预算拒绝。另有3个真实Flutter controller结果，两项实际Flutter测试PASS | 纯formatter不代替ArkUI controller/IME；Windows controller跨出composition后的清除行为单独记录；见 [native审计](editor-input-native-audit.md) |
| 待办行模型与ArkUI组件方法 | **55/55 PASS**，36项实际ETS row-model、19项实际component方法；完整newline草稿、行ID/选区、100行与总1000grapheme/剩余额度、owner/epoch/late reply、Add/remove/move/drag lifecycle | injected formatter/count是测试契约，不能证明native算法；未执行ArkUI declarative渲染或实际设备输入；见 [行模型审计](todos-source-audit.md)、[fresh log](editor-todos-model-tests.log) |
| ETS EditorInputPolicy | **15/15 PASS**；完整old/new/value回执、原request SHA、字段/mode/limit、Unicode16/计数/字节与错误检查，零额度retained的filter-finalize完整回执、accepted额度一致性及异步owner屏障 | Index尚未实例化或采用该adapter；算法另由实际Flutter/Rust对照验证，独立源码接口不证明现有页面行为已改变；见 [log](editor-input-policy-model-tests.log) |
| 新卡多行待办原子创建 | `Command.todos`/Rust Request可传完整raw；同一个原create事务创建全部真实V2 Task。**9项focused PASS**，2项默认条件ignored各独立运行PASS：50完整实际Dart整理结果、7个真实Store进程退出边界 | 属于HMOS development adapter的新卡V2投影；已有V2普通edit拒绝非空todos，原TaskId详情命令独立保留；见 [业务审计](create-todos-audit.md) |
| 冻结后的完整Rust与实际条件对照 | **123 PASS，0 FAIL，6默认ignored**；102.53s。5项实际Flutter/Dart对照PASS，13.74s；第6项Store crash以独立host fault-injection运行PASS | focused与combined重叠，不累计为产品覆盖率；默认ignored本身不是PASS；见 [完整suite](input-native-final-rust-tests.log)、[真实对照](input-native-final-all-flutter-compare.log)、[Store crash](create-todos-store-crash-tests.log) |
| 完整双ABI原生候选与产品采用 | 独立 `dev20-final` ARM6416.33s/x64 8.92s release staticlib PASS；两ABI包含editor_input机器对象导出。261项repository native输入、候选和保留库、实际Flutter依赖/fixture核验PASS；root采用final库并完整重建HAP，四条native ZIP内容核对PASS | 检查点包仍使用dev19 manifest版本、未安装；与发布过的dev19包按SHA区分，不是dev20设备资格。旧formatter-only候选不包含create_todos；见 [native来源清单](input-native-inputs.json)、[fresh check](input-native-input-check.log)、[采用核对](input-native-production-check.log)、[完整构建](hap-checkpoint-final-build.log) |
| 独立图片手势tester | API26真实multipointer runner及host driver **22 PASS**；独立unsigned main/test HAP构建PASS（6.287s/6.779s），准确artifact SHA已记录 | 不代表产品图片手势PASS；tester实际设备安装/注入/PNG位移验收仍NOT_RUN于审计；见 [图片审计](image-device-audit.md) |
| 新组件/策略显式SDK编译 | 独立API26工程 **10.754s SUCCESS**，307原源+5harness hash核对PASS；修正parent/child enabled与onFocus的基类命名冲突，改为editingEnabled/onRowFocused | 未修改产品Index/版本，独立smoke包未安装/运行；主应用入口尚未采用新UI；见 [SDK审计](sdk-smoke-audit.md) |

Root最终产品检查点HAP构建 **8.145s SUCCESS**，28,225,993 bytes / `36151E624FD61297436E979F9AA54D0682CDA8DF9B9F02F162AA0EF4CC07C3C5`，仍dev19 manifest版本、未安装。完整模型最终fresh **617/617 PASS**；完整313项源输入与artifact SHA核对见 [最终验证](validation.md)。先前候选、首SDK失败与旧已发布dev19包分别保留，不改写原证据。

新卡创建的raw上限先检查**完整**todos≤1000 Unicode16 graphemes、原始LF行数≤100，再按实际Dart的trim/去空/首次顺序dedup投影。原raw journal不被normalize或截短；单项Task≤2048 UTF-8 bytes、完整Properties≤64KiB等原共享预算继续生效。业务TaskId绑定card/原operation/完整raw身份，不使用view行ID，也不循环调用task_add。HMOS-owned未知字段50001仅保存原raw身份digest以验证原proposal，保持旧空create兼容；不是Flutter schema、加密日志或protected capture等价。

真实Store测试验证全部任务与原receipt/event同事务、重启后准确重试、blank/duplicate改变raw后的原operation拒绝、unknown properties经过后续编辑与任务操作保留，以及后续mutations/较新或已弃raw journal不会被原create重试覆盖。测试没有加入业务自动Unknown重放；host fault-injection feature不进入release构建。

## 当前设备与产品版本

root已另行安装并读回冻结的 **dev19**： [安装意图](device-migration/dev19-install-intent.json) 绑定dev19 HAP SHA `F7A913980CF3213536820727A677EEC04EA169FE2B65CE16E305F352D14BE4EC`；[bundle读回](device-migration/bundle19-final.json) 为 `1000019 /0.1.0-hmos-dev.19`。安装后的已有草稿读回另存于 [device19-current](device19-current/dev20-read-own-c-draft19-restored.capture.json)。这些证据验证对应dev19的安装/已观测状态，**不验证尚未接入Index的新行UI、输入formatter或创建接口，也不是dev20完整设备验收**。本报告作者未操作设备。

## 继续接入与开放差异

- Index需采用新卡完整newline row模型与InputPolicy，将exact raw TextValue、editor owner/epoch、剩余额度、异步旧/新回执和IME preview/commit结合；确认按钮、粘贴与业务保存必须等有效完整结果，原raw候选继续保留。
- 新卡创建需在原save/create意图中发送冻结完整 `Command.todos`。已有V2任务仍走独立TaskId路径；不能把现有tasks或待添加单条输入填充成legacy rows，不能调用多次task_add冒充同一创建。
- row Add/remove/reorder、outer-scroll edge handling、queued focus、跨行选区、IME恢复与停用/重开生命周期需在实际ArkUI页面验证。当前行TextArea固定76vp与Flutter min1/max3自适应高度不同；gesture380ms/16ms edge loop、字体/像素和selection affinity/direction仍未资格化。保留active preview后才采用formatter也与Windows enforced active composition有差异。
- root已采用最终native并完成当前检查点HAP构建；后续需完成Index UI接入、新版本与准确安装包，以及真实新卡任务/receipt、restart恢复和未知结果核对设备验证。当前不以源码测试或旧包设备结果代替这些步骤。
- 图片手势需依据fresh实际image/canvas/版本与fixture身份启动独立tester，查看真实before/after PNG特征位移和缩放。injection ACK、百分比或组件rectangle不单独构成像素手势PASS。

完整Flutter/Windows目标、保护存储/HUKS/捕获与迁移资格、签名和ARM64真实设备完整验收继续 **OPEN**。dev19与更早报告保留其原版本事实；本检查点没有覆盖或扩张历史接受范围。
