# 持久草稿交接提案：私有协议与独立客户端恢复

日期：2026-09-24。基于 `codex/io-safety-refactor` 工作树 `e89f4ff`，应用仍为 `0.1.9-test.54+58`。本轮保留此前五种实验风格，完成宿主提案能力到 Dart 客户端的接线；没有提交、推送或发布。正式编辑器自动保存仍未启用。

## 实现与边界

- 追加六个私有动作 117–122：准备、核对、分页发现、建立子草稿、退休父草稿、取消。原动作编号不变，未进入提案流程的旧 handoff 接口仍可用。私有契约摘要已更新，客户端与宿主须配套；保留动作不表示旧二进制可忽略摘要差异连接。
- Prepare 固定完整 WriteRequest、ParentLink 和退休操作号，经既有分段上传落盘。回复丢失或校验失败返回携带原身份的 Unknown，不换号重发。
- 单条回复包含完整提案，按 32 KiB 分段，复用 8 MiB 传输上限；分页只编码最多 32 条摘要。Dart 核对外层 card/parent/child operation、精确 u64、契约摘要、记录身份、状态和分页推进，整次发现共用草稿队列且限制 256 条。
- Inspect/Discover 只读。Complete/Retire/Cancel 显式发起并由宿主重新检查当前状态。取消后的旧提案不能再次建立子草稿；清理传输不会取消已持久提案。
- 宿主仍使用 full-record list 构造摘要来源，因此列表编码有界不代表宿主已实现按摘要索引读取或最优内存使用；不能据此宣称大库容量问题全部解决。
- 真实跨端测试发现并修复了单条回复带空分页字段、Absent 回复操作号被错误拒绝两处不一致；另修正新接口的命名参数，并拒绝新分页混入 lineage 列表。

## 实际验证

| 验证 | 结果与证据 |
| --- | --- |
| Rust 新协议与旧 handoff | 4 项通过，其中 2 项为复用的旧回归；最终单条回复修正另有真实 S1 专项重跑 |
| Rust 持久提案 | 8 项通过；既有宿主级状态、保留身份和恢复边界 |
| Rust 编码边界 | 32 条摘要页测试、Absent 字段绑定测试分别通过；不是 32 份最大正文的端到端内存测量 |
| Dart 新旧 handoff + codec | 12 项通过，无跳过；含真实最终 Windows 宿主和插件的保存、重开、分页、取消、附件导出 |
| 既有草稿/附件/导入/会话 + 基础 codec | 23 项唯一用例通过。初轮 15 通过、8 因未设置 Python 跳过；补充 Python 后对相关两文件运行 11 项通过，覆盖全部 8 个跳过项，3 个为重跑 |
| 独立客户端进程 | seed PID 3464、recover PID 14612，分别 4 项通过：prepare/complete/retire/cancel 落盘后首回执丢失 |
| 宿主在线损坏回执 | PID 14692，4 项通过：明确检查首个后续动作是清理原 upload/correlation，之后分页、完整分段读取可继续，原写动作不重放 |
| 静态与构建 | 8 个 Dart 目标分析无问题；生成绑定检查、Rust 所改文件格式、diff 检查通过；最终 Windows Release 构建退出码 0 |

独立恢复进程只接收测试库目录与故障场景名称，不接收原提案对象或请求身份。卡片、父子草稿及操作号随机生成，再从宿主摘要发现；原附件路径已删除。恢复验证120,000 字节 UTF-8 原始文本、UTF-16 选区/IME 数据和已固定附件。发现/核对期间检查真实请求轨迹无写动作；明确执行后续动作后核对业务修订未重复增加。取消场景拒绝继续建立子草稿。

EOF 代理在收到真实宿主成功回执后主动关闭通道；这是丢回复并正常关闭宿主的验证，不是断电、强杀中途写入或磁盘故障验收。在线损坏回执覆盖同一宿主中的缓冲清理。用例使用隔离测试库，无用户内容库修改。

主要日志：`build/handoff-proposal-native-tests.log`、`build/handoff-proposal-process-{seed,recover}.log`、`build/handoff-proposal-live.log`、`build/handoff-proposal-legacy-{regressions,faults}.log`、`build/handoff-proposal-final-analyze.log`、`build/handoff-proposal-windows-final.log`。源码、日志、故障轨迹和最终产物摘要见同名 JSON。

## 交付与剩余工作

本地 Windows 预览：`build/windows-corners/x64/runner/Release/morrow_studio.exe`，须使用整个配套目录。此轮未重新进行视觉帧耗时或 Android/Web 验收，五种风格及其首次 Raster 峰值结论沿用各自专项报告。

下一步仍需工作台会话持有完整提案状态，并把正式编辑器的 S1/S2、附件 provenance、恢复入口、关闭/切库、富捕获证据和临时资源归属接上。恢复扫描不得自动完成提案；界面应显式展示 Pending/ChildCommitted/ParentRetired/Cancelled/Conflict 并在用户处理时再次核验权限与库身份。最大正文/满额提案的端到端资源预算、历史回收和其他平台仍待验收。SDK 冻结不因本轮私有接线而达成。
