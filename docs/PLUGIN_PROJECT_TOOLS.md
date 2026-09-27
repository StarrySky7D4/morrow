# 插件项目工具

自 test.49 提供 `tool/morrow_plugin.py`，将 C11、C++17、Rust 的创建、构建和打包入口统一起来。本文描述工具合同；本轮实际运行范围、失败及限制以 [test.49 验证记录](../reports/test.49-sdk-project-tools.md) 为准，不把脚手架生成或静态检查当作功能验收。

2026-09-26 增加 `--kind service` 三语言模板：固定 `service.echo`、显式 `http-listen`／`http-publish`、原帧关联和现有服务 schema pin。打包器要求 `--service`，不产生监听授权；默认短期 profile，可显式声明有限长时运行。使用方法和有限范围见 [服务 SDK](../sdk/SERVICE_API.md)，本轮实际原包与真实 TCP 验证见 [报告](../reports/plugin-service-sdk-2026-09-26.md)。`verify_plugin_projects.py` 的旧五类完整流程保持原范围；新增服务流程使用 `verify_plugin_service_sdk.py`。

## 离线工程预检（2026-09-26）

`validate PROJECT` 或 `validate PROJECT/plugin.toml` 只读取工程声明、SDK 契约和相关 Rust Cargo 配置，检查工程路径及既有 build 树。无需 Cargo、Clang、WASI sysroot 或网络，不启动外部工具，不执行插件，不创建 build／dist，不使用旧二进制作为成功依据。成功时标准输出为可用 `tomllib` 解析的 TOML；失败时退出码为 1，只在标准错误给出原因。

摘要包含插件 ID／版本、语言、源入口、声明的内容／IO 能力与 handler、依赖槽、资源发现标记、原打包参数和本 profile 已核对契约的 SHA-256。`kind = "standard"` 表示项目未声明 IO／service 类型，不据源码推断它属于 content／transform／UI／dependency 中哪一种。契约摘要不证明作者身份；输出不是签名、完整源码指纹或构建收据。SDK 基础契约／版本始终核对，IO 项目额外检查 IO，服务项目检查 service／service_resources，出站服务还必须检查 IO。

`build` 与 `pack` 复用同一预检入口。Rust 的库名、cdylib 类型、源入口以及所选 SDK 路径／wasm-guest feature 不符时，在创建输出目录和查询编译器之前失败。预检不执行 Cargo 依赖求解，不验证锁文件完整可用性、编译脚本或业务实现，也不证明 handler 存在。实际依赖求解和构建继续由 `--locked` Cargo 与后续核心检查负责。文件读取不是原子快照，历史预检摘要不能替代构建时重新检查。

`validate` 检查源工程声明，`doctor` 检查本机工具链，`check` 由 Rust 核心准备已有 `.mplugin` 包。三者均不授予能力，实际构建／执行验证仍需独立完成。

```sh
# 在新目录保留 21 个模板工程及每项生成／预检日志；不编译，不运行插件。
python3 tool/verify_plugin_projects.py --preflight-only --output-root /absolute/new-directory
```

该模式覆盖三语言的六类基础模板及三种 service-http 模板，核对输出身份／检查范围，并比较预检前后全部生成文件。输出目录存在即拒绝，原证据不覆盖。未传 `--preflight-only` 时保留既有完整构建／执行路径，并在打包前加入预检；该完整路径仍需工具链。本轮范围见 [离线预检报告](../reports/plugin-project-preflight-2026-09-26.md)。

## SDK 源码锁（2026-09-26 后续）

工程可用 `sdk.lock.toml` 锁定所选 SDK 库输入。锁中只有相对文件名、字节长度及 SHA-256，没有 SDK 绝对路径，源码不变时可迁移到新目录。`validate`／`build`／`pack` 发现锁文件便自动检查；`--require-sdk-lock` 额外拒绝没有锁的工程。旧工程不强制迁移，预检摘要明确报告 `sdk_lock_status = "absent"` 或 `"verified"`，已验证时带锁文件摘要和文件数。

