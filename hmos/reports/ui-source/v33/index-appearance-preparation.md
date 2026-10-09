# v33 Index 外观接入准备

准备期间只读取当前 Index 与测试工具；Index task 尚未由 Root 冻结，未修改 Index。交接后使用局部 patch；禁止整文件重写或格式化。

接入字段计划：`appearancePreferences` 单个实例、`appearanceView` 可观察状态、独立 `appearancePage` UUID / `appearanceEpoch`、`appearanceInputRevision` 与输入校验错误。保留 `preferenceStore`，删除旧 `appearanceWrites` 裸写队列。

接入方法计划：`appearanceStore / appearanceModel / appearanceOwner / appearanceOwned / appearanceUiValue / applyAppearance / refreshAppearanceState / restoreAppearance / persistAppearance / recoverAppearance / saveAppearancePreview`。原数据读取依赖 actual Preferences.hasSync；读取错误、非 string、现有空串不会变成缺失。put 与 flush 复用同一实例。现有业务数据库、任务、音乐与绘制方法保持各自所有权。

同步候选完整验证先于 save；界面输入异常时显示未保存，不得因另一较早写入的回调恢复绿色成功文案。较早 ACK 仅刷新模型状态；不得将较早 current 回写到当前 UI。restore / 显式 recover 应用 current 需要核对页面实例、前台 epoch、输入 revision，以及不存在更新候选或校验错误。

外观状态 Builder 使用一个普通 Column，不增加框线或阴影，替换原无条件已保存文案；执行真实条件和 Button callback，显示“重新读取并核对”和“保存当前外观”入口。设置子页需显示同一状态，避免字体/材质修改失败后看不到结果。

实际 lifecycle 只增加独立 appearance epoch 增长和 dispose。前台回来刷新状态；只对 not-loaded 重试读取，read-failed/write-unknown 不自动恢复或保存。背景/关闭后迟到响应不能应用 UI，也不能继续未知写入队列。

独立源码 harness 将以 TypeScript AST 抽取上述实际 fields/methods 及真实 aboutToAppear/aboutToDisappear/foregroundChanged；实际 Appearance + AppearancePreferences 执行、fake ArkData port 记录 has/get/put/flush，其他 lifecycle 控制器为明确受控 seam。Builder 只替换 ArkUI 外层 Column 子节点语法，保留真实 guards、按钮 enabled、文本、callback。无需改共享 index-business harness。

计划故障向量：open/read 失败 no-put、missing 与非 string/空串/坏 JSON、put/flush unknown、显式恢复不自动保存候选、快速连续 UI 修改、较早 ACK、读取时的新 UI 输入、输入校验异常不误报已保存、前后台/关闭时的晚响应、queued stale owner 不发出、unknown 恢复晚响应、真实 Builder 按钮触发与条件。该证据仍不等于 SDK、真实存储持久性或设备资格。
