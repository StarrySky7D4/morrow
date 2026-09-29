# 第四轮：P02 batch002 执行与存储限定复核

限定签收通过：本轮独立审查源码/补丁/身份与生产方编译证据，并在 joint 内独立复跑两个已冻结的原 exe，**9 场景 / 39 断言通过**。没有独立编译 Core，没有重跑旧 43/14 测试或网络批探针。未发现需要退回本批的合同误用或与其声明相矛盾的行为。

P-02、J-00、G0 继续 blocked；产品 native/Wasm 图保持 0/2，84 产品验收保持 not_run。有限接缝拒绝通过不等于生产接管或 OS 隔离。

## 输入、源码与编译证据

交接 `receipts/p02-exec-store-002/handoff.json`，SHA256 `d3c19e26c5c04461eed188ff1d84518add5d8af711c3dfc78e6e49ad8312cd76`。129 本批输入、旧交接 17/35 输入、两个成功 build/run 的内部输入集合核验一致；合并后为 1184 个唯一路径，复跑前后完全一致。各集合有交叉，未直接相加计数。

固定 Codex 8697 文件的集合、大小、SHA256/Git blob 与根树 `3b868fad63be6ac5db91402b579fab37f587d7d5` 已重新独立重算。新工作副本为 8698 文件，实际仅 4 修改 + 1 新增；从原文件与工作副本重新产生的 163 行 diff 与冻结补丁逐字节一致，SHA256 `403f5fe49b107a4877324dc8560600c58e815e70e7257c7707480001ca3034b3`。ThreadStore、Core client 和独立 exec.rs 没有源码改动。

本批 host/fork 副本集合与 1108 文件摘要前后一致。新增依赖 mxc 1740 文件、nucleo 42 文件含许可符号链接均独立重算 blob 与 Git 树，分别得到 `338c5bffbca87051c8f106fb35aeb015b2d235da`、`a878a0495e5f84266b7e36918a9e1a9432b0ddd8`，匹配固定提交回执。未重新联网查询，也未把恢复后的完整源码快照称为完整下载归档或 Git checkout。

存储资格锁 882 包（841 registry + 41 本地），执行资格锁 1117 包（1013 registry + 104 本地）；registry 的名称/版本/来源/checksum 都属于固定原 Codex/kit 锁。这里是锁身份核验，不是本轮重新运行 Cargo metadata 或重建全依赖图。生产方成功编译的 `--locked --offline` 命令、日志摘要、before/after 输入、build-finished 和 compiler-artifact 路径及 exe 摘要均关联核验。

## 独立运行

两个 main 允许指定输出路径，无上一批编译期 receipts 限制。因此本轮直接运行以下原 exe，未修改 manifest、Rust 源码或二进制；回执、stdout/stderr、临时目录和隔离 profile 全部位于 joint。只继承必要 Windows 环境字段，没有继承用户账号/代理/令牌配置；PATH 限定系统目录。没有运行 Cargo、模型请求或项目操作。

| 探针 | 原 exe SHA256 | 本轮结果 |
|---|---|---|
| store | `935ff209a98004d3ff97eb79319f34ccc2e615d11088e65c7835d305f36b5963` | exit 0；6 案 / 12 断言 |
| exec | `61edda878479b093fe841e5d8c974af453da50cc220e5cdc09b033a7f489933e` | exit 0；3 案 / 27 断言 |

存储 runtime SHA256 为 `c1ed7ee99d83b4857937cc64c7797c7e4e7b199cca2cafae200018d589a5c28e`，与生产方回执逐字节一致。

执行 runtime SHA256 为 `55a2edd9399894f033a93a109edd4e82a7897e55cc1fd2fded225456e6fabbf7`。上游原参数转换会生成 `2-<UUID v4>` 逻辑进程句柄，所以该句柄、参数摘要、Proposal 摘要随运行改变。逐案检查 UUID 形状，并按原 ExecParams 字段序列独立重算实际参数 SHA256；归一化这三个关联字段后，其余回执完全一致。未宣称执行回执逐字节一致；未导出 Proposal 原始 wire，故该动态 Proposal 摘要没有另行独立解码核验。

