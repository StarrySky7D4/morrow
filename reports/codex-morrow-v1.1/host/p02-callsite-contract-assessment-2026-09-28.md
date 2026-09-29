# P-02 新批次具体调用点与宿主合同映射

本轮只读核对插件交接的真实上游调用点及 003 Schema，给出下一切片建议；没有
修改上游、003 或宿主源码，没有执行新的运行探针/构建。本文细化
`followup-contract-gaps-2026-09-28.md`，不代替插件的运行回执或联合验收。

源码根：`C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex\upstream\p02-source-batch-001\codex-source\codex-rs`。
固定提交 `44fe510ce3ee61c8ef623adcbf89b901c73ddd61` 的完整源码资格见联合第三轮
`../joint/review-round3-2026-09-28.md`；该轮已经解除完整源下载阻塞，不能继续
将其称为 partial snapshot。本轮对所读局部文件另存 SHA-256，见同名 `.json`；
未重复重建全源 Git 树。

## 已核实的源码事实

| 调用点（相对源码根） | 本轮所见 | 接缝覆盖含义 |
|---|---|---|
| `core/src/unified_exec/process_manager.rs:1309`、`:1322`、`:1349`、`:1352` | `open_session_with_prepared_exec_env` 仅在 remote 或 shell-snapshot 分支调用 `ExecBackend.start*` | 只替换 trait 不覆盖全部 unified exec |
| 同文件 `:1413`；`core/src/exec.rs:959` | 分别调用 `spawn_process` 和 `spawn_child_async` | 两条直接启动分支须有独立拦截/拒绝证据，不得用 backend 探针通过代替 |
| `exec-server/src/environment.rs:667`、`:683` | backend/FS/HTTP 字段私有；`default_for_tests` 实装 LocalProcess、unsandboxed LocalFileSystem、默认 HTTP | 该构造器不是隔离注入入口；改标 remote 或借 shell snapshot 也不能证明所有本地分支已安全替换 |
| `exec-server/src/process.rs:199`、`:223` | ExecProcess 除启动还要求 read/write/signal/terminate、事件/唤醒订阅；带策略 decider 的 start 有独立默认拒绝实现 | 不能仅有 Claim 就返回伪造的“完整进程句柄”；两种 start 和后续方法分别记覆盖 |
| `thread-store/src/live_thread.rs:182` | resume 先 `resume_thread`，history 缺省才 `load_history`；后者失败会调用 `discard_thread` | 应显式测有/无预置 history、加载失败清理，以及断开后拒绝；不能只断言 resume 一次 |
| 同文件 `:190` | Paginated 且实际对象可 downcast 为 LocalThreadStore 时可访问 state DB | 注入适配器不得包回 LocalThreadStore 后宣称无第二存储；在选定配置中证明该支路未进入 |
| 同文件 `:238`、`:280`、`:292`、`:308`、`:439` | append 后可能 record metadata；persist/flush 后可能 update/record metadata | 元数据写入也是存储接缝；不能用默认无操作成功掩盖遗漏 |
| 同文件 `:142`；`thread-store/src/thread_metadata_sync.rs:54` | create 在 create_thread 前调用 Git 根发现与 collect_git_info | 首切片优先 resume；create 的前置 Git 采集需独立注入或明确未覆盖，不能靠“测试目录恰好无 Git”外推隔离 |
| `thread-store/src/store.rs:66`、`:83` | Standard 要求持久且可读；部分背景 PersistContext 允许入队，仍需后续 durability barrier | 003 的内存 durableSequence 不满足真实 persist/flush 语义 |
| `core/src/client.rs:1176`、`:1197`、`:1233`、`:2218` | 返回具体 ReqwestTransport；有独立 WebSocket 连接、优先选择和 HTTP fallback | ResponsesClient 层替换不能外推 ModelClient 已接管；需逐项覆盖/拒绝 HTTP、WebSocket、预热与回退路径 |

## 可立即使用 003 验证的内容

| 真实接缝 | 003 可用的最小合同 | 可声明的通过范围 | 不可用它假装完成的语义 |
|---|---|---|---|
| ExecBackend / 上层实际 caller | `Tool.Propose/Claim/Report/Inspect`、固定 input/schema/executor digest、原 operation、防重复、Drain 后拒绝 | 从真实 caller 取得合成 ExecParams，比较固定编码字节/摘要后进入工具提案；无许可或断开时 start 返回明确错误且无进程；夹具许可下仅验证一次有效 Claim | inputDigest 不是 argv 载荷或可领取的命令；execute=true 只是资格 fake 状态，不是 OS 启动/退出/输出；不得返回成功启动来掩盖缺失 ExecProcess 能力 |
| LiveThread / ThreadStore adapter | 固定 fixture session 的 `Event.OpenWriter/AppendBatch/ReadAfter`、writerEpoch、tail CAS、原帧去重 | 真实 LiveThread 调用被适配器捕获；经明确 fixture bootstrap 后可验证有限 opaque 事件追加/回读、断开后拒绝及元数据调用序列 | OpenWriter 不是 create/resume；ReadAfter 不是 thread list/fork/archive；内存回执不是持久栅栏；未知元数据更新不能默默丢弃 |
| ResponsesClient / 将来的 ModelClient seam | `Stream.Open/WriteChunk/CommitRequest/Read/Inspect/Cancel/Close`、固定请求 digest、期限和资格来源标记 | 原请求到固定字节/attempt 的关联、目的地拒绝、断开前/后可观察失败、受限 fixture 分块与取消 | 无 status/headers/通用 URL；不能从一串 fixture bytes 假造真实 HTTP 200/SSE 完成，也不能掩盖 WebSocket 或本地 HTTP fallback |

