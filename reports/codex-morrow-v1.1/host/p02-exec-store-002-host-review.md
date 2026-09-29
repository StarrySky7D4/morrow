# P-02 batch002 宿主侧限定边界复核

结论：本批补丁、源码和生产方运行回执与其声明的有限拒绝探针范围一致，未发现
需要以“误用 003 合同”退回本批的问题。**这不是本宿主会话的独立构建/运行通过**；
实际重建重放与最终本批签收由联合验收完成。完整 P-02/G0/J-00 不升级为通过。

## 输入与本轮实际检查

插件目录：`C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex`。
输入：`receipts/p02-exec-store-002/handoff.json`。
SHA-256：`d3c19e26c5c04461eed188ff1d84518add5d8af711c3dfc78e6e49ad8312cd76`。

本宿主会话以只读文件检查实际核对了：

- 本批 129 个 handoff 绑定输入，全部 SHA-256 相符，包括两个 exe、探针源码、
  补丁、构建/运行回执。原交接 17 个及网络批次 35 个输入也逐项相符；复核结束
  后再次比对均未变。各集合有交叉，不将计数相加宣称独立文件总数。
- 补丁所列 5 个文件的固定原件 before 摘要及新工作副本 after 摘要，全部匹配；
  新桥接文件在固定原件中不存在。已阅读 163 行 patch；未据此宣称本轮重建了
  全部 8697 文件的 Git 树或完整依赖来源。
- 003 的 180 个清单文件和 12 个 canonical 源文件全部相符，manifest 仍是
  `5555751ab1cf590d68e9610dae396677b79422999ec61b7c3d3d7183e8ed4a01`。
- 两个探针 main、执行 deny adapter、五文件补丁、生产方 build/run/runtime
  回执，以及 delivery/callsite-assessment 的范围表述。

可复查的逐项摘要与前后状态保存在 `p02-exec-store-002-host-input-check.json`。
本轮没有运行插件 exe、Cargo、003 测试或真实 OS/网络监测。

## 存储：支持的结论与未覆盖项

生产方回执记录 **6 案 / 12 断言通过**，构建与运行退出 0。其来源为实际
`codex_thread_store::LiveThread`，没有复制该控制流；ThreadStore 实现为明确的
资格 FixtureStore。

| 场景 | 记录的实际 calls | 宿主侧语义判断 |
|---|---|---|
| resume 断开 | resume_thread | OpenWriter 失败即返回，不继续加载历史 |
| history 失败 | resume_thread → load_history → discard_thread | 清理尝试一次；discard Unsupported 没覆盖原 read_after 断开错误 |
| 非空 raw append 断开 | resume_thread → load_history → append_items | 实际 raw batch 编码并送到拒绝点；该场景没有 metadata 调用 |
| Standard persist | resume_thread → load_history → persist_context:Standard → persist_thread_requires_M04 | 明确 Unsupported，没有用 fake durableSequence 冒充持久化 |
| flush | resume_thread → load_history → flush_thread | 明确 Unsupported |
| 活动历史读取断开 | resume_thread → load_history → load_history | 初始化空历史与后续断开分开记录 |

Legacy、非 LocalThreadStore、无 rollout path、无 cwd、关闭 memory 的资格配置
避开了当前 Paginated downcast DB 路径和 create 的前置 Git 采集。只有明确的
003 空历史 fixture 可让初始化继续，不构造第二份独立可写业务历史。

限制仍需保留：此处 thread ID 到 fixture session 的生产身份映射未实现；
append 拒绝案并非成功追加/metadata 事务证明；空历史初始化下的 persist/flush
拒绝也不能证明非空 pending metadata 的事务行为。fixture writer 没有释放合同，
discard 返回 Unsupported 是正确暴露缺口，不是成功清理。M-04 会话生命周期、
持久栅栏、成功历史恢复、writer 释放和元数据行为仍未实现/验收。

## 执行：支持的结论与未覆盖项

生产方回执记录 **3 案 / 27 断言通过**，构建与运行退出 0。代码与回执吻合：

- `with_injected_capabilities` 要求明确提供 ExecBackend、FS 和 HTTP 对象；
  不使用 LocalProcess、默认本地 FS/HTTP 构造，也不伪装 remote。
- dispatcher 新增 injected 谓词，然后沿原参数转换与 backend 调用执行。
  桥接调用实际 `open_session_with_prepared_exec_env`，没有另写分派器或循环。
- 每案断言 `is_remote=false`、backend 一次、预期 argv/tty/空 env、无 shell
  snapshot 和明确失败；两个断开案与一个有 Tool.Propose 回执但拒绝启动的案
  分开。3 份实际 ExecParams、摘要及 proposal 摘要在 runtime 中保留。
- ProposeOnly 检查 qualificationOnly、Proposed、execute=false 后仍返回
  `qualification-unsupported`，从未把提案回执转换成 StartedExecProcess。
- FS/HTTP adapter calls 为空，只证明这几个注入对象未被本次路径调用；
  不能等同全局 OS 文件、进程或网络监测。

补丁只有资格桥接 module 由 `morrow-p02-qualification` feature 控制；Environment
注入构造器/标记与 dispatcher 谓词本身没有这一 feature guard。这与 delivery
所称“constructor 未接入生产 session startup”并不冲突，但未来产品合入时必须
单独评审 API/分派变更；本轮不批准其生产接入或安全模型。

所有执行案的 `network_policy_decider=None`，尚未覆盖
`start_with_network_policy_decider`；普通本地 spawn 分支及 `core/exec.rs` 独立
spawn 也未单独覆盖。本次可依据实际失败返回和该分支控制流说明“这一调用未
继续到同函数本地 spawn”；不能扩大为完整执行接管。真实 argv/cwd/env 载荷
领取、进程/PTY/输入输出/控制/收尾仍是 M-06（关联 M-02）缺口。

## 保留的总边界

Core ModelClient 的具体 ReqwestTransport、WebSocket、prewarm/reconnect、HTTP
fallback 仍未接管；上一批 ResponsesClient 不能替代这些入口资格。M-03 的
HTTP 元数据缺口仍存在。产品 native/Wasm 图保持 0/2，84 产品场景不因这 9 案
变成已运行。无真实登录/付费请求、生产持久化、成功进程或 OS 隔离结论。

本轮新增仅为本报告及宿主输入核查 JSON；没有修改插件、固定旧批、003、
canonical Schema 或联合验收目录，没有提交/推送。
