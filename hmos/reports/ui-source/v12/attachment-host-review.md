# dev.12 附件主机验证

2026-10-05，Windows 主机，隔离 `hmos-development.sqlite`，未操作应用数据库或设备。

本轮实现移植原 `workbench_host/src/editor_draft.rs` 的 `StoredAsset`、附件 pin、包含 blob 的预算计费和 `consumed_imports` 清理规则。允许来源资产 0、持久导入 2、同草稿前代 pin 3；来源 1/4、captured predecessor、parent/retirement lineage 继续明确拒绝。此处是开发适配，不继承原生产 host 测试资格。

JSON 草稿请求的选择清单位于 `draft.assets`；UI 快照的 `values.assets` 参与深复制、完整相等比较、脏状态和原请求冻结。回包保留选中身份、别名、pin、名称、media type、十进制字节长度、SHA256 和 consumed import operation。`ensureConfirmed()` 仅在显式操作中建立 owner，单纯打开未修改卡片仍不写草稿。

当前 publication 校验准确的当前草稿 generation/save operation、完整基线 source 和每个 pin 的完整流式字节。已证明 SAME business operation 提交的重试使用 `published_assets_history` 重建原 immutable draft metadata，再交核心比较完整历史业务 command。历史路径不重新发布字节、不用于新 mutation；新 mutation 仍经过当前 draft/pin 核验和业务 `VersionedContentChange` 完整 source CAS。

验证结果：

| 证据 | 结果 | 覆盖范围 |
| --- | --- | --- |
| `editor-draft-host-tests.log` | 24/24 PASS | 附件 pin 跨重启、origin 2/3 复用、预算、metadata、跨 owner 拒绝；清理失败前后 effect 与 exact retry；原 source CAS/历史回执 |
| `editor-draft-model-tests.log` | 25/25 PASS | 实际 ArkTS 模型的自动保存、别名和身份深复制、20 个附件、UTF-16、恢复只读、晚回包、unknown 原 JSON 显式重试、metadata 替换拒绝 |
| `attachment-integration-host-tests.log` | 3/3 PASS | 实际 Engine JSON + Read/Write；现有/新卡完整流程、98,321 字节跨 32KiB 与空文件、移除引用保持其他字段/TaskIds、原请求跨 raw 世代和 discard 重试、故障边界、256 卡容量/查询排除私有 journal/import |
| `attachment-host-check.log` | PASS_SCOPED | 独立新 runner 运行同一组真实 Engine 场景；删除原 spool、重新打开隔离数据库后仍可核验并导出 |

独立 fixture：`attachment-host-fixture-b890dd316470486599bcbbb8baf98ebd`。runner 与集成测试共享场景，不能作为两组独立产品验收重复计数。

Unix runner 已提供 prepare 成功/无效输入/同 inode 拒绝、64 次无效 import schema 与 export 无效 schema 的 transferred FD 关闭和计数检查。Windows 本次执行明确为 `NOT_RUN non-Unix host`；不能据此宣称 OHOS FD ownership、系统选文件器、ArkUI 界面或设备验收通过。生产 HUKS/audit/capture S1/S2、完整 HTML/Office、跨段连续全篇选择的既有资格边界没有改变。

来源字节记录：原 `workbench_host/src/editor_draft.rs` SHA256 `B4ADAFD9EDCA7ECBA9E4DD3A23963F226E3009233914D62C16630A1C7AC0DA28`；原 `schemas/editor_draft.proto` SHA256 `8F80DB114BCC480D9553AE60916170B1982C9CDE0D0452753A35B491279FDDDC`。

本轮记录时适配源码 SHA256：

- `rust/src/editor_draft.rs`: `49410A0892F3673716552FDB5C8BFE4A9188BF21109C31F850E32B349F43A500`
- `rust/src/draft_bridge.rs`: `EE7FDE6B0A6CBA9C62D6FDFC05F4CA53AF92877C74E410BBD9E25112DEE6A6FB`
- `rust/src/attachment_integration_tests.rs`: `8AF4193DDE1BFA45C6F7DB08CD9333C55A340B1E1DC8B048F10651E92D6E7267`
- `rust/src/bin/hmos-attachment-check.rs`: `2DB07F060F60ADE8EFE65EAD0A07D189D5AC040B5E2528DD8DA92DA749E507D3`
- `entry/src/main/ets/model/EditorDraft.ets`: `CAA435F968CF5B945E463508DAF84B04D7A56C594EDF653C529DDA3F2173A0F5`
