# P02 integration 004：宿主只读计划审查

日期：2026-09-28。对象：插件 `receipts/p02-integration-004/preflight.md`，固定上游及已验收 002/003 的实际调用点。本文只审计划和既有源码，不是新补丁、构建或运行结果。输入摘要见 `p02-integration-004-preflight-inputs.json`。

结论：同一新副本、同一 exe/lock/build/manifest 验证三处接入共存，是合理的下一有限切片。编译期 restricted-qualification 可避免 task-local 标记丢失，但只能标识该构建的局部拒绝规则，不能充当生产权限、宿主授权或 OS 隔离。下列位置与证据要求应在实现中落实；保持003和生产源码不变。

## 必须在副作用前阻断的位置

**执行：** 002 `core/src/unified_exec/process_manager.rs:1320-1360` 先调用 spawn_lifecycle.inherited_fds，再按 `has_injected_capabilities || is_remote || shell_snapshot.is_some` 进入 backend.start/start_with_network_policy_decider；随后才是直接本地 PTY/process spawn。仅在直接 spawn 前加 guard 会漏过未注入的 remote 和 shell snapshot 分支。受限构建应在这些选择及 lifecycle 回调之前拒绝缺失注入，或为未覆盖分支明确标注限制。不要实际连接远程 exec-server 验证拒绝。

固定上游 `core/src/exec.rs:445` 的 execute_exec_request 是另一入口，随后可到 Windows sandbox 或 exec→spawn_child_async（959附近）。该入口需要自己的早期拒绝；不能用 unified dispatcher 用例替代。更上层的 command/env/sandbox 请求准备可能发生在 guard 前，本批从 prepared 入口开始不能证明此前没有读取配置、环境或准备网络。直接 Environment.get_exec_backend、LocalProcess 和其他子进程调用仍须列为入口清单内的未覆盖项。

**存储：** `thread-store/src/live_thread.rs:143` create 先调用 ThreadMetadataSync::for_create，才调用 store.create_thread；`thread_metadata_sync.rs:53-58` 先 get_git_repo_root 再 collect_git_info。guard 必须早于 for_create，不能只拦 store.create_thread 或 collect_git_info。若该构建直接拒绝所有 LiveThread create，应明确“包括注入 adapter 的 create 也被拒绝”，不能称已实现安全创建。

`live_thread.rs:182-208` resume 在 Paginated+LocalThreadStore 时先 downcast→state_db().await→get_thread，再调用 store.resume_thread。因此仅在 LocalThreadStore.resume_thread 加 guard 太晚。需要 LiveThread 入口在 metadata 路径之前拒绝本地 store；若还宣称直接 LocalThreadStore 方法也受限，就必须分别审查其方法入口。`local/mod.rs:482,526` 的直接 create/resume、其他 list/read/append/persist/update 路径没有因为 LiveThread guard 自动获得保护。构造时传入的 state_db 可能已由调用者初始化；不能将之后的拒绝说成防止了此前初始化。本地拒绝夹具宜使用无 DB 的公开隔离配置，不应为了验证 guard 先打开真实 DB。

**网络与认证：** 003 `build_api_transport` 的缺失注入拒绝须在 create_client_for_route 前；connect_websocket 的缺失注入拒绝须在 API connect 前。保留 injected 错误返回，不得转入默认后端。新 session / 新 Tokio task 应实际调用对应入口验证缺失与存在注入；受限 feature 本身在新上下文存在并不等于入口已执行。

ModelClient 构造与 current_client_setup 的 provider/auth 路径仍早于 transport guard。沿用003逐键环境白名单、认证字段为空及构造前认证变量缺席检查；不能把晚期 network guard 称作阻止认证来源读取。HTTP override 尚未传递 redirect_policy、且跳过原 account-routing→Reject 转换；本批拒绝资格不能宣布继承默认策略。execute 与 stream、preconnect/prewarm 与 stream 应按实际执行分别计入，不以共享 helper 推导运行覆盖。

## 构建身份与最小验收证据

- 冻结新源副本与完整差异：明确002和003合并位置、冲突取舍、feature guard新差异。一个可执行文件实际依次进入 exec/Core network/LiveThread；不能再聚合三份旧 exe 结果。
- Cargo feature须实际传播到编译中的 codex-core 和 codex-thread-store，关联同一包路径、锁及 compiler-artifact。默认feature不得误开受限模式；没有默认构建/运行证据时，仅声明正常分支源码被保留。新 feature 不是003协商能力或运行时授权。
- 缺注入用例应命中真实入口并返回精确的受限模式错误；不要以无效cwd、空命令、缺文件、Unavailable factory或错误配置先失败代替guard命中。用无害公开参数，拒绝前不启动真实命令/真实请求。
- 执行至少区分无注入本地、无注入shell snapshot/remote选择（若安全可构造）、独立execute入口；注入后失败传播与是否调用start_with_network_policy_decider按实际用例列明。缺席的变体保留未测，不扩大证据。
- adapter计数、lifecycle回调计数和沙盒输出目录前后清单能证明所观察范围；没有外部进程/网络/文件观察时，不称OS全局无副作用。当前错误类型、失败来源、先后顺序及调用次数写入回执。

## metadata、取消与资源释放

LiveThread 的 append 在后端成功后才更新 metadata；persist 某些 context 会先应用 pending metadata；shutdown 则先尝试 metadata 再调用 shutdown。只测试 raw append 的拒绝不能推导其余顺序。若本批扩展这些入口，应固定 context 与 pending-state 并记录真实 metadata 调用，保留原始失败，不造成功保存或释放回执。

`LiveThreadInitGuard::Drop`（live_thread.rs:118附近）会在当前 Tokio runtime 上 spawn 清理任务；它还能继续等待尚在获取中的 acquisition，然后 discard。Arc/Drop 的断言应明确对象范围及所有克隆是否释放，异步清理是否已完成；runtime退出或一个Arc计数归零不能证明宿主writer释放。Unsupported discard仍只是未支持；没有实际writer就不要宣称验证writer生命周期。退出、取消、Drain和持久封存属于不同事实。

## 仍需的宿主合同

| 责任 | 准确缺口 |
|---|---|
| M-02（关联M-06/M-08） | 生产受控域/调用者/操作与获准后端的可信绑定、可撤销授权及有效期；compile feature或任意注入对象本身不是许可。 |
| M-03 | 获准目的地、HTTP方法/headers/status/重定向与阶段、取消/未知发送结果；若支持WS，还需upgrade/frame/control/close。不得以本地426冒充宿主网络事实。 |
| M-04 | 会话创建/恢复、metadata更新与history提交的一致性、durable fence、writer获取/释放及取消中的取得结果；Arc和Unsupported不提供这些保证。 |
| M-06（关联M-02） | 获准argv/cwd/env字节输入、执行域、单次领取/启动、PTY/stdin/输出、signal/terminate、进程退出与输出关闭独立事实及未知结果处理。 |
| M-08 | 账户/凭据来源、权限和代际、认证恢复、撤销；不将秘密写入普通事件，也不因宿主拒绝回到个人认证。 |

这些是后续设计需求，不是新Schema承诺。本轮不增第二份schema、不实现后端、不运行探针。成功网络、持久化、真实执行、集成产品和OS隔离仍未证明；即便本切片通过，P02/J00/G0、产品0/2与84项not_run保持原状态，待联合验收限定签收。
