# Drive 交付审查与 Windows 验证（2026-09-30）

本轮在 `codex/m03-stream-revocation-backpressure` 上继续开发，先快进到远端 `21f84aaebca31c8ef8d3bdfb3b9788f2a4cffb54`（新增提交仅修改文档），再审查、接入以下两份同基线源码交付。未推送、未创建标签或 Release；未修改既有冻结候选、失败记录或用户内容库。

## 来源和接入决策

来自 [Drive 工作区](https://drive.google.com/drive/folders/15Z4jRZuXU-p-BOT2jjoRff83QOZT-Ya-)：

| 交付 | ZIP SHA-256 | 决策 |
| --- | --- | --- |
| [partial-write-003](https://drive.google.com/file/d/1aYEnGwU3ADnTbDoZXPOC7-AzXTI69uHk/view) | `354d46825e414a43f9fe46cd615e45e7073f5bb3cc93e4a7283d84a709978ad8` | 核对 manifest 中 58 项长度及摘要；导入有变化的模块、测试、probe、runner 和原始报告 |
| [plugin-pack-prepublish](https://drive.google.com/file/d/1tsZTvLmsFEZu9LoWwtJeoKkcpjekCHtn/view) | `92878e9a84d363f75e66993662ba8657bdeabe190aad4d79ce26d07e50c3f250` | 核对 README 给出的三份源码/文档摘要；接入并补修编译前锁定边界 |

原始 ZIP、完整解包（含云端 Linux 和失败日志）保留于宿主 `morrow-backups/20260930-drive-review`。云端说明是待核验交付资料，其 Linux PASS 和 Windows 交叉编译不是本机运行结果。[云端原始报告](m03-partial-write-003-2026-09-29.md)按原日期保留；本报告补充 Windows 验证，不回写原报告为已通过。

## 本轮修改

- 流式响应在响应头/正文交付边界再次检查取消、原期限和动态 guard；已排队的正文不能在撤权后继续交付，撤权后取消保持生效。
- 管道写入按原帧长度、已确认前缀和操作 ID 记账；拒绝零进度、越界及错误操作完成。取消时正确区分已完成整帧与未完成尾部，不自动重发。
- 插件打包使用本次模块的临时字节副本，在安装前重新校验配置、SDK 选择/锁、原模块和副本。额外在编译前固定 SDK 选择和锁，拒绝编译期间显式换锁后将旧模块绑定到新锁的情况。
- Windows 本地 SDK 测试发现 `service_resources.capnp` 的 SDK 副本为 CRLF，宿主副本为 LF。两者 Git 内容相同，但严格字节比较拒绝服务模板。为这两个活动契约固定 LF 检出，未放宽摘要校验、未改冻结 SDK 原件。现有工程若锁定旧 CRLF 字节，应先检查差异，再显式更新 SDK 锁；工具不会自动换锁。
- 看板和插件状态补充当前证据；早期 IO 设计中的“codec 尚未实现”“全部执行不可用”已明确纠正为历史状态。

## 本机验证

Windows 11 x64，Rust 1.95.0，Python 3.14.5。日期按本地 UTC+8；回执采用 UTC，因而时间字段可显示 2026-09-29。

| 检查 | 结果 | 范围 |
| --- | --- | --- |
| 写入纯状态模型 | 8/8 | 部分成功、取消、错误 ID、长度等；不是 OS 故障注入 |
| 网络模块 | 21/21 | 5 项新交付边界 + 10 项客户端 + 6 项流测试 |
| Windows 窄范围 probe | 13/13 | 8 项状态模型 + 5 项真实同进程管道测试 |
| 原生宿主库 | 26/26 | 绑定、撤权、API、管道及状态模型回归；部分 authority fixture 模拟 owner，不启动子进程 |
| 原生宿主二进制 | 构建成功 | `cargo build --locked --offline --bins`；未据此声称完整产品运行 |
| Python 插件工程工具 | 62/62 | 预检、模板、SDK 锁和打包漂移；包工具测试使用 mock，不代表 C/C++/Rust 全模板实际编译 |

这些测试集合相互重叠，不能相加为独立场景总数。

证据：

- [Windows runner 回执](host/m03-partial-write-003/windows-20260930-001/receipt.json)及同目录日志：执行前后被测模块指纹一致。
- [原生首次回执](host/m03-partial-write-003/native-20260930-001/receipt.json)：23 项通过、3 项因缺少 `MORROW_OWNER_TEST_ROOT` 环境变量失败，保留原记录。
- [原生第二次回执](host/m03-partial-write-003/native-20260930-002/receipt.json)：补齐独立 owner/HTTP 测试目录及测试 client 路径，先重新构建二进制，再通过 26 项；源码前后未变。没有修改测试断言使其通过。
- [SDK 首次日志](host/m03-partial-write-003/sdk-20260930/sdk-tests-001.log)：61 项运行中的 17 个错误（含参数化子场景），均涉及服务契约换行差异；未翻判。
- [SDK 最终日志](host/m03-partial-write-003/sdk-20260930/sdk-tests-002.log)：修复检出格式并新增换锁回归后 62 项通过。该目录保存最终输入摘要及原生运行脚本；最终摘要不冒充测试前后回执。

真实 Windows 新增场景中，对端读到 1024/8192 字节后取消，观察到同一 OS 写操作待定、995 回收和 owner join，未报告完整帧或发起第二次写入。另一个场景在对端读完 128 字节后取消，实际完成只回收一次。这证明真实前缀交付/取消和完成后取消的边界，**不证明 OS 返回过非零的短成功 completion，也不是完整宿主撤权临界竞态验收**。

## 接下来

1. 在新候选中补完整 host/Core/child/HTTP 撤权与 deadline 临界路径、对端异常断开、真正的 OS 部分 completion 故障证据；保留控制准入到 OS issue 的间隙问题。
2. 扩展多会话并发、资源回收、同用户隔离及崩溃后 owner 核对。取消请求不能替代实际 join/reap。
3. 重新装配产品并验证 G0/原 84 项场景、系统选择器/人工审批及跨平台。现有 Windows 应用 ZIP 没有包含本次改动。

当前可将两份修订接入本地开发线继续推进；本轮未完成 M03 整体签收，未冻结 SDK。
