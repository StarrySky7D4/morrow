# 三语言结构化业务错误验证

日期：2026-09-11，Windows x64；应用基线 0.1.9-test.10。任务契约升级为 v3，guest ABI v2、内容消息 v6、包 schema v1 不变。主 Flutter 资料未迁移。

## 实际执行

Rust／C11／C++17 模块分别重建并打包三个纯任务处理器：bytes.reverse、bytes.ascii-uppercase、bytes.require-ascii。每个处理器处理空输入、非空 ASCII、含中文／emoji 的 UTF-8 原字节、全字节值和 64 KiB 输入。

三语言共 45 次队列任务：36 次输出由可信宿主独立计算并逐字节核对；9 次非 ASCII 验证失败完整执行并返回 UnsupportedInput 和固定消息，只有 failure 字段有值。另有 9 次未注册／类型不匹配请求在 guest 执行前被拒绝，保留全部 fuel。全部纯任务零核心调用，排空后重新打开 SQLite，原内容与事件不变。业务错误没有被当成执行崩溃，也没有触发自动重试。

## 回归与边界

- 核心严格格式／Clippy、109 项默认测试、7 项故障恢复、自检、CLI 与 wasm32 核心检查通过。
- 运行库 packages 配置 37 项测试通过。新增 4 项错误测试覆盖四类错误码、Unicode／1024 字节消息、任务原输入绑定、错误版本、缺失／混用结果、空／控制字符／1025 字节消息以及内容回复隔离。
- 实际 WAT guest 返回业务错误后 trap 或非零退出，均不交付错误；预先取消也不交付。成功输出上限为 0 的处理器仍可报告独立有界错误。任务未产生内容事件。
- SDK 15 项测试通过。新增 C ABI 检查四种代码、非法代码、无效 UTF-8、空消息、超长消息和不足输出容量；失败时长度归零且不改写输出区。
- 三语言旧内容命令、ABI v1 包、队列与撤权、C++ trap 回归通过。原生 C／C++ 严格构建和 Rust DLL／SQLite 授权链路通过。
- core-web 的实际 wasm32 配置检查通过。未进行 Flutter UI、浏览器插件或其他平台运行资格验证。

日志：build/core-test.10/failure-verification.log、build/plugin-runtime/failure-verification.log、build/plugin-sdk/failure-verification.log。

复现：tool/verify_core.ps1 -Web；tool/verify_plugin_runtime.ps1；tool/verify_plugin_sdk.ps1；cargo check --locked --manifest-path core-web/Cargo.toml --target wasm32-unknown-unknown --target-dir build/core-web-package-check。

## 本次产物

包目录：build/plugin-packages/55fd13c0d22f44ffa4986a376b7d9b73。以下均为实际字节 SHA-256，不是作者签名；历史报告保留旧 schema 对应的原摘要。

| 包／模块 | 字节 | SHA-256 |
| --- | ---: | --- |
| c-rename.mplugin | 42117 | `a570ff859251518ec7c1db9c64c914fa15dd164557520f9c834f455446da0d47` |
| c-task.mplugin | 49015 | `c41538dc0b91fa582dd3858e7f5dd2039da0d266ec511275a7159a422d7da6eb` |
| c-transform.mplugin | 44373 | `a116866b5bddc8fc3308d669c8b8a537acd2a62d8273b96b2cb0eb182d0e2def` |
| cpp-allocator.mplugin | 44511 | `09813d1b0593c43aac12551d174fc39e10b86fd6d40c3b83e67dab2658f894d3` |
| cpp-rename.mplugin | 43350 | `864f9e4b2ffece13f2b2e8a339fe5c506c9e6e8777f571c6b52706a46958a6c0` |
| cpp-task.mplugin | 49791 | `897c28afaba007579d482b504ae01a2f4ab1ce37aa4ef481d44f3cdca04e7303` |
| cpp-transform.mplugin | 45616 | `f3fc95e90bd5a6a686513331f068bf5b671f623a77d33698c1ab057a23a0a0d9` |
| rust-rename.mplugin | 51506 | `6248a68f2c0b5837d186682c9871984d67a2dacce5aeeaf414c0006dace5d252` |
| rust-task.mplugin | 51218 | `c07223ded27b1be5bc84795d6ab1b563811dfe641467924ad3367bd78a53ad87` |
| rust-transform.mplugin | 51251 | `045cc13d2f900ac09007e5ee3ce6b2b11b90878549ca222af5f0b6371bb9a079` |
| morrow_example_transform.wasm | 94343 | `a784656ffba1568d98b47b05c503422fbc66ad5efb5fa50f3b6b40bffb877613` |
| c_transform.wasm | 79353 | `9d8bbb45c0169171e191c0d60f171e983b6d89a21e5be6e750010b7b3f745415` |
| cpp_transform.wasm | 81172 | `b4c65840e1abb95c71a23cceb13f54aa3a4324d9f470242eaa7fa6fd912a5288` |

## 边界与后续工作

业务失败是插件提供的声明，不是核心权限或事务回执；宿主只核验协议、关联和上限。execution=Ok(0) 仅表示完整执行完成协议，调用方必须区分 output 与 failure。内容任务不能用插件错误跳过实际核心回复匹配。

任务 schema v1／v2 包在当前加载器中拒绝，开发者须同步 SDK 后重建、打包。没有主应用数据迁移、远程推送或 Release。下一步继续类型协商、跨包选择、修改提案及声明式 UI 事件／渲染；当前错误尚未显示在 Flutter 界面。
