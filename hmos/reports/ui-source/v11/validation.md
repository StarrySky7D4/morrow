# dev.11 阅读详情、卡片菜单与持久顺序

日期：2026-10-05。版本 `0.1.0-hmos-dev.11` / `1000011`。持续追平 Windows/Flutter，仍为独立开发预览。

## 包与参照

最终候选 HAP 为 `entry/build/default/outputs/default/entry-default-unsigned.hap`，23,718,916 字节，SHA-256 `5BC43903EE786FCF5CFDCAA739BF2F5075B397958AA61A8F312F89D00A4B94A5`。API26 / SDK26.0.0.105，未签名 ARM64/x64 包。仅 x64 Pura X View / 7.0.0.106 / `127.0.0.1:5557` 实际运行；1320×2232 px、density3 / 440 vp。未更改密度、旋转或系统参数，未重置、不操作已知故障5555设备。

本轮只改 ArkTS/UI 偏好与验收辅助；Rust/C++/共享快照无修改，复用 dev.10 的 ARM64 静态库 `AB0AC992BFBD41B59578360D9A69A6EDAAA567ACB29FBB6AF64A17B453EF7D99` 和 x64 `2FF21151E19F99F6485F11634E8DC5862C8867E8C4041C0EAFF4CC704EC25703`。不将 dev.10 Rust 测试计为本轮新增执行。

三参照 HEAD/应用版本在收尾时再次核对：`io-safety-refactor=925fb8ca` / test.57+61；`win-cloud-20261005=77246617` / test.58+62；`windows-sdk-reconstruction=20669f67` / test.58+62。布局主文件与手动顺序模块 Git blob 均一致，具体来源见 `upstream-audit.md`、`platform-audit.md`。源任务最新已开始 Rust 协议实现，报告首轮离线编译通过，仍在修正发现信息、分页游标和任务关联边界；这不是普通 Flutter 工作台或 HMOS 的运行接入证据。

## 实现

点击卡片先阅读详情，显式编辑再进入已有来源绑定的文字草稿流程。详情直接读取最新业务 CardView，Markdown 预览按 ID/修订隔离；分类、阶段、TaskId勾选、收藏和删除仍使用完整源 CAS。删除经确认且再次核对当前来源；回收站撤销保留共享核心8秒条件。复制标题+两次换行+完整正文。查看、关闭、复制不创建 journal；偏好排序也不修改业务卡。详情短内容按内容收缩，长内容在受限高度内滚动，修订更新保留滚动位置。

卡片正文长按与可见菜单提供查看、前后移动、编辑、复制、转项目、收藏、删除确认与组件材质设置。独立拖动柄避免与正文菜单竞争。原生触屏拖动由平台长按启动，落点上/下半区决定插入前/后；只有本地一次性nonce、当前完整 CardView.source、相同page/filter/query/sort/content generation/query IDs/preferences revision且查询已ready时接受。外部drop/过期视图拒绝，native拖放成功仅表示候选被接受，保存成功与重启回读分别验收。

`CardOrder.ets` 移植原 `{version:1,orders,manual}` 五个稳定页面。读取投影保留缺失ID，新ID追加；移动时只替换visible槽，隐藏成员不移动，并清理本次已移除ID。当前开发库上限256，较原10000收紧。候选完整复制；异步保存失败恢复旧orders/manual，Preferences缓存也回滚，无自动重试。回收站不参与manual，普通sort沿用全局状态。审阅发现的回收站绕过偏好busy/loading选择门已修正，旧写入失败不会覆盖保存期间的新排序选择。

## 本轮证据

- `hap-final-build.log`：最终候选构建通过；保留 Function.bind、系统控件容量和 PasteButton 静态权限提示。没有新增受限剪贴板读取权限。
- `arkts-model-tests.log`：实际 ArkTS 模型67/67通过，其中手动排序新增19，既有文字journal/查询/数据源/Markdown/粘贴48。涵盖隐藏槽、排序输入变化、新卡/删除、保存失败/恢复原子性、过期token/revision、切页、dispose和256完整候选。
- `readonly-final/result.json`：菜单修复前候选包的真实阅读、复制、关闭及重启无该业务卡journal，PASS。该目录仅保留早期候选证据。早期设备辅助误把同名搜索框/背景卡片当成目标，失败日志保留，失败不计通过。
- `readonly-release/result.json`：最终HAP再次实际阅读已有 `HMOS-markdown-20261005-A`、复制、关闭和重启后查看草稿列表，无该业务卡journal，1项PASS。`readonly-release/reading-markdown.png` 是最终包截图，标题、表格和关闭按钮已人工查看。该轮没有编辑或重新创建业务卡。
- `device-final/result.json`：六项流程PASS，三张唯一测试卡片 `HMOS-flow-20261005-A/B/C` 实际创建、按标题排序；最终菜单修复包确认前后移动、筛选隐藏槽保持、重启顺序恢复、同一业务身份正文修改、转项目CAS。创建发生在菜单修复前候选包，后续步骤在最终包继续，没有重seed或重发已确认操作。B身份 `37276789-82aa-42ad-aa76-0e49492b5464`，A `8c1176bd-e6d9-4558-bb1a-ecb3881ff991`，C `9827d318-4750-45ec-b735-c22dba961f15`。
- 设备辅助另停在保存提示覆盖卡片点击处，已先观察原正文确实保存，再关闭提示继续读回，没有重发edit。早期菜单的带参Function.bind生成裸函数，原生菜单接口拒绝；裸箭头也没有编译器包装，最终改为直接Builder调用，生成 `{builder:()=>CardMenu.call(...)}`。新包菜单、编辑和转换已实际通过。重装后过早读取一次得到非前台界面，守卫停止且没有执行手势；重新确认前台后继续。
- `interaction-final/drag-result.json`：最终HAP真实长按拖动柄，将B放在C之后、确认顺序C/B/A，并在进程重启后回读，2项PASS。`pages-result.json`：灵感页独立手动开关/顺序未覆盖概览页；正文长按打开实际菜单并进入阅读详情，2项PASS。原生drag使用UITest自带1500ms按住后移动，未用普通swipe替代。截图 `before-drag.png`、`after-drag.png`、`longpress-menu.png` 来自本包。
- `interaction-final/tasks-result.json`：最终HAP显式新增测试任务后弃掉未提交的文字草稿，阅读详情对同一TaskId勾选一次，再将阶段从计划中改为推进中；最新来源回读仍保留已完成任务，2项PASS。`task-detail.png`、`stage-detail.png` 来自本包，阶段截图已人工查看；短内容正常收缩，正文、任务与底部操作区可见。

## 未闭合范围

详情完整任务新增/重命名/重排仍通过显式编辑器，未全部直接放入阅读详情；附件显示与文件选择未接。拖动边缘自动滚动、源卡虚拟化移出视口、鼠标/键盘上下文手势、长文/全部主题/宽屏/IME/性能/拒权与实际偏好磁盘失败均未完整验收。未签名发布、ARM64真机和生产HUKS/owner/封存、captured S1/S2/正式未知跨进程恢复保持开放。

附件下一闭环已有具体原代码审计，见 `attachments-audit.md`：URI内容流→原durable import→草稿pin→重启恢复→已有卡正式保存→按修订读取/导出→移除当前引用。历史blob继续被事件保留，不能承诺即时回收空间。审计本身不是附件通路已实现。

仅提交/推送 `codex/ArkTsUI`，不合并或推送主线。完整持续目标仍开放。
