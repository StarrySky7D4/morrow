# IO-C1：受保护 IO 原件与发送前容量预留

日期：2026-09-17。结果：**PASS_SCOPED**。范围为核心存储层：`morrow-core 0.1.9-test.50` 新增数据库格式 **17**。主应用版本、冻结 SDK 原件、其它平台与已发布归档均未改动；已推送 `track-a/w1-io-contract`（`d086eaf`），未发布。

本层的对象是**历史材料**：固定原件不是发送许可、不是凭据存储，也不证明远端成功或用户收到结果。Broker 的当前实例绑定、唯一活跃执行与恢复核对属于 IO-C2。

## 实际交付

- `core/schemas/io_evidence.proto`：独立 `morrow.io_evidence.v1` 记录，`REQUEST`／`RESPONSE` 两种原件，载荷上限 16 MiB。
- `core/src/io_evidence.rs`：严格编码／解码（分配前字段与长度检查、非规范编码拒绝、载荷摘要复核），以及用于预留的容器上界计算。
- `core/src/store/io_evidence.rs`：Store v17 表、不变量校验、发送前预留、原件接纳、读取与缺失分类、真实满盘回滚测试。
- `core/src/store.rs`：v16→v17 迁移（只新建表）、`event_room` 计入材料预留与已存原件、完整性闭包接入；所有旧格式上界检查升到 17。
- `core/src/store/io_intent.rs`：跨发送边界的材料预留闭包（`Observed` 拒绝残留预留，`CancelledBeforeDispatch` 自动释放未消费预留），跟随预留容量计入材料字节。
- `core/src/lib.rs`／`core/src/response.rs`：新增 `Error::EvidenceUnavailable`，运行期失败码暂映射现有 `Failure::Storage`，细化留待 IO-C2。
- 测试：`core/tests/io_evidence.rs`（9 项）、`core/tests/io_evidence_crash.rs`（4 项 + 1 子进程）、`core/src/io_evidence.rs` 内 5 项 codec、`core/src/store/io_evidence.rs` 内 1 项满盘。
- 设计说明：[受保护 IO 原件](../docs/IO_PROTECTED_EVIDENCE.md)；[意图记录](../docs/IO_INTENT_RECORDS.md) 状态同步更新。

## 不变量

1. 每份原件与每条预留都指向存在的、同一主体持有的 `io_intents` revision 1 历史。
2. 原件的 `request_sha256` 必须等于命令的 `request_sha256`；请求原件字节必须与命令绑定的 `request_bytes` 完全一致，响应原件不得超过 `response_limit`。
3. 同一 `(operation_id, kind)` 只能存在原件或预留之一，接纳即消费，不容复制计入。
4. 预留不得与终态历史共存；`OutcomeUnknown` 期间响应预留可保留至原件接纳或历史终结。
5. 旧格式库若已含 v17 对象必须拒绝；迁移不重写既有原件、commit 或签名。

## 验证证据

| 检查 | 结果 |
| --- | --- |
| 全量默认回归 `cargo test --offline` | 396 → **411 通过，0 失败**（新增 9 集成 + 5 codec + 1 满盘） |
| 故障注入全量 `cargo test --offline --features fault-injection` | **450 通过，0 失败**（含 4 组真实子进程崩溃边界） |
| 严格静态检查 `cargo clippy --offline --all-targets --features fault-injection -- -D warnings` | 无警告 |
| wasm32 库编译 `cargo check --offline --target wasm32-unknown-unknown --lib` | 通过；无新增警告 |
| 迁移 | v16→v17 提交前后崩溃各保持完整旧／新 schema，`operations` 原件字节不变；v17 降级为 v16 且保留新对象被拒绝 |
| 崩溃边界 | 预留插入后／提交前／提交后、请求原件与响应原件写入后／提交前／提交后各自“提交与否恰好一次” |
| 真实满盘 | `PRAGMA max_page_count` 触发 `StorageFull`，删除预留与插入原件整体回滚，无半行、无额度漂移 |
| 撤权 | `authorize` 回调失败时预留与原件都不落盘，`integrity_check` 通过 |
| 缺材料 | 有历史但无原件报 `EvidenceUnavailable`；非本主体、无历史报 `Ok(None)` |

最终日志：`build/io-c1-final-default.log`（411）、`build/io-c1-final-fault.log`（450）、`build/io-c1-b-clippy.log`；基线 `build/baseline-core-test.log`（396）、`build/baseline-core-clippy.log`。首轮实现与修复过程日志 `build/io-c1-a-*.log`、`build/io-c1-b-*.log` 保留，其中包含已修复的失败。

## 审查发现与修复

- `accounted` 的容量查询写成裸标量表达式，SQLite 无法作为独立语句准备，导致所有 outbox 写入失败；改为 `SELECT (...)+(...)`。
- `store_io_material` 未核对调用方声明的 `kind` 与记录编码的 `kind`，可能先写入再在完整性检查暴露；改为接纳前拒绝。
- 请求原件原先只绑定摘要字段，不核对载荷本身；现要求字节数等于 `request_bytes` 且载荷摘要等于 `request_sha256`，响应不得超过 `response_limit`。
- 预留闭包最初错误地要求“存在预留即不得有非 revision 1 历史”，会拒绝跨发送边界合法保留的响应预留；改为只禁止与终态共存。
- `reserve_io_intent_followup` 未计入材料字节，与 `event_room` 口径不一致；已统一。
- 历史 schema 模拟测试需要一并移除 v17 对象，否则会按设计被拒绝；相关测试夹具已更新。

## 产物与边界

数据库格式 17，可由 `Store::open_existing` 从格式 4–16 原子迁移，旧 test.1 数据仍不导入。核心归档无独立二进制产物变化；本次仅源码与测试。

仍未完成：正式出站／监听／服务发布声明与 Registry 授权、broker 当前实例与唯一活跃执行（IO-C2）、实际后端效果核对与 Unknown 处置（IO-D）、异步作业（IO-B2）、录制式重放与证据访问策略（ROAD-08-IO）、按配额退休与 GC、主应用 UI 与三语言 SDK（IO-E）。本层不表示插件网络或文件系统授权已经可用。
