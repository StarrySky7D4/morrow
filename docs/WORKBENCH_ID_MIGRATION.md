# 工作台稳定业务标识：接入边界调查

日期：2026-09-15。ROAD-03 调查及首轮接入：已分离导航／筛选／排序身份与显示文案，未修改业务协议或迁移资料。源码基线 `0e93e49f56dd2534db5a0cd8fb31e9d5c705dfa7`；下方完整迁移向量仍待后续逐项执行；本轮只验证导航／筛选／排序接入范围。

首轮验证见 [31 项 UI／查询及真实宿主测试](../reports/road-03-ui-identities.md)。

## 首轮实现

[workbench_ids.dart](../lib/plugins/workbench_ids.dart) 提供类型化页面、排序与一般／阶段筛选；[workbench_labels.dart](../lib/plugins/workbench_labels.dart) 仅负责显示。QueryCoordinator 用 WorkbenchV1 在唯一边界输出固定旧中文，标签不参与查询条件、operation 或持久内容。主界面的 key 使用稳定 ID，概览外观配置继续使用显式保留的 summary:旧中文键。

这不是完整国际化：没有新增语言选项，类别／阶段编辑仍使用旧值，Unknown 正文处理与数据迁移没有改变。验证包含 180 组 v1 请求字节对照、标签替换与重试、原 UI 回归和实际宿主重启后的旧操作查询。旧未知阶段及旧概览自定义参数完整加载未在本轮新增测试中覆盖，不能据代码未改扩大验收。

## 调查时的耦合

| 范围 | 源码 | 必须保留的语义 |
| --- | --- | --- |
| 页面／导航 | [main.dart](../lib/main.dart)、[workspace_pages.dart](../lib/pages/workspace_pages.dart) | 标题目前兼作 section，菜单文本兼作过滤／阶段命令；页面状态不属于 snapshot 正式保存内容，可先拆类型 |
| 旧资料 | [main.dart](../lib/main.dart) 的 Idea、toJson/fromJson | category／stage 原样保存，缺失阶段按类别补默认值；未知数据不能在显示时被覆盖 |
| 业务规则 | [Rust 工作台](../plugins/workbench/src/lib.rs) | 三种中文分类、八个分类内阶段；计划状态与完成待办联动；页面／排序也为中文业务值 |
| 正文持久化 | [properties.proto](../plugins/workbench/schemas/properties.proto)、[persistence.rs](../plugins/workbench/src/persistence.rs) | version=1 严格校验已知分类／阶段，保留支持范围内顶层和附件未知字段；不代表未知业务值当前已可编辑 |
| 查询意图 | [query_plan.rs](../workbench_host/src/query_plan.rs)、[query_capture.rs](../workbench_host/src/query_capture.rs) | 同 operation 核对原请求字节；更换翻译可能产生意图冲突，不能当成等价重试 |
| 历史内容投影 | [projection.rs](../workbench_host/src/projection.rs) | 版本化核验仍要求 v1 默认值“概览／全部／最近添加”，不能随界面语言更新 |
| 核心边界 | [content.proto](../core/schemas/content.proto) | 核心存 type_id、format_version 和 body；工作台业务阶段应属于业务扩展，不能加为核心全局枚举 |

## 分步实施

1. 在 Dart 中引入类型化页面、分类、阶段、过滤、排序 ID；显示文案独立。阶段过滤应显式关联 StageId，不用一个字符串同时表示通用过滤与阶段。现有 v1 请求通过适配器转换回固定中文，保证语言切换不改变请求字节、查询 operation 或历史语义。
2. 旧 JSON／PB v1 暂不重写。展示层区分已知 ID 与保留原值的 Unknown；未知值只读保留，不能自动归为灵感或未经新格式支持就保存。既有默认值集中为 v1 适配规则。
3. 新第一方工作台 v2 另设 schema、摘要、正文版本与查询／投影路由；保留 v1 解码和历史重放。公开 guest-v1-rc1 原件不动，不能仅修改现有 workbench.capnp 摘要就宣告兼容升级。
4. 迁移先在可恢复副本上预检，形成新提交和来源关系，保留历史原件与签名关联。新查询用新 operation，旧 Ready 查询继续按原协议核对，不能重新翻译历史请求。

ID 命名、类型和 wire 版本属于待定接口，本调查不冻结新协议。界面身份适配可先实现；正式 v2 合并与数据库／权限变更由主线集中整合。

## 需要执行的兼容向量

- 三分类、八阶段完整往返；旧分类“进行中”不能误解为阶段“推进中”。
- 新建项目缺省“推进中”，ToProject 使用“计划中”；完成全部待办、撤销及切离“已完成”的关联修改不能被统一默认值破坏。
- 五页面、通用过滤、阶段过滤、三排序在中英文下业务结果一致；标题排序继续遵循既有 UTF-16 规则，界面语言不能暗改排序规则。
- A→B→A 切换、语言切换时迟到响应、Unknown 同 operation 核对；仅改变显示不触发新查询或意图冲突。
- 非法分类／阶段组合、未知值及未知字段保留；旧 JSON、PB 附件、原件摘要、历史内容和查询证据重放不变。

优先复用 `plugins/workbench/tests/parity.rs`、`workbench_host/tests/query_plan_guest.rs`、`workbench_host/tests/content_projection.rs`、`workbench_host/src/query_capture/tests.rs` 与 `test/query_coordinator_test.dart`，并补显示切换的真实应用向量。导航／筛选／排序的第一步接入不等于类别／阶段正文迁移或 ROAD-03 整包验收完成。
