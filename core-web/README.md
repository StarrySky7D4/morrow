# Morrow Web core — test.10

独立的第一方浏览器实验适配层，复用 `core::Store`、内容契约、附件事务和 HostPolicy。尚未接入 Flutter 工作台，也不承载第三方插件。

## 实际路径

普通 Dart JavaScript 的 `web.dart` → 页面 Rust/Wasm 编解码器 → Cap’n Proto 固定请求副本 → Dedicated Worker 的 Rust/Wasm → HostRuntime／HostPolicy → 同一 Store → SQLite OPFS VFS。修订使用 Dart BigInt／JS BigInt／Rust u64，不经过 JavaScript Number；存储载荷保持 Protobuf＋LZ4，附件保持原字节。

- 固定依赖 rusqlite 0.40.2、sqlite-wasm-rs 0.5.5、sqlite-wasm-vfs 0.2.0、wasm-bindgen 0.2.128，独立 Cargo.lock 纳入版本控制。
- 明确安装 `morrow-opfs` 命名 VFS，目录为同源 OPFS 下的实验池 `morrow-test10`，初始 64 个同步访问句柄。库默认内存 VFS 不被 Store 选择。没有 OPFS 时明确失败。
- Worker 独占池，同一 Rust/Wasm 实例只允许一个 BrowserStore，其他 Worker 争用同一池会失败。连接释放后可在同一 Worker 打开另一数据库；池句柄随 Worker 生命周期释放。不要把虚拟数据库文件当作独立普通 OPFS 文件直接改写。
- 原生默认 WAL／FULL 保留；浏览器选用 EXCLUSIVE／DELETE journal／FULL。业务写入仍走同一 IMMEDIATE 事务、操作去重和原子事件写入。共享核心格式已升至 5，事务迁移格式 4，拒绝更旧格式；OPFS 使用独立 morrow-test10 命名空间，不自动迁移旧数据。
- 浏览器 Worker 终止不代表 OPFS 文件锁立即释放。验证器仅对创建同步访问句柄失败进行最多 5 秒的重新打开尝试，每次关闭失败 Worker；不重放未知结果的写命令、不自动删库或退回内存。
- Web 适配当前单次暂存／导出上限 4 MiB，输入输出有复制；核心内部按 64 KiB 分块。新增附件读取每包至多 32 KiB，按预期修订解析当前引用，接收端须完成整件 SHA-256 校验后发布。完整 200 MiB Web 流式导入、配额恢复、浏览器持久存储授权与用户界面仍待实现。

`BrowserStore` 是可信第一方宿主入口：创建、导入、附件写入等方法还没有通用插件授权分派。HostRuntime 将连接绑定到实例，重命名、摘要读取、操作结果查询与附件读取分别授权并由宿主单调时钟复核期限；摘要读取没有正文与附件权限，附件授权精确到卡片和附件，其他方法仍是本地管理入口。重命名、摘要、操作结果查询与附件读取的 Worker 请求／响应都使用 Cap’n Proto v6；其余测试控制字符串、诊断文本、标量返回及 CDP JSON 仅用于资格探针，不是正式插件运行期协议。页面和 Worker 都加载编解码模块不代表两套持久化权威：数据库仅由 Worker 的 Store 持有。

## 复现

需要 Rust wasm32-unknown-unknown 目标、Cap’n Proto compiler 1.4.0、LLVM clang／llvm-ar、Dart 3.12、Node 和 Chrome。先安装固定绑定生成工具：

```powershell
cargo install wasm-bindgen-cli --version 0.2.128 --locked --root build/tools/wasm-bindgen
pwsh -File tool/verify_web_storage.ps1 -Clang 'C:\Program Files\LLVM\bin\clang.exe' -Ar 'C:\Program Files\LLVM\bin\llvm-ar.exe'
```

`-WasmBindgen`、`-Dart`、`-Node` 可指定路径；非 Windows 主机须指定对应 wasm-bindgen 可执行文件。CHROME_BIN 可指定浏览器。脚本不自动安装工具链，所有产物位于 build/core-test.10；main 为默认构建，fault 显式启用仅供测试的中断钩子。`web/` 是隔离资格探针，不应当作为生产网站发布；尤其不能发布 fault 目录或测试控制入口。

验证包括普通 Dart JS 编译及原生 Cap’n Proto 向量、撤权／到期、最大 UInt64 持久化后原生回读、原件字节、历史保留、事件容量回滚、单连接及跨 Worker 竞争、缺少 OPFS、跨页面恢复、47 个中断点（7 个重命名、5 个暂存、14 个附件发布／移除、3 个回收、18 个工作区／视图／草稿修改），100000 字节附件经普通 Dart JS 以 4 包读取并用共享校验器验证整件摘要，检查附件授权／撤权／到期及读取期间修订变化，以及摘要读写能力分离、受限操作结果查询、撤权／过期和响应关联。只验证实际 Chrome 版本，不代表其他浏览器、断电、平台分发或插件隔离。大文件、配额耗尽、真实浏览器进程退出和系统故障仍待验证。

依据：[rusqlite](https://docs.rs/rusqlite/0.40.2/rusqlite/)、[SQLite Wasm Rust 适配](https://github.com/Spxg/sqlite-wasm-rs)、[SQLite OPFS 持久化](https://sqlite.org/wasm/doc/6ee03a052f/persistence.md)。实际证据见 [test.10 记录](../reports/0.1.9-test.10-refactor.md)。

操作结果查询返回 locallyCommitted 或 absentSnapshot，缺少查询授权时不透露操作存在性。原生 morrow_host_* FFI 管理接口不编译进入 Wasm；Web 使用浏览器宿主持有的 Connection 和单调时钟，不能调用原生路径打开文件。

## 工作区与草稿管理入口

test.10 提供 workspace_local、placement_local、layout_local、draft_local／draft_save_local 和 record_revision_local，复用 Rust 的记录事务与共用 outbox。它们只供可信宿主与资格探针，不能作为第三方插件绕过授权的入口；没有通用运行期记录命令。

草稿创建携带固定的基准卡片容器，重试必须使用同一容器和正文；不在每次重试时重新读取卡片生成命令。浏览器探针验证正式卡片修订变化后原创建命令仍幂等。草稿正文复制限制 4 MiB；草稿提交、附件、生产编辑 UI、列表及全平台资格仍待实现。

故障探针的工作区／视图／草稿矩阵使用独立 morrow-test10-record-tests 测试池，仍限 64 个槽位。扩展矩阵曾在同一池达到 64／64 时被 SQLite CannotOpen 拒绝；分离测试数据集避免场景数量耗尽池，不增加默认应用容量、不删数据库或退回内存。仅 fault 构建包含该测试安装入口与 SQL／池占用诊断，默认构建不导出它。此资源边界不代表浏览器磁盘配额耗尽已验收。

核心 test.16 已提供 open_opfs_audited 与原子封存 API，但 BrowserStore 尚未绑定可信审计身份或暴露封存流程。普通 OPFS 存储回归不等于浏览器审计端到端验收。
