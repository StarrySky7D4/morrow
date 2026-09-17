# 后续编码看板

更新：2026-09-17。基线：test.52 开发线、本轮 Track A 定向修正、IO-C1 核心存储、IO-C2 唯一执行 broker、IO-B2 有界作业与 IO-D1 第一步 HTTP 提交帧（数据库格式 17）；应用版本仍为 `0.1.9-test.52+56`；本轮工作已推送 `track-a/w1-io-contract`（`d086eaf`），未发布。本看板随代码提交维护，是当前任务状态入口；总架构与退出门槛见 [主路线](FUTURE_ROADMAP.md) 和 [执行路线](ROADMAP_UPDATE_2026-09-15.md)。版本号、编译和测试数量不替代产品验收。

状态含义：已验子集＝对应限定实现通过；下一项＝可开始编码；待前置＝须先通过列出的门槛；可并行＝不修改正在整合的核心契约；研究＝不得作为运行后端上线。当前没有标记正在编码的后续任务。

## 已验子集

| 编号 | 范围 | 证据及仍未覆盖的范围 |
| --- | --- | --- |
| IO-A | 当前 IO 声明、Registry 批准、Manager／Pool 实例绑定 | [准入报告](../reports/road-07-io-admission.md)；声明不是资源授权 |
| IO-B1 | 当前协议 Read/Finish/Cancel、raw Runner IO、固定字节 FileBroker、预算／撤权／回收 | [整合验收](../reports/track-a-integration-2026-09-16.md)；15 codec + 12 raw + 15 managed 回归包含在核心396／运行时271项内；不是异步作业或选择器 |
| IO-C0 | Store v16 意图历史及后续意图／审计逻辑预留 | [意图记录](IO_INTENT_RECORDS.md)；没有受保护请求／响应材料或真实效果核对 |
| IO-C1 | Store v17 受保护请求／响应原件、发送前材料容量预留、缺材料分类 | [IO-C1 验收](../reports/io-c1-protected-evidence.md)；无退休／GC、无真实后端效果核对 |
| IO-C2 | 每个 operationId 唯一活跃执行、请求匹配、代次退休与恢复核对 | [IO-C2 验收](../reports/io-c2-unique-execution.md)；后端为合成回调、无真实远端效果与凭据 |
| IO-B2 | 有界 IO 作业 submit／poll／read／cancel、契约路由、停止回收与拒迟到结果 | [IO-B2 验收](../reports/io-b2-bounded-jobs.md)；后端为脚本路由、无异步 guest 挂起、无真实远端效果 |
| IO-D1a | HTTP 提交／结果帧严格编解码（IO-D1 第一步） | [IO-D1a 验收](../reports/io-d1-http-submission-codec.md)；未发起网络、无 origin／方法／凭据授权、无 Unknown 核对 |
| ROAD-04a | test.52 宿主中英文界面 | [i18n 范围](I18N_PREVIEW.md)；插件消息、RTL、业务值迁移未整项通过 |
| SDK-BASE | 冻结 C／C++／Rust 原包兼容 | 本轮36固定文件／13原包对完整性和12项执行回归通过；新 IO 仍实验性 |

## 编码队列

| 顺序／编号 | 状态／优先级 | 模块与前置 | 可评审产物与退出证据 |
| --- | --- | --- | --- |
| 1 / IO-D1 | 进行中（第一步 IO-D1a 已验，其余未完）/ P0 | guest→Manager/Pool→broker→network_node；依赖 IO-B2/C2 与 IO-D1a | 真实第三方 HTTP/HTTPS 出站；origin／方法／凭据授权与响应限制，重定向／取消／远端已执行但响应丢失、Unknown 核对；至少包含有副作用方法，不能以一次 GET 结项 |
| 1 / IO-D2 | 下一项（与 D1 同批）/ P0 | broker＋network_node API 节点；依赖 IO-B2/C2 | guest 发布授权路由；远端主体与插件权限交集，认证／路由冲突／限流／撤权／节点停止及响应丢失；实际请求证明服务端能力 |
| 1 / IO-D3 | 下一项（与 D1 同批）/ P0 | 平台文件适配＋broker；依赖 IO-B2/C2 | 系统选择、目录枚举、创建／替换／删除，资源越界／替换冲突／撤权／崩溃结果核对；固定读取保留兼容测试 |
| 2 / IO-E1 | 待 D1/D2/D3 契约验收 / P1 | sdk/rust、sdk/c、sdk/cpp | 三语言类型化 IO、同一正负向量与独立仓库插件；旧原包原样执行；新扩展单独形成兼容候选 |
| 2 / IO-E2 | 待 D1/D2/D3 / P1 | workbench_host＋Flutter 管理界面 | 文件／网络／监听／发布分别显示授权、任务及恢复状态；独立插件真实调用和提供服务，用户资料无隐式迁移 |
| 2 / ROAD-08-IO | 待实际后端 / P0 | 录制证据与独立验证器 | A→B→IO→内容提交→封存→删除安装来源→隔离重放；真实故障、合法退休与缺材料分类；重放禁止实际外发 |
| 3 / IO-E3 | 待基础双向 IO / P1 | NET-2–8／NODE-4–7 按各自依赖 | OAuth／多账号、上传下载、分页限流、流/SSE/WebSocket、webhook、持久服务与 TLS 运维；每个 profile 单独验收 |

## 可并行及研究

| 编号 | 状态 | 下一步与边界 |
| --- | --- | --- |
| ROAD-01b | 可并行 / P0 | 补主应用／测试／平台固定源码构建回执；不把本轮源码整合写入旧预览归档 |
| ROAD-02/04b | 可并行设计 / P0 | LiteralText／MessageRef、命名空间、任务语言上下文、坏包和RTL向量；固定接口后才接插件 UI；不新增 TS/JS 或动态 Dart 插件 |
| ROAD-12/AND-01 | 可并行探针 / P0 | Android Rust/Wasm 引擎提取与执行域能力；编译、设备、隔离分别记证据，不能继承 Windows 通过状态 |
| ROAD-05/06 | 可并行固定接口验证 / P0 | 真实跨进程 A/B 故障、证据容量与退休；v17 存储变更已随 IO-C1 合入，后续 schema 变更仍须与其串行整合 |
| QUIC-RESEARCH | 研究 / P1 | 先统一迁移开关、IPv4/IPv6共享端口计数及层次依赖；再提交真实 socket/TLS/传输原型，下载的计数器／布尔模型不接产品 |

## 每次合入检查

记录基线和实际范围，复核 schema／数据库／授权唯一权威；跑改动相关回归、冻结原包完整性及执行兼容。影响共享模块时检查 wasm32 编译，原生 IO 另报平台资格。完成一项只移动该子项状态，ROAD-07、完整 SDK 和 M0–M7 不因局部通过整体勾选。测试版继续沿 test.x 推进，0.2.0 仅在声明范围达到既定门槛后评审。
