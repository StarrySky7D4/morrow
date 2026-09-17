# 固定源码构建与二进制回执

状态：ROAD-01 第二增量，2026-09-15。实现独立工程回执，不改变应用审计密钥、用户内容、数据库格式或冻结 guest 契约。当前执行配置仅覆盖原生 `network-node-release`；其他构建、产品测试和平台资格仍待接入。

本轮已完成 [原生节点实际构建与回执验证](../reports/road-01-native-build-receipt.md)，17 项针对性测试及严格 Clippy 通过。

## 生成和核验

```powershell
cargo build --offline --locked --release --manifest-path audit/Cargo.toml --bin morrow-build-receipt --target-dir build/audit
build/audit/release/morrow-build-receipt.exe capture . build/network-receipt-new
build/audit/release/morrow-build-receipt.exe verify build/network-receipt-new
build/audit/release/morrow-build-receipt.exe report build/network-receipt-new
```

capture 要求输出父目录存在、目标目录全新。命令会真正编译原生网络节点，使用离线依赖；不会运行节点、开放监听、构建主应用、推送或发布。现有输出不会覆盖，失败／中断目录保留用于调查。

工具先列出 Git 跟踪文件与未忽略的未跟踪文件，把实际存在的源文件复制到 `run/source`。原工作区可以有未提交改动，回执同时保留 HEAD、dirty 和全部复制文件的字节摘要；删除的跟踪文件保持缺席。复制完成前核对 Git 文件集合、状态和原文件字节；随后构建始终使用这份副本。

依赖、Cargo.lock 和 source 内其他非忽略输入一并复制，外部工具链、全局 Cargo 配置和缓存仍在副本之外。这个过程不宣称密闭构建、来源签名或跨机器可复现；不可信构建脚本也不会因此成为已隔离代码。

固定配方为 `cargo build --offline --locked --release --manifest-path network_node/Cargo.toml --features plugin-adapter --bin morrow-api-node --target <rustc host> --target-dir ../target -j 2`，工作目录是 `run/source`。清除调用者常见的 Rust 编译标志、包装器与目标覆盖变量，但不声称剔除了全部机器配置。实际参数、工具版本、起止时间及采集器摘要都进入回执。

## 目录与状态

| 路径 | 用途 |
| --- | --- |
| `source/` | 实际编译的源码副本；核验要求文件集合及每项字节一致 |
| `target/` | 本次独立 Cargo 输出；不是旧构建目录复用 |
| `logs/build.stdout`、`logs/build.stderr` | 构建真实输出，摘要绑定回执 |
| `artifacts/morrow-api-node.exe`（Windows） | 构建成功后从本次 target 提取并逐字节核对的产物 |
| `build-receipt.pb.lz4` | 构建结束、源码副本及产物检查后创建的最终二进制记录 |

构建进程非零退出时保存失败回执及日志，产物列表为空。命令无法启动、采集受阻、进程被中断、最终回执未完整落盘等情况保留部分目录，不能仅据目录／产物存在宣布构建成功或确证编译失败。

verify 重新解码原回执、核对配方、源码完整集合、日志与产物。退出码 0 表示文件一致且记录为构建成功；1 表示文件一致但记录为构建失败；2 表示格式、完整性、文件或过程错误。report 先进行同样核验，再将原记录派生为可读 Markdown，不接受手填 PASS 参数。

## 格式与限制

[build_receipt.proto](../audit/schemas/build_receipt.proto) 是独立 `morrow.build_receipt.v1` 模式，构建期生成。`MORROWB1` 容器包含版本、原始／压缩长度、原始 Protobuf SHA-256 和 LZ4 块。原始数据至多 4 MiB，源文件至多 10,000，命令至多 64，产物至多 256；单文件采集上限 256 MiB、整个源码副本上限 2 GiB。

解码先做容器长度与有界解压，再校验摘要、字段类型／数量／重复项和版本。当前格式拒绝未知字段，不隐式跳过未来权限或状态。源路径必须规范、相对、唯一且排序；source-set 摘要按独立域、路径长度、路径、大小与字节摘要计算，不依赖 Protobuf 重编码。

成功记录必须包含成功完成的 build 命令及产物，所有列出的命令均成功；“只执行 rustc -V 成功”或编译失败不能组成成功记录。哈希能检出意外变化，但没有签名或外部可信锚；能同时重写所有原件和回执的人仍可伪造记录。

## 验证边界

编解码负面测试覆盖截断、尾随、错误长度／摘要、压缩膨胀、非法 varint、重复／未知字段、越界数量与矛盾结果。CLI 集成测试使用临时小型 Cargo 项目真实执行编译成功与失败，验证原目录后续修改不影响副本、产物／日志篡改拒绝、额外源码拒绝及已有输出不覆盖。这些合成样例不代替真实 Morrow 节点构建。

实际节点回执应单独保存并记录摘要。主应用运行、网络业务、旧 guest 执行、设备后台恢复、APK 合并参数、页面大小和渠道签名仍需各自的运行证据；不会从一次节点编译自动升级支持声明。后续继续将主应用／测试／其他平台接到版本化采集配置，而非让调用者自行声明成功级别。

与只读静态清单的关系见 [BUILD_INVENTORY](BUILD_INVENTORY.md)：静态检查记录现状，回执记录一次实际执行并绑定所用源码和产物，两者不能互相替代。