```sh
# 新建并锁定；也可对既有项目单独执行 lock-sdk。
python3 tool/morrow_plugin.py new /absolute/project --language rust --kind transform --id org.example.pinned --lock-sdk
python3 tool/morrow_plugin.py lock-sdk /absolute/existing-project
python3 tool/morrow_plugin.py validate /absolute/project --require-sdk-lock
python3 tool/morrow_plugin.py pack /absolute/project --require-sdk-lock
# 审查 SDK 改动后，显式更新已有锁；不会由 build/pack 自动更新。
python3 tool/morrow_plugin.py lock-sdk /absolute/project --update
# 对全部 21 种模板启用锁定资格模式。
python3 tool/verify_plugin_projects.py --preflight-only --lock-sdk --output-root /absolute/new-locked-evidence
```

profile `morrow-sdk-source-v1` 固定 SDK 根 LICENSE、rust/LICENSE、rust/Cargo.toml、rust/Cargo.lock、rust/build.rs，以及 rust/src、rust/contracts、c/include、c/src、cpp/include、cpp/src 中的全部文件。文件新增、删除或修改都会失败；最多 2048 个文件、8192 个目录内条目、单文件 4 MiB、总计 16 MiB，锁元数据最多 1 MiB。拒绝符号链接、junction、非普通文件、重复或越界记录。SDK 生成示例、测试、缓存、工程本身、编译器、环境变量、传递依赖的实际源码和宿主打包器不在该锁范围；不是完整可重现构建或供应链签名。

创建锁不覆盖已有文件；更新必须显式传 `--update`，仍需通过所选 SDK 与宿主的契约检查以及 Rust 路径绑定。写入使用同目录暂存，首次创建依赖文件系统硬链接以避免覆盖竞争，更新使用原子替换；文件系统不支持时明确失败，不降级为截断已有锁。失败清理暂存文件，原锁保持。多写者不能把它当作完整事务或跨进程互斥机制。

构建前和编译成功后均检查 SDK 文件及同一锁身份，发生变化时不接受候选供打包；失败时可能保留编译器写出的候选文件，后续 pack 仍必须重新构建，不使用旧候选替代。该检查不是原子文件快照，也不能检测两次检查间变化后又还原的输入；工具面向受信任本地开发环境，不提供恶意编译脚本沙箱。检查结果见 [SDK 源码锁报告](../reports/plugin-sdk-source-lock-2026-09-26.md)。

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
python tool/morrow_plugin.py validate "build/示例插件 项目"
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

## 起始模板与服务出站变体

每一行均可搭配 `--language c`、`--language cpp` 或 `--language rust`，基本模板共 18 个组合，服务出站另有三语言可选变体。`--kind` 默认 `transform`。

| `--kind` | 来源示例 | 实际起点与后续条件 |
| --- | --- | --- |
| `content` | 各语言 `*-task` | 七种内容命令任务，声明全部七种能力；实际调用需宿主输入身份与逐对象授权 |
| `transform` | `*-transform` | `bytes.reverse`、`bytes.ascii-uppercase`、`bytes.require-ascii`；输入／输出类型均为 `bytes` |
| `ui` | `*-ui` | `ui.form` 接收 `text.utf8`，`ui.edit` 接收 `morrow.ui.event.v1`，均输出 `morrow.ui.document.v1`；需宿主渲染与会话验证 |
| `dependency` | `*-dependency-caller` | `bytes.dependency-wrap` 调用 slot `reverse`；需 `bytes.tag-reverse` 提供者、`^1.0.0` 版本范围及显式批准锁 |
| `io` | `*-io` | 接收宿主选择的原始 IO 请求帧，调用一次受管 IO 并原样完成已验证响应；实验性 `io-v1`，需宿主绑定和逐项批准 |
| `service` | `*-service` | 接收已认证的宿主服务请求并回显正文；显式监听/发布声明与服务 schema pin，短期 IO profile，不授予真实监听能力 |

