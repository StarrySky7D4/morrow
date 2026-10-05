# Changes metadata SDK 接入与能力发现

`changes-metadata-v1` 是独立实验扩展，使用原 `channel-v1` 的 Events / Receive / ACK 传输，支持宿主批准的有限固定卡片集合。它不改变 SDK327、冻结原件或旧 channel wire，不提供内容读取权限、长期 watch 或自动同步。

## 查询选定宿主

使用仓库工具，或已独立校验的 source-only 诊断包查询明确选择的可信宿主：

```text
python tool/morrow_sdk_diagnostics.py profiles --host PATH_TO_TRUSTED_HOST
python tool/morrow_sdk_diagnostics.py preflight NEW_PACKAGE.mplugin --host PATH_TO_TRUSTED_HOST
```

在 `profiles` 中按 `id == "morrow.channel.v1"` 查找 channel 记录，再读取可选的 `payload_discovery`。该对象有自己的 `schema_version: 1`；缺省只代表宿主未发布这项信息，不能推断有生产绑定或授权。

| 字段 | 当前含义 |
|---|---|
| `id: morrow.changes-metadata.v1` | 已知实验 payload profile |
| `required_features` | `channel-v1` 与 `changes-metadata-v1`；完整包还须遵守 transform handler、Events-only 和空权限组合规则 |
| `payload_contract.version / sha256` | version 1；规范原字节 SHA256 `07fc0dc4c48eb17f85309207a441d6a07f3fb8e06aab5e2ce337a4b3c98f9089` |
| `wire_limits` | header 150、payload 最多 662、两个 ID 各最多 256、cursor 32 字节 |
| `count_limits.cards` | 一个批准集合最多 32 个、至少 1 个卡片 |
| `runtime_static_preparation_supported` | 当前宿主可做静态 package / import / entry / 声明校验；不会运行 guest |
| `native_source_adapter_compiled` | 原生 source library 是否编译存在；不证明 protected owner 或平台产品资格 |
| `native_prerequisites` | managed broker、当前 receiver-specific approval、精确 package binding 和原 live Store binding |
| `workbench_routes / production_public_binding_available` | 当前为空／false；工作台生产批准入口仍未接通 |
| `authority / automatic_run_available` | none／false；发现信息不签发 grant，也不启动任务 |

新消费者在字段存在时严格检查版本、摘要、限额、类型、批准前提和权限边界；未知协议或声称生产可用的矛盾信息被拒绝。原有 base / channel envelope 和四类 IO extension discovery 保留，C02 工具能忽略这项可选信息，C03 工具也能读取没有该字段的宿主。

## 三语言 SDK

Rust 库与 C / C++ 头文件位于 `extensions/changes-metadata-v1`，不属于冻结的 SDK327 分发。Rust codec 和 C ABI 校验 payload；C++ 包装使用同一 C ABI。真实 guest 通过原 Events endpoint 消费 metadata，验证 scope、epoch、cursor、sequence，再提交原 ACK。

准备新测试 guest 可使用 `tool/prepare_changes_metadata_guests.py`。原 WASI SDK 布局保持可用；Windows 安装 LLVM 与 sysroot 分离时，必须同时提供 `--clang`、`--clangxx`、`--sysroot`，任一缺失就拒绝。输出和两个 Cargo target 必须是明确的外部目录；工具不会下载依赖、覆盖输出或重建冻结原件。C / C++ guest 链接单个合并 Rust archive，避免同时链接两套 Rust runtime。

元数据包含批准集合中的 card ID、operation ID、revision 和完整 Card.encode() 摘要。摘要会暴露相等性及猜测确认能力，不是零信息泄露。它不直接包含正文或路径；后续内容读取必须另获当前授权。ACK 数量、业务效果、producer 退出和真实 join 是不同事实，关闭／恢复不能自动重放。

## 当前验收与后续

Windows C02 的普通合成 Store、有限 source 与三语言新 guest 已限定通过；C03 新增只读发现和静态准备兼容资格。见 [Windows 复验](../reports/reconstruction-2026-10-05/windows-sdk-revalidation.md)与[C03 接入验证](../reports/reconstruction-2026-10-05/changes-sdk-discovery.md)。

protected owner、生产 UI / catalog 绑定、长期 watch、retention / gap / 跨进程完整恢复、其他平台和整个 SDK 冻结仍 OPEN。独立WS/SSE类型库与源码分发、目录/blob载荷库及Windows原owner目录队列已有各自限定记录；这些增量不关闭生产通道或完整同步门槛。下一项C08私有secret factory仅处设计阶段，完整范围与后续实现见 [项目状态](PROJECT_STATUS.md)及[下一门槛](../reports/reconstruction-2026-10-05/sdk-next-gates.md)；发现信息不能替代运行和授权。
