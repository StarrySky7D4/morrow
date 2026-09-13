# 主应用第三方插件管理

状态：0.1.9-test.50，2026-09-14。Windows 默认工作台已接通第三方基础转换和标准声明式表单包的导入、批准、启停、升级与卸载。主应用复用现有 Catalog、Registry、Manager 和 Pool，不另建插件选择或授权权威。实际验证与范围见 [test.50 报告](../reports/test.50-application-plugin-management.md)。

这不是所有平台或所有插件能力的完成声明。网络、通用文件访问及对应授权接口尚未实现，后续方案见 [插件网络与文件接口设计](PLUGIN_IO_DESIGN.md)。SDK 仍按 [兼容候选基线](PLUGIN_SDK_COMPATIBILITY.md) 管理，不因本轮产品入口而成为全面稳定 SDK。

## 使用流程

1. 在设置的“第三方插件”区域选择 `.mplugin` 或 `.morrowplugin` 文件。预览显示名称、ID、版本、声明权限、依赖及不能运行的原因；此时只读取、校验和准备检查文件，不安装、不启用、不执行 guest。
2. 点击“导入”。宿主重新有界读取文件，核对预览摘要和登记版本，拒绝文件被替换、外部包覆盖内置工作台、同版本不同内容和降级。包原件按摘要保存，选择状态由同一 Registry 持久化。
3. 新导入的包默认禁用。勾选需要批准的声明权限，再明确点击“批准并启用”。勾选本身不授予权限。重复导入已选中的同摘要原件保持原启用状态，页面按刷新后的真实状态显示，不宣称它被重新禁用。
4. 对已启用、可准备运行的基础转换包，选择处理器，输入文字或选择一个小文件，再点击“转换”。结果只供预览，不自动修改卡片或保存文件。
5. 符合标准表单声明的包提供“打开界面”。表单使用既有 `ManagedPluginForm`，输入产生真实 UI 事件，经异步宿主 transport 执行 guest，校验返回文档后更新控件。
6. 停用或卸载前关闭当前外部表单。卸载移除选择、批准及相关依赖锁，保留用户内容和 Catalog 中的不可变包原件；本轮不提供这些包原件的物理清理功能。

本页不提供依赖批准 GUI。含必需依赖的包可以被检查、导入并显示说明，但当前入口拒绝启用和运行；不会隐式寻找或批准提供者，也不会把 optional 声明解释为自动批准。

## 批准、状态与生命周期

声明能力是包的上限，用户批准是该上限内的持久子集，两者均不等于具体卡片或附件的运行期 grant。当前可展示的七种内容能力为重命名、读取摘要、查询操作结果、读取附件、新建内容、编辑内容和读取内容。

第三方基础转换及表单以独立 Pool Session 运行，不获得对象 grant；它们不能借此读写内容库。转换输出、UI 文档和按钮回显都不是内容提交回执。批准第三方包不会恢复默认工作台的内容编辑权限。

内置工作台条目只展示状态，其原有管理按钮继续负责启停。默认工作台 Session 与第三方 Session 分开；第三方停用、升级、卸载或执行故障不应停止无关工作台功能。内置工作台停用或发生维护错误时，内容界面继续按原规则保持只读和可读状态。本页的“已启用”“可运行”分别描述选择和准备状态，不代表整个工作台已可写。

目录每页至多两条，按包 ID 排序。第一页返回登记 revision；之后每页必须携带相同 revision 和返回的 cursor。Flutter 读取全部页后才将列表视为已确认，拒绝中途版本变化、重复条目或循环游标，不默默只显示第一页。包文件缺失或损坏的已选条目仍显示不可用原因，不能作为健康可执行插件使用。

批准、启停、导入、删除及运行入口均绑定实际包 ID、摘要和登记 revision。升级先检查 SemVer 优先级确实增加，再进入现有 Manager 选择流程。新选择禁用，批准与新声明取交集，相关依赖锁移除；旧实例及需要它的消费者按原机制撤销。旧表单和旧回调不能绑定到新包或新实例。

`approve` 与 `set_enabled` 是先后两次持久调用，**不宣称批准和启用作为一个原子事务提交**。Catalog 安装与 Registry 选择也不是跨文件原子事务。写入失败或响应不明时，页面提示并重新读取状态；不自动重试导入、批准、启停或卸载。需要用户核对当前状态后重新选择，不能沿用旧 revision 猜测结果。

## 转换与表单边界

转换输入和输出分别受处理器声明及 64 KiB 上限约束。文字编码为 UTF-8；小文件作为原始字节传入，不把文件路径交给 guest。页面先检查文件长度，读取时再次限制字节数。输出按有效文本或十六进制二进制摘要显示，标明总字节数；文字最多预览 4096 个字符，二进制最多预览 64 字节。显示截取不改变宿主实际返回的字节，也不表示提供了大文件处理。

用户选择本机文件只是向纯转换提供输入，**不是 SDK filesystem 能力**。本页没有网络批准、任意路径读取、目录遍历或网络请求入口。

标准表单沿用现有精确声明：

