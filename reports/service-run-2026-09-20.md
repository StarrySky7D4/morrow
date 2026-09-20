# 有限服务运行租约验证

基线 `be087eee7f593a0705dbd53f8fecced3a10b12d6`，隔离分支 `codex/io-safety-refactor`；应用版本保持 `0.1.9-test.52+56`。**PASS_SCOPED**：新增显式版本化的有限运行租约，真实 Windows 本机 HTTP 在同一监听器和原插件实例运行31秒后执行新请求。未完成主应用常驻节点、续租或工作台共存。

## 实现

`IoDeclaration.service_run`（tag 8）与必需feature `service-run-v1`必须同时出现。profile版本1要求正确服务schema及HttpListen/HttpPublish，声明时长1到3,600,000毫秒。旧宿主的必需feature白名单不接受新feature；旧包未添加字段时字节编码保持不变。整个IO声明仍做规范编码校验，未知、重复和非规范嵌套字段拒绝。

可信宿主调用 `Manager::bind_service_run`，验证当前Registry修订、包摘要、原Host/instance和批准能力，再在原IoContext中签发一次运行。声明不是批准，加载持久发布配置也不自动创建运行。原bind_io拒绝新profile，新入口拒绝旧包；丢弃全部句柄、换worker或到期后都不能重新签发同实例租约。无效准入不消耗首次签发机会。

运行期限同时受可信单调时钟和真实Instant约束；回退、到期后在原共享上下文中永久失效，停滞的宿主时钟也不能延长真实期限。每请求仍取原IoBudget的短时限，不读取运行时长；并发容量与累计字节额度不改变，释放、取结果或取消不退款。配置化发布继续受原Store配置、认证与发布批准的更早失效约束。

一小时是有限原型的暂定声明边界，没有按该时长完成长期压力测试，也不是性能上限结论或SDK稳定承诺。当前没有新增累计作业总额计数；Usage.jobs仍是并发计数。

## 当前验证

| 范围 | 结果 | 本地证据 |
| --- | --- | --- |
| Core IO、service codec、包、Registry、依赖与新profile | 69个不同测试通过 | `build/service-run-core.log`、`build/service-run-package-regression-final.log` |
| Runtime绑定、服务、作业、持久请求、内容、拥有者与执行边界 | 110个不同测试通过 | `build/service-run-existing-runtime.log`、`build/service-run-binding.log`、`build/service-run-runtime-regression.log` |
| Network plugin-adapter完整回归 | 111 passed / 0 failed / 0 ignored，文档编译通过 | `build/service-run-network-full.log` |
| Windows原Storage回收专项 | 3 passed / 0 failed / 0 ignored | `build/service-run-storage-regression.log` |
| Core、Runtime、Network所有目标严格Clippy | 均通过，`-D warnings` | `build/service-run-{core,runtime,network}-clippy.log` |
| 冻结SDK基线 | 36固定文件、13原Wasm/包对通过 | `tool/verify_plugin_sdk_baseline.py` |
| 修改文件格式/差异检查 | 通过 | rustfmt、git diff --check |

Core两组日志分别26和51项，其中8项profile重复，去重为69；Runtime为39+8+63=110。合计293个不同目标内测试，没有将单独先跑的4项网络测试重复计入。此轮不是全部工作台/UI回归，也不是全平台验收。

新增20项包括8项profile、8项绑定和4项真实网络测试。网络持续测试精确比较不同call ID、正文和响应帧，在原监听器上31秒后执行第二个请求，不使用持久缓存回复。同一worker在接受正常请求前拒绝超过包短期限的请求；1毫秒请求上限的循环guest返回503，运行授权仍有效。其他用例分别验证Manager禁用、认证先到期、发布批准先到期及配置禁用关闭监听，真实线程结束后仍能回收原owner。

绑定测试验证声明不授予权限、错误Host/摘要/修订/能力/期限不消耗首次签发、原实例撤权、丢弃与到期后禁止重绑、过期后回退不能复活、冻结时钟不能超期，以及跨请求累计字节不退款。包测试另覆盖旧独立消息定义与当前编码相等、五种feature共存以及非规范嵌套字段拒绝。没有把声明可接受一小时当作实际一小时运行证明。

## 检查中发现的问题

扩展包回归首次编译报E0599，提示同路径同版本prost trait来自不同依赖产物。共享目标目录同时承载多个独立Cargo workspace，之前的构建变体留下不一致产物；本次命令已按顺序执行仍触发该问题。原失败保留在 `build/service-run-package-regression.log`。仅清理morrow-core的release构建缓存后，按原测试重新构建并通过，记录在final日志；没有修改这些旧测试或依赖版本规避失败。三个crate严格检查随后通过。

独立设计审查确认：运行租约必须存于原实例共享上下文，显式可信宿主批准不必强制改写持久Publication；后者仍独立限期和撤权。审查也指出并发jobs不是累计作业总额，本轮未把它算作已实现。

## 后续边界

继续[实施方案](../docs/PLUGIN_SERVICE_RUNTIME_PLAN.md)：完整运行预算和显式续租、包含原Storage/Pool/Manager/内容状态的WorkbenchState、有界调度及长IO等待期间的工作台响应，之后接入主应用启动/停止/修复与真实用户路径。长服务占用内容库的Busy问题仍未解决。Unknown证据核对、文件系统、完整三语言IO SDK与跨平台资格继续保留原门槛。

没有修改Dart/UI、升级应用版本、迁移用户库或改写SDK原件。仅本地验证和提交；没有推送、发布或关机。内置子代理参与契约、测试和审查，未实际调用DeepSeek。
