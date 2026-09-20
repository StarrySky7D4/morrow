# 原工作台业务命令派发与敏感缓冲清理

基线 `ea7dfe0`，分支 `codex/io-safety-refactor`，版本保持 `0.1.9-test.52+56`。**PASS_SCOPED**：本阶段把业务处理接到原State的可信命令通道；主应用常驻服务准入和界面异步命令路径仍待完成。

## 实现与边界

`WorkbenchState`实现原生CommandOwner，在原Storage、Manager、Pool及内容/编辑状态上执行业务。本地和worker共用私有协议的输入预算、帧/版本/摘要校验、业务分支、错误码、部分结果与一次令牌清理及128KiB响应限制。处理器拒绝HttpStart和六项Io调度动作，避免在worker内递归移动拥有者；外围仍在校验后尝试真实回收，并保留StateSlot原读取/写入与显式修复门槛。服务访问错误先于脱敏处理，Busy/RecoveryRequired不会丢失。

命令队列保持8项保留、64KiB输入及既有回复预算。排队输入和未读回复使用Zeroizing；取消、停止、排空、最终退出和有效性检查清理敏感载荷，但不提前归还正在排队/执行票据的容量。索引只弱引用载荷状态，不拥有票据，清理时不会因最后一份票据析构重入状态锁。成功dispatch/read通过移动原Vec转移清理责任，State处理器立即保护输入；调用方成功取出的敏感回复仍须自行擦除。运行中的任意可信handler内部副本不属于队列清理保证。

业务协议错误是一次明确的业务响应；handler已经开始后取消/停止或交付失败仍属于外层Unknown。已提交内容不会回滚或自动重试。普通IO的Ready结果不在命令载荷索引内，继续沿原最终授权检查交付；排空时宿主命令本来就不可再交付，不改变此规则。

本阶段没有新增guest ABI、公开网络管理接口、持久命令记录或Flutter调度协议。现有start_io仍自动drain，因此主应用后台调用仍可能返回Busy；测试专用Running worker只验证原State命令能力，不能当成已完成的常驻产品路径。后续Dart接线须把部分现有64KiB上传块缩小，为64KiB队列输入中的协议封装留空间；现有128KiB私有帧不能一律直接排入队列。

## 验证

| 范围 | 结果 | 本地证据 |
| --- | --- | --- |
| Runtime库及命令/续租/拥有者/IO专项 | 120项通过，0失败；1项子进程harness在顶层标记ignored，由父测试执行 | `build/owner-command-runtime-regression.log` |
| 网络节点/配置/原拥有者/有限运行回归 | 42项通过，0失败，包括实际31秒监听 | `build/owner-command-network-regression.log` |
| Windows宿主release/all-features完整回归 | 180项不同测试通过，0失败；库50、CLI 3、集成127，不重复累计崩溃子进程 | `build/workbench-command-tests.log` |
| Runtime所有目标严格Clippy | 通过，`-D warnings` | `build/owner-command-runtime-clippy.log` |
| 宿主所有目标严格Clippy | 通过，`-D warnings` | `build/workbench-command-clippy.log` |
| 宿主编译检查 | 通过 | `build/workbench-command-check.log` |
| 冻结SDK完整性 | 36固定文件、13原Wasm/包对通过；无guest重建/重打包 | `tool/verify_plugin_sdk_baseline.py` |
| 限定Rust格式与diff检查 | 通过 | rustfmt、git diff --check |

新增真实工作台用例将原State移入Running worker，实际Rust guest创建内容，插入真实HTTP请求后继续编辑和读取；旧修订写入拒绝且没有提交记录。回收后HostBinding、Pool根连接、查询身份和原undo数据一致，创建/编辑各有一份证据。另一个用例取消已Ready的真实创建回执，取得Unknown，实际原库仍只有修订1、一次guest调用和一条操作证据。七项调度动作、坏摘要与尾随字节被拒绝后，合法业务继续可用。

维护失败回归验证原只读语言偏好仍可读取，UiClose、ServiceConfigPage及BeginPreferences保留111；待封存额度不变，未静默修复。Runtime四项新增生命周期测试验证排队取消、未轮询Ready的stop清理、权限失效清理和成功读取的原分配转交，计入120项，不重复累计。

独立本地代理复核未发现明确新增P1/P2。另用已授权有限配额调用一次GLM `max`，返回COMPLETE，建议重点测试取消/回执的清理责任；这只是概念设计辅助意见，不是源码或仓库审查通过证明。实际锁顺序和取消后的无回复路径由本地审查与上述测试核对。无额外源码或密钥外发，无自动重试或预算重置。

## 后续

下一切片接原持久服务配置、发布/认证批准与有限运行准入，保留原State独占归还；然后给主应用提供有界命令提交、查询、读取、取消，以及监听/worker分别停止和真实回收。之后推进长IO等待可暂停、界面响应、Unknown持久核对、完整文件系统与三语言SDK。IO-D2b/IO-E2和全平台资格仍未关闭。本阶段无Flutter构建、安装包、远端推送或Release。
