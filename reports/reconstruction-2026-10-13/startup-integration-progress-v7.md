# Windows 密钥读取边界与限定验证（v7，2026-10-13）

## Windows 密钥读取边界修正与限定验证（2026-10-13）

本轮仅合入 `audit/src/keys.rs`：Windows 最终叶使用同句柄、有界读取并拒绝 reparse／元数据不稳定；不新增 unsafe，不修改 provider、DPAPI 或 Linux 原读取表达式。真实编译与 4 项普通合成文件 IO 测试通过，包括实际文件 symlink 的拒绝，均未跳过；独立回读零漂移。

失败启动 runner 的一个本机静态目标分析退出 0、双流 EOF，No issues found!；未执行 runner main，不能替代 native 验收。新的真实失败启动验收仍待闭合；本次仅保留待定状态，不作通过断言。

公开 Dart 三个生产文件仍比实际选定的正常／失败夹具基线旧，本轮未全量合入。新读取修正也不在既有正常验收二进制中；不将旧二进制结果当作本次修正的产品验收。

详见[本次源码与验证边界](startup-integration-progress-v7.json)。旧 v1–v6、历史失败与 sealed Unknown 均保留，不重放。SDK26／G04、ancestor、namespace、生产安全与网络约束继续 OPEN／未验收，release_eligible=false；不宣称 SDK 冻结。Agent 会话层与安全执行层完成后暂停准备预览，扩展执行层延期。版本、锁、工作流、tag 与 Release 不变。

参见[结构化结果](startup-integration-progress-v7.json)与[正常路径历史快照](startup-integration-progress-v6.md)。原始运行日志、合成文件、运行包和本机控制脚本不公开。