`--kind service --service-http` 选择公共 SDK 的 `*-service-http` 变体，声明 `service.http.forward`、四项服务/HTTP 能力、资源目录及四个资源槽。仅把 POST 正文转发到恰好一个宿主端点的 `/`，无任意 URL、调用者凭据转发或自动重试。使用持久宿主路由；缺少或含多个端点时明确失败。行为与 Unknown 边界见 [服务 SDK](../sdk/SERVICE_API.md)。

`[io].max_resources` 可显式设置 1–8，映射 `--io-resources N`；缺省仍为 2。服务出站的四个槽对应监听、发布、端点及在途 HTTP。设置不授予能力，不改变单作业、字节及期限预算。`[service_run]` 另声明累计期限/任务/字节，三个字段须同时提供。

UI 模板的 `ui.form` 输入上限为 32 字节，`ui.edit` 为 65536 字节；输出上限均为 65536 字节。依赖包装模板输入上限为 65531 字节，声明 `read-content`、`edit-content` 能力，但调用依赖或产出结果都不自动取得保存权限。这些值描述原模板，修改源码后应同步修改相应声明。

模板复制现有 SDK 适配实现，不生成任意本地 shell 命令，也不新增 TS／JS 或 Dart 插件支持。UI 文档结果不是已经绘制的 Flutter 页面；依赖模板准备通过不等于完整依赖调用已通过。

IO 模板默认只声明 `file-read` 和 `io.request` handler。选择 `http-request` 时生成器改为声明固定 `morrow.http.forward.v1` handler，以便现有工作台识别这个恰好转发一次原始请求帧的 guest；即使同时选 `file-read`，也仅声明此 HTTP profile。需 HTTP 时明确选择相应上限：

```powershell
python tool/morrow_plugin.py new "build/HTTP IO 示例" --language cpp --kind io --id org.example.http --io-capability http-request --io-capability credential-use
python tool/morrow_plugin.py pack "build/HTTP IO 示例" --sysroot "C:/path/to/wasi-sysroot"
```

`--io-capability` 仅用于 `--kind io`，可取 `file-read`、`http-request`、`credential-use`，不得重复；`credential-use` 必须同时声明 `http-request`。这些是包的能力上限，不是批准、资源句柄或连接参数。工作台还会独立核对 `morrow.http.forward.v1` 声明、原始请求帧逐字节相等及调用序号；仅有 `http-request` 能力不足以进入该 HTTP 入口。插件只处理宿主下发的受限引用；路径、URL、凭据原文及 OS 句柄不写入 `plugin.toml`。此 starter 不支持普通内容能力、纯转换 handler 或依赖调用混用。`transform` 命令不能执行 IO 包，需受管 IO 宿主运行。

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
| `build.kind` | 出站 IO 使用 `io`，入站服务使用 `service`；两者均须有 `[io]` 声明 |
| `budget.fuel` | 1–100000000，默认 20000000 |
| `budget.memory_bytes` | 65536–67108864，64 KiB 的整数倍；默认 16777216 |
| `budget.host_calls` | 0–1024，默认 16 |
| `handlers` | 最多 16 项；name 唯一，输入／输出类型精确匹配，两个限额必需且各为 0–65536 |
| `dependencies` | 最多 16 项；slot 唯一；handler、类型、提供者版本范围必需，optional 默认 false |
| `io.capabilities`／`handlers` | IO/服务项目必需；`io` 使用上述三种能力，`service` 恰为 `http-listen` 与 `http-publish`；1–16 个唯一 handler，不与普通转换及依赖声明混用 |

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

IO 项目的 `plugin.toml` 额外包含以下结构；核心打包器生成精确 `io-v1` Schema 摘要和 guest ABI 2 清单，不以 TOML 本身作为宿主授权：

