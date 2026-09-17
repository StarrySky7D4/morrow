# 版本与能力事实清单

状态：ROAD-01 第一增量，2026-09-15。提供只读源码／产物观察工具，不把静态清单当作当前构建、旧插件兼容或平台产品验收。

本轮 [限定验证报告](../reports/road-01-source-inventory.md)：36 项针对性测试通过，并用现有节点产物生成清单。

## 使用

需要 Python 3.11+、Git；无第三方 Python 依赖。在项目根目录运行：

```powershell
python -X utf8 tool/inspect_build_inventory.py
python -X utf8 tool/inspect_build_inventory.py --flutter-sdk C:\flutter --artifact build/network-node/release/morrow-api-node.exe --output build/engineering-inventory.md
python -X utf8 -m unittest discover -s tool/tests -p test_build_inventory.py -v
```

默认输出到终端，`--output` 只创建新文件，父目录须已存在；已有报告不会被覆盖。可重复指定 `--artifact` 核对多个产物。工具不扫描用户资料库、不执行产物、不构建 guest、不重封兼容样本，也不读取账号、密钥或签名配置。

## 实际读取与检查

- 应用 pubspec、八个 Rust crate 的版本及存在的 Cargo.lock；不同组件允许独立版本，不能自动统一或提升应用版本。
- 宿主／SDK 的 runtime、task、UI、dependency-call 版本；当前加载器接受的 guest ABI 与冻结原件的 ABI 分列，本地回调 ABI 不混入 guest ABI。
- 数据库源码的可接受格式范围和迁移目标相互核对；不打开用户数据库，也不声称已成功迁移。
- 核心、审计、宿主、第一方工作台的 schema 原字节 SHA-256 及仅 CRLF→LF 后的摘要；五个宿主／SDK 契约镜像与版本快照对比。注释依然参与摘要。
- 复用原有冻结样本验证器，检查 36 项固定文件及根摘要。通过只说明完整性，不能证明旧二进制已在当前宿主运行或具备发布者信任。
- 插件包 capability 枚举只标为源码声明；全部平台的本次编译、集成、设备／恢复、渠道状态均为 NOT_RUN。历史报告单独列路径和摘要，不自动继承 PASS。
- Git HEAD 与工作区修改状态、所读取源码的字节摘要；工作区有修改时不能仅用 HEAD 标识当前源码。读取结束前复核源码与 Git 状态，途中变化则拒绝报告。
- 显式产物按块读取 SHA-256，核对已打开对象和路径身份；不把旧文件的存在、文件名或时间视为源码关联。工具链版本仅调用版本查询，Flutter 通过显式 SDK 路径读取已有缓存和 Gradle 默认值，不运行 Flutter／Gradle。

Flutter 的源码默认 minSdk／targetSdk／compileSdk／NDK 仅作为可解析配置观察，表达式无法静态求值时标 UNRESOLVED。这不是 APK 合并 Manifest、原生 ABI、16 KiB 页面或设备支持证明；最终打包配置仍需构建时取证。

缺文件、版本／契约不一致、冻结样本变化、歧义声明、读取期间文件变化或不能确定 Git 状态均返回非零；不会刷新基线或给出假通过。Markdown 中来自文件名、状态和工具输出的文本按表格安全转义。

## 输出边界与后续任务

输出是开发工具的派生可读报告，不是运行期权限／能力清单，也不是正式持久验证记录；不新增 JSON 权威存储。源配置、第三方 Flutter 元数据与工具输出属于构建输入／外部观察。后续正式构建／验证回执仍按架构基线用版本化 Protobuf＋LZ4 保存，Markdown 从其派生。

ROAD-01 已新增[固定源码构建回执](BUILD_RECEIPTS.md)，当前只覆盖原生网络节点的实际构建；静态工具不会自行读取回执后提升其他能力状态。其他真实构建与测试入口仍待接入：把源码快照、目标、features、工具链、命令、退出码、产物摘要及验证范围绑定到同一次运行；读取 APK 实际配置、原生库架构／页面资格，并让报告明确区分历史、当前构建和设备资格。没有这些回执时，产物栏始终为 UNVERIFIED。当前工具不提供手填 PASS 开关。

当前实施顺序见 [路线修订](ROADMAP_UPDATE_2026-09-15.md)。此增量不提升版本、不改 schema、运行时导入或数据库格式，也不将完整 SDK 标为稳定。
