# Windows SDK 限定进度（2026-10-13）

## 启动隔离源码与验证准备更新（2026-10-13）

Dart 候选 r002 修正了正常关闭与失败启动收尾共用标记的问题：只有原通道明确请求正常关闭后，才允许接受 Released 与实际退出 0。原有 15 项测试保持原字节，另增 1 项“未请求正常关闭却收到 Released”的反例；共 16 项测试均为 `NOT_RUN`，并未据此宣称缺陷已通过运行验证。

Rust 候选 r002 保留此前隔离、心跳和账本改动，删除 binary 中三行遗留模块声明，由 library 持有 supervisor 实现。这是源码层最小修正；完整 crate 的编译、测试和 Rust／Dart 联合运行仍为 `NOT_RUN`。此前 Stage355 编译通过的是 Stage349 基线，不覆盖本次候选。

Stage359 的 A／B 两套隔离测试数据准备已分别退出 0，各生成 667 个源码文件及 2 个配置文件；未运行 Dart、Flutter 或原生程序。A 计划仅运行旧 channel 的新增反例，B 计划运行修正版 16 项，计划结果不能算作实际测试结果。

历史 Stage353 九项合成回归、Stage354 五项原逻辑摘取测试、Stage355 基线编译及产物核验的限定范围不变；Stage341 仍为 `CLOSED_NOT_ACCEPTED`，旧 sealed Unknown 不重放。SDK26／G04、namespace 与祖先目录持续 reparse 防护仍 OPEN，`release_eligible=false`。

下一步：执行 A／B 对照并核对原始退出和机器测试事件，随后编译完整 Rust 候选、验证联合运行与 Windows 原资源闭合。本次仅同步开发分支的源码候选和进度，生产入口、应用版本与依赖锁不变，不创建 Release。

见[本次源码与验证边界](reconstruction-2026-10-13/startup-integration-progress-v1.md)。

以下保留 Stage358 及之前的历史快照，其 NOT_RUN 仅描述当时时点；各次通过范围不自动覆盖后续源码。

## Windows 构建与启动隔离候选进度（2026-10-13）

Stage349 固定组合基线的 Windows AMD64 host／supervisor 已实际完成离线、锁定依赖编译，原 Cargo 退出 0、双 EOF。产物读取包装器曾因 Cargo 硬链接而退出 1，首个恢复读取器也失败；两次失败保留。修正版独立恢复两个产物，并通过两个原编译时间窗的完整选定 Cargo HOME 后验，未重新编译或执行程序。这是构建与产物核验，不是 Windows 产品运行验收。

Stage357 的 Dart 失败启动收尾已作为候选归档：私有失败标记关闭业务入口并停止心跳，只在原通道验证异常状态后单次请求隔离；实际退出 2 和双 EOF 才能记录单独快照，业务仍为 Unknown、owner 为 Retained。三个生产 overlay 与 15 个内存测试均未激活，Dart 静态分析和测试为 `NOT_RUN`；完整 Stage354 Rust 迁移及 Rust／Dart 联合运行仍未验证。

此前 Stage353 九项合成回归、Stage354 五项原逻辑摘取测试的通过范围不变。历史 Stage341 仍为 `CLOSED_NOT_ACCEPTED`，旧 sealed Unknown 不重放。SDK26／G04、namespace 与祖先目录持续 reparse 防护仍 OPEN，`release_eligible=false`。

下一步：先完成 Dart 候选的静态分析和 15 项回归，再验证完整 Rust／Dart 源图、原资源闭合与 Windows 停止序列。应用版本不变，本次仅同步开发分支，不创建 Release。

见[原生构建核验](reconstruction-2026-10-13/native-build-progress-v2.md)与[Dart 候选](reconstruction-2026-10-13/source-candidates/startup-quarantine-dart-r001/README.md)。

以下为 Stage356 及之前的历史快照，其中 NOT_RUN 仅描述当时时点，当前状态以上节为准。

## 本次开发分支更新

| 验证范围 | 实际结果 | 边界 |
| --- | --- | --- |
| Stage349/353 组合 library | 原 Cargo 退出 0，双 EOF；固定源码与编后 HOME 检查通过 | 2,392 个源码文件、20 个锁文件；复用已有 target 缓存；不是完整 audit all-feature 或产品构建 |
| Stage353 九项 exact | 每项 1 passed、0 ignored、退出 0、双 EOF；独立回读零漂移 | 四项真实受控 AgentWorker + 五项内存 SQLite 账本；预期 worker panic 原样保留；不是原生资源证明 |
| Stage354 heartbeat 修补 | std-only 原函数与真实 LeaseSignal 摘取编译，五项原测试通过 | 完整 runtime/library/bin 尚未编译；无真实 Job、stdin、reader、硬截止时间或 VM 运行证据 |

组合源码仅包含 Stage345/346/347；不包含 Stage350/354 的完整 supervisor 迁移。编后检查覆盖有限 HOME、原始源码、固定工具与 1,668 个 registry 元数据文件，未声称重扫所有依赖负载或整个 target。

