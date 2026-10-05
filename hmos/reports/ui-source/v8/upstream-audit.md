# 2026-10-05 Windows 参照版本审计

本报告基于实际工作树、Git 差异和文件 SHA-256 的只读检查；未执行 Windows 测试或构建，不继承其测试通过结论作为 HMOS 验收。

| 工作树 | 当前 HEAD | 应用源码版本 | 已跟踪工作区状态 |
|---|---|---|---|
| `build/io-safety-refactor` | `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` | `0.1.9-test.57+61` | `sdk/rust/contracts/service_resources.capnp` 有在途修改；`core/`、`lib/`、`pubspec.yaml` 未改 |
| `build/win-cloud-20261005` | `772466177fe589cee53bc633e69f411c34610104` | `0.1.9-test.58+62` | `network_node_stream_001/Cargo.lock`、`plugin_runtime/Cargo.lock`、`plugin_runtime/Cargo.toml`、`plugin_runtime/src/directory_io.rs`、`plugin_runtime/src/io_jobs/directory_commands.rs` 有在途修改；`core/`、`lib/`、`pubspec.yaml` 未改 |
| `build/windows-sdk-reconstruction` | `20669f671152972470340a65eac3458dd2f61b4d` | `0.1.9-test.58+62` | 已跟踪文件干净 |

状态按 `git -c core.longpaths=true status --porcelain=v1 --untracked-files=no` 检查。未开启 `core.longpaths` 的初次清单误报大量第三方长路径删除，不能据此判断源码删除。三工作树及适用祖先目录未发现覆盖本次源码范围的 `AGENTS.md`；`companions/` 内第三方规则不适用于本工程。工作树和原跟进会话仍在推进，以上为本次观察而非冻结发布版本。

## 自 b9225f64 以来的变化

- `b9225f64` → `925fb8ca` 的 `lib/main.dart` 仅新增文件变更/恢复设置项标题映射（第 3137 行附近）。`lib/workspace_viewport.dart`、`lib/stable_masonry_grid.dart` 和 `plugins/workbench/src/query_v2.rs` / codec / TaskId / 卡片逻辑没有提交差异；`925fb8ca` → `77246617` 同样没有普通工作台布局变更。不能把应用 test.58 版本号当成新 UI 设计证据。
- `lib/main_rust.dart` 接入独立 supervisor、通道退出观察、`recoveryRequired` / `closingUnconfirmed` 页面和所有者显式恢复。`workbench_host/src/workbench_supervisor.rs`、`workbench_supervision.rs`、`owner_recovery.rs` 是对应宿主变化。
- `core/src/file_content.rs`、`file_content_receipt.rs`、`file_mutation.rs`、`file_effect/`、`channel.rs`，以及 `workbench_host/src/mutation_*`、`channel_tasks.rs` 和 Flutter 对应管理页是新文件变更/通道能力。较新汇合分支再增加 changes metadata、Linux 存储/VFS、网络和目录 SDK 底座。它们需要平台权限、预算、生产 owner 与生命周期适配，不能仅拷贝页面视为功能接入。
- 当前 `win-cloud-20261005/docs/DEVELOPMENT_BOARD.md` 记录 C07 的有界目录生命周期资格，仍明确完整 SDK26/G04、生产 protected owner/GUI、picker 祖先来源等未闭合。原跟进会话的当前紧凑快照为 `inProgress`，最新消息报告新测试及目录/插件/网络回归通过并正在整理可恢复交接包；本次未复验该消息中的运行证据，不复制其通过数量作为 HMOS 结果。

## 可直接复用的查询业务块

以下 SHA-256 同时核对 `hmos/shared/` 与三个参照工作树，四份文件逐字节相同：

| 相对源码路径 | SHA-256 |
|---|---|
| `plugins/workbench/src/query_v2.rs` | `1A36C8C55BFE90EC127CEABD61EE1E1FFCD28555E50B09BC5F90DBD4AF617837` |
| `plugins/workbench/src/query_v2_codec.rs` | `0FF6287236A4A990848AA23DB0B076C8241058B4F0E2A94AD2F716EF48F39613` |
| `plugins/workbench/src/tasks_v2.rs` | `43849B828B2C6F4605EA8FE677CBB4E95A64FCFB68EEC4639346556E9947C70D` |
| `plugins/workbench/src/cards_v2.rs` | `ED92F85AF871A1AF28B6BD680217DDE9F39E1A8CB5701F5DE15A0EA04AC86CED` |

调度参照 `build/io-safety-refactor/workbench_host/src/query_plan_v2.rs` 的 SHA-256 为 `D88DC19EFE7A07C3F9D62DD7995C6E78E81BBDCA7595C585FA0C3FAA13E885E9`。它独立于存储和权限，适合本轮移植；目前不在 HMOS 的冻结来源清单，应保存独立来源记录而不覆盖旧清单。

具体对齐目标：

- `query_v2.rs:155` 起的 V2 过滤覆盖标题、描述、假说、结论和附件名称，阶段取显式字段；待办覆盖未完成及歧义项，附件按 kind 判断。当前 HMOS `Index.ets:139` 起只筛标题/描述、硬编码附件无结果，与原语义有差距。
- `query_v2.rs:201` 的标题排序按 UTF-16；收藏排序稳定保留输入同类顺序。当前 ArkTS `localeCompare` 不能保证同一顺序。
- `query_plan_v2.rs:163` 起按严格升序唯一 ID 读取候选、按 codec 字节与 128 条预算拟合分段过滤，再反转 ID 顺序并执行稳定分段排序/归并。第 215 行的反转是历史“最近添加”协议，未读取创建时间；该事实必须保留。
- `query_plan_v2` 的 `Backend::invoke` 抽象可由 HMOS 调用已有纯 Rust codec/业务执行器；这种接入证明查询语义和调度复用，不证明 Wasm guest、权限或完整快照来源资格。原模块文档也明确这些边界。

## 共享核心与平台界限

`hmos/shared/reference.json` 固定 228 个来源文件，基准为 `ddd9cc8e` 且包含当时未提交内容；本次与 `io-safety-refactor` 比较有 50 个既有路径的字节差异。不能以工作树 HEAD 或应用版本替换原冻结快照。

HMOS 冻结 `core/src/store.rs:48` 为 schema 21，较新 Windows `core/src/store.rs:71` 为 schema 24。整体升级会引入文件内容/回执与通道相关迁移、schema 验证、审计事件种类和平台依赖，必须单独审查和验证，不能作为查询同步的附带覆盖。

Windows `workbench_host/src/lib.rs:718–775` 的密钥恢复、快照恢复和活动内容库控制均显式拒绝非 Windows。新增 Linux owned VFS 也不等于 HMOS protected Store；`core/src/store.rs:457–479` 仍拒绝未完成的 Linux protected admission。当前 HMOS 使用独立 `hmos-development.sqlite`，没有 HUKS、生产身份/单 owner/封存/恢复保证，本轮查询复用不扩大该资格。

dev.7 已有懒加载瀑布流、卡片修订刷新、16 张记录及 880/1488 vp 设备验证；dev.8 的查询功能、构建、设备结果尚未在本审计阶段验证。完整 UI/功能追平目标保持，后续设备证据必须来自实际 dev.8 产物。
