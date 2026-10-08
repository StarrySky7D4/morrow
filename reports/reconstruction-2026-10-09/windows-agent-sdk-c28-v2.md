# Windows Agent SDK：C28 实际进度与 Native V2 诊断源码

更新：2026-10-09。本次向 `codex/windows-sdk-convergence-20261005` 同步诊断源码、纯合成投影工具和限定进度说明。保留既有相对路径依赖入口与冻结上游，不更新 main、标签、应用版本或 Release。Agent 接口仍 experimental，SDK26/G04 未冻结。

## 已完成的有界实测

| 范围 | 实际结果 | 验证边界 |
| --- | --- | --- |
| Stage1 文件交付 | 166 个块获确认，三个最终映像的大小与哈希匹配 | 仅创建与传输验证，不是 PE 执行或 SDK 验收 |
| Stage2 文件预检 | 实际退出 0，三个映像匹配，系统动作数 0 | 仅文件预检，不是沙箱或操作系统启动验收 |
| Session-four | 原 context、worker、run、join 返回；七项 import、tail=1、原 acknowledgment 与同 owner 持久化已核对，耗时 1377 ms | 同一原 owner 的限定会话链，不代表完整会话产品、Codex IPC 或跨平台完成 |
| Native-eight | 前五个原步骤返回，一次 claim；第六步 native Start 为 Unknown，observe/join 未派发 | 未取得可确认的 Started handle，不证明没有 OS 子进程或副作用，不重放该 Start |

两组沿用原 60 秒授权与 40 秒宿主调度预算，没有刷新 TTL。原 native 组停在第六步后，未派发剩余 observe/join。

当前真实 native 诊断只有 worker `PORT_REJECTED/R2CommitUnknown`、delivery `RECEIVER_DISCONNECTED/Unknown` 和 native `BACKEND_START_RETURNED/BackendError`；invocation/backend-task 标志为 true，handle/provider 标志为 false。旧错误细节在 backend 返回时被折叠，因此不能认定 `ServerInternal`，也不能推断权限、账号、令牌或启动失败的具体原因。

同一原 owner 的一次 cleanup 与一次独立显式 repair-cleanup 已有记录，但尚未建立正常清理、真实 join、owner finish、factory release、终端退出与完整捕获。资源债务和 Unknown 保留。新源码不授权重试、重放、采用另一个 owner 或刷新旧授权，也不能回溯恢复已丢失的旧错误类别。

## 本次源码改动

Native V2 仅在已有 backend 错误折叠点前借用 `ExecServerError`，穷举 19 个外层变体，并将 Server 的数值 code 映射为四个固定类别，共 22 个 backend 类；不读取、格式化或保留错误正文。`ConnectionAttempt` 保留为外层类别，不递归展开。

诊断继续使用同一 AtomicU64，保留原 stage/error/四个单调标志。新增 backend 类占用独立字节，首个主错误与首个 backend 类分别保持；晚到的 backend 错误不会覆盖先到的 timeout。CommitUnknown、原调用次数、R2 claim/admission/CAS、时钟、期限、live/stop 检查、原 owner、观察者与清理路径保持原逻辑。

可观察的 Debug 名称改为 `NativeStartDiagnosticV2`，native 字段从六个变为七个，新增 `backend_error_class`。独立投影工具使用完整字符串匹配：V1 保留原 12 字段语法；V2 严格接收 13 字段和 22 个固定类加 NOT_OBSERVED，拒绝未知值、额外或乱序字段、对象 message、带引号的布尔值等输入，不回显原始错误。固定 timeout 仍记为 MISSING。原 guest-identity journal 支持及 envelope、sequence、大小、重复键、路径与控制记录边界保留。

这些类别只说明 backend 返回家族，不定位具体 OS 阶段，不证明零效果、清理成功或请求授权。ServerInternal 出现在源码映射和合成夹具中，不是本次真实运行观测。

## 检查与未运行项

| 检查 | 已有结果 | 范围 |
| --- | --- | --- |
| Native V2 源码独审 | 哈希、单文件补丁重建与 typed/atomic 不变量有限通过 | 静态源码审核，不是编译或运行证明 |
| 投影工具 | 229 项内存合成检查退出 0；七种无文件输入的错误参数组合按预期退出 2 | V1/V2 解析与拒绝边界；没有读取原 journal 或执行业务 |
| Native V2 离线依赖元数据 | 第二次 locked/offline 检查实际退出 0，源码与原锁不变 | 原隔离候选依赖图；不等于公开相对路径副本的编译资格 |
| 初次元数据失败 | 沙箱无法检出现有离线 Git 缓存，Cargo 退出 101；失败记录保留 | 后续检查未下载依赖，未放宽锁定或离线约束 |
| Native V2 库编译与九个纯诊断方法 | 库编译、完整清单核验实际退出 0；九个精确方法各执行一次，9/9 通过，实际退出均为 0 | 独立区分编译、完整方法清单和每个实际执行结果 |

本轮隔离候选的保存测试程序为 Windows x64，SHA-256 `86a6c2e86096d122d33a3b92549da7cb30daa76f44dc92f95b90a5efda94f092`，大小 30331904 字节。它仅用于纯诊断测试，不随提交分发；隔离候选的编译结果不能直接等同公开相对路径副本或完整产品的运行资格。脱敏收据与方法清单见 [结构化摘要](native-v2-validation.json)。

既有 H008 编译与两个库共八项纯诊断测试属于上一源码身份，不能累计替代新 V2 资格。新增 payload 样本与 Server 测试实际构造了 15/19 个外层错误变体；WebSocketConnect、ApplicationNetworkPolicy、Json、EnvironmentRegistryRequest 的构造/分类路径未被这些样本执行。并发 CAS 交错、完整状态空间和诊断开销没有测量。原文件读取边界保持源码身份，不代表运行时文件系统竞态已经验证。

本次 V2 没有新的 native Start、VM、原 journal、生产业务或完整回收验证。公开仓库只收录可移植源码、合成夹具和脱敏结果摘要；原始账户日志、捕获、控制器身份、资格收据、机器绝对路径、数据库、凭据、二进制与缓存保持私有。

## 后续门槛

精确 V2 源码的编译、方法清单与九项限定纯诊断资格已完成；保留所有失败及 NOT_RUN 项，继续定位具体 OS 阶段并完成原 owner 的正常回收。若另有显式授权开展新操作，须单独绑定其 owner、原授权、源码/产物/投影器身份与实际事实；当前 Unknown 不得重放。随后核对真实 exit、stdout/stderr EOF、事实身份、原 owner 回收与 scheduler join。

安全执行层、SDK26/G04、完整 Codex IPC、PTY/stdin/resize、完整网络隔离、其他平台资格及 SDK 冻结仍 OPEN。尚未达到完工暂停点或测试预览发布条件。本次开发分支同步不创建 Release。