## 存储语义

真实入口是未改动的 `codex_thread_store::LiveThread`，使用 Legacy、非 LocalThreadStore、无 rollout path/cwd、memory disabled 的资格适配器。

| 场景 | 实际完整调用序列 | 独立结论 |
|---|---|---|
| resume 断开 | resume_thread | 不继续 history |
| resume history 失败 | resume_thread → load_history → discard_thread | 一次清理尝试，discard Unsupported 不覆盖原 read_after 错误 |
| 非空 raw append 断开 | resume_thread → load_history → append_items | 未调用 metadata，原 append 错误返回 |
| Standard persist | resume_thread → load_history → persist_context:Standard → persist_thread_requires_M04 | 明确 Unsupported |
| flush | resume_thread → load_history → flush_thread | 明确 Unsupported |
| 活动 history 断开 | resume_thread → load_history → load_history | 初始化空历史与后续拒绝分开记录 |

只使用 003 FakeHost 空历史完成初始化。没有把模拟 durableSequence 当持久化栅栏；append 未成功确认，persist/flush 未伪报成功，discard Unsupported 也不是成功 writer 释放。空历史拒绝案不证明非空 pending metadata 事务、成功追加、生产身份映射或存储恢复。M-04 生产持久化与 writer 生命周期仍缺失。Paginated downcast DB 路径和 create 前置 Git 采集未覆盖。

## 执行语义与补丁边界

资格桥接直接调用原 Core `open_session_with_prepared_exec_env`。补丁新增 injected 条件，因此 `is_remote=false`、无 snapshot 的请求也走明确注入的 backend。每案实际 start 一次，检查固定 argv/cwd、空 env、TTY 形状、无 shellSnapshot、无 FS/HTTP adapter 调用和原错误传播。两个断开案都失败；ProposeOnly 案检查 qualificationOnly、Proposed、execute=false 后仍返回 Unsupported，从不制造 StartedExecProcess、PTY、输出或进程成功。

读取真实分派函数可确认：本轮 backend 错误沿 `?` 提前返回，没有继续同函数普通 local spawn 分支。这不证明其他 spawn 路径被拦截，也没有 OS 级进程/文件/网络监测。

额外边界已独立确认，并与宿主只读报告相符：

- 三案 `network_policy_decider=None`，未覆盖 `start_with_network_policy_decider`。
- 只有资格桥接 module 受 `morrow-p02-qualification` feature 控制；Environment 注入构造器/标记与 dispatcher 谓词本身非 feature-gated。当前只在新工作副本、尚未接入生产 session；未来产品合入须另审 API 和分派变化。
- 桥接从 prepared request 开始，前置审批、sandbox 转换、高层工具编排和完整 agent loop 未验证。
- 普通 local spawn 分支、独立 `core/exec.rs` spawn 未单独覆盖；Core ModelClient 的 HTTP、WebSocket、prewarm/reconnect、HTTP fallback 未接管。
- M-06（关联 M-02）真实 argv/cwd/env 载荷、执行/PTY/IO/控制与终态收尾仍缺失；M-03 是 HTTP 元数据，不能误标为执行合同。

## 可复查证据

本轮 `runs/p02-exec-store-002-review-001/` 中有 `result.json`、`pre-run-review.json`、`inputs-before.json`、`inputs-after.json`、两个 `*-runtime.json`、stdout/stderr、环境策略与实际复核源码。`review_p02_exec_store.py` 只向全新 joint 运行目录输出并拒绝覆盖。

宿主补充报告 `../host/p02-exec-store-002-host-review.md` 仅作为合同边界交叉审查来源；其未独立运行，本轮签收基于联合会话自己的两个实际进程结果。原计划、插件、权威 Schema 均未修改；无提交、推送、发布。
