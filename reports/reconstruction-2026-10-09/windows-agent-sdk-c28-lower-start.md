# Windows Agent SDK C28：逐次启动诊断

更新：2026-10-09。基于开发分支 `codex/windows-sdk-convergence-20261005` 的 `e9659312c6667a413e13ebe14a6b8d34ca2b8f9b`，本次同步诊断源码和阶段状态，应用版本保持不变。

## 实现

新增 Windows 启动阶段观察器，沿已匹配的 runner 路由传递固定枚举，记录权限准备、账户选择、runner 校验、管道、握手与 SpawnReady 等阶段。每次 backend 调用创建独立记录器，保留首个失败以及原路径已有的一次刷新前后的观察。普通 backend 的默认入口仍只调用原 Start 一次，不增添执行、重试或权限。

Native 保存最新调用历史，旧调用的迟到结果不能覆盖较新记录；序号耗尽或锁中毒只使观察不可用。`OWNER_LATEST_CALL_NOT_OPERATION_PROOF` 明确说明它是 owner 历史，既不是当前请求证明，也不是与原有标志组成的联合原子快照。返回 backend handle 不代表 exit、EOF、ACK、清理或 join。

同步独立 V3 日志投影规则，保留 V1/V2 的完整匹配和原日志、控制器、文件及大小限制。新增规则仅接收固定字段和枚举，不输出错误正文、路径、账户、命令、环境或原始 OS 错误码。原始依赖锁、相对依赖路径和冻结上游保持原字节。

## 本次实际验证

| 项目 | 结果 | 范围 |
| --- | --- | --- |
| 隔离候选离线 metadata、库编译、完整测试清单 | 通过，实际退出 0 | 12 条依赖路由确认；完整清单 28 项 |
| Native 纯诊断测试 | 11/11 通过，各为 1 passed / 0 failed，退出 0 | 原 9 项与调用隔离、历史溢出 2 项；其余 17 项未运行 |
| V3 投影器 | 1,987 项内存合成检查通过 | 原 V1/V2 229 项及 V3 1,758 项；AST 检查退出 0，无参数调用退出 2 |
| 底层 observer 8 项、runner 3 项、默认 backend 1 项 | NOT_RUN | 依赖库 cfg(test) 不会随 Native --lib 执行，须另行验证 |
| 公开相对路径副本的完整 Rust 构建 | NOT_RUN | 隔离候选通过不能替代该副本或完整产品验收 |
| 新源码的实际 Windows Start、正常清理与 join | NOT_RUN | 不重放原 Unknown，不继承诊断测试通过 |

独立回读核对了 14 组资格记录、真实退出、84 份日志的哈希和长度，3994 个输入无漂移，原锁的 819 个外部依赖记录保持不变。保存的 Windows x64 纯诊断测试程序 SHA-256 为 `86fece65a0d38c401f0c968e949386037f50e33a9226720da102c8b58c490157`，大小 30,355,968 字节；该程序和私有日志不随源码提交。脱敏结果见 [结构化摘要](native-v3-validation.json)。

公开派生目录沿用已有选择集：原 532 个输入中已发布 510 个，加上许可、NOTICE 与 provenance；此次仅同步相关 Rust 文件，不补全既有未发布的 22 个辅助输入，不据此声明跨平台完整树。

## 仍待完成

原真实 Native Start 保持 `SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY`，已观察到 `BACKEND_START_RETURNED/BackendError`；新诊断未在 VM 中运行，不能推断失败的具体 OS 阶段或不存在副作用。原 owner 的正常回收、完整输出、EOF、ACK、factory release 与 scheduler join 仍待验收，资源债务保持 pending。

SDK26/G04、安全执行层验收、完整 Codex IPC、PTY/stdin/resize、完整网络隔离与其他平台资格仍 OPEN；SDK 未冻结，`release_eligible=false`。达到用户指定的 Agent 会话层和安全执行层验收点后暂停并准备下一测试预览，扩展执行层不作为该暂停点的前提。下一步先完成底层库的独立测试资格，再沿原 owner 的已授权恢复范围继续处理回收，新增实际执行与破坏性恢复须具有相应授权。

此提交包含源码、脱敏验证摘要和进度文档，不包含构建产物、原始 VM/账户日志、数据库、凭据、私有连接材料或 Drive 信息；不更新 main、标签或 Release。
