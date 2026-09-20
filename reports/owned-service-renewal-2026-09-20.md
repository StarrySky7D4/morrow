# 原拥有者内部Manager续租验证

基线 `d073f6153025502a1653ff0560727a4ee54e0864`，分支 `codex/io-safety-refactor`；版本仍为 `0.1.9-test.52+56`。本阶段增加原生可信宿主的类型化管理命令，完整WorkbenchState与主应用常驻服务路径仍未完成。

## 实现与边界

- `IoWorker<ManagedHostOwner>::queue_service_run_renewal`在原执行线程借用owner中的原Manager；ServiceHost提供本地转发。无须实现字节命令handler，不增加guest ABI、HTTP管理路由、序列化契约或持久操作记录。
- 类型化请求与普通宿主命令共用8项保留槽；排队、运行及Ready未取走均占槽。请求保留原ServiceGrant的共享引用及标量，准入计入请求/回复静态大小；不复制Registry、Manager、实例或Store。普通字节命令既有输入和回复上限保持。
- 内外两种续租入口共用Control校验路径：原Manager/Control/连接/包、当前批准与Registry修订、原grant及运行修订。外来grant在入队容量与原时钟采样前拒绝；执行阶段复核原owner和当前授权。
- 续租只允许原声明和首次签发总期限内扩大截止/累计预算，保留已计费任务与字节；无预算profile不能升级。原每请求截止、认证/发布/配置/监听授权保持独立。
- 取消与CAS在worker状态锁下串行；先赢得该锁的取消阻止续租提交。开始后的取消/停止只会让回执成为Unknown，即使CAS已发生也不回滚。`read`外层错误表示调度/交付失败或不确定，内层Result表示仍有效时的明确批准或拒绝；成功入队不是续租成功。
- 异常仍按原worker退出路径归还完整owner；不自动重试，不通过新实例或新的期限恢复失效授权。同步handler/router不可抢占，保留队列不保证长IO期间的响应延迟。

## 验证

Runtime相关9组共107项测试通过（含新增11项内部续租专项），日志 `build/owned-renewal-runtime.log`。专项覆盖原Manager移动/原对象回收、旧授权副本与累计账本、无字节handler、语义拒绝、外来grant无调用方采时、排队取消/过期/撤权、两方向共享8槽、缺失Manager、Ready取消/停止后的Unknown以及同修订竞争。

Windows原受保护Storage服务最终5项通过，日志 `build/owned-renewal-storage-verified.log`。新测试在同一真实HTTP监听器上提交不同Idempotency-Key及call ID，WAT完整比对两个请求帧，保证第二次实际执行而非复用持久缓存。第二次处于旧期限之后且新期限以内；两条原Store记录均为Observed，累计任务从1增至2、字节继续累加。原HostBinding、审计身份、数据库/Registry保护租约、Manager修订及Pool根均保留，并完成真实退出、封存、重开完整性检查。该边界使用可控单调钟，不代表真实长时间压力验收。

初次Windows运行4通过/1失败（`build/owned-renewal-storage.log`）；第二次仍4通过/1失败（`build/owned-renewal-storage-final.log`）。测试误将socket关闭视为原grant撤销：初版要求入队失败，第二版等待结果仍得到成功。实现检查确认监听关闭只归还监听资源，保留的授权副本需要显式撤销。最终测试按既有关闭顺序先 `listener.revoke()`、再关闭监听并确认旧grant续租拒绝与账本不变。没有修改运行时授权语义来迁就测试，也没有把这两次失败计入最终通过总数。

Network plugin-adapter完整回归115项通过（含同一监听器真实31秒后的第二次执行），文档编译通过，日志 `build/owned-renewal-network.log`。最终共227项不同测试通过、0失败；此前失败已按上文单独保留。Runtime和Network所有目标、Workbench库与测试的严格Clippy均通过 `-D warnings`，日志 `build/owned-renewal-runtime-clippy.log`、`build/owned-renewal-network-clippy.log`、`build/owned-renewal-storage-clippy.log`。

冻结SDK核验通过：36固定文件、13原Wasm/包对；只验证完整性，没有重建或重打包。限定Rust格式与diff检查通过。独立子代理只读审查未报告新的明确P1/P2；实际Cargo验证由主代理串行执行。结论为 **PASS_SCOPED**，本轮未构建Flutter应用或验证Web运行能力。

## 后续

下一项抽出完整WorkbenchState并接原内容与批准/撤权管理命令，随后处理长IO可暂停和主应用启动/停止/恢复。当前组合owner测试不包括全部undo、附件暂存、capture与UI会话，主应用内容Busy尚未消除。Unknown证据核对、文件系统后端、三语言IO SDK及各平台资格保持原门槛；本阶段不作SDK稳定或全平台可用承诺。
