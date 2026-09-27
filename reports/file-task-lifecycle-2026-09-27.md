# 文件任务生命周期与恢复验收

日期：2026-09-27。基于 `b9225f6`，应用仍为 `0.1.9-test.56+60`。本轮直接在 `codex/io-safety-refactor` 开发目录推进；未提交、推送或发布。

## 修复

完成回执携带 `Finished` 的同时，宿主可能已经观察到实际 worker 退出。原 `FileTaskSession` 先接收退出状态，将仍处于 finishing 的任务标成中断；随后消费 Finished 又标成 verified，导致界面同时显示“校验完成”和“任务中断”。

新增两个确定性用例，分别覆盖正常回收和维护失败。修复前均因 `FileTaskNotice.interrupted` 失败，日志为 `build/file-lifecycle-20260927/finish-exit-before.log`。现在仅在原任务身份通过校验、预期为 Finished 且完整字节摘要已验证时，避免把同回执的正常退出标成中断。维护失败仍保留 recoveryRequired，修复/确认按钮仍以实际退出和回收状态为条件；未放宽 Unknown 重试、授权或摘要校验。

## 本轮验证

| 范围 | 结果 | 日志 |
| --- | --- | --- |
| 独立 Dart 文件协议/会话 | 30 通过，无跳过 | `build/file-lifecycle-20260927/session-regression.log` |
| 文件、HTTP、服务 UI 组合 | 66 通过 | `build/file-lifecycle-20260927/ui-regression.log` |
| 四个改动 Dart 目标分析 | 无问题 | `build/file-lifecycle-20260927/analysis.log` |
| 最终成品真实文件任务 | 3 通过，无跳过 | `build/file-lifecycle-20260927/final-native.log` |
| Windows Release | 构建成功，118.3 秒 | `build/file-lifecycle-20260927/windows-build.log` |
| 实际 Release 新临时库自检 | 退出 0，无 Flutter 错误 | `build/file-lifecycle-20260927/release-self-check.md` |

UI 回归增加：进行中离页返回不重发、隐藏停止展示轮询、切换内容库后丢弃迟到选择结果、未知启动不重复提交、取消/修复在途或结果未知时不重复操作、观察到实际 running/recoveryRequired/reclaimed 状态后恢复正确按钮。HTTP 面板在文件尝试（含 Unknown）存在时停止自己的控制与轮询，解除后保留原请求草稿，没有隐式外发。

真实 Windows 测试除既有捕获/删源/分块/摘要/撤权路径外，增加两项：

1. 分块已经 Ready 后取消，拒绝继续交付；等待实际 owner 退出并确认后，内容库恢复本地访问。
2. 真实宿主已接受 start，但 Dart 包装层主动丢失一次启动回执；使用 ioStatus 只读恢复，`FileTaskSession.verify()` 完成 90,007 字节及 SHA-256 校验、有限预览、Finished 和最终确认。启动计数始终为 1，成功历史不带中断通知。

复验使用 `tool/verify_file_task_windows.ps1`，从固定 Rust IO Wasm 临时打包测试插件，在临时内容库运行；没有安装到用户库。首次扩展测试使用上一轮的同源码 Release 宿主，最终复验再次使用本开发目录本轮构建成品。

## 本地预览

目录：`build/windows/x64/runner/Release`，运行 `morrow_studio.exe`，须保留同目录 data、DLL、宿主和 plugins。

- Dart AOT `data/app.so` SHA-256：`d790cf937a7bc49c27ed76bed4e5f7b9148b086d37ea23c71eb15c015ce9b59c`。
- 宿主 SHA-256：`d6e869d109e778be946717b0a3e4d021aff06e1058f8c8a527e36786d5378b8d`。
- 继续显式核验并安装兼容 workbench 原包 `c71dc27866378894d5288053238fef9e0ef15f6b578449279bc681e7fe8b560e`；源码 guest 另行编译，不覆盖已批准的同版本包身份。
- Native runner EXE 本身无源码变化，可保留旧时间戳；本轮 Dart 修复体现在 AOT 数据与完整构建结果中。
- 构建保留既有 5 条 Rust dead-code 警告。

## 后续边界

本轮缩小了原生读取路径的验证缺口，不等于完整文件系统或 SDK 冻结。系统选择对话框仍未经过人工点击验收；UI 选择器通过注入结果验证，而真实宿主测试直接提供临时文件路径。维护失败按钮与完成同回执竞态有确定性模拟证据，不宣称真实磁盘故障已在本轮注入。跨重启 Unknown 的业务核对、目录授权、创建/替换/删除、有界写入、TLS/凭据生命周期与其他平台资格仍开放。

下一开发切片优先明确目录授权与写入的独立能力、路径/重解析边界、持久副作用意图及 Unknown 核对合同；不得把目前只读选择器路径直接升级为任意文件系统授权。正式实现仍须按该合同逐项验收。