共同要求：请求及参数只用批准的合成夹具，保留真实 caller → adapter → 003
的调用轨迹、次数、固定输入摘要和结构化拒绝。直接调用自制 adapter/trait 的测试
只能称 adapter 测试，不能升级为真实 caller 被接管。上游调用中遇到 003 没有的
必需能力应失败，不给泛化的成功或空结果。

003 的具体限制也要保留：每事件 inline payload ≤4096 字节、每批 ≤8 条、内存
历史最多128条，ReadAfter 必须按回执分页；超大 RolloutItem 应在当前切片拒绝，
不能无声截断或自行创造宿主分块协议。Append/Propose/Report 重试必须保留完整
原帧，包括 requestId；不能换新 requestId 后仍期待相同幂等键被视为相同字节。
ResourceRef 目前只接受固定 fixture 引用，并不代表真实 cwd/文件/URL 已获授权。

## 必须新增的宿主能力

- **M-03 / M-08：** HTTP 请求 method/目的地/必要头的受控描述，可信实际目标、
  status、受限响应头和阶段；错误正文/重定向/压缩的有界处理；凭据用途与代次。
  ModelClient 注入和 WebSocket 选择是插件侧接入问题，不能靠向 003 payload 塞
  私有状态来补齐。若 WebSocket 本期不实现，应在选择/连接/预热/重连点明确拒绝
  或按已审批 HTTP 能力选择，不能自动启用未验证的另一发送路径。
- **M-04 / M-05：** 通用会话存在性/打开与恢复、范围查询、分支来源关系、归档、
  元数据事务、opaque 检查点和持久确认点；writer 代次、tail/CAS、gap/outbox。
  宿主不照搬上游 ThreadStore 的全部产品管理接口；按实际 caller 的必需闭包设计。
  在这些能力缺失时，真实 Standard persist/flush 必须返回未支持/失败，不能把
  003 fake append 回执当落盘完成。
- **M-06 / M-02：** 已批准固定 argv/cwd/env 等载荷的引用/领取，与输入摘要、
  executor artifact、原 operation 和活连接绑定；进程句柄、输入/输出流、事件、
  PTY/resize、signal/terminate、控制额度、退出/输出关闭分离和未确认收尾。
  现有 ToolReport.outputDigest/exitCode 不证明输出材料存在或宿主观察到实际退出。

以上扩展需独立授权切片、实验 revision/摘要/新向量和新 kit；当前不修改 003，
不以新 DTO 另造一份权威 Schema。

## 建议插件首个可审查切片

1. **存储先 resume：** 以真实 LiveThread 加自有 ThreadStore adapter，先验证
   resume_thread 拒绝时不再 load_history；随后用明确标识的内存 fixture 验证
   history 缺省加载、失败 discard 和真实 append/metadata 路径。persist/flush
   单独记录 Unavailable；如为探针临时模拟成功，应在回执标“仅控制流夹具”，
   不作存储适配成功证据。
2. **执行先真实拒绝入口：** 覆盖实际可达 start caller 和两条直接 spawn 路径，
   断开/无许可时返回错误、没有 native fallback。Environment 私有注入若需最小
   patch，应在插件新批次可构建副本记录精确补丁/feature/输入摘要；不修改固定
   原件，不使用 unsafe 私有字段注入，也不拿 default_for_tests 充当隔离环境。
   完整 core caller 尚不可构建/不可调用时直接报告该层未覆盖。
3. **网络补 core 路径覆盖表：** 继续保持上一轮 ResponsesClient 两拒绝场景
   的已验收范围；新批次分别记录具体 ReqwestTransport 构造、WebSocket 连接、
   prewarm 和 fallback 能否被截获或被显式拒绝，不重新声明完整 ModelClient。

建议每个探针回执至少列：固定调用点/源摘要、实际选择分支、合成输入摘要、
预期/实测调用次数、宿主帧身份和错误、未调用能力、进程/文件/网络效果的观察
方式及其限制。没有 OS 观察时，只能说代码路径未构造/调用对应能力，不能宣布
操作系统级隔离。完整 core loop、真实执行与持久化仍由后续独立资格证明。

本轮新增仅为宿主评审报告及其局部输入摘要；没有改动原插件已签收输入或联合
验收产物，未重复运行 003 已通过测试。
