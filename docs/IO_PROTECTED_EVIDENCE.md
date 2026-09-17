# IO-C1：受保护 IO 请求／响应原件

状态：core 存储层已实现并通过验收（Store v17）。本层只保存**历史材料**：固定原件既不是发送许可、不是凭据存储，也不证明远端成功或用户收到结果。Broker 的当前实例绑定、唯一活跃执行与恢复核对属于 IO-C2，不在此文件范围内。

## 记录格式

`core/schemas/io_evidence.proto` 定义独立 Protobuf 记录，使用现有 LZ4 信封，magic 为 `MROWIOE1`、格式版本为 1，解压后最多 `io::MAX_JOB_BYTES + 4096` 字节。单份原件载荷上限为 `io::MAX_JOB_BYTES`（16 MiB）。

| 字段 | 含义 |
| --- | --- |
| operation_id / subject | 归属的命令与主体，必须通过 `identity()` |
| kind | `REQUEST` 或 `RESPONSE`，`INVALID_KIND` 拒绝 |
| request_sha256 | 该命令的原请求摘要（原件的绑定字段） |
| payload_sha256 / payload_bytes | 载荷摘要与精确长度，解码时重新计算核对 |
| payload | 原件字节本身 |

`Material::decode` 在分配前检查字段号、重复字段、线类型与长度，要求重新编码与原字节一致，并校验 `payload_bytes == payload.len()`、`payload_sha256 == SHA-256(payload)`。未知字段或语义必须走显式新版本。记录自身的 SHA-256（`digest`）用作存储键，但摘要不证明材料存在或远端成功。

## Store v17 与不变量

格式 17 新增三个对象：

- `io_evidence`：每个 `(operation_id, kind)` 至多一份原件，保存容器、摘要、精确字节数与请求摘要绑定。
- `io_evidence_kind`：`(operation_id, kind)` 唯一索引。
- `io_material_reservations`：发送前的逻辑字节预留，`(operation_id, kind)` 主键。

`verify` 强制以下闭包，违反即 `Integrity`：

1. 每份原件与每条预留都必须指向存在的、由同一主体持有的 `io_intents` revision 1 历史。
2. 原件的 `request_sha256` 必须等于该命令的 `request_sha256`；解码后的字段与列值逐一相符。
3. 同一 `(operation_id, kind)` 不能同时存在原件与预留（消费式转换，不是复制）。
4. 预留不得与终态（`Observed`／`CancelledBeforeDispatch`）历史共存；`OutcomeUnknown` 期间响应预留可合法保留，直到响应原件被接纳或历史终结。
5. v17 之前的库若已含这些对象必须拒绝，不静默修复。

迁移只新建空表；不重写任何既有 operations、原件、commit 或签名。较旧程序不能读取 v17。

## 发送前容量预留

`reserve_io_materials` 在跨越发送边界前为同一命令建立完整材料预留：请求按 `command.request_bytes`、响应按 `command.response_limit` 的容器上界计入。预留是 Store 内的逻辑配额，通过 `event_room` 从所有 outbox 写入者共享的字节额度中扣除，其他写入无法挤占；它不是物理磁盘空间保证，SQLite 真实满盘仍以 `StorageFull` 报告。相同 operationId 幂等不重复扣减，不同命令或不同主体拒绝。

`store_io_material` 在同一立即事务内消费对应预留并写入原件，因此任一时点 `(operation_id, kind)` 只计一次容量。请求原件允许在 `Prepared` 或 `OutcomeUnknown` 阶段接纳；响应原件只允许在 `OutcomeUnknown` 阶段接纳。请求原件必须与命令绑定的 `request_bytes`／`request_sha256` 完全一致；响应原件不得超过 `response_limit`。

跨越发送边界后，`OutcomeUnknown` 期间的历史可由响应原件或核对结果收尾：未接纳的原件槽位通过 `release_io_material_reconciliation` 显式释放（仅限 `OutcomeUnknown`、仅限从未接纳的 kind），已接纳原件不可删除；终态提交前必须没有残留材料预留（`Observed` 会拒绝，`CancelledBeforeDispatch` 自动释放未消费的预留）。已接纳的原件始终随命令历史保留。唯一活跃执行与核对顺序见 [IO-C2](PLUGIN_IO_EXECUTION.md)。

## 读取与缺失分类

`io_material(subject, operation, kind)`：

- 没有该主体可信的历史，或操作根本没有历史：返回 `Ok(None)`，与不存在不可区分。
- 主体持有历史但该 kind 的原件缺失：显式返回 `Error::EvidenceUnavailable`。摘要或签名不能替代它。
- 存在但不满足绑定：`Integrity`。

`io_material_reservation_usage` 暴露当前预留行数与字节数，用于界面与诊断。读取不授予任何回放或发送权限。

## 保留规则

原件为不可变历史：本格式没有删除或 GC 入口，保留期与命令历史一致。一旦需要按配额退休或删除，必须显式新增格式版本与独立验证器，不能改旧字节。签名只证明记录来源与完整性，不认证远端诚实、文件未变化或用户已收到结果。

## 崩溃与故障边界

`reserve_io_materials` 在插入后、提交前、提交后提供故障点；`store_io_material` 在写入原件后、提交前、提交后提供故障点；v17 迁移在提交前后提供故障点。真实 SQLite 进程退出验证覆盖：预留提交与否恰好一次、请求／响应原件提交与否恰好一次、迁移保持完整旧或新 schema 且原件字节不变。真实满盘通过 `PRAGMA max_page_count` 触发 `StorageFull`，验证删除预留与插入原件整体回滚、不产生半行。授权回调失败时预留与原件都不落盘。

## 仍未覆盖

正式出站／监听／服务发布声明与 Registry 授权、broker 当前实例与唯一活跃执行、实际后端效果核对、通用 guest IO、凭据注入、录制式重放与证据访问策略、跨进程证据容量退休、主应用 UI 与三语言 SDK 都未在本层完成。原文见 [IO 设计](PLUGIN_IO_DESIGN.md) 与 [路线更新](ROADMAP_UPDATE_2026-09-15.md)。