heartbeat 修补保留丢失时钟与业务门禁，只在异常标记发布前后保护原控制读取器；原始 reader 的 IO、协议、队列、panic 或期限失败仍不能生成隔离 prepare。修补以新候选增量归档，旧候选和历史失败记录保持不变。

完整 host/supervisor 构建、新 VM 产品复验、Dart/runner 隔离接线、实际原资源闭合、真实 DPAPI 与 sealed-basic 仍未运行。Stage355 仅有本地未执行构建准备，不作为产品通过证据。历史 Stage341 仍为 CLOSED_NOT_ACCEPTED，旧 sealed Unknown 不重放。SDK26/G04、namespace 和祖先目录持续 reparse 防护仍 OPEN；release_eligible=false。

见[脱敏验证摘要](reconstruction-2026-10-13/windows-integrated-validation.json)与[heartbeat 源码增量](reconstruction-2026-10-13/source-candidates/quarantine-runtime-r003/README.md)。应用版本不变，仅推送开发分支，不创建 Release。

## 上次推送时的历史记录（Stage352）

以下文字保留当时的验证范围；其中 NOT_RUN 状态仅描述 Stage352 时点，当前结果以上节为准。


本轮生产改动限定为 Agent 停止状态修正与 Windows 审计密钥的同句柄读取。应用版本不变；开发分支源码更新不构成完整产品或 SDK 验收。

345 与346 合并后的完整 crate 以及 host／supervisor 双产品构建仍为 `NOT_RUN`；349／351 尚未执行。各自限定测试通过不等于合并构建通过。

## 已完成的本地验证

| 范围 | 实际结果 | 可支持的结论 |
| --- | --- | --- |
| Stage345 SOURCE002 | library 测试编译通过；四项 exact 合成回归全部通过 | 原 live AgentWorker 取消后立即报告 Stopping；错误 key／非 Agent 无效果；失败 join 不升级为 Reclaimed，也不开放 ACK |
| Stage346 SOURCE002 helper | 一次限定编译、六种普通文件模式通过 | 发布后的原 File 保留至同步、受限读取、验证及 checked close；边界、失败与 Drop 路径的限定合成行为 |
| Stage346 真实 observer 摘取 | 四项原诊断测试、两项 helper 接线测试全部通过 | 原错误对象／固定阶段保留；超限读取报告 ReloadRead，验证错误不被改标为 PublishedSync |

Stage345 使用新建普通合成 SQLite 文件库与受控 worker，不构造受保护 WorkbenchState，不使用真实用户库或 DPAPI。早先内存数据库 fixture 因生产 WAL 要求失败，第一项退出后其余三项未运行；该失败保留，由 SOURCE002 的合成文件库修正后重新完成四项限定回归，未降低 Store 要求。

Stage346 第一组 helper harness 使用 observer stub，不能证明真实诊断；随后第二组直接摘取原诊断声明、真实 observer 和原四项测试，并增加两项接线验证。二者范围分开记录，不累计为完整 audit crate、保护服务或产品测试。真实 OS 同步／读取／关闭故障、完整 audit crate 构建、真实 DPAPI 以及新组合的 VM 产品复验仍未完成。

## 业务结果与未完成范围

历史 Stage341 的进程退出、双 EOF、Process 策略恢复与独立连接释放已核对，业务仍为 `NOT_ACCEPTED`：原 stop 未报告 Stopping，所需 `Stopping → Reclaimed` 序列未完成。Stage345 的本地回归针对该状态缺陷，但不能追溯改写 Stage341 结果。旧 sealed Unknown 保留，不自动重放，不伪称正常释放或策略已恢复。

Stage347 是私有账本验证／提交后读回候选，五项 exact 测试为 `NOT_RUN`。prepare token 仅确认账本事实；调用方布尔值不是内核资源观察，不能据此宣称 Job、原 child、reader 或 supervisor 已闭合。它未接运行链。

Stage350 SOURCE002 是失败启动隔离的后续源码候选，尚未编译，Dart 未接入。异常收尾期间的有效 heartbeat 只作保守处理，不续期丢失状态或硬截止时间；缺少原资源、join 或账本确认时不发 prepare。它不替代正常 Released，也不授权接管或重放。

Stage347／350 的 overlay 仅放入 `reconstruction-2026-10-13/source-candidates` 源码候选目录，状态为 `SOURCE_ONLY_NOT_COMPILED_NOT_CONNECTED`；本地构建及 VM 载体入口不随本次公开文档发布。

`SDK26_G04=OPEN`，namespace、祖先目录持续 reparse 防护、旧锁域兼容与 sealed-basic 验收仍 OPEN；`release_eligible=false`。同句柄读取消除本轮已识别的发布后路径重开窗口，不能证明祖先目录在整个操作期间不可重解析。

## 下一步

在精确源图和真实新产品产物绑定后，独立复验停止状态与密钥读取的产品序列；祖先目录保护和失败启动隔离各按真实资源证据推进。提交、推送、CI 与 Release 状态分别以对应实际记录为准，本页不预先声明已推送或已发布。
