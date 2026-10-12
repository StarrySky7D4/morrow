<!-- C28-WINDOWS-REGRESSION-20261013-BEGIN -->
## 启动 RPC 故障夹具与限定验证（2026-10-13）

本轮同步四个已编译 Rust 源文件：显式故障 feature 默认关闭，原 owner 校验、三条私有服务路由及正常路径保留。四阶段 Windows AMD64 编译与严格 HOME 后验通过；原 14 项与开启夹具后的 21 项合成回归全部通过，合计 35 次执行、21 个不同名称，其余清单条目未运行。保存的两份选定程序完成独立完整哈希／身份核对，未执行。

真实 RPC 失败测试仅归档源码候选，当前组合 native／VM 验收 NOT_RUN。宿主验收工具仍为未封存禁用草稿；原 Unknown 保留且不重放。SDK26／G04、namespace、祖先目录持续 reparse、生产沙箱与网络仍 OPEN，release_eligible=false，SDK 未冻结。此前 23 项 Dart 回归保持原范围，本轮未重跑。

详见 [v12 验证与后续任务](reconstruction-2026-10-13/startup-rpc-fault-validation-v12.md)。Agent 会话层与安全执行层完成后暂停准备预览，扩展执行层延期；版本、锁、工作流、main、标签与 Release 不变。本轮仅推送开发分支。
<!-- C28-WINDOWS-REGRESSION-20261013-END -->

## 当前 Rust 编译与 14 项指定合成回归（2026-10-13）

Stage501 当前组合的库测试编译（--lib --no-run）与宿主／supervisor 双 binary 编译均实际退出 0、双流 EOF；原锁与源码守卫、两次编译后的独立依赖目录严格复核均通过。双 binary 仅编译读取，未运行。

Stage503 从实际库测试产物列出 345 项，仅指定的 14 项合成测试逐项执行，均为 1 通过、0 忽略、退出 0 和双流 EOF；其余 331 项仅列出未运行。Stage496 的 5 个 Flutter/Dart 测试文件、原 23 项通过结果保持；十处生产源码／测试变更已提交开发分支。

历史失败与原 native Unknown 保留、不重放。Windows native／VM 联合验收尚未完成；SDK26／G04、namespace、ancestor、生产沙箱与网络边界仍 OPEN，release_eligible=false，不宣称 SDK 冻结或 Release。Stage505 故障注入候选尚未编译、未运行、未应用，本次不发布。以下旧段落保留其历史时点。

## 当前 23 项回归通过与十路径应用（2026-10-13）

Stage496 的原 23 项 Flutter/Dart 回归全部通过（5 个测试文件，0 失败、0 跳过），恢复页面原中文“操作结果未知”断言通过。九份真实语言资产已绑定，原测试方法、语言校验和断言未放松。原外层／内部 runner 返回 0、双流 EOF 与原工具组闭合已记录；工具清理实际 -1 原样保留，外层与内部 PID 分列，不据此证明操作系统祖先关系或全部后代静止。

Stage500 已应用十处生产源码／测试变更：五个 Rust 叶、三个 Dart 生产叶及两个测试叶。现有密钥读取加固与 20 份锁保持。Stage490 的 2394 文件源码与独立依赖种子准备实际完成，范围仅为数据字节；当前组合 Rust 编译、14 项 Rust 回归、宿主／supervisor 双 binary 构建和 Windows native 验收均未完成。

Stage479 原 22 通过／1 失败，Stage491003 的入口语法失败，以及 Stage491004 的配置引用守卫拒绝均保留；后两次没有执行 23 项测试。本次通过不抹除历史失败，也不证明真实 RustWorkbench.open/catch、异常关闭或受保护数据路径。原 native Unknown 保留且不重放。

下一步为当前组合的受控编译、原 14 项 Rust 回归与双 binary 构建，之后分别验证真实正常／异常 native 路径。SDK26／G04、namespace、ancestor、生产安全、网络仍 OPEN，release_eligible=false；SDK 未冻结。Agent 会话层与安全执行层完成后暂停，扩展执行层延期。

[前次失败与未运行记录](reconstruction-2026-10-13/startup-integration-progress-v11.md)保留原字节。以下旧段保留各自历史时点，不作为当前未应用状态。

## 当前诊断与回归入口状态（2026-10-13）

旧 23 项结果保持 22 通过／1 失败。资源绑定入口先因 launcher 解析失败退出（原退出 254），随后在创建进程前因一处包配置哈希引用不一致被拒绝；两次均未运行 23 项测试，未确认原中文提示失败已修复。本机失败记录器已限定结束，仅确认本机闭合。

