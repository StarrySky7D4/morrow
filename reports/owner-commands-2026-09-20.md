# 原拥有者宿主命令通道验证

基线 `fce0e2bee1fcdde263380bded0437bd514c65887`，分支 `codex/io-safety-refactor`，版本仍为 `0.1.9-test.52+56`。**PASS_SCOPED**：原生worker增加独立宿主命令容量，并允许原Manager随拥有者整体移动。完整WorkbenchState、主应用调度和内部续租接线仍未完成。

## 实现

- `CommandOwner`由可信拥有者显式实现；输入、回复都是拥有的数据，不跨线程借用工作台对象。`IoWorker::submit_owner_command`与ServiceHost本地转发不增加guest ABI、HTTP管理路由或远端主体权限，没有给裸HostRuntime提供默认处理器。
- 独立8项保留覆盖排队、执行、Ready未读。输入最多64KiB，回复批准最多1MiB，准入保留输入长度加完整回复上限。句柄取消/丢弃不提前释放排队或运行中的票据；取走回复后的内存由调用方管理。handler须限制内部工作和分配，回复长度检查不是内存隔离沙箱。
- 每循环一项宿主命令之后仍处理原IO消息，原作业/Ready配额耗尽不占用宿主通道。两条队列均有执行机会；同步handler/router仍不可抢占，不保证长IO期间的交互延迟。
- 准入、准备后正式启动前、执行后和读取均检查原实例有效性；时钟来自原绑定。启动前取消/关闭不会调用handler；启动后的取消/停止、handler错误或超回复大小均返回Unknown，不能据此认定回滚或自动重试。业务的持久操作身份与核对仍由后续应用命令负责。
- handler执行及准备/维护钩子不持有worker控制锁。异常沿原线程恢复路径返回原owner，执行、断连、维护分别报告；不创建替代Runtime/Store，不重放失败命令。
- `ManagedHostOwner`与 `spawn_managed_owner`在移动前借用内部原Manager，和现有外部Manager入口共用认证辅助逻辑。缺失或错误Manager在采时前拒绝，并返回原owner/instance，解决完整拥有者中的Manager借用与移动冲突。

## 验证证据

| 范围 | 结果 | 日志 |
| --- | --- | --- |
| Runtime作业、托管准入、拥有者、服务及续租回归 | 80 passed / 0 failed | `build/owner-commands-runtime.log`，剔除其中重复的13项初版新测试 |
| 宿主命令专项最终版本 | 16 passed / 0 failed | `build/owner-commands-runtime-final.log` |
| Network plugin-adapter完整回归 | 115 passed / 0 failed，文档编译通过 | `build/owner-commands-network.log` |
| Windows原受保护Storage服务专项 | 4 passed / 0 failed | `build/owner-commands-storage.log` |
| Runtime、Network严格Clippy所有目标 | 通过，`-D warnings` | `build/owner-commands-runtime-clippy.log`、`build/owner-commands-network-clippy.log` |
| Workbench库与测试严格Clippy | 通过，`-D warnings` | `build/owner-commands-storage-clippy.log` |
| SDK冻结原件 | 36固定文件、13原Wasm/包对通过 | `tool/verify_plugin_sdk_baseline.py`输出；未重打包 |

最终215项不同测试通过，0失败；新增17项（Runtime16、Windows组合拥有者1），首轮通过的新13项不重复计数。Runtime使用release/all-features，Network使用release/plugin-adapter，Workbench使用release/all-features的 `--lib storage::service_tests`。格式及差异检查通过。没有Flutter构建或主应用内容编辑验收。

16项专项覆盖原对象身份与归还、8项Ready保留、精确字节边界、排队取消、运行取消/丢弃不提前返还容量、IO Ready饱和、两队列实际可执行时的轮转、停止/排空、handler失败/超额/panic/替换Runtime，以及缺失/错误内部Manager拒绝。时钟专项在prepare内推进到期，确认handler未执行；另在第二项handler阻塞而监测线程无法运行时推进时钟，确认第一项Ready立即拒绝交付。同步使用事件通道/条件变量，不依赖偶然线程顺序。

新增Windows实测组合拥有者持有原Storage、Pool、Pool根会话和Manager：真实HTTP服务运行期间，宿主命令先读取原库的Missing，真实认证HTTP完成后再读到同一持久意图的Observed；随后同一监听仍返回原请求结果。全程原数据库、签名身份和Registry保护租约保持独占。实际监听和worker退出后，原HostBinding、审计身份/公钥、Manager修订及Pool会话保留，原库可重开。该处理器只做限定的执行事实查询，不绕过内容修改管线。

## 审查与未完成项

独立审查未发现可定位的锁序、预算提前释放或启动前/后取消误分类问题。审查要求的到期复验与实际队列轮转已补专项并通过。

审查确认一个后续集成缺口：Manager移入拥有者后，外部调用方无法再提供原 `renew_service_run` 所需的 `&Manager`。当前外部Manager方式的续租保持可用；完整拥有者方式需要内部续租/撤权管理命令，不能复制Manager、另开Registry或从handler重入worker句柄。

本轮没有把应用的undo、内容/UI会话、偏好/捕获/导入暂存迁入新拥有者。后续按[实施方案](../docs/PLUGIN_SERVICE_RUNTIME_PLAN.md)完成内部管理命令、完整WorkbenchState与现有内容管线接入，再处理长IO可暂停及UI响应，最后接启动/状态/停止/修复用户路径。当前组合测试不能替代主应用服务期间内容编辑、完整SDK或全平台资格。

本轮仅本地修改、验证与提交；未推送、发布或关机。使用内置子代理实施、测试和独立审查，未实际调用DeepSeek。
