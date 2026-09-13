# Morrow 第三方插件 SDK

SDK 源码包版本 `0.1.9-test.49`，当前处于测试开发阶段，提供 C11、C++17、Rust 的受限 Wasm 插件接口。test.48 建立 **guest-v1-rc1 二进制兼容候选基线**；实际 guest ABI v2、运行协议 v7、任务 v3、UI v1、依赖调用 v1。该基线与 SDK crate 版本分别管理，不宣称完整 SDK、原生动态库 ABI 或全平台已经稳定。详见 [兼容边界与演进规则](../docs/PLUGIN_SDK_COMPATIBILITY.md)。

Flutter 负责宿主界面绘制，第三方插件使用声明式 UI，不要求编写 Dart 插件。当前不提供 TS／JS guest。主应用默认 Windows 工作台已经使用 Rust 宿主和受限 Wasm 业务插件；通用多插件安装管理和全平台接入仍在推进。

## 开发入口

新项目可使用仓库内 `tool/morrow_plugin.py` 的 `new`、`doctor`、`build`、`pack`、`check` 和 `transform`。提供 C／C++／Rust 的内容、转换、UI、依赖四类模板；正式包仍由核心生成并在发布前检查运行准备。`plugin.toml` 只作构建输入，打包不产生授权或启用状态。详见 [项目工具](../docs/PLUGIN_PROJECT_TOOLS.md)。

```powershell
python -B -X utf8 tool/morrow_plugin.py doctor --language rust
python -B -X utf8 tool/morrow_plugin.py new "build/My plugin" --language rust --kind transform --id org.example.my-plugin
python -B -X utf8 tool/morrow_plugin.py pack "build/My plugin"
# 独立新目录保留全部生成项目与日志，运行三语言四类完整包及失败保护检查。
python -B -X utf8 tool/verify_plugin_projects.py
```

项目工具需要 Python 3.11+，默认 Cargo 离线构建，使用受信任的本地工具链；它不是源码沙箱。当前打包与诊断仍依赖本仓库的核心／运行时工具，尚非完整独立预编译 SDK 分发。

| 语言 | 入口 | 说明 |
| --- | --- | --- |
| C11 | `c/include/morrow_plugin_*.h` | 类型化命令、任务、UI、依赖调用；编解码复用 Rust 库 |
| C++17 | `cpp/include/morrow_plugin_*.hpp` | RAII 包装；STL 不跨本地 ABI；当前 Wasm profile 无异常、无 RTTI |
| Rust | `rust/Cargo.toml` | 路径依赖 `morrow-plugin-sdk`；`wasm-guest` 启用 guest 导入 |

SDK 不链接可信核心，不提供自建宿主、打开 Store、自选身份或授予权限的接口。权限由宿主绑定实际实例后核验。SDK 校验不构成对绕过 SDK 的不可信代码的安全边界。

## 当前接口

七种内容命令包括重命名、读取摘要、查询操作结果、读取附件片段、创建正文、编辑正文、读取正文片段。类型化输入／响应保留 UInt64 修订与偏移，响应必须与原请求及适用的操作 ID、目标、修订和片段范围关联。详细用法见 [正文 API](CONTENT_API.md)。

消息有界传递；运行消息上限 64 KiB，附件片段上限 32 KiB。调用成功不等于业务成功：`MP_OK`／`MP_CODEC_OK` 仍须检查响应类型和业务拒绝。取消、传输错误或失去完成响应不能推断提交已回滚，重试需保留操作身份并查询权威结果。整文件组装与摘要核对仍需由调用者完成。

任务接口接收宿主管理的命令和纯转换输入，返回关联完成、输出或固定业务失败代码。纯转换 handlers 必须在包中声明类型和限额；不能把计算成功当作获准修改内容。见 [任务契约](../docs/PLUGIN_TASK_PROTOCOL.md) 和 [包格式](../docs/PLUGIN_PACKAGE.md)。

UI 提供有界文档节点、事件和三语言样例；宿主验证会话、代次、修订及事件序号，再绘制或分派。当前通用协议含 column、row、text、button、textInput、toggle。基础文档往返不代表专业编辑器、全部视图扩展点或持久草稿已完整接入。见 [UI SDK](../docs/PLUGIN_UI_SDK.md)。

依赖 SDK 通过批准的 slot 发送字节输入并接收关联输出，不能自行选择提供者或扩张授权。宿主已有锁定和有界多层执行支持；完整异步服务及依赖图证据仍需建设。见 [主动依赖调用](../docs/PLUGIN_DYNAMIC_DEPENDENCIES.md)、[依赖图](../docs/PLUGIN_DEPENDENCY_GRAPH.md)。

C 响应句柄拥有其视图，释放后 span 失效；C++ 响应对象不可复制、可移动；Rust 返回自有值。指针和回调仅在其本地适配器生命周期有效，不是可序列化身份或能力。

## 编译与验证

需要 Rust、Cap’n Proto 编译器、Python 3、PowerShell、LLVM/Clang，以及 `wasm32-unknown-unknown` Rust 目标。C/C++ Wasm 使用固定 WASI sysroot 构建标准库，最终 guest 不获得 WASI 文件、网络、时钟等导入。准备脚本和支持限制见 [执行后端](../plugin_runtime/README.md)。

```powershell
# 先检查旧 SDK 原件能否在当前宿主运行，不重新编译或重打包 guest。
pwsh -File tool/verify_plugin_sdk_compat.ps1
# 当前源码、本地编解码器、类型化 C/C++ 与真实核心适配验证。
pwsh -File tool/verify_plugin_sdk.ps1
# 当前三语言 Wasm 构建与运行验证（内部也先执行旧原件兼容检查）。
pwsh -File tool/verify_plugin_runtime.ps1
```

固定契约随 SDK 分发，可脱离宿主源码构建。`tool/sync_plugin_sdk_contracts.py --check` 核对包括依赖调用在内的契约；更新它们之前必须遵守兼容规则，不能为消除检查失败而直接覆盖旧契约。

三语言内容／转换／UI／依赖样例位于 `examples/`；wire 黄金样本位于 `tests/fixtures` 和 `tests/ui_fixtures`。另有 [固定 Wasm 和完整包](compat/guest-v1-rc1/)，用于验证旧二进制；两类样本不可相互替代。当前完整运行证据以 Windows 为限，Wasm 可编译不等于其他平台产品已验收。

历史 test.11 的原生和正文增量、test.28–31 的依赖增量是当时的阶段结果；最新范围以源码、[路线](../docs/FUTURE_ROADMAP.md)及各版本报告为准。SDK 源码 API、本地回调 ABI、预编译库分发及更多开发诊断仍在推进；test.49 已补充项目模板和完整包验证入口。

## 许可

test.1 之后的第一方 SDK、示例和界面客户端使用 [AGPL-3.0-only](LICENSE)，详见 [NOTICE](../NOTICE)。第三方依赖保留各自许可；test.1 及更早历史发行版保留原许可。