十路径候选未应用生产，新的联合构建／Rust 测试未运行。下一步同步配置引用并重跑原回归，再推进生产组合验证。原 native Unknown 不重放；SDK26／G04、namespace、ancestor、生产安全与网络仍 OPEN，release_eligible=false。见[最新限定进度](reconstruction-2026-10-13/startup-integration-progress-v11.md)。以下旧段保留各自历史范围。

## 当前 23 项内存回归与组合准备（2026-10-13）

本轮 22 通过、1 失败，原退出 1 保留；恢复页面中文提示失败的根因尚待核对，不宣称已修复或全通过。新增可空 JSON 测试本项通过。十路径候选组合映射已准备，保留现有读取加固和 20 份锁；未应用生产，新的联合构建与真实正常／异常关闭待验收。

原 native Unknown 不重放。SDK26／G04、namespace、ancestor、生产安全与网络仍 OPEN，release_eligible=false。详见[最新限定进度](reconstruction-2026-10-13/startup-integration-progress-v10.md)。以下旧段保留历史范围。

## 失败启动 kind4 时序修正与本地正反回归（2026-10-13）

新增 r003 仅归档四个 Dart 源文件，不激活生产入口。kind4 准入同步锁存 pending；原 flush 成功或失败后才重新评估完成，失败仍保留 Unknown／无回执／不重试。相同测试源的 fixed-B 21 项逐项成功且未跳过；negative-A 使用旧 channel，只运行一个焦点，真实在原第 435 行提前 settled 断言失败（Expected false / Actual true），原非零退出保留。B 外层与内部 runner 均返回 0；A 均返回 1，双流 EOF 分别记录。

两个选定静态目标均退出 0；channel 保留 1 项 info，测试目标 No issues found。原 Flutter 编译器／测试器工具清理的实际 -1 保留，不改写为 0。外层捕获进程和内部 launcher 的 PID 分别记录，不据此声称操作系统同进程／祖先关系或全部后代静止。

本轮 B 源图 667 项中有 663 项与公开树完全同字节，仅三条生产 Dart 源不同及一个新增测试缺失。该结果覆盖实际 channel 的内存流／IOSink 时序，不覆盖 RustWorkbench.open 的原 catch 接线或真实 Windows job／owner 事务；既有 11 项启动观测测试未在此轮重跑。公开 Rust 生产入口仍缺少配套扩展，五叶候选保留在独立 r002 档案；因此不是全 runtime 配套合入。

原 native Unknown 继续保留、未确认闭合，不重放或终止。本轮没有新的 VM／native／DPAPI／真实库执行；旧正常验收与历史失败各自保留。SDK26／G04、namespace、ancestor 持续 reparse、生产安全及网络约束仍 OPEN，release_eligible=false；不宣称 SDK 冻结。Agent 会话层与安全执行层完成后暂停，扩展执行层延期。

See [v9 bounded summary](reconstruction-2026-10-13/startup-integration-progress-v9.md). Historical text follows unchanged.

# Windows SDK 限定进度（2026-10-13）

## 异常启动等待边界与静态复核（2026-10-13）

本次真实异常启动调用达到 1200 秒等待上限后仍未闭合；原始 Unknown 与原控制进程／读取器保留，不重放。实际退出、双流 EOF、原连接关闭与 Process 临时策略恢复均未确认，不能作 native 通过断言。

六个关键源码的双遍完整 SHA 核对零漂移，静态字段门禁未发现确定性错配。发现 kind4 发送确认晚于退出及双 EOF 时可能导致隔离回执缺失的条件性误拒绝风险；尚未证明本次触发或构成根因，未实施修补，也未运行对应时序回归。缺失／无效最终回执时保留 Unknown 的门禁未绕过。

此前四项 IO 和 v6 正常路径结果保持原范围；以下 v7、v6 与更早段落保留各自时点。SDK26／G04、namespace、ancestor 与生产安全仍 OPEN，release_eligible=false。详见[最新限定状态](reconstruction-2026-10-13/startup-integration-progress-v8.md)。

## Windows 密钥读取边界修正与限定验证（2026-10-13）

本轮仅合入 `audit/src/keys.rs`：Windows 最终叶使用同句柄、有界读取并拒绝 reparse／元数据不稳定；不新增 unsafe，不修改 provider、DPAPI 或 Linux 原读取表达式。真实编译与 4 项普通合成文件 IO 测试通过，包括实际文件 symlink 的拒绝，均未跳过；独立回读零漂移。

失败启动 runner 的一个本机静态目标分析退出 0、双流 EOF，No issues found!；未执行 runner main，不能替代 native 验收。新的真实失败启动验收仍待闭合；本次仅保留待定状态，不作通过断言。

