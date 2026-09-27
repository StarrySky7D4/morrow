# Windows 文件变更原 owner 队列（2026-09-27）

本轮将已验证的 Windows 文件变更后端接入原 `IoWorker` 的有界命令队列：目标选择、准备、分块暂存、内容提交、执行、历史查询及本地资源释放均在持有原 Manager／Runtime 的后台线程运行。**这是可信原生宿主接口，尚不是三语言 guest SDK 或 Flutter 用户入口。**

工作目录 `build/io-safety-refactor`，分支 `codex/io-safety-refactor`，基线 `b9225f64f6c62584ad7243e30249d8a088bcb155`。本轮本地未提交，不构建安装包、不推送。

## 执行与退出

- 复用原 8 个 owner command 名额、一次读取的回执、原工作线程、身份校验、停止和回收机制；没有额外宿主、Store、Manager 或重试线程。
- `MutationSession` 绑定原 worker；foreign session 在入队、时钟采样和额度消耗前拒绝。未实现分借用的 owner 默认拒绝；返回替代 Runtime 的实现被 HostBinding 检查拒绝。
- `CommandControl` 按 state→ticket 状态→原共享 clock→实例 IO 检查顺序工作，时钟取样与核验在同一临界区完成。OS 调用和 Store 事务主体不持该锁。单个同步 OS 调用仍不可强行中断。
- 目标句柄与 buffer 由 worker 的资源集合持有，返回句柄中不携带 owner。未交付的目标选择被取消后由后台回收；已交付选择保留到 release 或 worker 真实退出。`try_reclaim` 仍非阻塞，实际 join 后才交还原 owner。
- 调用句柄的 cancel 是当前命令／交付的取消，不等于撤销实例或删除已持久 Prepared。`release_mutation` 释放本地选择和 buffer，不取消持久计划，也不重放效果。SDK 层的显式计划取消／跨重启任务恢复仍需后续接线。

## 分块与配额

`stage_mutation_chunk` 单块最多 60 KiB，为原 64 KiB command 输入上限留出元数据；offset 必须等于已接收长度，不接受乱序、重复块或超过声明长度的内容。完整内容必须与原计划长度／SHA-256 一致后才进入 Core 的原子内容＋回执事务。Create 保持不覆盖发布，Delete 保持原句柄一次性效果，Replace 在 claim 前返回 Unsupported。

- 块放在原 `Status.input` 的 `Zeroizing<Vec<u8>>` 中；排队取消／stop 立即清除输入，执行中输入由当前调用持有至返回。长度和容量都检查，拒绝 len 很小但 capacity 过大的 Vec。
- 路径／subject 在检查原长度后紧凑复制，队列费用按实际保留容量；准备请求从有界规范容器重建，避免可信调用者传入的多余 String／Vec 容量长期留在队列。
- 每 session 的 buffer 按完整声明长度一次预留，分块追加不反复扩容。所有 retained buffer 的 capacity 合计最多 64 MiB；正文 codec 上限 16 MiB，不代表每个实例均获准使用这么多。
- 首次正文 buffer 分配、可能核验正文的 Prepare／Query，均先检查原 worker 的 max_job_bytes／累积 max_total_bytes 与实例准入。传输、物化、持久暂存与查询是分别计费的成本，取消／失败不返还累计额度。临时 job 租约及时释放，不能与后续原 broker 的 job 准入互锁。
- 普通 `IoBinding::admit` 的 jobs 是并发占用，`IoLease` 释放时递减；字节费用不返还。独立的 `service_run.run.jobs` 是累计计数，不能混作同一种可恢复预算。
- 64 MiB 是本队列 retained spool 的上限，不是进程总内存上限。Core 编解码／校验、数据库和其他 IO 任务另有工作内存；本轮没有做进程峰值内存基准。

## 历史查询与请求匹配

查询沿用当前实例权限，返回原操作历史、已暂存长度及持久暂存状态。取消已完成的执行回执后，外层返回 Unknown；后续查询可读取 Observed，重复执行仍被拒绝。状态查询不重新执行文件操作。

新增 Core `lookup_matching_io_intent(&Command)`：先在同一读事务中匹配小体积历史命令，再做原完整 file history 校验。文件 Prepared 的写入入口也先匹配现有命令；冲突 operationId 不先加载另一计划的大正文。旧 lookup 保留完整校验。匹配后的损坏正文仍必须报 Integrity；新增 1 MiB 样本测试同时验证拒绝顺序与完整性边界。

现有回执读取内部也会核验正文，不能把它称为轻量元数据查询。本轮保持强校验：Unknown／Observed 的匹配历史已验证内容＋回执，直接使用该结果；Prepared／Cancelled 才读取可选暂存，避免同次查询重复解码正文。仍需进一步设计面向长期大库的有界摘要索引，而非跳过校验。

## 验证

| 范围 | 最终结果 | 本地日志 |
|---|---|---|
| Core 全量，fault-injection | 739 通过、0 失败、15 ignored | `build/file-mutation-owner-core-full.log` |
| Windows runtime lib＋九个相关集成目标，packages/fault-injection | 228 通过、0 失败、6 ignored | `build/file-mutation-owner-runtime-final.log` |
| Core 定向严格 Clippy | 通过 | `build/file-mutation-owner-core-clippy.log` |
| Runtime 上述全部目标严格 Clippy | 通过 | `build/file-mutation-owner-runtime-clippy-final.log` |
| 实际 workbench_host lib 编译检查 | 通过，5 条既有 dead_code 警告 | `build/file-mutation-owner-host-final.log` |

Clippy 保留仓库既有 `clippy::collapsible_if` 例外。ignored 项未计入通过数；子进程入口由相应父测试调用的范围以日志为准。定向 rustfmt 与 `git diff --check` 通过。上述结果不是完整应用构建、Flutter UI、三语言 guest SDK 或跨平台验收。

真实 Windows 测试使用临时目录，不触碰用户资料库：多块创建→持久查询→删除、空内容、Unsupported、foreign worker、原 owner opt-in／替换 Runtime 拒绝、队列满与取消、丢回执查询、独占句柄 release／真实 stop＋join，以及畸形准备和分块边界。保留原文件读取、owner 命令、服务续期、IO binding/execution 与崩溃回归。

过程修复包括首版 Debug 派生无法编译、测试 manifest 的 max_jobs 误设为 8（协议上限 4）、大 enum 的消息体改成 Box、输入容量限制和 Store 隐含正文读取的准入顺序。以最终结果为准，不将中间失败当作通过。

## 剩余范围

1. 将这组可信宿主方法接到 Workbench 应用协议、任务模型与 Flutter 选择／审批／进度／结果页；目前工作台仅编译通过，未进行 UI 操作验收。
2. 将变更与分块协议统一到 C／C++／Rust guest SDK，补契约版本、跨语言兼容、包声明与开发模板。SDK 未冻结。
3. 接通明确的持久计划取消、分块中断的显式恢复／重置、跨 worker／跨重启 Unknown 核对和遗留临时项的归属验证；查询不能恢复旧现场授权。
4. 目录列举、其他平台资格、网络流式能力与完整产品回归继续开放。Windows 条件替换仍明确不支持。
