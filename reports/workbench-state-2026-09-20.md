# 完整WorkbenchState提取与原对象回收

基线 `cd287d6`，分支 `codex/io-safety-refactor`；版本仍为 `0.1.9-test.52+56`。**PASS_SCOPED**：本阶段将完整业务状态移到原执行者，业务命令排队和常驻服务界面仍待接入。

## 实现

- WorkbenchState直接持有原Storage、Manager、Pool、默认/外部插件会话、UI代次、查询身份、undo、附件暂存、上传缓冲与capture范围。State内部没有可空StorageSlot、worker句柄或第二份Runtime；HostOwner/ManagedHostOwner始终返回原对象。
- 外围Workbench只管理StateSlot与HTTP提交去重/回执关联。StateSlot在实际worker join之后归还完整State；准入失败先还原原owner，再清理原实例。清理失败保留确切实例，封存失败仍允许原内容只读，显式repair仅清理/封存，不重发HTTP。
- 原内容、查询/归档/证据、设置、插件/凭据/端点/服务管理实现迁到State。外围方法显式取得原状态，未引入复制、默认Deref、旁路写入或跨线程借用；现有真实guest及逐对象授权/证据提交路径不变。
- 短IO退出只调用原Storage的维护；最终应用finish才关闭Pool、编辑器和capture范围。Drop不等待阻塞router，实际线程仍持有原Storage和Manager租约直到退出。
- 由于Manager、编辑器和缓冲区也已移入worker，后台目录/状态/关闭/上传入口现在必须明确拒绝Busy；Rust plugin_status/ui_close/close_capture_scope改为Result，私有协议继续返回现有110等错误码，避免编造修订或丢弃关闭操作。没有改动消息布局或冻结SDK。常驻期间可编辑尚未实现，不能以此过渡行为作为最终交互验收。

## 验证范围

所有Rust目标编译检查通过（`build/workbench-state-check-all-3.log`）。首次与第二次检查暴露旧字段/返回值调用，修正为显式State借用和Result处理；未增加panicking Deref来消除编译错误。

| 验证 | 结果 | 证据 |
| --- | --- | --- |
| Windows宿主release/all-features完整回归 | 177项不同测试通过，0失败；含库47项、CLI 3项及集成127项；不重复累计崩溃子进程的同名测试 | `build/workbench-state-tests.log` |
| 宿主所有目标严格Clippy | 通过，`-D warnings` | `build/workbench-state-clippy-all.log` |
| 冻结SDK完整性 | 36固定文件、13原Wasm/包对通过；没有重建或重打包 | `tool/verify_plugin_sdk_baseline.py`输出 |
| 限定Rust格式及diff检查 | 通过 | rustfmt、git diff --check |

完整回归包括1100次连续内容创建跨封存阈值、崩溃后原身份恢复、实际guest捕获/偏好/查询、归档重放、插件失效和服务管理协议。初次lib Clippy发现新转发方法漏掉原8参数接口的限定allow，补齐原接口的标注后最终所有目标通过；没有忽略整模块警告。未构建Flutter界面、发布安装包或新增平台资格证据。

宿主47项库测试已通过，包括新的真实工作台guest、UI与capture移交用例：原编辑器从修订2/序号1，在实际HTTP任务回收后沿相同generation继续到修订3/序号2，解码得到新预览；原粘贴ID可幂等重试、冲突内容拒绝，新粘贴继续接受；随后真实apply/create与读取成功。后台UiClose/CloseCaptureScope返回110，回收后编辑会话仍在，未假装关闭成功。

另一个移交测试保留原HostBinding、Pool根ConnectionBinding、Registry修订、查询身份、起始时钟、counter、undo、附件暂存和两个上传令牌；Ready期间没有从外围读取Manager/Pool，回收后完成原上传。应用Drop但实际router仍阻塞时，数据库/身份/库Registry及插件Registry租约仍拒绝旁路打开。维护失败及Observed不重发沿原测试验证。

本阶段实际调用了一次DeepSeek `max`对相关约20KiB源片段做辅助审查；输出达到1024-token上限而被标记PARTIAL/response_truncated，不能列为通过的审查。返回的三项意见经主代理/独立代理核对未确认缺陷；未自动重试、未扩大输出限制，代码正确性仍依赖本地审查与实际测试。两个供应商的最小连通测试另属插件配置验证，不计入项目测试通过数。

## 后续验收

下一项在原State上接有界业务与管理命令，并共用本地/worker协议执行入口；现有短IO自动drain，不应直接宣称可承载长期并行编辑。之后实现常驻节点的实际准入、可暂停长IO、主应用启动/停止/修复，保留原期限、累计预算、撤权、Unknown和真实退出规则。文件系统、三语言SDK、全平台资格及0.2.0门槛未关闭。
