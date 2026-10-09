# v33 实际 Index 外观偏好接入

本轮在 Task agent 冻结 Index `619592c0e8483199ea1ca92ab48fa6eed9efa25be37051e368060e6410786664` / 350,258 字节后进行局部 patch。SDK、设备与 Git 由 Root 单独验证，本报告不声称这三类资格。

## 实际产品行为

Index 通过 actual `@kit.ArkData` Preferences 实例接入 `AppearancePreferences`，namespace 保留 `studio-appearance`、key 保留 `appearance-v1`。成功 hasSync 证明不存在才返回 undefined；现有空串、非 string、创建/has/get 失败都成为 read-failed，当前完整预览不会覆盖它。

完整的当前 UI 值经同步校验后才成为 candidate。已确认 current、最后原字节、实际观测值、固定 unknown 发出字节彼此独立。普通保存按顺序执行 preflight exact raw、put、flush、exact readback；旧 ACK 只刷新状态，不反向覆盖更新滑块/颜色/语言等输入。最新无效输入不会被较早 ACK 改成保存成功提示。

AppearanceState 执行实际 state/busy/dirty 条件，显示准确读取、保存、读失败、未知或未保存状态；坏记录/未知结果提供“重新读取并核对”，核对成功仍有新候选时提供“保存当前外观”。两步是不同动作，核对不 put 候选或默认值。主外观和字体/材质/颜色子页均显示同一状态。

页面实例 UUID 与独立 foreground epoch 约束每次响应；后台和关闭停止后续未知写入，真实已发出的 Promise 仍须终止。新前台可以观察已有真实 appearanceOperation 的终止，仅更新状态，不采用旧响应、不自动恢复或重放。该状态观察避免旧恢复结束后 UI 永久留在 loading。

同 namespace 的模型在模块中共享实际 live owner。旧页面 dispose 不伪造取消，也不清除仍在途的 owner、原 current/raw 或固定 unknown。新页面在旧 read/put/flush/readback 或 recovery flush/readback 仍活跃时不能读取认定成功、不能 put，也不能启动 flush。终止后 unknown 保留，必须显式核对，然后新候选才可以另行保存。

## 本轮冻结源码和直接证明

- Index.ets：356,113 字节，SHA-256 `57a9e5aa56c647c9635bf23483a69da359867a60c1606f89f1a0348e1765c4a8`。
- AppearancePreferences.ets：22,653 字节，SHA-256 `849abce141efb3656db058a13bb8240e5bff04d001fe8d51558eebde6ddf41e5`。
- `appearance-preferences-a2-result.json`：61/61 PASS，0 fail/skip/cancel，7 执行输入、4 参考，exact before/after，415.3108 ms；日志 SHA-256 `bc4c857f4994c34785f68b67bd9603f46e1c30230351f9cf494c20f55c4e7ceb`。
- `index-appearance-a1-result.json`：32/32 PASS，0 fail/skip/cancel，10 执行输入、5 参考，exact before/after，6230.7914 ms；日志 SHA-256 `86815b0a33a7639792ad7daa282eb80103cdd8182aa00fc981a60dc50d2e674a`。
- `index-appearance-a1-task-preservation.json`：PASS_DECLARED_APPEARANCE_CHANGES_ONLY。只逆转声明的外观 import/fields/methods/lifecycle/UI patch，即逐字重现 Task frozen Index 的完整原哈希；任务、business、音乐、pure paint 和所有其他字段保持原样。报告明确比较时统一 CRLF，实际 canonical 原哈希仍等于 619592c0…86664。
- 上述两组是不同测试文件的 scoped source 证明；完整全量回归另由 Root 提供，不能将 scoped 总数重复加到全量总数。

独立审查另在相同 frozen model 上执行 61 项和 6 个交接向量，覆盖旧 preflight read、put、flush、readback、恢复 flush、恢复 readback，均通过。旧失败复现保留；实际结果见独立审查报告，而不是把旧失败改写为通过。

## 执行范围与待验项

测试执行实际当前 Index fields/methods、actual Appearance / AppearancePreferences / UiStrings，包含真实 aboutToAppear / aboutToDisappear / foregroundChanged。ArkData 与无关生命周期控制器为明确受控 seam。AppearanceState 仅转换 ArkUI 外层 Column 子节点语法，保留真实 if/enabled/文本/回调；没有用手写模型替代存储逻辑。

共享屏障只覆盖同进程、同 namespace、经本模型访问的调用，不是跨进程原子 CAS、断电持久性或崩溃 journal。unknown 重读后的 flush ACK 是平台操作级确认，不能替代设备重启/真实服务数据检验。本报告 SDK_BUILD、DEVICE_PERSISTENCE、ARKUI_PIXELS 均 NOT_RUN；未运行 Rust 构建/测试，未声称字体导入、背景资产或完整 Windows parity 已完成。

`run-index-appearance.cjs` 的完整 task-preservation 资格需要本机已捕获的 ignored `.build/appearance-integration/v33/index-task-baseline.ets`，不把它当可发布产品输入。普通 `index-appearance.test.cjs` 本身不依赖该 ignored 基线，可以在源码 checkout 上单独运行。
