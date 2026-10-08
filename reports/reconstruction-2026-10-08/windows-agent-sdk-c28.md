# Windows Agent SDK：C28 开发源码同步

更新：2026-10-08。目标分支为 `codex/windows-sdk-convergence-20261005`，基线提交为 `e83cdf1d3506b001d07ffceaaa9f98b688bcc150`。本次同步开发源码与公开的限定进度说明，不更新 main、标签、应用版本或 Release。应用源码版本仍为 `0.1.9-test.58+62`。

## 当前可用范围

Agent 接口仍为 experimental。会话、进程控制、插件目录与可信原生 owner 已有实现；工作台通过原 Manager、Core、Store、runtime、连接和批准链承载任务，保留 Unknown、撤权与资源债务，不能依据页面状态重新执行不确定操作。新增的 Wasm 客户端及包装插件是独立能力，不给普通插件目录管理权、任意路径访问或任意命令启动权。

本次收录 C11–C16 的目录与 Agent 增量，以及后续隔离验证中形成的会话、原生适配、生命周期和诊断源码。派生的 Windows 执行库与冻结上游分别保存；原始上游输入与原锁不得被派生实现覆盖。构建路径需要统一为仓库相对路径，并通过新的依赖闭包检查；原隔离目录的测试结果不能自动证明移植副本已经运行成功。

## 已有验证及其边界

| 范围 | 已观察结果 | 不代表什么 |
| --- | --- | --- |
| 原 protected owner 的会话链 | 原 session 组返回；创建、写入、检查点、父会话快照、分叉及子会话快照在同一原 owner 下完成，原 worker join 与持久化确认路径有记录 | 不代表整个会话产品、Codex IPC 或跨平台均通过 |
| Windows 原生启动 | context、worker、proposal、可信 review 与一次性 claim 已返回；随后 Start 为 Unknown，未取得可确认的 Started handle | 不证明没有子进程或副作用，不允许重放 Start |
| 原 owner 清理 | 清理与显式修复均仍记录 pending，资源债务保留 | 不属于正常回收成功或 SDK 验收 |
| H008 诊断源码 | 新 harness 编译及两个库共 8 项纯诊断测试通过，原始失败另行保留；源码与实际产物分别复核 | 8 项不是 OS 启动、沙箱或端到端验收 |
| 静态栈帧分析 | 固定入口分配由 192,248 降至 4,280 字节；`Workflow::step` 增加 32 字节，所选其余 12 项不变 | 不证明动态峰值、完整调用栈或安全余量 |

H008 的固定保存产物 SHA-256 为 `5b88968e81e5211f89cfe997c7d17f3d848328b2fd6595b0461584b98a362541`。这用于关联既有隔离构建证据；本提交不分发该二进制。最新诊断版在新的 Windows 场景下的 transfer、文件预检与 live 测试仍为 **NOT_RUN**，尚未解决原生 Start Unknown。

## 诊断与资源生命周期

诊断使用固定错误类别和单调阶段标志，保留首个显式失败；不把原错误正文、账户、命令环境或凭据放入 Debug 输出。诊断信息不产生执行权限，也不延长原 grant。不同采样时点不是一个联合原子快照，缺少某标志也不能推断从未发生效果。

任务隐藏、视图释放和业务取消继续分开；原 owner、native 资源、事实历史、scheduler 与 worker 的实际 join 分别核验。取消请求、资源计数为空、强制结束或外部环境重置，均不能代替原任务的正常 Close/ACK、事实确认与债务清偿。

## 后续顺序

1. 核验本次相对路径移植后的依赖图、固定上游身份与限定源码检查，保留所有失败和未运行项。
2. 对精确诊断产物执行一次新的 Windows 文件预检与原 session/native 链，定位 Start Unknown 的具体阶段；不采用旧 owner，也不重放旧 claim 或 Start。
3. 按实测结果做最小修补，复验实际 exit、stdout/stderr EOF、事实身份与修订、原 owner 清理和 scheduler join。
4. Agent 会话层及安全执行层通过各自验收后暂停，准备下一次测试预览；扩展执行层不作为这一暂停点的前提。

完整 Codex A/OS IPC、PTY/stdin/resize、完整网络隔离、其他平台资格、SDK26/G04 与 SDK 冻结仍为 **OPEN**。目前没有达到安全执行层完工或测试预览发布条件。

## 本次移植副本的实际检查

四个 Windows 依赖图均以 locked/offline 模式退出 0：Workbench、原生适配、H008 harness 和 SessionProcessHost 的 normal/build 本地包分别为 55、43、56、7；所有本地包路径位于本次检出目录内，manifest 与锁文件前后字节不变。包数量不是通过测试的方法数。

现有 R2 工具回归 42 项中 41 项通过、1 项跳过；目录 guest 构建工具的 15 项合成回归通过。首轮 R2 测试因临时目录祖先中的既有 Cargo 配置而被门禁拒绝，退出 1 的记录保留；改用新的专用合成临时目录后通过，未放宽门禁或修改系统配置。

[结构化检查摘要](publication-validation.json) 区分元数据、合成回归与未运行项。此次未编译移植副本的 Rust 库或 harness，未运行其 Rust 测试、Windows live 链、Flutter 产品或其他平台验收；既有隔离 H008 编译和 8 项诊断测试保持原身份与范围。

## 相关入口

- [原生 owner 接口](../../docs/PLUGIN_AGENT_NATIVE_OWNER.md)
- [会话与执行合同](../../docs/PLUGIN_AGENT_SESSION_EXEC.md)
- [进程控制接口](../../docs/PLUGIN_AGENT_PROCESS_CONTROL.md)
- [项目状态](../../docs/PROJECT_STATUS.md)
- [下一 SDK 门槛](../reconstruction-2026-10-05/sdk-next-gates.md)

公开仓库仅保存源码、合成测试和限定结果摘要；私有虚拟机连接工具、凭据、真实账户标识的原始日志、数据库、截图与构建缓存不进入本次提交。历史报告中的“未提交／未推送”保留为其记录时点，不覆盖本次开发分支同步。

