# Windows channel 夹具释放复验（W08）

2026-10-03，基线 commit `63f38d4a8a5bf453248dc7532197bee6980dc86f`，本地未提交开发线。本轮只使用普通合成临时库与无密钥数据。

首次真实离线 Release 测试为 executor 9、controls 15、faults 21，共 45 个不同方法通过，failed/ignored/filtered 均 0。清理检查发现 controls 留下 51 个文件、faults 留下 69 个文件，合计 14,095,152 bytes；这些原始失败证据与残留保持原样。

最小修复将 `channel_controls.rs` 和 `channel_faults.rs` 的 Fixture 内 TempDir 字段置于其他字段之后，使持有库文件的资源先释放，再清理目录。初始化、测试体、断言、预算与超时不变。应用后仅复跑 controls 15 与 faults 21：两条 Cargo 命令实际 exit0，36 个原方法通过，每个新临时目录均零普通文件残留；旧 120 文件逐路径哈希保持。executor 9 未重复，不把复跑计成新增 36 种能力。

`channel_catalog_native_test.dart` 的临时目录前缀统一为 `morrow-external-channel-product-ui-014-`，符合现有 `external_plugin_native_test.dart` 的限定 cleanup guard；关闭 transport 后再清理的顺序保留。该 Dart/GUI 测试本轮未执行，原 catalog available/生产 channel 资格仍未补证。

命令形态：`cargo test --manifest-path plugin_runtime/Cargo.toml --locked --offline --release --features packages,fault-injection --test channel_controls -- --nocapture --test-threads=1`，faults 同形。实际绝对 argv、工具、目录、输入/产物哈希与原始 stdout/stderr 位于外置阶段证据。

W08 green ZIP：`w08-channel-cleanup-green-evidence.zip`，SHA256 `6cc2743390ad5938f2ef1ed10fc881f88fa98ae2720012f3a623b83cae801ce8`。fixture 资源清理通过不等于生产 G01、旧原 channel guest 或整个 SDK 冻结；未应用 diagnostic-only 补丁或修改预算。
