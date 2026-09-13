# 插件项目工具

自 test.49 提供 `tool/morrow_plugin.py`，将 C11、C++17、Rust 的创建、构建和打包入口统一起来。本文描述工具合同；本轮实际运行范围、失败及限制以 [test.49 验证记录](../reports/test.49-sdk-project-tools.md) 为准，不把脚手架生成或静态检查当作功能验收。

## 环境与分发边界

需要 Python 3.11+（标准库 `tomllib`），不需要 Python 第三方包。构建依赖 Cargo、Rust、`wasm32-unknown-unknown` 目标及 Cap’n Proto 编译器；C/C++ 另需 Clang、对应 WASI sysroot，C++ 使用无异常的标准库、C++17、无 RTTI。

这是依托 Morrow 仓库的工具：打包、准备与执行会构建仓库内的 Rust example。`--sdk-root` 可选择兼容 SDK 源码目录，但不把该工具变成独立安装的 SDK 发行包。SDK 根需包含契约、源码、示例、锁文件和许可；工具核对相关 Schema 与协议版本。它不验证 SDK 作者身份。

命令以 Cargo `--locked --offline` 运行；新环境若尚未缓存依赖，会明确失败。每个子命令可显式添加 `--allow-network` 允许 Cargo 下载依赖。离线选项不隔离编译脚本、编译器或其它进程的网络访问，也不自动安装缺少的工具和系统标准库。

```powershell
# 以下命令在 Morrow 仓库根目录运行。
python tool/morrow_plugin.py doctor --language rust
python tool/morrow_plugin.py doctor --language cpp --sysroot "C:/开发工具/WASI/wasi-sysroot"
```

`doctor` 不指定语言时检查三语言所需工具。它检查工具可用性、已安装的 Rust 目标、相关 sysroot 文件及 SDK 契约；不编译插件，不运行插件，也不代替实际构建测试。

## 创建与执行

```powershell
python tool/morrow_plugin.py new "build/示例插件 项目" --language rust --kind transform --id org.example.reverse --version 0.1.0 --name "字节转换"
python tool/morrow_plugin.py build "build/示例插件 项目"
python tool/morrow_plugin.py pack "build/示例插件 项目"
```

`new` 拒绝已存在的目标目录，不覆盖原文件；生成 `plugin.toml`、源码、README、LICENSE 和 `.gitignore`。Rust 另生成 Cargo 配置及锁文件。创建失败可能保留未完成目录，应检查后再选择新目录，不自动删除它。模板沿用 SDK 的 AGPL-3.0-only 许可。

`build` 编译当前入口，输出 `PROJECT/build/plugin.wasm`。`pack` 无论之前是否构建过，都会在本次调用中先构建，再生成临时候选包、静态准备，最后发布到 `PROJECT/dist/<完整归档SHA256>.mplugin` 并核对原字节。配置在构建后重新核对；构建失败会停止后续打包，不从旧产物中挑选替代品。旧包继续保留在自己的摘要路径下。

根据 `pack` 输出的 `PACKAGE` 路径继续操作：

```powershell
python tool/morrow_plugin.py check "build/示例插件 项目/dist/<sha256>.mplugin"
# input.bin 必须已存在，output.bin 必须不存在；替换上面的摘要占位符。
python tool/morrow_plugin.py transform "build/示例插件 项目/dist/<sha256>.mplugin" bytes.reverse bytes bytes "input.bin" "output.bin"
```

`check` 校验并静态准备：报告包摘要、版本、声明能力、处理器、依赖及有效预算，不执行 guest、不安装、不启用、不授权。它不证明声明处理器确实存在于插件业务实现中，不解析实际提供者与批准锁。

`transform` 显式执行一次关联转换，输入最多 65536 字节；在合成临时资料库内使用空批准上限且无内容授权，禁止混入普通核心命令交换。成功才无覆盖发布输出；业务失败、协议错误、Trap 等以失败退出，不写成功输出。存在必需依赖时拒绝：该简易执行器不配置 provider，也不创建批准锁。它不提供内容任务身份、UI 会话或生产资料访问。

