# ROAD-07 / IO-C 候选契约验收

结果：**PASS_SCOPED**。本轮新增 `core/schemas/io_intent.proto`、`core/src/io_intent.rs`、构建入口与独立 `core/tests/io_intent.rs`。完成有界二进制格式、准确命令匹配、前驱绑定和不可变状态转换候选；未接入 Store 或任何实际 IO。

## 实际验证

| 检查 | 结果 | 日志 |
| --- | --- | --- |
| 离线锁定依赖的核心 Release 检查 | 通过 | `build/io-intent-check.log` |
| 独立专项测试 | 10 通过、0 失败、0 忽略 | `build/io-intent-tests-first.log` |
| 核心库严格 Clippy | 通过，告警视为错误 | `build/io-intent-clippy.log` |

专项覆盖全部命令字段、状态／来源／修订矩阵、准确前驱、终态拒绝变更、Unknown 重新解码仍只允许核对，以及损坏信封、LZ4、长度、未知／重复字段、错误线类型和非规范 Protobuf。语义负例重新计算合法外层摘要，确保进入语义校验。

独立测试另证明：单条记录中伪造但格式合法的前驱摘要能够通过解码，却无法通过原记录的 successor 校验。因此解码不被宣称为历史真实性证明。

```powershell
cargo check --offline --locked --release --manifest-path core/Cargo.toml --target-dir build/core -j 2
cargo test --offline --locked --release --manifest-path core/Cargo.toml --target-dir build/core --test io_intent -j 2
cargo clippy --offline --locked --release --manifest-path core/Cargo.toml --target-dir build/core --lib -j 2 -- -D warnings
```

## 范围与下一验收门槛

这不是持久意图、真实崩溃恢复、签名审计、实际文件／网络调用、完整 IO 或 SDK 稳定的通过证据。后续须完成现有 Store 的原子接入、证据预留、签名与快照闭包，再以真实子进程崩溃验证发送边界和 Unknown 恢复；详见 [设计](../docs/IO_INTENT_RECORDS.md)。

该增量发生于 test.52 Windows 国际化预览归档之后，未重新构建或替换该预览 ZIP，也未提交、推送或发布 Release。预览仍以其固定源码归档和 SHA-256 清单为准。
