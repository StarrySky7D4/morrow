# Windows 异常启动等待边界与静态复核（v8，2026-10-13）

本次真实异常启动调用达到 1200 秒等待上限后仍未闭合；原始 Unknown 与原控制进程／读取器保留，不重放。实际退出、双流 EOF、原连接关闭与 Process 临时策略恢复均未确认，不能作 native 通过断言。

六个关键源码的双遍完整 SHA 核对零漂移，静态字段门禁未发现确定性错配。发现 kind4 发送确认晚于退出及双 EOF 时可能导致隔离回执缺失的条件性误拒绝风险；尚未证明本次触发或构成根因，未实施修补，也未运行对应时序回归。缺失／无效最终回执时保留 Unknown 的门禁未绕过。

此前四项 Windows 普通合成文件 IO 测试与 v6 正常原生会话通过结果保持原范围。本次仅更新进度文档，不新增产品编译或验收，不将正常路径和文件读取结果扩展为异常启动通过。公开 Dart 三个生产文件与选定夹具基线的差异、新读取修正未进入既有验收二进制的限制仍保留。

下一步通过单独审核的只读诊断获取固定合成证据，先以可控事件顺序回归验证静态风险，再决定最小修补；原调用不重放。诊断连接脚本、原始日志、机器和运行身份保留本地。

SDK26／G04、namespace、ancestor 持续 reparse 防护、生产安全与网络约束仍 OPEN／未验收；full_SDK_frozen=false，release_eligible=false。Agent 会话层与安全执行层完成后暂停准备测试预览，扩展执行层延期。版本、依赖锁、工作流、main、tag 与 Release 不变。

参见[结构化状态](startup-integration-progress-v8.json)、[v7 读取修补历史](startup-integration-progress-v7.md)与[v6 正常路径历史](startup-integration-progress-v6.md)。历史失败与 sealed Unknown 保留。
