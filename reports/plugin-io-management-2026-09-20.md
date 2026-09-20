# 主应用 IO 类别管理验证

结论：`PASS_SCOPED`。基于 `8d2ebc5`，主应用插件库已接通独立的网络／文件类别批准，经过真实 Flutter→Rust 私有协议验证并持久保存。具体端点、凭据管理界面及实际主应用网络任务仍未接通；整个插件系统和 IO-E2 保持进行中。

## 行为变化

- 展示十类 IO 声明和已批准类别，明确保存／全部撤销。保持原按钮、主题、中英文与窄屏布局。勾选不授权，保存不启用插件，不修改内容批准。
- 原 Manager／Registry 校验包摘要、修订与声明子集；未知、重复、超限及未声明类别被拒绝。非空批准必须验证包；已运行的管理器在包文件丢失时仍能清空全部 IO 批准。
- 保持缺包启动时的原完整性拒绝，不把损坏安装当作可执行内容。恢复完全相同的原包后，先前撤权仍然有效。
- 后端切换使用会话代数，防止 A→B→A 接收旧 A 的目录／结果，旧操作 finally 不再清除新操作的忙碌状态。旧表单关闭失败不污染新后端，也不重复销毁控制器。
- 新私有动作和独立 IO 字段同时生成 Rust／Dart 绑定及摘要；不修改冻结 SDK 或原 guest。版本仍为 `0.1.9-test.52+56`。

## 验证

| 范围 | 结果 | 本地证据 |
| --- | --- | --- |
| core Registry 与 IO Registry | 20 通过 | `build/plugin-io-ui-core.log` |
| runtime IO binding 与 Manager | 19 通过 | `build/plugin-io-ui-runtime.log` |
| workbench_host 目录与 IO 控制，Release | 15 通过 | `build/host-io-control-final.log` |
| Flutter 插件库、工具标签和语言界面 | 25 通过 | `build/plugin-io-ui-widget-final.log` |
| 真实原生进程 IO／目录／表单传输 | 5 通过，无跳过 | `build/plugin-io-ui-native-tests-final.log` |
| 语言资源包 | 4 通过 | `build/plugin-io-ui-language-tests.log` |
| 语言资源生成器 | 6 通过 | `build/plugin-io-ui-i18n-tests.log` |
| core 定向严格 Clippy | 通过 | `build/plugin-io-ui-core-clippy.log` |
| workbench_host 全目标／全部特性严格 Clippy | 通过 | `build/plugin-io-ui-host-clippy.log` |
| 改动 Dart 文件定向分析 | 无问题 | `build/plugin-io-ui-analyze-final.log` |
| 宿主 Release 二进制及测试夹具生成器 | 构建通过 | `build/plugin-io-ui-native-build.log` |
| core 默认 wasm32 库检查 | 通过，7 条既有 dead_code 警告 | `build/plugin-io-ui-core-wasm.log` |
| 私有协议与中英文资源重新生成校验 | 通过 | `tool/generate_workbench_client.py --check`、`tool/build_i18n.py --check` |
| 冻结 SDK 完整性 | 36 固定文件、13 对原 Wasm／包通过 | `tool/verify_plugin_sdk_baseline.py` |
| Flutter Web Release JavaScript 构建 | **失败**：既有生成代码的 64 位 schema ID 不能精确表示为 JavaScript 数值 | `build/plugin-io-ui-web-build.log` |

共 94 项相关测试通过，非全项目回归；其中新增 15 项：core 1、host 5、Flutter 界面 8、真实原生传输 1。不重复累计子代理专项或前期复跑。

真实原生测试使用新构建的宿主、既有内置工作台包和明确标识的合成 IO 声明包，经历实际进程关闭重开，证明类别批准与撤销持久化、过期修订拒绝、内容批准／enabled 保持。`prepare_io_control_fixture` 使用冻结 Rust task 的原模块，只生成另一个明确命名的元数据夹具，拒绝覆盖已有文件。这个夹具没有实际发出网络请求；不能用它声称网络执行已接入。

界面测试包含 320px 英文布局、独立批准、撤销、关闭失败阻止保存、冲突不自动重试，以及可控 Future 的 A→B→A 迟到回包和旧关闭失败。宿主真实 UI 测试证明无效扩权不撤销合法旧会话；纯 InlineUi 当前拒绝 IO 声明包，因此没有把这个测试冒充实际 IO 作业撤权。

## 故障与审查记录

缺包重开测试初次触发原 Registry 完整性拒绝，修正测试预期并恢复同一原包后再验持久撤权，生产启动校验未放宽。原日志保留于 `build/host-io-control-initial.log`。

新增原生测试初次使用了不符合既有清理函数约束的临时目录前缀；统一为受支持前缀后复跑通过，原日志保留于 `build/plugin-io-ui-native-tests.log`。自动审批拒绝清理旧测试目录（`blocked by policy`）；`C:\Users\Administrator\AppData\Local\Temp\morrow-io-control-d995415a` 已保留，没有绕过限制。

现有子代理完成宿主实现及独立生命周期复核，发现并修正上述两项异步问题。按用户指定以 `deepseek-flash / max` 尝试了最小 Rust 片段审查，但本机执行授权过期，新写授权也未被运行中的服务登记，调用均在执行前拒绝；本轮没有 DeepSeek 审查结论。

Web 失败定位为未修改的 `packages/morrow_core_client/lib/src/generated/ui.capnp.dart` 中反射 schema ID 的大整数常量。当前依赖的 schema metadata 用 Dart `int` 存储 64 位 ID，生成器未提供关闭元数据的选项。本轮没有截断、舍入编号或跳过协议校验。该项单列后续修复，需要验证精确身份与原二进制向量，而非仅使编译变绿；Wasm dry run 成功不代表完整 Web 构建或浏览器运行通过。

## 后续顺序

按[管理接口合同](../docs/PLUGIN_IO_MANAGEMENT.md)继续：原 Store 的有界批准／凭据元数据列表与录入轮换；保留原审计 Session／HostRuntime／Pool 唯一权威的作业所有权接线；短响应任务协议与实际主应用 HTTP 链路。Web 大整数生成代码问题并行修复。具体资源批准、主应用任务、文件系统、服务发布、Unknown 核对与三语言 SDK 仍未完成。

本轮仅本地开发、验证与提交，未推送、发布或生成安装包。
