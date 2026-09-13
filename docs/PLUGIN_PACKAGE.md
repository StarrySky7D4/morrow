# 插件包与开发流程

本文描述 test.49 的当前工具合同，实际验证范围见 [test.49 开发者工具记录](../reports/test.49-sdk-project-tools.md)。包 Schema v1、guest ABI、运行协议和 SDK 源码版本分别管理；新项目使用 guest ABI v2、运行协议 v7、任务 v3、UI v1、依赖调用 v1。固定旧二进制的兼容规则见 [SDK 兼容基线](PLUGIN_SDK_COMPATIBILITY.md)，不得通过重建旧原件使兼容检查通过。

## 当前开发入口

使用仓库内的 Python 3.11+ 工具创建、构建和打包 C11、C++17、Rust 项目。以下命令在 Morrow 仓库根目录运行，目标项目目录必须不存在：

```powershell
python tool/morrow_plugin.py doctor --language rust
python tool/morrow_plugin.py new "build/我的转换插件" --language rust --kind transform --id org.example.reverse
python tool/morrow_plugin.py build "build/我的转换插件"
python tool/morrow_plugin.py pack "build/我的转换插件"
# 将下列 <sha256> 替换为 pack 输出的 SHA256；不要任取目录中的旧包。
python tool/morrow_plugin.py check "build/我的转换插件/dist/<sha256>.mplugin"
python tool/morrow_plugin.py transform "build/我的转换插件/dist/<sha256>.mplugin" bytes.reverse bytes bytes "input.bin" "output.bin"
```

`input.bin` 必须已存在，`output.bin` 必须不存在。`pack` 自行重新构建；单独执行 `build` 用于开发检查，不是打包的前置必需步骤。命令、配置、依赖模板及路径规则见 [项目工具](PLUGIN_PROJECT_TOOLS.md)。

`plugin.toml` 是人可读编译输入，不是应用运行期配置。正式 `.mplugin` 仍仅使用 Protobuf＋LZ4，保留原始 Manifest 与模块字节，并以完整归档 SHA-256 命名。应用不能在加载包后再读取 TOML 覆盖清单，使二者成为两套权威来源。

## 校验、发布与授权

| 阶段 | 实际行为 | 验证边界 |
| --- | --- | --- |
| `new` | 复制经过维护的 SDK 示例源码，生成项目配置、说明及许可；Rust 另带 Cargo 配置与锁 | 12 个模板是起点，不是 12 个完整业务插件 |
| `build` | 使用可信本机工具链编译当前源文件 | 不验证业务功能，不是本机源码执行沙箱 |
| 容器与清单校验 | 校验有界 PB＋LZ4、原文摘要、模块摘要、版本、能力、处理器、依赖与预算 | SHA-256 不是作者签名；声明不是授权或功能证明 |
| `check` | 静态准备 Wasm，禁止 start，验证导入、导出和入口，报告声明及实际宿主预算 | 不执行 guest，不解析实际依赖锁，不安装、不启用、不授权 |
| `pack` | 本次构建成功后生成临时候选，运行 `check`，再发布到项目 `dist`，核对发布字节 | 不修改生产插件注册表，不获得内容权限 |
| `transform` | 在合成临时资料库中执行一次纯转换；空批准上限且无内容授权，核对关联输出 | 不提供依赖提供者，不是任意内容任务或 UI 会话启动器 |
| 宿主连接与执行 | Manager／Pool 管理实际实例、批准上限与生命周期；每次业务调用继续校验权限 | 同名、新版本或新实例不能恢复旧连接权限 |

Catalog 安装采用暂存、同步与无覆盖发布，相同归档收敛到同一摘要文件；目标损坏时拒绝，不静默覆盖。Catalog 仅管理不可变包文件。实际选择、启用、批准与依赖锁由独立 [Registry](PLUGIN_REGISTRY.md) 管理，已接入 [实例池](PLUGIN_INSTANCE_POOL.md) 和默认 Windows 工作台。升级默认禁用；配置变更撤销相关旧实例，重新启用建立新实例。通用多插件管理界面、作者信任与签名分发仍需分别建设。

原件位于可信宿主管理的目录；这不构成对可修改整个目录的外部进程的隔离。已提交事务也不因随后取消、Trap 或回执丢失而自动回滚，重试应保留原操作身份。

## 清单能力与限额

新入口支持全部七种能力：`rename`、`summary`、`operation`、`attachment`、`create-content`、`edit-content`、`read-content`。清单声明限定能力上限，实际每卡片、操作与期限授权由宿主另行提供。

