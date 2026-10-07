# Index UI 集成源码检查点

2026-10-07。在已发布检查点 `7c3d3bcadd7b4967563c93859bbdba6e4983bced` 上接入实际主页面。仅向 `codex/ArkTsUI` 交付；不合并主线。产品身份仍 **0.1.0-hmos-dev.19 /1000019**。这是源码与构建检查点，不是 dev20 发行或设备验收。

## 本轮改动

- 新卡编辑器采用实际 `EditorTodos`，按 Flutter 源码显示1～3行待办、完整聚合输入状态、行控制和已有拖动接口；现有 V2 TaskId 面板仍按各自身份操作。新卡的完整待办文本随同一个 create 送入此前实现的原子事务。
- 主页面采用 `EditorDirectInput` 与原生 `EditorInputPolicy`，按 owner、字段修订和完整 TextValue 核对纯格式回复。检查未确认时业务保存被阻止，完整 raw 的保留独立处理。
- 粘贴和两处附件自动标题先同步完整文字及 UTF16 选区，再绑定输入协调状态，避免把外部已校验输入误当键盘格式任务。修复前后台 stop 后多个未变化字段无法重新确认的问题。
- 旧业务回执不覆盖替换的编辑器。清理草稿期间同 owner 的迟到普通字段/待办 SDK 事件被记录，撤销关闭资格；已成功清理的身份阻止再次 flush、业务保存和新的原身份写入。

## Fresh 限定验证

| 检查 | 当前结果 | 证据 |
| --- | --- | --- |
| 全部实际 ETS/工具模型与 Index 方法检查 | **678 PASS /0 FAIL /0 SKIP**，6158.0315ms | [完整最终日志](models-publish-tests.log) |
| 实际 Index 新卡业务边界 | **16/16 PASS**，含原请求 Unknown 核对、完整待办、owner/epoch/选区/候选变化与退稿守卫 | [业务审计](index-business-audit.md)、[最终日志](todos-business-final.log) |
| 实际 Index 字段集成 | **20/20 PASS**，既有业务断言保留 | [最终日志](field-integration-final.log) |
| 实际 Index 剪贴板集成 | **15/15 PASS**，新增真实 DirectInput 粘贴/自动标题全值交接 | [最终日志](clipboard-integration-final.log) |
| 输入协调模型 | **24/24 PASS**，含多字段 stop/原值 rebind | [最终日志](direct-input-final-tests.log)、[合同审计](direct-input-audit.md) |
| 待办模型和实际组件方法 | **74/74 PASS**，含禁用期间同身份迟到事件通知/拒绝旧票据 | [最终日志](editor-todos-uncaptured-tests.log)、[组件审计](todos-component-audit.md) |
| 实际 API26 产品 HAP | **SUCCESS /10.732s**；新组件已被 Index 导入并参与产品编译 | [最终构建日志](hap-final-integration-build.log) |
| 构建输入 | **314项**构建前冻结、构建后同字节，disk 与 staged 清单核对 PASS | [冻结清单](build-inputs.json)、[disk核对](build-manifest-disk.log)、[staged核对](build-manifest-staged.log) |
| 原生复用身份 | **261源输入、双架构产物、生产 staticlib、实际 Flutter 参照及外部依赖均 fresh 哈希核对 PASS** | [采用核对](native-adoption-check.log) |
| 包内原生库 | **4/4 PASS**，ARM64/x64 `libmorrow.so` 与 `libc++_shared.so` 的完整字节哈希匹配 SDK stripped 输出 | [包内核对](native-package-check.json) |

最终未签名 HAP **28,533,476字节**，SHA256 `4769015976A302099D4CDC6ABB49DE3F7A836AE3559FED1AD6F56A677F6C460E`。本地归档 `.build/artifacts/dev20-integration-checkpoint/entry-default-unsigned.hap`，**未安装**。全局 [build-manifest.json](../../../build-manifest.json) 现指向此构建；此前发布检查点和 dev19 的原日志、归档、设备证据保持原身份，不追改为本轮结果。

本轮不新增 Rust/native 代码。此前123项Rust、5项条件实际Flutter/Dart对照及7个真实Store进程丢失边界为已有源身份的历史限定证明；本轮新跑的是上述原生/参照哈希采用核对，不把旧测试记为再次运行。模型使用合成 provider/格式回复或抽取实际组件方法，不代表 SDK 像素、真实输入法或 native 持久化验收。

首轮 SDK 编译、模型及业务失败日志均保留。`models-publish-stage1.log` 为当时 **672 PASS /1 FAIL**；该失败暴露真实 stop/rebind 缺口，由生产修复及上述最终回归验证。不同 stage 的源码身份和结果不得与最终冻结构建混用。

## 仍未完成

**成功清理旧草稿后，迟到输入的新 draft/source/CAS 持久接续仍 OPEN。** 已失效身份不能重新激活，也不能靠附件重导入假造权限。当前守卫保存本地 SDK 事件和可确认的普通字段值、阻止关闭或把旧 confirmed 标作新输入已保留；这些较新输入可能仅存在当前编辑器内存，不保证跨重启恢复。完整附件 pin 接续和精确业务回执基线需后续实现。

新 UI 的实际设备布局、1～3行高度、焦点/输入法/光标、拖动边缘滚动、全篇选区、原子建卡读回和重启恢复均 **NOT_RUN**。设备仍使用此前已安装的 dev19 包；无新安装、签名、发行、主线合并或全 Flutter/Windows 对齐验收。主题颜色/矢量及 IME 合同差异、HUKS/保护存储、ARM64真机和完整功能目标继续 **OPEN**。
