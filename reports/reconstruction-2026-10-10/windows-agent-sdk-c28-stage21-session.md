# C28 阶段 21：新 guest 七步会话通过与公开 fixture 接入

截至 2026-10-10，新公开 guest 已在真实 Wasmi 宿主中通过单次合成会话 ABI 与七步行为测试；公开 fixture 接入源码及 1,654 文件载体已完成独立核对。生产根库/主程序编译和新增四项 identity 测试仍为 `NOT_RUN`。`SDK26_G04=OPEN`，`release_eligible=false`，SDK 未冻结。

本记录接续[上一轮构建及静态接口记录](windows-agent-sdk-c28-stage21.md)，保留其当时“七步尚未执行”的历史状态。本轮同次发布七份 fixture 接入文件（四份修改、三份新增）及本报告，新 fixture 已接入；当前完整公开树编译状态为 `NOT_RUN_PENDING_ROOT_BUILD`，四项 identity 测试为 `NOT_RUN`。本机入口、完整控制器、原始日志、缓存、普通 SQLite 数据及测试程序不属于公开载荷。

## 本轮实际结果

| 工作 | 结果 | 限定范围 |
| --- | --- | --- |
| 七步会话资格 | metadata、测试编译、单个精确测试及外层均实际退出 0；`1 passed; 0 failed` | 新固定 Wasm、普通合成 SQLite 会话；不是生产沙箱或原生生命周期验收 |
| 原始输入与新缓存守卫 | 626 项输入前后记录相等，三个阶段的新缓存守卫通过；原锁、原冻结 Wasm 和 `SESSION_SHA` 未改写 | 独立读回核对三个原始捕获、产物、选定当前输入和缓存文件；未重复散列全部依赖载荷 |
| fixture 接入源码 | 七份文件完成独立源码、身份、相对引用和有限隐私检查，无阻塞发现 | 新身份采用固定长度及 SHA256 的独立路由；旧三个身份常量、旧校验器及回执解析语义保留 |
| 新公开源码载体 | 1,654 文件全部长度及 SHA256 独立读回通过；原 20 份锁和 60 份 manifest 不变 | 来源为固定公开提交的 1,650 份选定原文件，加接入变更；不代表公开 Git 树已经完成编译 |
| 生产根库/主程序及四项 identity 测试 | `NOT_RUN`；编译控制器源码已审核，测试入口已准备 | 尚无本轮生产根编译或这四项测试的通过结论；不执行主程序、runner、setup 或 VM |

七步是七次 R2 `agent_call` 交换：创建父会话、打开写入器、追加事件、写检查点、读取父快照、创建子会话、读取子快照；另有读取任务输入和完成任务两次 ABI 调用。实际核对 48 字节回执、事件与检查点摘要、子会话继承，以及两次运行后只读快照；八个旧工厂拒绝新 guest。测试仅授予两个会话的 read/write，ProcessControl 为 0，未注册进程 provider，未调用 Claim 或 native Start。

测试使用原 127 包宿主锁（7 个 path 包、120 个 registry 包），离线构建，没有新增包、换锁或下载。有效 fuel 上限为 2,000 万。编译工具运行、系统随机数及独立普通 SQLite 写入是该测试的必要效果；本次不证明操作系统写入隔离、真实断连、管理器所有权或完整 SDK 行为。

## 固定身份与接入范围

新 Wasm 为 **425,912 字节**，SHA256：`cca04ebb2e787f69e84ec7260aca3e93ec895ec17b68afbb660e3c6896ae2f2b`。原冻结 Wasm 的 SHA256 仍为 `b1f0f44ad2bbe2abc89eee0ee971683419bf3407f370194f5557df57fa45799e`，未修改；新载体只省略旧私密 fixture，采用独立的 `public-session-r2-v1` 身份。此前固定机器路径模式扫描零命中的结论保持其有限模式范围，不称作通用秘密检测。

七份接入文件位于 `companions/morrow-codex/qualification/`：更新 `c28-basic-harness/src/formal.rs`、`wrapper.rs`、`c28-basic-support/wasm/sealed.rs` 和 `c28-basic-fixtures/README.md`，新增 `c28-basic-fixtures/public-session-r2-v1/` 下的 Wasm、README 与 provenance。新 formal 使用独立 package/archive 身份；运行时保持两个会话的 read/write 与 ProcessControl=0。新增四项纯 identity 测试覆盖精确身份、旧路由拒绝、长度及内容变异，尚未运行。

载体构成为原 1,651 文件减旧私密 Wasm、替换三份源码、增加四份文件，共 1,654；fixture 总 README 在原载体中不存在，属于新增文件。历史 1,651 文件及其两份 formal 清理源码的来源证明单独保留，没有改写旧 schema 或旧失败。

| 证据 | SHA256 |
| --- | --- |
| 七步实际回执 | `6659396658007c2a6be4a36b7ddfc0def1f5cdeec8531e2d1b28b203cca263a7` |
| 七步独立有限读回报告 | `b6041b99dccdae67d0491b4ac26a521b7f6669bafd50f7a3926a53ba065e022c` |
| 七份 overlay 清单 | `2fd9f084dae4d4a0cdc8d70720aab8aed464451666c5526f622c92d5cfa6ad37` |
| 1,654 文件载体清单 | `f90f7acac89463e2699f45e410a113b28146079e24d2e773da1a201028ceba15` |
| 载体物化回执 | `56e2b5aadd1190bc4c612ba78e644862f13d1d3c0dcfe4958a56824f9efcf400` |
| overlay 与载体独立审核报告 | `043eaec20c7c19aef452b7a5969b422468e1f3098f67bfb98a195fb97306f696` |

载体清单为 315,736 字节，物化回执为 2,259 字节。这些哈希用于追溯固定证据，不表示原始本机材料随本报告公开。

## 保留的边界

原生 Start 仍为 `SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY`，未自动重放；owner finish、factory release、cleanup/join、真实断连及 Windows 生产沙箱仍待验收。阶段 18 原根 build003 仍为 Cargo 退出 0、外层退出 1，其后置守卫没有补记通过。此前 guest 第一次 metadata 的权限失败、第二次 metadata 退出 0 而外层退出 1 的固定图计数错误也保留原结果。

匹配 helper 的离线构建通过结论沿用上一轮，runner/setup 未运行。本轮七步通过不扩大为 SDK、VM、生产发布或完整安全执行通过。下一步仍是完成生产根编译及四项 identity 测试，再按授权推进会话层与安全执行层验收；完成后暂停准备测试预览，不等待扩展执行层。