`pack` 发布到项目目录仅是开发产物；要接入真实应用，还需宿主选择包、批准能力及依赖、启用并创建实际实例。见 [包格式](PLUGIN_PACKAGE.md)、[注册表](PLUGIN_REGISTRY.md) 和 [依赖锁](PLUGIN_DEPENDENCY_LOCKS.md)。

## 十二个起始模板

每一行均可搭配 `--language c`、`--language cpp` 或 `--language rust`，共 12 个组合。`--kind` 默认 `transform`。

| `--kind` | 来源示例 | 实际起点与后续条件 |
| --- | --- | --- |
| `content` | 各语言 `*-task` | 七种内容命令任务，声明全部七种能力；实际调用需宿主输入身份与逐对象授权 |
| `transform` | `*-transform` | `bytes.reverse`、`bytes.ascii-uppercase`、`bytes.require-ascii`；输入／输出类型均为 `bytes` |
| `ui` | `*-ui` | `ui.form` 接收 `text.utf8`，`ui.edit` 接收 `morrow.ui.event.v1`，均输出 `morrow.ui.document.v1`；需宿主渲染与会话验证 |
| `dependency` | `*-dependency-caller` | `bytes.dependency-wrap` 调用 slot `reverse`；需 `bytes.tag-reverse` 提供者、`^1.0.0` 版本范围及显式批准锁 |

UI 模板的 `ui.form` 输入上限为 32 字节，`ui.edit` 为 65536 字节；输出上限均为 65536 字节。依赖包装模板输入上限为 65531 字节，声明 `read-content`、`edit-content` 能力，但调用依赖或产出结果都不自动取得保存权限。这些值描述原模板，修改源码后应同步修改相应声明。

模板复制现有 SDK 适配实现，不生成任意本地 shell 命令，也不新增 TS／JS 或 Dart 插件支持。UI 文档结果不是已经绘制的 Flutter 页面；依赖模板准备通过不等于完整依赖调用已通过。

## `plugin.toml`

示例是仅声明一个反转处理器的 Rust 项目；现成 transform 模板会声明三个处理器：

```toml
schema = 1

[plugin]
id = "org.example.reverse"
version = "0.1.0"
name = "字节反转"
capabilities = []
dependency_calls = false

[build]
language = "rust"
source = "src/lib.rs"

[budget]
fuel = 20000000
memory_bytes = 16777216
host_calls = 16

[[handlers]]
name = "bytes.reverse"
input_type = "bytes"
output_type = "bytes"
max_input_bytes = 65536
max_output_bytes = 65536
```

配置最多 64 KiB；未知字段、重复 TOML 键、错误类型及重复能力／处理器／slot 拒绝。`schema` 必须为整数 1；布尔值不作为整数预算接受。TOML 是编译输入，正式清单仍由核心校验并编码为 PB＋LZ4；应用不读取此文件作为第二套配置。

| 字段 | 合同 |
| --- | --- |
| `plugin.id` | 1–128 个 ASCII 字符，首字符为字母或数字，其余可含点、下划线、连字符 |
| `plugin.version` | SemVer，最多 128 UTF-8 字节；主／次／补丁版本适配 UInt64 |
| `plugin.name` | 可省略，默认 ID；非空、最多 128 UTF-8 字节、无控制字符 |
| `plugin.capabilities` | 可省略，默认空；七种支持的能力名且不可重复 |
| `plugin.dependency_calls` | 可省略，默认 false；true 要求声明处理器，生成固定依赖调用功能标记 |
| `build.language`／`source` | 必需，语言为 rust/c/cpp；source 为项目内存在的入口文件 |
| `budget.fuel` | 1–100000000，默认 20000000 |
| `budget.memory_bytes` | 65536–67108864，64 KiB 的整数倍；默认 16777216 |
| `budget.host_calls` | 0–1024，默认 16 |
| `handlers` | 最多 16 项；name 唯一，输入／输出类型精确匹配，两个限额必需且各为 0–65536 |
| `dependencies` | 最多 16 项；slot 唯一；handler、类型、提供者版本范围必需，optional 默认 false |

七种能力名为 `rename`、`summary`、`operation`、`attachment`、`create-content`、`edit-content`、`read-content`。处理器／slot／类型标识非空、最多 256 UTF-8 字节，不含控制字符、斜杠、反斜杠或冒号。提供者版本范围最多 128 UTF-8 字节，完整语义由核心 SemVer 校验。

