# 插件 IO 三语言 SDK 增量

2026-09-24，基于 test.55 的 `c04ae1407faf44688fd006d2fbee0e5b2b7ffec4`。状态 **PASS_SCOPED**；源码 SDK 仍为 test.50，应用仍为 test.55+59。本轮未提交、推送、打应用包或发布；并非 SDK 冻结。

## 改动

- Rust 新增 IO 请求／响应编解码和 Wasm 读取、单次调用、原帧完成接口；C ABI 提供有界编码、验证、拥有型响应；C++ 提供拥有型请求和不可复制、可移动的响应。
- 添加三语言最小 Wasm guest；请求输入使用原始 IO 帧，避免错用普通任务 Invocation。响应同时检查版本、schema、callId、原请求精确字节摘要和动作约束；完成时转交原响应，不重建或伪造回执。
- 接口在调用前拒绝非法输入，传输或关联失败不自动重发。业务拒绝／OutcomeUnknown 与传输失败分开；HTTP 429 的合法状态、响应头、正文可保留。
- 原 IO schema 不修改，新增 SDK 镜像和同步检查。Read／Finish／Cancel／SubmitHttp 对接已有宿主；SubmitFileRead／Poll／QueryOperation 只提供实验编码形状，当前核心仍标记 Unsupported。原 guest-v1-rc1 不增加承诺。
- 新增 `build_plugin_io_wasm.ps1` 和 `verify_plugin_io_sdk.ps1`。普通 Cargo 显式忽略需要三种外部编译产物的五个用例；专项入口传齐三种产物、显式执行，缺少任何一个即失败。
- `verify_plugin_sdk.ps1` 纳入 C／C++ IO 原生验证，先构建当前 DLL 与绝对路径夹具。运行时 lockfile 仅跟随已有 workbench manifest，从 test.52.1 校正为 test.54.6。

## 验证

| 检查 | 结果与边界 |
| --- | --- |
| 完整 SDK 验证入口 | exit 0；fmt、严格 Clippy、Wasm check 通过；Rust 53 项通过，含 IO codec 5 项及 IO C ABI 6 项 |
| C／C++ 原生调用 | 编译运行通过；精确原帧关联、拥有型视图、容量边界及 C++ 移动语义 |
| 实际 Rust DLL／SQLite 适配器 | 内容授权、关联、重复提交、完整性及零缓冲泄漏检查通过 |
| 旧插件原件 | 36 个固定文件、13 对 Wasm／完整包摘要检查；12 项运行时兼容测试通过，未重建旧 guest 或包 |
| Python 契约／基线防护 | 15 项通过；SDK 新 IO 镜像与当前核心一致 |
| IO Wasm 专项 | 五项用例均执行 Rust／C／C++，零跳过；三种 guest 实际编译 |

IO 专项包含受管 FileBroker 真实 Read／Finish、缺批准拒绝绑定、跨实例引用、未知引用 NotFound、坏输入在调用前拒绝、错关联响应不完成任务。核心可编码跨实例 Denied，guest 可验证它；受管载体还会以 InactiveConnection 抑制跨所有者交付，这是原有权限保护，未为测试放宽。

HTTP 用例由核心编码 429 响应，经过真实 Wasm runner 并验证原帧、头、正文及错关联拒绝。**它没有经过真实 HTTP socket，也不构成完整双向网络 SDK 资格。** 本轮不新增其他平台或 UI 性能结论。

运行日志保存在 `build/sdk-full-qualification.log` 和 `build/sdk-io-qualification.log`；产物及日志 SHA-256 见 [机器可读记录](plugin-io-sdk-2026-09-24.json)。首次 IO 构建因活动 worktree 下缺少默认 sysroot 路径失败；随后显式传入现有根项目的 WASI sysroot，两轮完整构建与资格测试均成功，最终日志对应三语言强制检查版本。

## 后续顺序

1. 将现有 HTTP transport／服务节点能力接入真实三语言包验收，补齐项目工具 IO 模板、端点及凭据绑定入口，验证真实方法／正文／拒绝／Unknown，不能仅靠 codec 往返。
2. 按冻结规则设计并实现完整文件系统、写入和异步任务／恢复协议；当前预留动作不能冒充已实现。
3. 补齐 OAuth／多账号、上传下载流、SSE／WebSocket、跨重启 Unknown 核对及第三方独立接入；完成平台矩阵后再评估 IO SDK 冻结。

开发入口见 [IO API](../sdk/IO_API.md)。正式 UI autosave、历史审核与平台开放项不在本轮关闭。
