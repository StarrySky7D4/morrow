# v24 真实恢复初始化回调诊断

此文保留两次诊断结束时的状态；随后准确 B62 UI 修复候选已完成恢复与保留关闭限定检查，最新结果见 [v24 验证](validation.md)。下文“当前设备”和“下一步”均属于此诊断阶段。

2026-10-07。Root 只在已有 API26/x64 `127.0.0.1:5555` 上操作本任务自有公开草稿 `HMOS-todos-20261007-D`；未输入业务文字、未提交业务卡，未执行 keep 或 discard。两次安装前均读回已知公开 title/body/Todo；完整 IME/选区保全资格为 false，不声称 force-stop lossless capture。v23 的 FAILED_OR_UNKNOWN 原结果未改判或重放。

## 准确候选与范围

两候选都从 Git `7f07d07480660bbc868369b9565dc98a47e8af08` 的完整产品源码复制，包含最终 `E0A3` Todo 快照门禁与未变的 v22 native。不采用工作区正在改的 Index 普通字段门禁、Todo 初建回声能力、Rust intent 或 ETS session。只在隔离副本加 console 元数据诊断，不记录正文或 preview 文字。

| 候选 | 包字节 / SHA256 | SDK / 输入 | 实际设备 |
| --- | --- | --- | --- |
| Index 分类诊断 | 29,104,652 /`6FCB10286838BC6EAD15266C7E07FADA89D739E76A725CE66814345CECE4FA0D` | API26 SUCCESS /13.508 s，34 tasks全部执行；312复制输入、267旧 native Git 输入及2个archives准确核对 | 安装/启动 ACK；PID16951 前后相同。恢复原文可见，captureIncomplete=true |
| Index + Todo 详细元数据 | 29,106,549 /`7D0CA55831AD0A6C3A70D6D945BF7E579692E2B9BF719126A88CBC41171A0D19` | API26 SUCCESS /13.984 s，34 tasks全部执行；相同有界输入身份核对 | 安装/启动 ACK；PID19675 前后相同。恢复原文可见，captureIncomplete=true |

源码清单/产物：[分类准备工具](ui-diagnostic/prepare.cjs)、[分类清单](ui-diagnostic/source-copy-manifest.json)、[分类产物](ui-diagnostic/artifact.json)、[分类 build](ui-diagnostic/hap-build.log)；[详细准备工具](ui-diagnostic-detail/prepare.cjs)、[详细清单](ui-diagnostic-detail/source-copy-manifest.json)、[详细产物](ui-diagnostic-detail/artifact.json)、[详细 build](ui-diagnostic-detail/hap-build.log)。HAP 保存在 ignored immutable archives，没有把诊断代码写进生产文件。

安装绑定是 host archive SHA + 成功日志中准确路径 + saved/fresh bundle 元数据；bundle 版本仍 dev19，不能充当设备 installed-byte 摘要。[分类安装记录](device-diagnostic/installation.json)、[详细安装记录](device-diagnostic-detail/installation.json)。当前设备是详细 `7D0C` 候选，不是未来修复或新 intent backend。

## 回调原证据

[首个 PID16951 分类日志](device-diagnostic/capture-diagnostic.log)实际记录：

```text
uncaptured name=todos kind=todo_input focused=false restore=true owned=true event_length=76
selection name=title start=21 end=21 length=21 focused=false restore=true owned=true
selection name=description start=41 end=41 length=41 focused=false restore=true owned=true
```

[PID19675 详细日志](device-diagnostic-detail/capture-diagnostic.log)实际记录同一个首回调：

```text
owned=true enabled=false focused=false same_display=true same_control=true
text_length=15 preview=true preview_length=0 preview_offset=-1
raw_composing_start=-1 raw_composing_end=-1 pending=false incarnation=1 serial=1
```

随后准确进入 `leaseUncaptured(name=todos, kind=todo_input)`。普通 title/body 的初始化选区均合法，**本次提示的实际入口是 Todo input，而非普通字段非法选区**。普通字段顺序修复仍有独立边界价值，但不能说它解决此观察。

该事件在还未聚焦、restore=true、控件disabled时到达，文字与已设置的 display/control 相同，preview 是空值/-1，原raw没有composition。因 `EditorTodoRowInput.changeFor` 的 disabled 分支把它报告为未捕获事件，Index 保留原事件并阻断清理。诊断说明了这次实际入口与参数，不给 SDK 没有提供的 native-origin token 补造来源证明。

完整阶段动作/raw/截图：[分类观察](device-diagnostic/progress-clipboard.json)、[详细观察](device-diagnostic-detail/progress-clipboard.json)。阶段的 PASS 仅表示观察和日志获取完成；不是草稿恢复可继续保存/完整关闭 PASS。可见原Todo仍为 `first 汉字 🧪 é.`，没有据相同文字清 Unknown。

## 下一步与资格边界

一次真实 mount 的初建 display 回声能力正在生产实现：冻结同controller、ticket、完整原row、初值，首个输入消耗一次；真实focus、row更新、不同输入、preview或销毁撤销。匹配时只忽略自己已设置初值的回声，不更新raw/epoch/composition或清除Unknown。最终源码、方法测试和新准确包仍需独立核对及设备恢复/关闭/输入验证。

SDK onChange 仅提供text/preview，没有完整 TextEditingValue 或明确initialization origin。本诊断不证明 SDK 队列排空、任意 IME/affinity/全篇选区已保全。持续追平目标、严格业务 session/Index/source0 交接、ARM64真机/保护能力和完整验收仍 OPEN。