依赖模板额外包含以下声明，并将 `plugin.dependency_calls` 设为 true：

```toml
[[dependencies]]
slot = "reverse"
handler = "bytes.tag-reverse"
input_type = "bytes"
output_type = "bytes"
provider_version = "^1.0.0"
optional = false
```

该声明不指定提供者包身份。批准者在运行期选择具体提供者及包摘要，guest 仅使用 slot。不要只给普通 transform 源码加此声明，就把它当作已实现依赖调用。

清单预算不是资源保证：`plugin_check` 的当前默认宿主政策为 2000 万 fuel、16 MiB 内存、16 次调用，实际取较小值并在 `check` 中报告。C/C++ 链接模块的最大内存使用项目声明预算；运行时仍有独立限制。较小预算可能不足以实例化标准库或完成任务，静态准备不替代执行验证。

## 路径、构建与错误处理

项目路径可以包含空格及 Unicode；在命令行中整体加引号。`build`／`pack` 也接受项目内的 `plugin.toml` 路径。`source` 只接受项目相对路径，解析后不能跳出项目；输出在项目的 `build`／`dist` 下，拒绝输出路径的符号链接及 junction。编译前还扫描已有 build 树，不跟随链接，最多检查 200000 项。超限会明确失败，不静默跳过。

这不是对并发修改目录的敌对进程的隔离。构建运行可信本机编译器与 Cargo，Rust 的构建脚本、宏和开发环境配置仍可能执行本机代码；不能把入口路径校验理解为对所有 include、依赖或进程访问的源码沙箱。

Rust 的 Cargo `[lib]` 必须命名 `morrow_plugin`、包含 `cdylib`，入口与 TOML 的 `build.source` 一致；`morrow-plugin-sdk` 必须指向所选 SDK 的 `rust` 目录并包含 `wasm-guest`。若移动 SDK，应同时修改依赖路径并传入匹配的 `--sdk-root`。C/C++ 使用所选 SDK 源码及项目内构建缓存，C++ 目前不支持异常和 RTTI。

所有子命令都可接受 `--sdk-root PATH`、`--sysroot PATH`、`--allow-network`；选项放在子命令后。`--sysroot` 仅用于 C/C++ 构建及相关 doctor 检查，默认仓库 `build/tools/wasi-34/wasi-sysroot-34.0`。工具不接受自定义 shell 构建命令，子进程参数按独立参数传递。

失败以非零退出，不通过重建固定兼容基线或放宽核心校验消除错误。新工具也不代表全部平台 SDK 分发、签名／作者信任、插件市场或通用管理界面已经完成；后续工作与验证范围见 [test.49 记录](../reports/test.49-sdk-project-tools.md)。

## 可重复的项目流程验证

```powershell
python -B -X utf8 tool/verify_plugin_projects.py
# 可选；指定目录必须尚不存在。
python -B -X utf8 tool/verify_plugin_projects.py --output-root "build/插件工具 本次资格"
```

默认创建 `build/SDK projects 空间 <UUID>`，保留各步骤日志、项目源码、包和输出，不覆盖先前证据。当前入口运行元数据单元测试与 doctor，创建并打包全部 12 个模板，再由 `qualify_sdk_projects` 执行这些实际生成的原包；不通过重打包替换传入的原件使运行通过。

另对 C／C++／Rust 转换 CLI 检查含零字节和非 ASCII 字节的输入、精确输出、业务失败不发布文件、已有输出不覆盖。负面用例同时核对预期阶段、诊断及适用的底层退出码，不把任意非零退出视为通过。生成的 Rust 项目用于同源重编译摘要一致性、故意坏源拒绝旧产物、已有摘要包损坏时拒绝且不覆盖；故意修改的样本在 `finally` 中恢复。这个本机重复构建检查不等于跨机器可复现构建证明。

验证会执行可信本机构建和受限插件任务，遵循同样的非源码沙箱边界，当前完整资格范围为 Windows。流程不是插件 UI 的逐像素验收，也不代替 Android、Web 或其他系统的实际运行验证。执行中的步骤和最终结果分别保留；不能仅因验证脚本存在就宣称全部通过，实际结论见 [test.49 记录](../reports/test.49-sdk-project-tools.md)。