公开 Dart 三个生产文件仍比实际选定的正常／失败夹具基线旧，本轮未全量合入。新读取修正也不在既有正常验收二进制中；不将旧二进制结果当作本次修正的产品验收。

详见[本次源码与验证边界](reconstruction-2026-10-13/startup-integration-progress-v7.md)。旧 v1–v6、历史失败与 sealed Unknown 均保留，不重放。SDK26／G04、ancestor、namespace、生产安全与网络约束继续 OPEN／未验收，release_eligible=false；不宣称 SDK 冻结。Agent 会话层与安全执行层完成后暂停准备预览，扩展执行层延期。版本、锁、工作流、tag 与 Release 不变。

## Windows 正常原生会话联合实测（2026-10-13）

1 项可见 native qualification test 实际通过；原 16 份公开证据文件双遍完整 SHA 核验零漂移。31 帧 trace、11 项 observations 在同一身份下记录 `Running → healthy Stopping/Reclaimed → ACK → Released`；PS30 generation 1、effects 10、consumed completions 7，原生 host 退出 0。宿主捕获退出 0、双流 EOF，独立会话关闭且 Process 临时策略恢复；原编译器／测试器工具清理退出 -1 保留，不冒充退出 0 或全部后代静止。

此前 14 项 Rust 合成回归、16 项 Dart 内存回归以及启动失败夹具 4 项纯测试／4 项静态检查保持各自有限范围；真实失败启动 native 夹具仍 NOT_RUN。NORMAL ONLY，完整 SDK26／G04、namespace、祖先目录持续 reparse、生产安全及网络约束仍 OPEN／未验收，release_eligible=false。历史 Stage341 CLOSED_NOT_ACCEPTED 与旧 Stage324 sealed Unknown 保留且不重放；本次通过不升级历史结果。

详见[本次正常路径实际边界](reconstruction-2026-10-13/startup-integration-progress-v6.md)。Agent 会话层与安全执行层完成后暂停准备测试预览，扩展执行层延期。生产入口、版本、依赖锁与工作流不变；不创建 Release。下方旧段保留历史范围。


## 启动失败夹具实测与联合验收包进度（2026-10-13）

此前归档的启动失败夹具 r001 已完成四项纯数据测试：每项成功一次、未跳过，外层退出 0、双流 EOF，独立回读零漂移。原 Flutter 编译器／测试器清理时的实际退出 -1 保留，不改写成 0，也不据此宣称全部后代进程静止。四个选定文件的静态检查均退出 0、双流 EOF、No issues found!；其中包括 120,739 字节的新 workbench_native.dart。真实 native 失败夹具仍未运行。

正常路径联合验收包已完成本地数据准备：9,046 个负载文件、9,048 个 ZIP 条目、666 个 Dart 源码文件，两轮完整文件 SHA、ZIP SHA、逐项 SHA／CRC 回读通过且零漂移。包使用此前编译的当前 host／supervisor 和三个正常路径 Dart 文件，不包含新失败夹具；正常 19 字段与原资格合同保持。此结果仅证明本地包完整性，未执行产品程序或虚拟机验收。

后继宿主载体源码已通过独立审核，补入旧测试进程碰撞检查及不可读字段拒绝；它只是固定进程名和路径前缀的时点检查，不是连续命名空间或静止证明。传输、执行入口尚未选择，VM 传输与 Rust／Dart 联合运行均为 NOT_RUN。

此前 14 项 Rust 合成回归和 16 项 Dart 内存回归的限定结果保持。SDK26／G04、namespace 与祖先目录持续 reparse 防护仍 OPEN，release_eligible=false。Stage341 仍 CLOSED_NOT_ACCEPTED，旧 sealed Unknown 保留且不重放。下一步完成正常路径联合验收，再单独验证异常隔离；Agent 会话层与安全执行层完成后暂停，准备测试预览，扩展执行层后置。

见[本轮实际边界](reconstruction-2026-10-13/startup-integration-progress-v5.md)。下方历史 NOT_RUN 描述各自时点，不覆盖本轮新增结果。


## 原生接线静态检查与启动失败测试候选（2026-10-13）

当前 r002 的 workbench_native.dart 静态检查已实际通过：原分析器退出 0、双流 EOF，输出 No issues found!，源码／配置守卫及独立回读通过。这只覆盖 120,251 字节的 r002 文件；第三方包仅核对目录元数据，不能据此宣称完整 SDK 已冻结。此前当前 Rust 编译、14 项合成回归和 16 项 Dart 内存测试的限定结果保持不变。

上一阶段源码与原始编译／测试证据已生成本地增量归档，220 个选定输入、223 个 ZIP 条目通过完整 SHA／CRC 复核。归档依赖已保留基线及工具环境，不是自包含 SDK，本次不上传该 ZIP。

