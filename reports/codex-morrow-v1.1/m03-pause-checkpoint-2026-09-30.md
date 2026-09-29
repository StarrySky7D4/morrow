# M03 暂停与同步检查点（2026-09-30）

用户要求暂停后续任务并同步 GitHub 与 Google Drive，使用单个完整 ZIP，不分卷。长期推进目标已设为 paused；宿主、guest 和联合审查聊天均确认 idle、无在途命令，会话保留。此后不启动新 HTTP、构建、开发或独立复核。

## 最后运行状态

| 场景 | 真实运行 | 当前结论 |
| --- | --- | --- |
| network-abort-001 | 单次本机 POST；runner exit 0 | 生产方 expected_fault_observed；200 响应实际解析 138B、真实 Core delta，首取消 code26/source2；Unknown、无 HTTP EOF、无响应材料持久化，owner Released、线程回收确认。运行后独立全链审查尚未进行。 |
| authority-deadline-001 | 已启动的单次本机 POST 收尾；runner exit 1 | unconfirmed：guest worker joins unconfirmed。宿主最终 code20、Unknown、Released；guest 数据线程实际失败后已 join，但 evidence writer 未确认 join，缺 Close/ACK，宿主 exit2。不得据此授予完整原期限资格。 |
| pipe-partial-close | 未启动 | 保留已冻结入口；暂停期间不运行。 |

两份结果和全部 manifest 文件摘要及 guest 文件摘要已核对，无 seal-failure 标记；这仅证明封存完整，不改变未确认结果。

network result SHA-256：`3c59c6a8f44cf1bc3f833ac6f573ac55677db08ccb64e4ffcb5e858f218ae475`；manifest `caaff5ffbc9d9b134225ab446f317b604fef033b04988355c67afaf452155772`。

authority result SHA-256：`90ac88cf017a2c7e646bd1771b0b4739465f4e263c8d1d0891b7375588075a85`；manifest `2e172bbb628ae5923a4371e3a7d0c0d5a9a7b463e6453510413193be27983176`。

## 冻结与审查

[runner003 独立实现审查](joint/m03-stream-001/frozen-passive-review-003-002/review.md)给出限定 READY，仅准许已有授权的三项运行，不代表运行结果或 SDK 冻结。宿主270项与 guest78项输入、default/feature、Core15/native35、root13项纯检查及旧001/002阻塞原件保留。[阶段报告](m03-passive-fault-005-2026-09-30.md)记录各自边界。

fixture003 新源码和回执逐字节复制到 `companions/morrow-codex`，[复制清单](m03-fixture-003-source-archive.json)绑定原 manifest；独立 guest 目录原件保留。冻结二进制和 SQLite、原始日志进入 Drive 单包备份；GitHub 以源码、回执、报告和文本证据为主，manifest 中机器绝对路径是原运行证据，不视为任意机器可直接运行的入口。

## 恢复顺序

1. 用户恢复任务后，先读取 authority-deadline-001 的原 G3、host/control 和 evidence writer 结果，定位未确认回收；旧批次保持 unconfirmed，不覆盖、不改首因、不续原期限、不自动重放 Unknown。
2. 必要修复创建新候选和新证据批次，保留旧 manifest 和原始失败。然后补齐 pipe-partial-close，仍按每批一个明确本机请求验证。
3. 联合聊天只读复核真实 wire/ledger/原始事件和回收，限定签收已达到的目标；最后更新看板。真实原完整 WriteFile 非零短成功 completion、并发/崩溃 owner、完整产品 G0 和跨平台资格仍开放，SDK 未冻结。

## 同步边界

当前开发分支 `codex/m03-stream-revocation-backpressure`；同步前远端该分支 `21f84aaebca31c8ef8d3bdfb3b9788f2a4cffb54`，main `adccf2a601609963ee4092f32423d0d433efe51b`。本次仅同步开发检查点，不合并主线、创建标签或 Release。实际推送与 Drive 上传结果另保存到本地备份目录 delivery-receipt.json；必须以远端 readback 和上传后元数据为准。

备份覆盖当前 Git 工作树的完整受版本控制源码、当前新增源码/报告、选定非缓存证据、独立 guest 源码与冻结候选及本次真实运行文件，逐项 SHA-256 清单置于 ZIP 内。可重建 target/构建缓存、临时用户 profile AppData 与凭据不打包；原件不删除。一个已有上游 dangling symlink 如无法读取，保存 Git 原 blob 及其 mode，在清单中明确标识，不能冒充实际目标文件。
