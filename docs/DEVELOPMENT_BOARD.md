# 后续编码看板

更新：2026-09-19。基线：test.52 开发线 457e023 与隔离分支 `codex/io-safety-refactor` 的 Track A 修补／重构；应用版本仍为 `0.1.9-test.52+56`。本看板随代码提交维护，是当前任务状态入口；总架构与退出门槛见 [主路线](FUTURE_ROADMAP.md) 和 [执行路线](ROADMAP_UPDATE_2026-09-15.md)。版本号、编译和测试数量不替代产品验收。

状态含义：已验子集＝对应限定实现通过；下一项＝可开始编码；待前置＝须先通过列出的门槛；可并行＝不修改正在整合的核心契约；研究＝不得作为运行后端上线。本轮隔离修正与验证证据见 [修正报告](../reports/io-safety-refactor-2026-09-19.md)；隔离分支通过不代表已推送、发布或主应用端到端验收。

## 已验子集

| 编号 | 范围 | 证据及仍未覆盖的范围 |
| --- | --- | --- |
| IO-A | 当前 IO 声明、Registry 批准、Manager／Pool 实例绑定 | [准入报告](../reports/road-07-io-admission.md)；声明不是资源授权 |
| IO-B1 | 当前协议 Read/Finish/Cancel、raw Runner IO、固定字节 FileBroker、预算／撤权／回收 | [整合验收](../reports/track-a-integration-2026-09-16.md)；15 codec + 12 raw + 15 managed 回归包含在核心396／运行时271项内；不是异步作业或选择器 |
| IO-C0 | Store v16 意图历史及后续意图／审计逻辑预留 | [意图记录](IO_INTENT_RECORDS.md)；没有受保护请求／响应材料或真实效果核对 |
| ROAD-04a | test.52 宿主中英文界面 | [i18n 范围](I18N_PREVIEW.md)；插件消息、RTL、业务值迁移未整项通过 |
| SDK-BASE | 冻结 C／C++／Rust 原包兼容 | 冻结基线为36固定文件／13原包对；本轮执行结果见修正报告；新 IO 仍实验性 |

## 编码队列

| 顺序／编号 | 状态／优先级 | 模块与前置 | 可评审产物与退出证据 |
| --- | --- | --- | --- |
| 1 / IO-C1 | 已验存储子集 / P0 | core 证据存储；依赖 IO-C0 | Store v17 受保护原件、原容器身份和共享容量预留；读取／幂等重试有界校验；满额、真实满盘、撤权、崩溃重开、材料缺失均可解释，旧签名原件不改写 |
| 2 / IO-C2 | 已验 broker 子集 / P0 | runtime broker＋core，沿用 IoBinding | 同 operationId 唯一活跃执行、请求匹配、原代次退休及恢复核对；重复提交、并发绑定、发送边界中断不导致重发，历史记录不恢复授权 |
| 3 / IO-B2 | 已验调度＋托管准入＋持久子调用 / P0 | runtime 作业调度＋独立契约路由 | 有界 submit/poll/read/cancel、Ready 最终交付撤权、声明预算、温和排空已验；已接真实 Manager/IoBinding 的撤权与原实例共享 job/bytes；[托管证据](../reports/managed-io-jobs-2026-09-19.md)。同一作业子调用已贯通 Prepared／发送边界／Observed，无重复计费；[接线证据](../reports/brokered-io-jobs-2026-09-19.md)。HTTP端点与原实例资源批准已接真实传输；后续连接主应用、持久批准与其它资源 |
| 4 / IO-D1 | 已验本机 HTTP/TLS 出站子集 / P0 | guest→Manager/IoBinding→broker→network_node | [托管 HTTP](PLUGIN_MANAGED_HTTP.md)：原实例端点批准、精确 origin/方法/凭据引用、真实 POST/状态/重复头/原件、发送后断线不重发与 Ready 撤权已验；仍待持久批准/主应用、真实提供者核对、路径范围和更多平台 |
| 4 / IO-D2 | 已验本机受管服务子集 / P0 | broker＋network_node 受管服务 | 独立 service 帧／声明 tag 7、真实 Manager 的发布与监听批准、同 worker 路由及 Principal service scopes 已接线；这是宿主显式发布，非 guest 动态注册。本机 HTTP/TLS 的认证／冲突／额度／撤权／节点关闭已验；入站持久幂等和内容权限交集仍待完成；持久批准、UI 与新三语言 SDK 未完成，见 [实现合同](PLUGIN_MANAGED_SERVICE.md) |
| 4 / IO-D3 | 待 IO-B2/C2 / P0 | 平台文件适配＋broker | 系统选择、目录枚举、创建／替换／删除，资源越界／替换冲突／撤权／崩溃结果核对；固定读取保留兼容测试 |
| 5 / IO-E1 | 待 B2/D1/D2/D3 契约验收 / P1 | sdk/rust、sdk/c、sdk/cpp | 三语言类型化 IO、同一正负向量与独立仓库插件；旧原包原样执行；新扩展单独形成兼容候选 |
| 5 / IO-E2 | 待 B2/D1/D2/D3 / P1 | workbench_host＋Flutter 管理界面 | 文件／网络／监听／发布分别显示授权、任务及恢复状态；独立插件真实调用和提供服务，用户资料无隐式迁移 |
| 5 / ROAD-08-IO | 待 C1/C2 与实际后端 / P0 | 录制证据与独立验证器 | A→B→IO→内容提交→封存→删除安装来源→隔离重放；真实故障、合法退休与缺材料分类；重放禁止实际外发 |
| 6 / IO-E3 | 待基础双向 IO / P1 | NET-2–8／NODE-4–7 按各自依赖 | OAuth／多账号、上传下载、分页限流、流/SSE/WebSocket、webhook、持久服务与 TLS 运维；每个 profile 单独验收 |

## 可并行及研究

| 编号 | 状态 | 下一步与边界 |
| --- | --- | --- |
| ROAD-01b | 可并行 / P0 | 补主应用／测试／平台固定源码构建回执；不把本轮源码整合写入旧预览归档 |
| ROAD-02/04b | 可并行设计 / P0 | LiteralText／MessageRef、命名空间、任务语言上下文、坏包和RTL向量；固定接口后才接插件 UI；不新增 TS/JS 或动态 Dart 插件 |
| ROAD-12/AND-01 | 可并行探针 / P0 | Android Rust/Wasm 引擎提取与执行域能力；编译、设备、隔离分别记证据，不能继承 Windows 通过状态 |
| ROAD-05/06 | 可并行固定接口验证 / P0 | 真实跨进程 A/B 故障、证据容量与退休；存储／schema 变更需与 IO-C1 串行整合 |
| QUIC-RESEARCH | 研究 / P1 | 先统一迁移开关、IPv4/IPv6共享端口计数及层次依赖；再提交真实 socket/TLS/传输原型，下载的计数器／布尔模型不接产品 |

## 每次合入检查

记录基线和实际范围，复核 schema／数据库／授权唯一权威；跑改动相关回归、冻结原包完整性及执行兼容。影响共享模块时检查 wasm32 编译，原生 IO 另报平台资格。完成一项只移动该子项状态，ROAD-07、完整 SDK 和 M0–M7 不因局部通过整体勾选。测试版继续沿 test.x 推进，0.2.0 仅在声明范围达到既定门槛后评审。
