# 服务插件发现已批准资源

日期：2026-09-21；基于本地 `65e0a69`，应用版本仍为 `0.1.9-test.52+56`。

## 结果

主应用现可将明确选择并重新批准的出站资源交给声明 `service-resources-v1` 的插件。新增独立、带摘要的有界 Cap'n Proto 目录，使用受保护的 Invocation 头传递引用、允许方法和限额。实际条目取自已批准路由集合；外部伪造头先剥离，元数据不替代任何活跃授权。完整契约见[说明](../docs/PLUGIN_SERVICE_RESOURCES.md)。

旧包不接收新目录，原选择与调用行为保留；未选择端点不能从已保存记录或 Registry 批准隐式获得出站能力。目录参与服务请求身份，同政策重新批准不重复外发，变更政策与旧请求键冲突。新构造器对无效目录、scope 不匹配和无效选项均保留原 worker。

## 验证

- core 两文件 **17 项通过**：6 项目录 codec 测试与 11 项包 IO 声明回归。覆盖 8 项/全部方法/最大数值/持久凭据长度组合、完整服务帧、截断所有前缀、尾随/超长、版本/schema 摘要错误、重复头、非规范十六进制、字段排序/重复/边界与 feature 前置条件。
- network 三文件 **47 项通过**：managed_http 28、managed_content_service 10、managed_service_owned 9。新增目录凭据引用的实际 HTTP 调用，服务端收到宿主注入的凭据值而目录不包含该值；撤权后无新外发/意图。旧拥有者目录不能授权重连实例；错误目录构造后仍取回同一 worker/owner。
- 原生工作台服务 **15 项通过**。真实编译的 Rust/Wasm guest 解码宿主目录，替换请求体中刻意伪造的端点/凭据引用，再经过原 guest IO 完成真实 HTTP；响应关联原请求摘要。混合大小写、重复及 Connection 提名的伪造目录被剥离。同记录重新批准保持重放，修改修订后旧键冲突。旧 feature 包分别验证错误请求不会因隐式目录变为成功、正确模板仍可调用；空选择无网络效果。
- core/指定测试、network/指定测试与 workbench lib 严格 Clippy 通过；修改文件格式和 diff 检查通过。Rust/Wasm fixture 构建保留 core 的 7 项既有平台相关 dead_code 警告；原生 lib test 保留既有 prepare_write 未使用警告。

合计 **79 项相关测试**，不重复累计历史套件；本轮没有改动 Flutter 界面，也没有重跑窗口测试。实际测试日志在忽略目录 `build/service-resources-native-tests.log`，构建步骤见[fixture 说明](../workbench_host/tests/fixtures/service-outbound/README.md)。冻结 IO/service schema 与 `sdk/` 相对基线无改动。

## SubagentBridge

当前安装通过官方 CLI 实际执行 GLM/max 两次及 DeepSeek/max 一次。GLM 提供验证器和边界测试草稿，主代理修正了排除合法最大值、对不可变切片排序、错误字段/API 及不合法正向样本，再编译验证。DeepSeek 提供设计审阅；其关于构造器必然丢失拥有者的推断没有实现证据，现有直接移动和同一 owner 回收测试通过。旧宿主反向资格尚未运行，明确保留边界，不以推测宣称兼容验收完成。

任务：`task_15e09717be4e2669c5620c66`、`task_c7c7046ed116431af1bb1b76`、`task_b8e773c620a4423dbc7fc56d`。三次均完成并释放；provider 报告 input/output 分别为 340/1038、342/1533、421/949。这些是辅助调用的报告值，不是整个任务的成本或节省比例。结果保存在忽略目录 `build/bridge-resource-{validation,tests,review}.json`。

## 尚未完成

下一项是完整 Windows 窗口中选择资源、实际出站、运行中撤销与停止竞争。大帧分段、应用服务 TLS、Unknown 核对、文件系统和三语言 IO SDK 仍未完成。本轮仅原生源码/fixture 构建与限定验证，未生成完整 Windows 新预览，未推送或发布。
