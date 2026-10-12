# Windows SDK 限定进度（2026-10-13）

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
