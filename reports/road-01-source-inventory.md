# ROAD-01 自动事实清单：第一增量验证

日期：2026-09-15。结论：**PASS_SCOPED**，限定只读源码／产物观察与报告边界；ROAD-01 的真实构建回执及产物来源绑定尚未完成。

源码基线：`0e93e49f56dd2534db5a0cd8fb31e9d5c705dfa7` 加本地未提交修改。应用版本保持 `0.1.9-test.50+55`，网络组件为 `0.1.9-test.51`。没有重新构建产品、执行旧 guest 或进行设备／渠道测试。

## 已实现

[清单工具](../tool/inspect_build_inventory.py)读取八个 Rust crate、应用、协议版本、数据库源码迁移目标、四个 schema 目录、宿主／SDK 镜像和冻结兼容样本。记录 Git 工作区状态、源码摘要、现有产物字节、工具版本及显式 Flutter SDK 元数据，输出派生 Markdown。能力声明、历史证据与当前验证分列，当前平台资格统一标为 NOT_RUN；产物不凭文件名／时间自动绑定当前源码。

详见 [使用及限制](../docs/BUILD_INVENTORY.md)。工具会拒绝版本／契约差异、歧义字段、冻结篡改、观察期间变化及覆盖已有报告。遇到 Gradle 计算表达式不会冒充已解析 APK 配置。

## 验证证据

运行环境：Windows AMD64，Python 3.14.5；仅查询版本的 Rust 1.95.0、Cargo 1.95.0、Cap’n Proto 1.4.0。Flutter 缓存观察为 3.44.0、Dart 3.12.0，不是本轮 Flutter 构建证明。

```powershell
python -W error::ResourceWarning -X utf8 -m unittest tool.tests.test_build_inventory tool.tests.test_sdk_baseline -v
python -X utf8 tool/inspect_build_inventory.py --flutter-sdk C:\flutter --artifact build/network-node/release/morrow-api-node.exe --output build/road-01-inventory-reviewed.md
```

- 针对性验证 **36/36 通过**，退出码 0：新增清单测试 25 项，既有冻结样本验证器测试 11 项。日志：[road-01-validation.log](../build/road-01-validation.log)。
- 实际清单生成退出码 0：[生成报告](../build/road-01-inventory-reviewed.md)。冻结原件完整性验证为 36 项；不会重编译或重新封存这些原件。
- 产物检查仅覆盖现有 `morrow-api-node.exe` 的字节身份，摘要为 `82ec8ad82000c5fee53341ef7461519c24a7c187338a462bf8bfd319687e2e46`；没有因此声明当前源码编译、功能或公开部署通过。
- 独立审查修复了 Flutter 动态默认值误分类、重复声明、Markdown 链接／图片注入以及打开文件与当前路径对象不一致的问题。
- 初次实际运行因遗漏同一数据库格式的第三处保护检查而拒绝生成，随后修正解析；没有放宽数据库一致性断言。
- 独立测试的两次失败来自夹具：Windows 已打开文件不能直接替换，以及新增第四个 schema 目录后未同步复制夹具。原日志保留在 `build/test-inventory-independent-initial.log`、`build/test-inventory-independent-schema-fixture.log`。最终独立记录见 `build/test-inventory-independent-final.log`。
- 文件替换回归使用两个真实文件对象模拟“旧打开对象／新路径对象”，因为 Windows 默认句柄不允许实际 rename；不声称在该测试中完成了 Windows 打开后重命名。

## 本轮证据摘要

| 文件 | SHA-256 |
| --- | --- |
| tool/inspect_build_inventory.py | `6508f5c7d69a99e8d06b137c8a399ef45034a0df40b3043c72ee98d93745e3c3` |
| tool/tests/test_build_inventory.py | `e25f8b5c4e8ac40c1cc97063e5fbf0e0029cebcfb0358993955d39ff3000e3bc` |
| build/road-01-validation.log | `eb44d3fd9478891dda1bb073fc02baf9b791b8493729ccf5eae8c0accefa11d2` |
| build/road-01-inventory-reviewed.md | `69c628e3dea4739cee4fb8afa7d09a859b47a66888faa0f668f15e52d7a4d6e5` |

Markdown 为本轮开发验证的派生可读说明；不建立新的权限权威或正式 JSON 验证存储。报告和摘要是回归证据，不是签名或完整仓库受攻击时的可信证明。

## 剩余范围

真实构建／测试回执尚需版本化 Protobuf＋LZ4，绑定源码快照、目标、features、工具链、命令、退出状态、产物及验证范围。APK 合并配置、各原生 ABI／页面大小和设备／渠道证据仍未采集，不能依据源码 minSdk 值声明兼容。

ROAD-03 同步完成[稳定业务 ID 边界调查](../docs/WORKBENCH_ID_MIGRATION.md)，类型化 UI 身份、v1 固定字节适配和未来 v2 迁移尚未实现。完整双向 IO、执行／审计闭环与完整 SDK 稳定门槛继续按主路线推进。
