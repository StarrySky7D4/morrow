# 独立文件变更 guest 契约与 Rust 编解码

2026-09-27，`codex/io-safety-refactor`。完成独立实验性 `mutation.capnp`、Core 与 Rust SDK 两套编解码、共享正反向量和同步门禁。旧 IO v1 与兼容原件未修改。尚未连接 guest import、C／C++ ABI 或文件执行；SDK 未冻结。本轮未提交、推送或发布。

## 实现与复核

请求包含 callId、宿主签发引用、命令提交身份、操作编号与期限，提供 PrepareCreate／PrepareDelete／Chunk／Commit／Execute／Query／CancelPlan／Release。响应核对全部相关字段及动作，拒绝伪造成功阶段、错动作结果和错误已传字节数。限制从 schema 生成常量读取，帧 128 KiB、块 60 KiB、内容 16 MiB；有界遍历／嵌套和字段复制。编解码无文件／网络效果、无自动重试。

`tool/sync_plugin_sdk_contracts.py` 已纳入独立扩展，新增专门的漂移拒绝测试。21 个二进制向量由外部 Cap’n Proto CLI 生成：9 个合法请求、7 个拒绝请求、3 个合法响应、2 个拒绝响应；两端分别消费同一原件，工具校验精确清单与 SHA-256。测试不是由各自 encoder 生成输入后仅做自身 round-trip。

复核期间修复了 SDK 只拒绝 ASCII 控制字符而 Core 拒绝所有 Unicode 控制字符的差异，并添加 U+0085 原件与 U+009F／中文字符串用例；修复 SDK 将错误动作的 OutcomeUnknown／None 当成普通失败接受的问题。Completed Chunk 的 stagedBytes 必须对应本块末尾；解码响应先验证请求，再复制操作标识。独立只读复核未再发现决定性的相关性或有界分配问题。

Cap’n Proto 合法分段不要求唯一二进制表达；尾随检查指声明消息之后的额外字节。运行时提交去重将绑定已验证语义或规范命令编码，不依赖原始帧布局。

## 最终验证

| 检查 | 结果 | 日志 |
| --- | --- | --- |
| Core mutation + 旧 IO codec | 7 + 23 通过 | `build/mutation-guest-core.log` |
| Rust SDK 全量测试 | 71 通过，0 失败／跳过 | `build/mutation-sdk-test.log` |
| SDK 全目标严格 Clippy | 通过 | `build/mutation-sdk-clippy.log` |
| 契约同步／向量门禁 | 8 通过 | `build/mutation-guest-tools.log` |
| SDK 项目锁定 | 12 通过 | `build/mutation-guest-sdk-lock.log` |
| 同步 CLI | 所有镜像一致 | `python tool/sync_plugin_sdk_contracts.py --check` |
| transport 原件完整性 | 17 个文件验证通过 | `python tool/plugin_transport_baseline.py verify` |
| 旧 guest 原件完整性 | 36 个文件／13 对 Wasm 与包验证通过 | `python tool/verify_plugin_sdk_baseline.py` |

本轮没有重新执行旧三语言 Wasm 原件；上表后两项是完整性校验，不是新运行时验收。也没有完整应用构建、C／C++ 新扩展编译或其他平台验收。SDK 锁定测试最初直接按模块运行因其邻接测试 import 路径失败，改用该套件的 unittest discover 入口后 12 项全部执行通过。

## 下一实现阶段

按 [guest 扩展说明](../docs/PLUGIN_MUTATION_GUEST.md) 和 [SDK 接入计划](../docs/PLUGIN_MUTATION_SDK_PLAN.md) 继续 C／C++ 类型化接口及同向量验证，再连接独立 import 与原 owner。

运行时复核确认已有 Wasm 暂停／恢复机制可复用，不能在 import 内排入并等待同一 owner 队列。必须显式选择 managed mutation job、拒绝只读／raw owner、计费并复核撤权。租约绑定用户批准的精确操作／内容，Execute 另有可信许可，不能拿路径批准替代内容批准或绕过应用二次确认。非空内容中间故障、三语言真实 Wasm 及旧原件运行仍是后续门槛。
