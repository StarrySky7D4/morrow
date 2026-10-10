# C28 阶段 21：checkout 来源验收、匹配 helper 构建与公开 guest 后继

截至 2026-10-10，阶段 20 的严格 checkout 第二轮验证已实际通过并完成独立有限读回；第三轮匹配 helper 离线构建的 Cargo 与外层均退出 0。公开 guest 的第二次 metadata 子进程退出 0，但外层按原图契约拒绝；第三次后继仍为源码准备，未运行 metadata 或编译。

本次开发分支更新仅同步脱敏进度文档。已选产品源码与已推提交 `58391f3c640d6856c7cf6be2ffb2bc05cb915ba5` 一致，应用版本保持 `0.1.9-test.58+62`。

## 已完成的有界验证

| 工作 | 本次实际结果 | 验收边界 |
| --- | --- | --- |
| 三个 Cargo Git checkout 的来源与固定元数据 | 第二轮验证退出 0；2,904 个 loose objects 与冻结数据库压缩字节、Git SHA1、长度及 zlib 完整性一致，选定 commit/tree 闭包、2,662 个 tracked files、索引及 TREE cache 通过。新报告、审核绑定、源码、计划和实际终端记录已独立有限读回 | 独立读回核对已完成报告及其身份，不重复重算全部对象。选定快照缺少历史 parent objects，不是完整历史 fsck；此结果不补记旧生产者后置守卫通过，也不接受整个 Cargo home、运行时或 SDK |
| 当前库、runner 与 setup 的匹配离线构建 | 第三轮构建的 Cargo 退出 0、外层退出 0；原始输入前后守卫和物理集合一致，完整新 Cargo home 的严格来源、索引、固定元数据及已知 bookkeeping 后置守卫通过。保存当前库与两个程序的三个新产物；独立读回核对三个产物与 169 个源码文件当前字节，并完成日志、严格守卫报告及真实外层终端记录的交叉绑定 | 独立来源与哈希读回已完成。未运行 runner、setup、原始库测试、真实 Windows 沙箱或 VM；不能据此声明三程序运行验收或生产部署通过 |

checkout 第一轮验证曾因错误要求六个未复制的空数据库目录而退出 1。第二轮仅修正这六个固定目录的期望，冻结原缓存仍按原目录契约核验；未知文件、目录、外部对象路由和活动 hooks 仍被拒绝。第一轮失败及其原始输入保留。

helper 三个新产物的生产者记录如下，仅公布身份，不发布二进制：

| 产物 | 字节数 | SHA256 |
| --- | ---: | --- |
| `libcodex_windows_sandbox.rlib` | 22097724 | `a48ca6c612f8da64289c5646f9b879f23729fcc89e741d970a52904abcbba983` |
| `codex-command-runner.exe` | 24378368 | `c75530dbc7b0c789d4908c02628bafdf66cb73e46b109f1eeecc35e11dcebf8a` |
| `codex-windows-sandbox-setup.exe` | 37725696 | `3072a6c5443c6ec11fbaac266a66e88f0539fba34a6d8738c0251a526161de3d` |

这些哈希对应本轮保存并经独立当前字节读回的新产物。此前 runner/setup 的来源不匹配记录继续作为历史证据保留；当前 helper 的验收范围仍限于本次构建、来源和哈希读回。

## 公开 guest：两次失败与未运行的第三次后继

第一次 metadata 实际退出 101，独立退出记录及完整捕获已保存；解包 anyhow 时路径规范化遇到权限拒绝（os error 5）。旧主收据因记录追加前抛错而保留空记录列表，不能据此将实际退出视为未知。完整 Cargo home 的空目录集合守卫也拒绝；原输入复核相等，没有被接受的 guest 产物。第二次 metadata 子进程退出 0 并完整保存原始输出，外层退出 1，错误为原检查器要求全部 27 个锁定包出现在 target-filtered graph 中。第二次的原始输入、锁、冻结 Wasm 及 Cargo home 守卫均保持通过，未启动 guest 编译。

第二次实际 `wasm32-unknown-unknown` 图包含 25 个 packages 和 25 个 resolve nodes，恰好是原 27 包锁减去 `cpufeatures 0.2.17` 与 `libc 0.2.189`。归档清单中 sha2 的目标条件不为 wasm32 引入 cpufeatures，libc 仅由该锁中的 cpufeatures 路径引入。metadata 本身不提供 checksum；来源仍由未改的锁与已核对归档约束。

第三次后继只准备固定的“原 27 包减去这两个指定包”的 25 节点契约，不开放任意子集或版本偏移。它保留两个失败尝试、原锁和冻结身份，并锁定原有四个本地包路径与 feature 选择。第三次 metadata、编译、新 Wasm 全字节隐私检查和七步会话行为资格仍为 `NOT_RUN`；ABI 兼容性为 `PENDING`。没有新 Wasm 哈希被接受或接入公开 harness。

310 个公开 guest 源码文件与既有开发分支逐字节一致。已选载体中的 1,650 个公开文件也与该分支一致；唯一未公开的既有文件仍是含本机路径的原 session Wasm。公开 Git 树的完整构建状态继续为 `NOT_RUN_MISSING_SESSION_FIXTURE`，不能用静态来源或 metadata 通过代替完整公开构建。

## 保留的失败与开放事项

阶段 18 的 build003 保持 **Cargo 退出 0、外层退出 1**，原始后置守卫未补记通过。新的 checkout 来源验收与 helper 构建各有独立范围，不修改旧收据。

原生 Start 保持 `SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY`，不得自动重放。owner finish、factory release、cleanup/join、真实断连、Windows 生产沙箱及后续 11 项原始库测试仍待验收。`SDK26_G04=OPEN`，`release_eligible=false`，SDK 未冻结。

下一步按审核后的固定契约运行 guest 第三次后继，完成新字节隐私检查与真实七步会话资格；其后在明确授权范围内复验 Windows 生命周期与安全执行。会话层与安全执行层验收后暂停准备测试预览，不等待扩展执行层。

本次只更新既有开发分支文档，不更新 main、tag 或 Release，不触发新的构建、VM 操作或手动 CI。本机专用入口、原始路径表、缓存、操作记录及二进制不随文档公开。[阶段 20 准备记录](windows-agent-sdk-c28-stage20.md)与[阶段 18 编译记录](../reconstruction-2026-10-09/windows-agent-sdk-c28-stage18.md)保留各自历史时点。
