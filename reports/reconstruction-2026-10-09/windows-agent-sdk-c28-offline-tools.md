# Windows Agent SDK C28：离线工具与恢复准备

更新：2026-10-09。承接 [前一阶段](windows-agent-sdk-c28-qualification-preparation.md)。本次保存六份通用工具源码、限定验证摘要及当前看板；应用版本与运行逻辑不变。私有控制入口和原始日志保留在本地。

| 项目 | 已完成的限定结果 | 尚未证明 |
| --- | --- | --- |
| 子进程捕获 | 10 项合成检查退出 0；真实退出先于日志后验保存，超时保留同一子进程，日志故障继续排空 | 原测试 owner 正常退出及资源债务清除 |
| 验证字节加载 | 另 1 项检查退出 0；路径变化不改变已验证缓冲区的执行 | 全系统或内存依赖闭包 |
| 恢复准备 | 禁用候选完成有限独审；实际 PowerShell 5.1 解析四份源码无错误，七个命令参数组合核对通过 | 脚本正文、虚拟机恢复、沙箱或 H011 执行 |
| registry 缓存 | 12 项合成检查退出 0；固定 Cargo 的 toy `--offline --locked metadata` 实际退出 0 | 真实依赖获取、安装、编译、build script 与完整传递闭包 |
| Git PACK | 31 项内存合成检查退出 0；对象、delta、树和路径边界检查 | 实际仓库获取、SHA1DC、完整历史或 Cargo Git 缓存采用 |
| Git HTTP 后继 | 36 项 mock 检查退出 0；两项源码持有/加载问题已实现修正，最终独审在本次快照中待完成 | 真实 DNS、TLS、网络、Git 获取与物理网络资源边界 |

原字节 [工具源码](../../companions/morrow-codex/qualification/offline-preparation/README.md)仅属于 experimental qualification preparation。PACK 和 registry 安装候选的 CLI 仍返回 78；捕获库没有该 CLI 门禁，测试 fixture 与 unittest 脚本各有自己的测试入口。不能把整个目录宣传为已启用或正式 SDK。Git HTTP 和 Git 缓存采用源码仍在本地准备，不纳入公开的六文件子集。

发布前在新目录对公开副本实际执行了相同的 31 项 PACK 和 12 项 registry 方法，均退出 0，来源字节前后一致。这是重定位复验，不能和原结果相加。捕获 10 项及加载 1 项保持原执行身份，没有在本次发布中重跑。结构化摘要、原始收据哈希及重定位收据哈希见 [验证摘要](offline-tools-validation.json)。保留前驱候选和失败记录；Git HTTP 旧 32 项也不能与后继 36 项相加。

## 当前剩余门槛

真实公共依赖获取和真实缓存采用仍未验收；Git 缓存布局验证继续研发。底层 observer 8 项、runner 3 项、默认 backend 1 项仍 NOT_RUN。H011 仅隔离构建通过、程序未执行；公开副本完整构建仍 NOT_RUN。三个嵌入 Wasm 的当前 include 路径缺少输入，其中两个原始 fixture 已存在于其他公开目录，第三个有公开 producer；路径/可复现构建接线尚未完成，不能称全部原件都不存在。

历史 Native Start 保持 `SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY / BackendError`；正常退出、EOF、ACK、cleanup/join 和资源债务保持待确认，不能由恢复或新的合成测试解除。Agent 会话层、安全执行层、SDK26/G04 仍 OPEN，SDK 未冻结，`release_eligible=false`。完成前两层实际验收后暂停准备下一测试预览；扩展执行层不是该暂停点前提。
