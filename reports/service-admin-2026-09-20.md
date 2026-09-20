# API 节点管理底座验证

基线 `c5f4b3a212f66f220d7070687d69624d35b303b5`，隔离分支 `codex/io-safety-refactor`；应用版本仍为 `0.1.9-test.52+56`。**PASS_SCOPED**：原 Store 管理分页和 Workbench Rust 服务配置/认证/发布批准接口通过。没有新增Flutter页面、私有管理消息或工作台监听器，不称作API节点产品完成。

## 已实现

- 原Store稳定分页，配置每页1条、批准最多2条；整表有界校验和原容器摘要使页外变动/损坏不能被忽略。没有新增数据库或持久schema。
- Workbench创建/替换/停用服务配置，生成稳定namespace和预留发布引用；校验当前包摘要、Registry修订、Listen/Publish批准、handler及服务协议。
- 明确创建或轮换时生成一次性高熵bearer，自有随机和文本缓冲区使用Zeroizing；持久化只保存验证摘要，返回列表不含token或摘要。停用无需原包仍存在。
- 发布批准绑定完整配置摘要/修订/引用，实际期限不超过最早认证到期；当前七种服务方法、路径与TLS政策均校验。保存和重开不启用插件、不监听。

## 验证结果

| 检查 | 结果 | 本地证据 |
| --- | --- | --- |
| 新分页测试 | 7 passed / 0 failed | `build/service-management-listing.log` |
| 新管理接口测试 | 8 passed / 0 failed | `build/service-admin-host-tests-final.log` |
| 宿主完整Release/all-features | 165 passed / 0 failed / 0 ignored，26个顶层目标 | `build/service-admin-host-full.log` |
| 原有配置/服务批准/出站批准回归 | 9 + 13 + 18 = 40 passed | `build/service-admin-core-regression.log` |
| 宿主全目标严格Clippy、核心新测试严格Clippy | PASS | `build/service-admin-clippy-final.log`、`build/service-admin-core-clippy.log` |
| core默认wasm32 Release库 | 构建成功，7项未使用代码警告 | `build/service-admin-core-wasm.log` |
| 冻结SDK原件 | 36固定文件/13原Wasm与包对通过 | baseline verifier；未重打包 |

完整宿主数量按顶层Running/Doc-tests区块统计，不重复计算内部审计子进程。全套已含新8项；最后一次改动仅将认证随机材料直接填入Zeroizing缓冲区，随后重跑这8项和全目标Clippy通过。未因统计方便重复累计。

分页覆盖空表/重开、无授权pin或撤权、历史记录、精确游标、页外新增/更新、相同修订但原容器变化、首页外7类配置/8类批准损坏，以及128/512条上限和额外一行拒绝。原生宿主管理使用临时真实受保护资料库与合法Wasm包，但管理测试故意不执行guest；它证明批准/配置操作，不是入站网络执行证明。

测试核对持久化验证摘要与一次返回token一致、轮换改变token并递增修订、旧修订/主体变更拒绝、配置升级保留namespace、失效包/handler/schema拒绝、过期认证拒绝、禁用与重启持久化、单日和多主体期限取实际最小值。端口被测试预先占用时保存成功，关闭重开后仍可再次绑定该端口，证明管理操作没有自动监听。

## 审查与失败记录

独立复审发现并修复：一天认证的剩余期限略短于新申请一天发布而永远被拒绝；缺服务schema的合法旧IO包能保存但不能运行；TRACE等方法能批准但不能编码服务请求。现分别采用请求上限/真实到期返回、服务schema检查和七方法前置校验。新增测试覆盖这些场景。

首轮check失败于测试Store::open的PathBuf借用，修复后正式测试通过，保留 `build/service-admin-check.log`。一次回归命令指定了不存在的目标缓存目录，路径解析失败后Cargo使用已存在的core默认缓存，实际40项测试正常完成；不将该路径错误当成测试失败或通过证据。Wasm构建警告未作豁免，也不把构建成功等同Web运行验证。

## 后续真实工作

详见[管理接线合同](../docs/PLUGIN_SERVICE_MANAGEMENT.md)：先接私有消息/一次令牌回执/Dart模型与双语表单；监听前必须区分常驻服务租约和单请求30秒预算，再泛型化ServiceHost以保留完整原Storage，统一停止、监听join、worker回收和恢复。常驻期间内容Busy仍需统一调度设计，禁止通过旁路数据库或循环重新绑定掩盖此限制。

完整入站用户路径、Unknown持久证据核对、文件系统、三语言IO SDK和跨平台资格均未完成。本轮仅本地改动和验证，未推送、发布或关机。