模块最多 4 MiB。默认预算为 2000 万 fuel、16 MiB 内存、16 次宿主调用；清单允许 fuel 1–1 亿、内存 64 KiB–64 MiB（64 KiB 的整数倍）、调用 0–1024。运行时取清单与宿主政策的较小值，增加清单预算不保证宿主提供全部资源。

每包最多 16 个纯转换处理器，名称唯一；每项绑定 handler、输入类型、输出类型及各 0–65536 字节的限额，0 表示仅接受空数据。当前类型名称精确匹配，尚不等于类型 Schema 协商。调用前校验注册、类型与输入；完成后再核对关联、输出类型和输出上限。低层 Runner 不替代包策略。

每包最多 16 个依赖声明，以唯一 slot、处理器、输入／输出类型、提供者 SemVer 范围及 optional 描述接口。提供者身份和实际摘要由宿主批准并写入 [依赖锁](PLUGIN_DEPENDENCY_LOCKS.md)，不能由 guest 自选。主动调用与有界多层执行见 [依赖图](PLUGIN_DEPENDENCY_GRAPH.md)。

当前必需功能为 `transform-handlers-v1`、`dependencies-v1`、`dependency-calls-v1`，工具按声明组合生成。未知或重复必需功能、能力、错误版本和摘要均拒绝。未知可选字段随原始包保留；新增必需语义不能仅追加未知字段让旧宿主静默忽略。

UI 通过处理器输出有界文档、接收关联事件；三语言 SDK、Flutter 渲染器与在线会话已有实现，见 [UI SDK](PLUGIN_UI_SDK.md) 和 [渲染器](PLUGIN_UI_RENDERER.md)。会话检查代次、修订及事件序号；专业编辑器、完整扩展点和持久草稿不能由一次文档往返推断完成。没有 TS／JS guest；第三方不需要编写 Dart Widget。

## 底层与历史兼容命令

当前底层 `core/examples/plugin_package.rs` 提供 `pack-v2`（新文件）与 `pack-v2-catalog`（摘要目录），支持重复 `--capability`、`--handler`、`--dependency` 及预算参数；不执行 guest。Python `pack` 在此之上增加当前源码构建和实际准备检查。`inspect` 仅做容器／清单校验；它不同于运行时 `check`。

以下是 test.10 起保留的低层命令形式，不是当前推荐的项目流程，也不会自动迁移既有旧包：

```powershell
# ABI v1 旧入口；MODULE 与 OUTPUT 为待替换的实际路径，OUTPUT 必须不存在。
cargo run --locked --offline --manifest-path core/Cargo.toml --example plugin_package -- pack MODULE OUTPUT org.example.legacy 0.1.0 rename
# ABI v2 内容任务；CAPS 保留逗号列表或 none 的旧形式，当前解析器支持七种能力。
cargo run --locked --offline --manifest-path core/Cargo.toml --example plugin_package -- pack-task MODULE OUTPUT org.example.task 0.1.0 rename,summary
# 旧转换声明列表仍是一个参数；无内容能力。
cargo run --locked --offline --manifest-path core/Cargo.toml --example plugin_package -- pack-transform MODULE OUTPUT org.example.transform 0.1.0 'bytes.reverse,bytes,bytes,65536,65536'
```

`qualify_package`、`qualify_transforms` 是匹配特定样例协议的历史资格工具，不能作为任意插件启动器。历史结果分别见 [test.10 包验证](../reports/plugin-package-validation.md) 和 [处理器验证](../reports/plugin-handler-validation.md)。当前工具合同与本轮结果见 [test.49 记录](../reports/test.49-sdk-project-tools.md)；Windows 结果、静态准备和 Web 编译各有独立边界，不能代替全平台产品验收。

## 当前项目流程验证

```powershell
python -B -X utf8 tool/verify_plugin_projects.py
```

该入口在新的 `build/SDK projects 空间 <UUID>` 下保留日志和生成项目，覆盖元数据测试、doctor、12 个模板打包、原包实际执行及三语言转换 CLI 的失败与无覆盖行为。它还检查同源重编译摘要、坏源不能复用旧产物、损坏既有包不被覆盖。可用 `--output-root` 指定一个尚不存在的目录。此流程会执行可信本机构建及受限 guest；当前完整资格范围为 Windows，实际是否通过以本轮报告及保留日志为准。
