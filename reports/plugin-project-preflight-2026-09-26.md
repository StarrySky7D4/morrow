# 插件工程离线预检与服务出站契约检查

2026-09-26，基于 `49937b4bc5f923c16f50a9d13df85ad0e2e125d7`。应用仍为 `0.1.9-test.56+60`，主线基准保持 `d9c0431`；本轮不使用 CI/Actions、不推送、不发布。

## 新增功能

`tool/morrow_plugin.py validate PROJECT` 提供不依赖编译器的工程预检。复用工程元数据、路径、能力／预算、SDK 契约／版本以及 Rust 库／路径／feature 的既有约束，输出 TOML 声明摘要，便于开发者和接入工具在构建前检查。支持目录或 plugin.toml 路径；中文、空格和引号均可正确解析。命令不会启动编译器、Cargo、宿主或插件，不授予权限，不创建构建输出或消费旧二进制。

正式 build／pack 复用同一个预检函数。Rust Cargo 绑定不符现在在创建 build 目录及查询工具之前失败；旧包和旧模块保持。摘要列出声明的能力、IO handler、依赖槽、资源发现、打包参数及本 profile 已核对的契约摘要。标准工程不从源码猜测模板种类。

新增 `verify_plugin_projects.py --preflight-only`：在新目录生成三语言六类基础模板及三种 service-http 模板，逐一执行真实 CLI，检查摘要身份与范围，并核对源文件没有变化、build／dist 没有出现。原完整资格路径保留，并增加打包前预检；本轮没有运行完整编译／执行路径。

## 实际修复

此前 service-http 生成模板时检查 IO 契约，但后续编译服务工程只检查 service／service_resources。已生成或手工声明 HTTP 出站的服务，如果外部 SDK 的 IO 契约或 codec 版本变化，可能绕过早期相容性检查。现在所有声明 http-request 的服务在 validate／build／pack 中都检查 IO，包括未启用资源目录的自定义服务。

新回归分别修改外部 SDK 的 io.capnp 和 IO VERSION，证明在编译前失败且没有输出目录；没有修改 guest、schema、原包或冻结契约。

## 验证

| 检查 | 实际结果 |
| --- | --- |
| Python 工具全量 | 106 项通过、无跳过，含新增 8 项预检和 1 项资格摘要门槛；模板参数化子场景不另加总 |
| 三语言模板真实 CLI | 21 个生成工程全部预检通过；逐项 TOML 输出及汇总保留，检查前后工程文件相同 |
| 无工具链环境 | 清空 PATH、传入不存在的 sysroot，C++ service-http 预检仍通过；不把它当作编译资格 |
| 失败路径 | 声明非法、Rust lib／入口／SDK 路径／feature 不匹配、深层输出 symlink、IO schema／版本漂移；没有编译或部分成功输出 |
| 既有证据保护 | 实际对已有资格目录再次调用，按预期退出 2，所有已有文件摘要保持 |
| SDK 契约 | `sync_plugin_sdk_contracts.py --check` 通过 |
| 冻结原件 | 17 个 transport 固定文件摘要保持 |

日志和 21 份 TOML 摘要见 [本轮证据](evidence/plugin-project-preflight-2026-09-26/)。源码摘要用于关联本轮记录，不是第三方签名。没有重新运行 Rust、Wasm guest、Dart、Flutter 或 Windows 测试；历史通过数不计入本轮。

## 使用与剩余边界

```sh
python3 tool/morrow_plugin.py validate /absolute/plugin-project
python3 tool/verify_plugin_projects.py --preflight-only --output-root /absolute/new-evidence-directory
python3 -m unittest discover -s tool/tests -v
```

需要 Python 3.11+ 和当前 Morrow 源码／兼容 SDK；没有新第三方 Python 依赖。预检不解析完整 Cargo 依赖图、不验证锁文件能否完成解析、编译脚本或 handler 实现，也不验证平台、授权、提供者锁、凭据或端点。TOML 是声明摘要，不是源码快照或构建收据；构建时必须重新检查。真正构建仍由 Cargo --locked、核心打包和准备流程约束。

SDK 独立分发、第三方完整构建接入、Windows／Flutter 产品验收、九语言文件面板文案、完整文件写入／目录、流式网络和跨进程恢复仍开放。本轮补齐开发工具入口及一处契约检查遗漏，不宣布整体插件底座稳定收尾。

源码、累计增量 bundle、累计证据、报告、恢复说明和校验清单继续保存到 Google Drive 工作检查点。
