# ROAD-03 导航／筛选／排序身份与显示分离

日期：2026-09-15。结论：**PASS_SCOPED**，限定本轮 UI 身份与 v1 请求适配；不是类别／阶段正文迁移或完整国际化。

## 改变

[类型化身份](../lib/plugins/workbench_ids.dart) 包含 WorkbenchPage、WorkbenchSort、GeneralFilter 和 StageFilter，显示由 [WorkbenchLabelsScope](../lib/plugins/workbench_labels.dart) 提供。导航、选择、动画 key 和 QueryConditions 使用稳定身份；QueryCoordinator 仅在请求边界通过 WorkbenchV1 转为原中文令牌。

更换显示标签不会改变请求条件或新建 operation。概览组件仍用显式旧键 `summary:小项目` 等，避免把保存外观配置的身份随导航 key 改名。Idea.category／stage、stageMenu／changeStage、旧 JSON／PB、Rust 业务、schema 和历史原件未改。

## 验证

```powershell
flutter test --no-pub test/workbench_ids_test.dart test/query_coordinator_test.dart --reporter expanded
flutter test --no-pub test/widget_test.dart test/compact_settings_test.dart test/appearance_test.dart --reporter expanded
$env:MORROW_WORKBENCH_HOST = (Resolve-Path build/workbench-host/release/morrow-workbench-host.exe).Path
$env:MORROW_WORKBENCH_PACKAGE = (Resolve-Path build/workbench-host/bundle/workbench.morrowplugin).Path
flutter test --no-pub test/workbench_ids_native_test.dart --reporter expanded
flutter analyze --no-pub
```

**31 项通过**，其中：

- 身份／查询专项 **9 项**，包含 **180 组** v1 Cap’n Proto 请求字节对照、标签变化不重发、Unknown 原 operation 重试及同文案不同身份。[日志](../build/road03-ids-query-pass.log)
- 原 UI、窄屏设置、组件外观回归 **21 项**。[日志](../build/road03-ui-regression.log)
- 实际 Rust 宿主 **1 项**：先用旧中文字面字段保存查询意图，再通过类型化适配重复查询；删除当前内容后，旧 operation 仍返回原结果，新 operation 返回最新结果，关闭重开后再验证。[日志](../build/road03-native-query-compat.log)

最终 analyze 无问题、退出码 0；[日志](../build/road03-analyze-final.log)。独立只读复核未发现新增实质缺陷，已核对显示标签不进入查询身份、StageFilter 值相等和旧 summary 键。

真实宿主测试使用已有产物，没有重建宿主或 guest。下列摘要为测试后的只读补采，原件修改时间早于测试日志：

| 测试对象 | SHA-256 |
| --- | --- |
| build/workbench-host/release/morrow-workbench-host.exe | `85bb6c624b545dd4117c5593291fa6125ed8df56e730074a6f1acc010518e9a7` |
| build/workbench-host/bundle/workbench.morrowplugin | `14f2fc208631a606aff7ab11f472cf60f4f722068380e89a47b34d581416c4e1` |
| lib/plugins/workbench_ids.dart | `3cdaa3effa734337266147a63578e7bd684871b104d71459bf9f65423c22110f` |
| lib/plugins/workbench_labels.dart | `2e7dcd6fc6fbab74a428cf777577edd7f6fa1d916094ac970eb996f91bf2e349` |

最初的编辑／测试迭代失败日志仍保留在 build/road03-* 中，涉及编码处理、花括号 lint、常量 Map 测试及首帧夹具，最终针对性记录均通过。没有通过重建旧 Rust 原件解决兼容性问题。

## 未完成与未新验证

没有新增产品语言选项、语言包或核心消息契约。类别／阶段正文和编辑命令仍是原 v1 字符串；下一步需要独立版本化而非全局替换。未知项目阶段的显示回退是既有行为，本轮没有改变。

本轮未新增验证未知阶段持久化或旧概览自定义参数的完整加载；当前测试只证明已有已知 JSON 值和实际 componentId 保留。不能将这些范围扩大为全部旧数据迁移通过。没有重新构建 Windows 主应用，也没有 Web／Android 设备运行资格。

后续边界见 [工作台 ID 与迁移](../docs/WORKBENCH_ID_MIGRATION.md)；新旧协议、历史意图和投影原件继续分版本保留。
