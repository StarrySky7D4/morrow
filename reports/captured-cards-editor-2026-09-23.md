# V2 卡片编辑器来源与保存接线

本阶段接通普通卡片文本、附件及粘贴来源的 V2 编辑保存路径。应用保留 `0.1.9-test.54+58`，第一方包保留 `0.1.9-test.54.3`。未提交、推送或发布；测试仅操作临时内容库。

## 已实现

- 新增独立 `morrow.workbench.cards-capture.v1` 投影，将实际剪贴板转换观察、用户采用记录、最终编辑快照与 cards.v2 的真实执行观察放入同一提交证据。旧 V1 和既有 cards-edit/tasks-edit 投影保持不变。
- 范围绑定源卡片、修订、完整源摘要及插件包；保留 TaskId、历史歧义、来源映射和未知字段。普通文本编辑不把待办文字当作 TaskId，拒绝此路径中的 todos 快照与粘贴。
- 最终 guest 与前序转换共享原有总燃料限额；只有已采用的捕获进入证据。附件先暂存，仅选中的附件进入提交。
- 提交前冻结操作 ID、字段和捕获证据。SQL 失败或提交结果不确定时保留原意图；拒绝夹带新粘贴或修改后重试。历史重试重新授权且不重新执行 guest。
- Flutter 新增类型化编辑会话；通过私有 FinishCapturedCard 消息上传。提交后另读当前记录，刷新失败保留原回执及请求字节；后续核对使用原操作。

## 验证与边界

| 检查 | 结果 |
| --- | --- |
| 纯投影 | 4 通过；为合成观察元数据测试，不冒充实际 guest 执行。`build/review-captured-cards-projection.log` |
| 实际 owner | 4 通过；真实 Wasm、独立 CLI 重放、摘要及执行证据篡改拒绝、撤权/缺包，以及 SQL/deferred COMMIT 故障后的原操作重试。`build/review-captured-cards-owner.log` |
| 旧路径回归 | 15 通过；既有捕获投影、传输与私有内容协议。`build/review-captured-cards-legacy.log` |
| Windows 原生客户端 | 新编辑测试 2、既有版本客户端 4、旧编辑捕获 4、宿主组合 3，共 13 通过。含 HTML 转 Markdown、附件引用、待办身份不变和提交后刷新失败重试。`build/review-captured-cards-native.log` |
| 静态与生成 | 4 个目标无问题，生成绑定检查通过。`build/review-captured-cards-analyze-final.log`、`build/review-captured-cards-codegen.log` |
| Windows Release | 宿主、Dart 应用和插件安装构建成功。`build/review-captured-cards-windows.log` |

独立交叉审核未发现可复现的越权或重复提交路径。以上为各自覆盖范围内的证据，不能替代完整应用迁移、设备或跨平台验收。

## 后续

主工作台仍未接入 V2/TaskId/歧义确认，迁移按钮未开放，用户资料未迁移。Dart 进程重启后尚不能自动恢复未完成编辑会话与请求；宿主历史重试能力不等于界面已持久保存恢复材料。完整保存恢复、错误协议、远端 Unknown 和 SDK 冻结仍未闭环。
