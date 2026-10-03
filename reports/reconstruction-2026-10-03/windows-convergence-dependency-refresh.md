# 汇合后 Windows 原冻结依赖复验

2026-10-03T07:38:28Z–07:38:51Z，在合入 Linux 17 路径并保留 Windows 本地增量后的源码上，选取原 `sdk_frozen_dependency` 三方法进行一次真实 Windows x64 Release 复验。原 Rust/C/C++ dependency guest 与原 rust-provider 未重建、重打包、重封存或修改 pin。

实际命令为 `cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --target-dir <external runtime-only target> --release --features packages --test sdk_frozen_dependency -- --nocapture --test-threads=1`，使用既有 Rust 1.95.0/MSVC/Capnp 工具与批准的离线缓存；复用原 Runtime 独立 target，所有本轮日志和 TEMP 使用新目录。

| 原方法 | 实际结果 |
| --- | --- |
| frozen_rust_dependency_binary_preserves_routing_and_authorization | ok |
| frozen_c_dependency_binary_preserves_routing_and_authorization | ok |
| frozen_cpp_dependency_binary_preserves_routing_and_authorization | ok |

Cargo 原始 exit0，3 passed，failed/ignored/filtered 均0，无 zero-match。方法使用既有 `dynamic_dependencies::run` → `SharedObjects::publish` → Windows `FrozenRegion` 的冻结输入路由及授权断言；不把直接 transform、Linux 测试或编译排除当此路线证明。

746 项受保护源码／构建输入清单、SDK327、57冻结原件、工具与实际 HEAD/index 前后保持；测试 exe/PDB 被 Cargo 合法重新生成，变化明确记录，未宣称测试产物字节恒同。新普通合成 TEMP 零文件残留。环境由空字典按24项公开构建/系统字段白名单组成，不继承 MORROW/CL/LINK/RUSTFLAGS/wrapper 或凭证字段。

driver、Cargo 与实际 test child 的直接 HANDLE token 采样为 elevated high，查询前后 STILL_ACTIVE259。这次3方法不继承 W10 受限 medium token 资格，也不表示普通桌面用户资格。未初始化 protected Storage/Session/DPAPI，GUI、其他39项全新运行及完整SDK冻结保持本轮未运行范围；之前同输入42项证据按历史身份保留，不重复相加。

真实 argv、工具与输入哈希、原始 stdout/stderr、三个方法、实际 child token、重新生成产物差异及前后记录保存在外置 `w11-convergence-frozen-dependency/execution-001`。本结果属于合入后新增的限定 Windows 运行，原 W11 仅文件应用 receipt 的新执行数0仍保留其时间范围。未提交／推送／CI／发布。
