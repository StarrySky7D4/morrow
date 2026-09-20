# 原生应用服务准入与原状态监督回收

基线 `ad346fd`，分支 `codex/io-safety-refactor`，版本保持 `0.1.9-test.52+56`。**PASS_SCOPED**：本阶段接通原生Workbench的有限服务运行、原状态业务命令与退出恢复；私有调度协议及Flutter服务界面尚未接入。

## 实现

`Workbench::start_service(ServiceStart)`使用原持久配置和发布/认证记录，先固定原Store授权锁，再核对配置摘要与修订、发布修订、包身份及Registry修订，并只按该发布记录绑定地址/方法/路径。审查发现的“检查后再pin”竞态已修正；绑定不会悄然采用校验期间替换的新配置。当前入口限定已批准的单个loopback HTTP服务、明确有限期限与累计任务/字节预算；未配置的出站调用拒绝，不自动续租。

同一个StateSlot用执行者变体持有短IO或服务，原Storage、Manager、Pool、内容/UI及暂存继续整体移动。新的服务监督线程持有Tokio runtime、ServiceHost及监听监督，只有监听和worker都结束、监督线程实际join后，StateSlot才归还原State并报告Exited。短IO自动drain保持原行为。准入准备失败清理原新实例；worker构造失败恢复原owner；适配器或监督线程构造失败保留原worker并停止回收；端口绑定失败同样要求真实回收和显式确认。

`service_status`分别提供启动绑定、监听、监督与worker执行/断连/封存结果，保留实际地址和原提交身份。512项会话提交去重限制不随确认清空；丢启动回执应查询同一任务，不重发。服务任务期间不会把上一HTTP请求的提交身份附到服务上。`submit_service_command`只允许已运行服务接收原8槽可信命令，保持原64KiB输入、128KiB工作台回复和敏感缓冲清理责任。它返回原生句柄，不增加HTTP管理路由或guest能力。

取消、repair和ack复用原任务身份与StateSlot规则。撤销监听、请求worker停止和监听退出彼此独立；封存失败只允许显式修复原库，不能据此重发HTTP或内容命令。Drop只发送停止请求，实际监督线程继续持有runtime及owner直到完成清理；丢失owner时禁止旁路重开。ManagedNode新增非阻塞request_stop、is_finished以及借用式join；取消join future保留原监督句柄，终态只消费一次。

## 验证

| 范围 | 结果 | 本地证据 |
| --- | --- | --- |
| 新应用服务真实用例 | 7项通过，包括原Rust工作台/UI、真实HTTP、准入拒绝、绑定冲突、真实到期、撤销、封存修复与Drop重开 | `build/service-admission-host-regression.log` 的库测试 |
| Windows宿主release/all-features完整回归 | 187项不同测试通过，0失败；库57、CLI 3、集成127，不重复累计崩溃子进程 | `build/service-admission-host-regression.log` |
| ManagedNode库与原拥有者专项 | 9项通过，0失败，包括取消join后继续等待、终态错误/panic一次性消费及worker独立回收 | `build/service-supervisor-network-tests.log` |
| 既有配置、监听、TLS及有限运行回归 | 35项通过，0失败，包括实际31秒监听 | `build/service-supervisor-network-regression.log` |
| 宿主库严格Clippy | 通过，`-D warnings` | `build/service-admission-clippy-lib.log` |
| 宿主所有目标严格Clippy | 通过，`-D warnings` | `build/service-admission-clippy-all.log` |
| 网络层所有目标严格Clippy | 通过，`-D warnings` | `build/service-supervisor-network-clippy.log` |
| 冻结SDK完整性 | 36固定文件、13原Wasm/包对通过，无重建/重打包 | `tool/verify_plugin_sdk_baseline.py` |
| 限定Rust格式与diff检查 | 通过 | rustfmt、git diff --check |

原生应用用例通过公开start_service/status/command接口启动持久配置服务，以两个不同durable请求身份执行实际WAT入站响应，逐字节核对认证主体、内容scope摘要和请求。两请求之间由实际Rust工作台guest创建、编辑、读取卡片，并沿原UI generation继续事件与预览；退出后保留原HostBinding和Pool根连接。入站fixture不代表完整Rust服务插件或性能验收。

旧配置/发布/Registry修订或错误摘要不移动owner，不占用任务；合法重试仍可使用未开始的提交身份。端口冲突回收后必须确认，已消费提交身份不能再次启动，新的显式提交成功；旧/外来任务不能取消、修复、确认、查询或发送命令。真实1.5秒到期自动关闭监听并归还原状态，证明未使用短IO立即排空冒充常驻服务。

原服务命令禁用配置后，仅接受成功业务回执或已开始的Unknown；最终配置只到修订2、监听退出，查询状态不重放写入。真实HTTP后的封存故障保留原Observed容器与卡片编码，repair只清理封存且历史错误不改写。丢弃运行中Workbench后等待原审计库和插件Registry真实可重开，验证端口关闭、原卡片/配置/Observed历史保留且pending为0；不声称Drop瞬间已经释放所有资源。

本阶段共231项不同测试通过；首次4项服务专项已包含在宿主187项内，不重复累计。线程创建失败和应用监督panic的恢复分支经过代码审查，未做系统线程创建失败注入；ManagedNode终态panic另有专项。上述证据不是吞吐、长期稳定性、应用TLS或跨平台资格证明。

## 剩余范围

下一项是私有服务调度和有界命令句柄表，再接Flutter的启动、状态、停止与业务异步交付；需要缩小部分64KiB上传块，为命令封装留空间，并让停止/状态请求能在等待命令期间插入。之后继续长IO可暂停、应用TLS和出站资源接线、Unknown持久核对、文件系统、三语言SDK和全平台资格。当前没有Flutter构建、Windows安装包、远端推送或Release，不宣称整个插件系统或IO-D2b/IO-E2完成。
