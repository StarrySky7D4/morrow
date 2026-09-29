# P02 integration 004：宿主合同与守卫顺序复核

日期：2026-09-28。handoff SHA256：`513b1cfbf4b19b32c247b06810430ae29b1e1b779078b9761100a86c7a8d1751`。

结论：在“同一受限资格构建的三处接入共存及选定默认入口拒绝”范围内，未发现新的合同误用或早于守卫的目标副作用阻断项。实现落实了宿主预审的六处边界。**24案/109项计入断言是生产方运行结果；宿主本轮没有独立构建、metadata或重跑exe。** 独立运行复现由联合第六轮单列。本报告不授予生产授权、成功网络/执行/持久化资格或G0。

## 独立只读检查

检查器 `check-p02-integration-004.py` 与结果 `p02-integration-004-host-input-check.json` 均在本宿主目录。

- 新83项输入、旧17/35/129/53项输入全部匹配，成功build/run内部输入前后相同且当前匹配。
- 原固定源码8697项与清单逐项匹配；新副本8700项，12处差异（9修改3新增）。从实际原/新字节重产693行diff，与封存补丁逐字节一致，SHA256 `ce4af5db99badb4fc0c8d86e6701db710d5df8c1a989a101dd53b15f6d462a01`。Git符号链接按链接文本核对。
- 原始成功build日志中的905个compiler-artifact包身份已重算，core的三个qualification features与thread-store的restricted feature与构建证据一致；所检查core/thread-store/exec-server/protocol/rollout均关联004路径，最终exe路径和摘要对应同一构建。
- exe SHA256 `0592d5e69bfe7fe1c6a5ecdcb86eb3e6f6ce703e4a4f80a66c409146e4bfb682`；runtime SHA256 `8c8c681c597a86e993c7252ba60048bb5f445d6ccc1f1deea700051c97643f35`。
- 回执24案计数合计109；新增共享对象/八个新task守卫的错误、callback和对象计数字段已独立核对，6份HTTP正文UTF-8原字节长度/摘要全部重新计算。没有把解析后JSON摘要冒充原字节摘要。
- host-kit003的180项kit文件、12项canonical文件、manifest均保持冻结摘要。没有改003。

## 六处边界

位置均指004新副本。逐项阅读实际合并补丁和桥接调用，不只使用guard-order摘要中的行号。

| 边界 | 实际守卫顺序与证据 | 不能推出的结论 |
|---|---|---|
| prepared unified exec | 缺注入立即拒绝，早于 inherited_fds、remote/snapshot选择及start/direct spawn。两种TTY在新Tokio task命中专用错误，lifecycle调用数组为空。 | remote/snapshot变体和network-policy decider没有动态覆盖；准备请求之前的配置/环境/shell捕获不在该入口之内。 |
| 独立 execute_exec_request | restricted拒绝早于解构、cwd转换、Windows sandbox/get_raw_output_result；回执命中特定错误且after_spawn=0。 | 未授予其他直接LocalProcess/底层spawn入口资格。 |
| HTTP build_api_transport | 有注入则分派；缺注入在create_client_for_route和具体transport前拒绝，真实Core stream回传guard错误。 | execute/Realtime未运行；guard之前的provider/auth处理依靠本批公开夹具约束。 |
| connect_websocket | 缺注入分支在API connect前拒绝；外层Core header/telemetry/timeout仍存在。真实Core stream回传特定错误。 | 缺注入prewarm/preconnect、最终wire/auth/upgrade/frame及重连未动态覆盖。 |
| LiveThread create | 读取thread_id/history_mode后立即拒绝，早于for_create/get_git_repo_root/collect_git_info和store.create_thread；fixture调用为空。 | 所有create连同注入store均被拒绝，这是拒绝资格，不是安全创建。目录未创建不等于全局文件系统无读取。 |
| LiveThread resume | concrete LocalThreadStore类型检查后拒绝，早于Paginated metadata/state_db/get_thread以及resume_thread；Legacy/Paginated均命中。 | 不保护调用者预先初始化的DB；包装/自定义ThreadStore、直接LocalThreadStore方法不受此具体类型检查约束。 |

受限feature在core中显式传播到两个bridge和thread-store；正常构建不默认启用。`cfg!`为false时保留源码默认分支，但本轮没有normal-mode编译/运行证据。可注入任意caller-owned backend、或用具体类型识别LocalThreadStore，均不是生产可信权限验证。

## 三处接入共存与资源事实

004的main在同一Tokio runtime运行1个共存用例、8个新task拒绝用例和重新编入的15个旧用例，不是相加三份旧exe结果。共存用例先取得真实LiveThread，在其存活期间join实际prepared exec与Core HTTP调用，之后Standard persist得到Unsupported，再显式await LiveThreadInitGuard.discard。

该源码断言实际store顺序是resume/load_history/persist/Unsupported/discard，exec与network各一次，附带FS/HTTP capability未被调用；释放句柄后四个Weak观察的strong_count均为0。这只证明这些本地对象的所有权释放。discard返回Unsupported，InitGuard把错误记为warning；as_ref为空也不代表远端writer已成功释放。无实际child/socket/durable writer需要或能够据此认证。runtime在写回执前drop，进程exit0由生产方runner记录；这些事实分别保留。

## 合同与剩余缺口

原002 ExecParams→003 Tool.Propose仍只生成提案，Proposed/execute=false不生成StartedProcess；断连路径是本地夹具错误。原LiveThread adapter只支持受限fake空历史，append/persist/flush/metadata/discard缺能力均拒绝，未冒充生产持久化或writer释放。网络本地Build错误和synthetic426的来源保持明确，没有成功ResponseStream/WS连接或宿主HTTP状态伪造。

HTTP注入仍提前跳过原account-routing→redirect Reject，且未传redirect_policy；三处接入同构建不会修复该合同缺口。M-02的域/主体/授权与撤销、M-03的获准网络/重定向/HTTP-WS阶段、M-04的metadata一致性/durable fence/writer取得与释放、M-06的固定执行输入/PTY/IO/终止（关联M-02）、M-08的账户与凭据恢复仍需要正式设计和新证据。003未新增这些语义，也没有第二份Schema。

未覆盖清单合理保留：直接LocalStore/LocalProcess、shell capture、Realtime与其他网络消费者、全部metadata context、acquisition取消/异步Drop、正常产品构建、生产ThreadManager/完整agent loop、成功传输与持久化、OS旁路。受限构建三处接入共存已比独立批次更进一步，但仍不是同一生产产品全路径接管。

本轮仅新增宿主检查器/结果/报告，未改插件封存、联合目录、003或生产源码，未执行探针、个人账户操作、真实模型请求、提交或发布。P02/J00/G0仍blocked，产品0/2，84项not_run；本片后不开始下一实施批次。
