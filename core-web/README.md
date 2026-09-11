# Morrow Web core — test.7

独立的第一方浏览器实验适配层，复用 `core::Store`、内容契约、附件事务和 HostPolicy。尚未接入 Flutter 工作台，也不承载第三方插件。

## 实际路径

普通 Dart JavaScript 的 `web.dart` → 页面 Rust/Wasm 编解码器 → Cap’n Proto 固定请求副本 → Dedicated Worker 的 Rust/Wasm → HostRuntime／HostPolicy → 同一 Store → SQLite OPFS VFS。修订使用 Dart BigInt／JS BigInt／Rust u64，不经过 JavaScript Number；存储载荷保持 Protobuf＋LZ4，附件保持原字节。

- 固定依赖 rusqlite 0.40.2、sqlite-wasm-rs 0.5.5、sqlite-wasm-vfs 0.2.0、wasm-bindgen 0.2.128，独立 Cargo.lock 纳入版本控制。
- 明确安装 `morrow-opfs` 命名 VFS，目录为同源 OPFS 下的实验池 `morrow-test7`，初始 64 个同步访问句柄。库默认内存 VFS 不被 Store 选择。没有 OPFS 时明确失败。
- Worker 独占池，同一 Rust/Wasm 实例只允许一个 BrowserStore，其他 Worker 争用同一池会失败。连接释放后可在同一 Worker 打开另一数据库；池句柄随 Worker 生命周期释放。不要把虚拟数据库文件当作独立普通 OPFS 文件直接改写。
- 原生默认 WAL／FULL 保留；浏览器选用 EXCLUSIVE／DELETE journal／FULL。业务写入仍走同一 IMMEDIATE 事务、操作去重和原子事件写入。数据库格式仍为 2。
- 浏览器 Worker 终止不代表 OPFS 文件锁立即释放。验证器仅对创建同步访问句柄失败进行最多 5 秒的重新打开尝试，每次关闭失败 Worker；不重放未知结果的写命令、不自动删库或退回内存。
- Web 适配当前单次暂存／导出上限 4 MiB，输入输出有复制；核心内部按 64 KiB 分块。完整 200 MiB Web 流式导入、配额恢复、浏览器持久存储授权与用户界面仍待实现。

`BrowserStore` 是可信第一方宿主入口：创建、导入、附件、读取等方法还没有通用插件授权分派。HostRuntime 将连接绑定到实例，重命名与摘要读取分别授权并由宿主单调时钟复核期限；读取没有正文与附件权限，其他方法仍是本地管理入口。重命名与摘要查询的 Worker 请求／响应都使用 Cap’n Proto v3；其余测试控制字符串、诊断文本、标量返回及 CDP JSON 仅用于资格探针，不是正式插件运行期协议。页面和 Worker 都加载编解码模块不代表两套持久化权威：数据库仅由 Worker 的 Store 持有。

## 复现

需要 Rust wasm32-unknown-unknown 目标、Cap’n Proto compiler 1.4.0、LLVM clang／llvm-ar、Dart 3.12、Node 和 Chrome。先安装固定绑定生成工具：

```powershell
cargo install wasm-bindgen-cli --version 0.2.128 --locked --root build/tools/wasm-bindgen
pwsh -File tool/verify_web_storage.ps1 -Clang 'C:\Program Files\LLVM\bin\clang.exe' -Ar 'C:\Program Files\LLVM\bin\llvm-ar.exe'
```

`-WasmBindgen`、`-Dart`、`-Node` 可指定路径；非 Windows 主机须指定对应 wasm-bindgen 可执行文件。CHROME_BIN 可指定浏览器。脚本不自动安装工具链，所有产物位于 build/core-test.7；main 为默认构建，fault 显式启用仅供测试的中断钩子。`web/` 是隔离资格探针，不应当作为生产网站发布；尤其不能发布 fault 目录或测试控制入口。

验证包括普通 Dart JS 编译及原生 Cap’n Proto 向量、撤权／到期、最大 UInt64 持久化后原生回读、原件字节、历史保留、事件容量回滚、单连接及跨 Worker 竞争、缺少 OPFS、跨页面恢复、28 个中断点（7 个重命名、4 个暂存、14 个附件发布／移除、3 个回收），以及摘要读写能力分离、撤权／过期和响应关联。只验证实际 Chrome 版本，不代表其他浏览器、断电、平台分发或插件隔离。大文件、配额耗尽、真实浏览器进程退出和系统故障仍待验证。

依据：[rusqlite](https://docs.rs/rusqlite/0.40.2/rusqlite/)、[SQLite Wasm Rust 适配](https://github.com/Spxg/sqlite-wasm-rs)、[SQLite OPFS 持久化](https://sqlite.org/wasm/doc/6ee03a052f/persistence.md)。实际证据见 [test.7 记录](../reports/0.1.9-test.7-refactor.md)。
