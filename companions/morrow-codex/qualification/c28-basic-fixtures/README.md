# C28 固定合成夹具输入

`c28-basic-harness/src/formal.rs` 从本目录读取三个固定 Wasm 夹具。本目录不自动下载、构建或执行插件。

本次源码同步不包含 `morrow_codex_session_exec_guest_r2.wasm`。本地原资格夹具为 427258 字节，SHA256 `b1f0f44ad2bbe2abc89eee0ee971683419bf3407f370194f5557df57fa45799e`，其 data section 含构建机源码路径。原字节保留为本地证据，没有被修改、裁剪或重新打包后冒充原产物。

公开 Git 树仍缺此编译输入，因此不是自足的完整构建快照。后继需要从审核后的 session 合成 guest 源码，在独立环境以路径映射构建可公开的产物，并验证接口、行为和新哈希；新产物不能沿用旧字节资格。相关可复现构建入口和公开夹具验收仍待补齐。不要使用未知插件或空文件代替输入。

分支中原有的 proposal/process 夹具保持历史状态，本次没有重建或重新验收。完整本地载体的 Cargo 成功与公开 Git 树的完整构建是不同证据；本次公开树完整编译未运行。详见 [阶段 18 记录](../../../../reports/reconstruction-2026-10-09/windows-agent-sdk-c28-stage18.md)。
