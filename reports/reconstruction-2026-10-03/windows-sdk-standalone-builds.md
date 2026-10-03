# 独立 SDK 原模板真实编译（W09）

2026-10-03，从已验证的 source-only SDK ZIP 解出独立开发目录，在 SDK 之外创建 Rust、C、C++ task 项目。没有补入 Core、HostRuntime 或原生 host 产物。原工具的 `--sdk-only` 与 `--require-sdk-lock` 均保留；项目创建按原模板生成 Cargo.lock。

三语言均通过原 CLI 首次真实离线构建，外层 exit0。Rust 使用现有 Rust 1.95.0 的 wasm32-unknown-unknown；C/C++ 使用 Clang 22.1.0、已安装 WASI 34 sysroot 及相应原 SDK 包装库，C++ 使用现有 noeh libc++/libc++abi。Cargo `--locked --offline`，每个项目具有独立 target/build 目录；本地复制的注册表包及索引与锁中身份核对，没有下载或改动全局缓存。

| 原模板 | 实际 Wasm bytes | SHA256 |
| --- | ---: | --- |
| Rust task | 260852 | `921d316639e18d6594ac85af2319e4acc79fe00d0b3a4cc5ab720d10631565cf` |
| C task | 96532 | `39ac6fd5170e9d5c219ea62b07ddcc47f4323c76397724effa93e637e46699b2` |
| C++ task | 97498 | `697f2d4351873af23efd433bc3d5cc56596535a247184f8e99176ce01600d211` |

原 CLI `run()` 对每个内层非零返回码抛错，外层成功证明其编译序列返回成功；没有宣称逐个内层进程退出码均被独立采样。静态 Wasm 检查确认导出；本轮没有 pack、host 或 Wasm 业务执行，也没有第三方插件、普通桌面用户或跨平台资格。

原 source-only SDK ZIP SHA256 `666e12ddfb8c6ffaf51c1781925d24c615aef85001beb34b6f79baccb1601566`；SDK 327 文件、57 冻结原件、模板/锁与实际工具、sysroot 在构建前后不变。Rust 记录器首轮 ctypes 前置错误在 CLI/编译器启动前，原失败日志保留；修正记录器后按原 CLI 执行，SDK 源码未改。

Rust 证据 `w09-standalone-build-evidence.zip` SHA256 `ecb2e478dbb6085c8c03f55db04da623387cd9e2b6534c1e3413c139bd426698`；C/C++ 最终证据 `w09-final-002-evidence.zip` SHA256 `9a63b1bd29c383aa07a2bc58c310cd3de788208eb5bc19ec509e54be6cd51f29`。原封存未改写；独立模板编译通过后，整个 SDK 范围仍为 OPEN。