```toml
[build]
language = "cpp"
source = "src/plugin.cpp"
kind = "io"

[io]
capabilities = ["http-request", "credential-use"]
handlers = ["morrow.http.forward.v1"]
```

当前项目打包入口给 IO 声明固定上限：2 个资源、1 个作业、总量及单作业各 1 MiB、持续最多 30000 毫秒。HTTP 端点和活动调用可各占一个资源。受管宿主为该包创建的 worker `JobLimits` 也必须落在这些包上限内；超过上限会被拒绝，不能靠静态准备通过。`pack` 与 `check` 输出 IO 能力、handler、Schema 摘要和预算；它们只做打包及静态准备，不能证明运行时授权或实际 HTTP 成功。

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
python -B -X utf8 tool/verify_plugin_projects.py --output-root "build/插件工具 本次资格" --sysroot "C:/path/to/wasi-sysroot"
```

默认创建 `build/SDK projects 空间 <UUID>`，保留各步骤日志、项目源码、包和输出，不覆盖先前证据。当前入口运行元数据单元测试与 doctor，创建并打包全部 15 个模板；`qualify_sdk_projects` 继续执行原有 12 个非 IO 原包，新增 3 个 IO 包仅静态准备。IO 的真实运行需要独立的受管宿主测试；不通过重打包替换传入的原件使运行通过。`--sysroot` 会传给项目工具的 doctor、新建、构建和打包命令；默认路径在当前工作树不存在时，应显式提供已验证的 WASI sysroot。

另对 C／C++／Rust 转换 CLI 检查含零字节和非 ASCII 字节的输入、精确输出、业务失败不发布文件、已有输出不覆盖。负面用例同时核对预期阶段、诊断及适用的底层退出码，不把任意非零退出视为通过。生成的 Rust 项目用于同源重编译摘要一致性、故意坏源拒绝旧产物、已有摘要包损坏时拒绝且不覆盖；故意修改的样本在 `finally` 中恢复。这个本机重复构建检查不等于跨机器可复现构建证明。

验证会执行可信本机构建和受限插件任务，遵循同样的非源码沙箱边界，当前完整资格范围为 Windows。流程不是插件 UI 的逐像素验收，也不代替 Android、Web 或其他系统的实际运行验证。执行中的步骤和最终结果分别保留；不能仅因验证脚本存在就宣称全部通过，实际结论见 [test.49 记录](../reports/test.49-sdk-project-tools.md)。


## 显式长时服务与资源发现（2026-09-26）

`new --kind service` 可同时指定 `--service-run-ms`、`--service-run-jobs`、`--service-run-bytes`，生成完整 `[service_run]` 声明。三个参数缺失、非正数或超界会在编译前失败；其他模板不能带运行声明。单请求的原短期限和单作业额度继续有效，运行上限不产生授权或自动续租。

自定义服务配置还可显式声明 HTTP 出站能力和 `[io] service_resources = true`；不提供出站能力时拒绝该选项。三语言资源目录解析及调用边界见 [服务 SDK](../sdk/SERVICE_API.md)。`verify_plugin_service_sdk.py --native` 增加 Linux 原生句柄检查，并实测生成长时原包的续租、累计额度、撤权和历史重开。


### 原生文件变更声明的实验边界

2026-09-27：底层 Rust `plugin_package pack-v2` 现可打包 `file-create`／`file-replace`／`file-delete` 声明，用于可信 Windows 宿主选择与恢复测试。它只写包声明，没有路径、选择权限或执行授权。上述 Python 项目生成器的 IO starter 能力范围仍如本节所述，尚未提供完整文件变更 guest SDK 模板；不要据此推断 guest 能直接调用私有宿主调度协议。Windows 条件替换继续明确 Unsupported。恢复会话及真实进程验证见 [本轮报告](../reports/mutation-recovery-session-2026-09-27.md)。