| 处理器 | 输入类型 | 输出类型 | 输入／输出上限 |
| --- | --- | --- | --- |
| `ui.form` | `text.utf8` | `morrow.ui.document.v1` | 32／65536 字节 |
| `ui.edit` | `morrow.ui.event.v1` | `morrow.ui.document.v1` | 65536／65536 字节 |

UI 契约仍为 v1 的有界基础控件，不加载 Dart 插件，也不执行任意 HTML、TS 或 JS 界面。宿主固定包摘要、实际连接、view 与 generation，并检查 revision、serial、节点和动作；前端序号不是权限。

全局最多一个外部表单，和原内置工作台表单分别管理。打开另一个外部表单、更新、停用或卸载前先关闭已有外部表单。关闭失败会显示提示并阻止继续修改状态；用户明确刷新可重试幂等关闭，不自动重放 UI 事件。

宿主仅对本进程最近 **64 条已确认关闭的 `(包 ID, generation)`** 保存有界确认记录。相同关闭请求可以再次确认成功；任意未确认身份、已淘汰的旧记录不获无限期幂等承诺，也不能关闭当前其他表单。该记录不是持久日志，进程重启不恢复它。原生 transport 在关闭请求失败后清除失败 Future 的缓存，避免用户刷新永远等待同一个失败结果。

## 代码入口

| 层 | 入口与职责 |
| --- | --- |
| Flutter 管理页面 | [`lib/plugins/plugin_library.dart`](../lib/plugins/plugin_library.dart)：DTO、`ExternalPluginControl`、完整分页、预览、批准、转换预览及单外部表单 |
| 原生 transport | [`lib/plugins/workbench_native.dart`](../lib/plugins/workbench_native.dart)：目录和操作参数、摘要/revision 绑定、外部 UI 请求及关闭重试 |
| 主界面 | [`lib/main.dart`](../lib/main.dart)：在具备 `ExternalPluginControl` 的后端显示管理区域；内置按钮保留 |
| 宿主协议 | [`workbench_host/schemas/host.capnp`](../workbench_host/schemas/host.capnp)、[`protocol.rs`](../workbench_host/src/protocol.rs)：目录、检查、导入、批准、删除、转换及外部 UI 消息 |
| 宿主控制 | [`plugin_catalog.rs`](../workbench_host/src/plugin_catalog.rs)：`catalog_page`、`inspect_plugin`、`import_plugin`、`configure_external`、`remove_external`、`run_external_transform`、`external_ui_open/event/close` |
| 既有底座 | [`Manager`](../plugin_runtime/src/manager.rs)、[`Pool`](../plugin_runtime/src/instance_pool.rs)、[`InlineUi`](../plugin_runtime/src/inline_ui.rs)：选择与撤权、实例生命周期、真实 UI 事件校验和执行 |

对新包开发与本地检查使用 [开发者工具](PLUGIN_PROJECT_TOOLS.md)；包摘要只能证明字节身份，不是作者签名或信任背书。

## 复验入口

在仓库根目录的 Windows PowerShell 中运行。需要现有 Flutter、Rust、`wasm32-unknown-unknown` 和 Python 工具链，依赖按仓库锁文件准备。以下构建会生成当前内置工作台，并创建独立升级测试候选；不会重写 `sdk/compat/guest-v1-rc1` 冻结原件。

```powershell
pwsh -File tool/build_workbench_bundle.ps1
python tool/prepare_plugin_management_fixtures.py
cargo test --offline --locked --manifest-path workbench_host/Cargo.toml --release --target-dir build/workbench-host --test plugin_catalog --test plugin_control
$env:MORROW_WORKBENCH_HOST = (Resolve-Path build/workbench-host/release/morrow-workbench-host.exe).Path
$env:MORROW_WORKBENCH_PACKAGE = (Resolve-Path build/workbench-host/bundle/workbench.morrowplugin).Path
flutter test test/plugin_library_test.dart test/external_plugin_native_test.dart
```

`prepare_plugin_management_fixtures.py` 使用冻结的 Rust/C UI 模块生成不同目录中的升级候选，前后验证冻结清单；缺少或不同的候选不会通过覆盖原件来修复。

- [`plugin_library_test.dart`](../test/plugin_library_test.dart) 的八项 widget 测试通过 fake backend 验证实际点击、两条分页 revision、选择／预览取消、明确批准、冲突不重复写入、结果预览、真实 `ManagedPluginForm` 编解码交互及关闭失败恢复；它们本身不证明真实宿主执行。
- [`external_plugin_native_test.dart`](../test/external_plugin_native_test.dart) 的两项测试使用上述实际宿主和内置包，加上冻结第三方包，验证真实目录 transport、批准、二进制转换、UI 事件与内容保留。升级候选及升级失效由宿主测试覆盖。环境变量缺失或非 Windows 会跳过，不能将跳过计为通过。
- [`plugin_catalog.rs`](../workbench_host/tests/plugin_catalog.rs) 与 [`plugin_control.rs`](../workbench_host/tests/plugin_control.rs) 覆盖宿主控制、版本与摘要预检、内外部生命周期、关闭确认及既有内置行为。

本轮完成记录以 [test.50 报告](../reports/test.50-application-plugin-management.md) 为准。测试只使用合成内容库和资格包，不对真实用户内容库执行清理。