另归档默认禁用的启动失败夹具 r001：四项纯数据测试与一项真实 native 测试源码尚未编译或运行。新候选的 workbench_native.dart 为 120,739 字节，不继承旧 r002 静态 PASS；原正常合同与 checker 不变。

Rust／Dart 联合运行、真实 Windows 沙箱与资源回收仍 NOT_RUN，SDK26／G04、namespace 与祖先目录持续 reparse 防护仍 OPEN，release_eligible=false。Stage341 仍 CLOSED_NOT_ACCEPTED，旧 sealed Unknown 保留且不重放；优先继续联合验收，扩展执行层后置。

见[本轮实际边界与未运行候选](reconstruction-2026-10-13/startup-integration-progress-v4.md)。下方历史 NOT_RUN 字段描述各自时点。


## 当前 Rust 候选编译与 14 项回归实测（2026-10-13）

修正版数据准备已实际成功，2,392 个源码文件、5,589 个 Git seed 文件完成字节核对，20 份依赖锁保持原字节。实际源码映射及五个 overlay 与已归档的 startup-quarantine-runtime-r002 完全对应；旧候选文件中的 NOT_RUN 保留为当时快照，本节记录后续实际结果。

当前候选的完整 Rust 库测试目标与 host／supervisor 两个程序均完成离线、锁定依赖的 Windows AMD64 编译；两次原 Cargo 均退出 0、双流 EOF。当前两个编译后的 HOME 核验通过。复用 TARGET 已明确记录，编译警告原样保留；这不是 Flutter GUI 构建或完整 SDK 验收。

14 项精确合成回归全部实际通过、未跳过：4 项 AgentWorker 停止、5 项内存 owner 账本、5 项 supervisor 解析与心跳。每项原测试进程均退出 0、双流 EOF，守卫无错误。库测试程序已用于这 14 项测试；host／supervisor 产品程序仅编译和核对，未运行。此前 Dart 修正版 16 项内存测试通过的限定范围保持不变。

当前 workbench_native.dart 静态检查、Rust／Dart 联合运行、真实 Windows 沙箱及资源回收仍为 NOT_RUN。SDK26／G04、namespace 与祖先目录持续 reparse 防护仍 OPEN，release_eligible=false；Stage341 仍 CLOSED_NOT_ACCEPTED，旧 sealed Unknown 保留且不重放。下一步优先完成原生接线检查与 Windows 联合验收，扩展执行层继续后置。

本次更新开发分支的实测报告与进度文档，保留历史失败和范围边界；生产入口、应用版本与依赖锁保持不变，不创建 Release。

见[本次实际验证边界](reconstruction-2026-10-13/startup-integration-progress-v3.md)。

以下保留历史快照，其中 NOT_RUN 描述各自时点，不覆盖本轮当前 Rust 候选的实际结果。

## 启动隔离 A/B 实测与准备工具修正（2026-10-13）

旧 channel 的新增反例已实际复现：未请求正常关闭时仍接受 Released 并返回 0。负对照保留原退出 1 与 CLOSED_CAPTURE_FAILED；修正版完整 16 项 Dart 内存回归全部成功、未跳过，原 runner 退出 0。两路捕获、runner Future 与双流 EOF 已闭合；编译器／测试器的原工具清理退出 -1 保留，不据此宣称全部后代进程静止。

本轮测试只导入 channel／supervision，不导入 workbench_native.dart；该文件当前静态检查、完整 Rust 候选编译与联合运行、真实 Windows 资源回收及 VM 验收仍为 NOT_RUN。此前基线通过结果不覆盖当前候选，Stage341 仍 CLOSED_NOT_ACCEPTED，旧 sealed Unknown 保留且不重放。

另修正数据准备脚本中 overlay 局部变量覆盖控制快照的问题，17 个 helper 原字节及逆差量已核对。修正版入口仍 SOURCE_NOT_RUN；公开归档仅含语义补丁和核对清单。编译捕获与 HOME 守卫已形成禁用草稿，尚未绑定真实准备记录或运行编译。

SDK26／G04、namespace 与祖先目录持续 reparse 防护仍 OPEN，release_eligible=false。下一步执行修正版数据准备，再验证完整 Rust 编译、原生接线和 Windows 原资源闭合。本次同步开发分支的实际测试进度与准备工具修正，生产入口、应用版本和依赖锁不变。

见[本次实际验证边界](reconstruction-2026-10-13/startup-integration-progress-v2.md)。

以下是此前源码准备与验证快照；NOT_RUN 描述各自时点，不覆盖本轮实际 A/B 结果。

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
