# SDK 扩展合同与未来能力边界（Windows 013）

本文件区分**已实现的兼容边界**与**后续 profile 必须完成的共享合同**。它不宣布整个 SDK 冻结，不把设计字段或 fixture 当作流式、Cloud、Codex 的实现。具体插件及依赖获取继续暂停。

## 已实现的共用入口

可信宿主 `morrow-workbench-host --sdk-capabilities` 在打开 owner、资料库、worker 或监听之前返回 JSON descriptor schema1。开发工具 `morrow_plugin.py profiles --host EXE` 要求显式可信程序路径，绑定 host 与原响应 SHA，拒绝重复字段／错误类型／坏摘要，限制两路输出和等待。它报告当前编译的版本、规范合同摘要、字节上限、默认预算、实际 Workbench 路线限制及实验／未支持项；**不授予权限，不替代包、模块或实际路线准入**。

当前 base profile 为 `morrow.guest-task.v3`：ABI2、runtime7、task3、UI1、dependency-call1。实际版本协商是「发现明确 profile → 选择调用者已理解的版本／摘要 → 正式包与 Runner 再严格准入」。目前只有当前固定版本；没有接受任意旧 schema 的多版本回退，也没有 guest 内动态 discovery import。

descriptor 中 runtime 的 content／transform 模式和依赖 feature，不能推导 Workbench 的外部入口支持它们。实际 external_transform／external_ui 不支持外部 content task、依赖调用或 required dependencies；这些约束显式为 false。依赖功能须由受信任的 managed dependency router 绑定，不能将回调指针当作插件权限。

2026-10-02 当前扩展补充：`morrow.channel.v1` 是已实现的三语言实验 profile，独立 `channel.capnp`／import 不改旧 ABI。compiled discovery 仍为 `authority=none`、`status=experimental`、`workbench_routes=[]`；content/dependency/required-dependency/channel-binding 约束全部明确为 false。有限 caller-data 私有 Workbench 接线不是正式公共 binding 资格，网络 SSE/WS、授权 Cloud change source 和异步依赖仍不支持。旧 S013 证据只保留其历史范围。

## 兼容扩展规则

| 规则 | 接口级依据和本轮证据 |
| --- | --- |
| 旧 ABI／合同保留，新增协议独立命名 | `Runner::new_task`、`new_dependency_task`、`new_io_task`、`new_mutation_task` 是已有独立模式。旧 task3/runtime7 文件不追加字段或注释；规范化 SHA 仍是准入条件。新 schema／import 不借旧名字改变含义。 |
| 新必需语义必须显式声明 | `Manifest.required_features` 白名单与独立摘要拒绝未知必需项；未来 stream/event/cloud/exec fixture 在显式声明未知 required feature 时，真实 `Package::build` 返回 UnsupportedVersion；省略声明不提供该能力，本轮没有证明自动检测漏报。未来新增受限字段／import／route 必须强制匹配 feature 与 schema，不能将必要语义设计为可选降级。新增合法 feature 需新 host 明确注册、schema 校验与路线绑定。 |
| 新 import 不进入旧默认模式 | 本轮真实 Runner fixture 拒绝未知 stream/events/exec imports。已实现的 dependency import 在 ordinary task 模式拒绝、专用模式可准备，但缺绑定 callback 仍在执行前 UnsupportedAbi、host_calls0。这证明实际扩展缝，不证明未来新 Runner 已实现。 |
| 仅非必需元数据可向前保留 | 本轮 fixture 在 Manifest 添加未知可选 Protobuf 字段，真实 Package 原字节保留且能力／dependency 摘要仍空；保存字段不赋予新行为。不能把此机制当作接受未知必需语义。 |
| 稳定 API 不被未来业务复用改变 | 有界新业务可增加 handler／输入输出类型，在既有 Transform64KiB内处理；旧错误、一次输入／完成、取消不证明回滚、权限与预算语义保持。Rust 不向已有穷尽 enum 偷添 stable 分支，C/C++ 不扩大旧结构体或把 STL／指针作为跨域数据。新模块／profile／显式大小版本适配分别演进。 |
| 版本路由必须显式 | 若未来真的需要 task4 或新版 UI，保留 task3 decoder／Runner／原件，与新 decoder 并存；不能给旧摘要开宽松通配。新客户端只在懂得 profile、准确摘要和路线限制时使用它。当前尚无这种新版本实现。 |

当前模式也有组合限制：dependency、IO、mutation 三种额外 import 互斥，不支持将现有三个 factory 任意混接。未来同时需要依赖与流式 IO 时，须单独设计并验证组合 profile／调用图／共享预算，而不是放松旧 factory。此新增路线可保留 ABI2 和旧 imports；仍需真实调度与授权证明。

## 已知未来需求：必须先完成什么，哪些可兼容后加

这里的「可后加」指能在不改已冻结基础 ABI 的前提下增加；不表示已经实现、不必验收或可先宣布完整 SDK 充分。

| 需求 | 不破坏旧接口的接入方式 | 必须完成的共享合同／准入条件 | 当前状态与冻结影响 |
| --- | --- | --- | --- |
| 供应商有界 JSON／结构化转换 | 新 Transform handler/type；每条输入输出64KiB，宿主单独代办 IO | 明确不支持字段、工具/多轮语义、转换错误及不重试；较大请求使用独立资源 profile | 扩展路径已有实际基础证据；012 image-capability不是 Responses 转换实测。具体实现暂停。 |
| POST SSE、WS、双向会话与工具进度 | 独立 streaming profile/schema/import，或宿主授予的有界资源任务；旧 task3不改为多次完成 | 身份/代次/精确 origin 权限、sequence/credit/累计字节预算、原 deadline、cancel/revoke gate、EOF/error/Unknown、Close/ACK与可信资源回收分开；无自动重放。三语言所有权与实际 broker/Runner 必须闭环 | 私有 M03 stream已有监督证据；公开三语言实验 channel 合同及有限本地 runtime 已实现；SSE/WS 网络 source adapter、正式公共 Workbench binding 与相应产品资格仍未完成。这是完整目标 SDK 的共享前置，不能因基础 ABI 候选通过消失。 |
| Cloud scoped changes、事件订阅、同步 cursor | 独立 content-change/event profile；保留旧7条内容命令。事件可作为新版本固定任务输入，但不能冒用 UI eventId 或 CommitReceipt.eventId 当同步 cursor | 明确授权范围、分页/ACK、持久 cursor与epoch、撤权后禁止消费、重开重新授权、重复/缺失处理、快照/CAS、无提交就不推进 cursor；不能公开 SQL／Store 路径 | 有限本地 Events channel 不授予内容变更来源；当前 guest 无授权 listChanges/subscribe 公共命令，私有 content_api不是SDK。实现和验证前不能称完整 Cloud-capable SDK 冻结。 |
| 大块附件／上传下载／结构化对象 | 独立 blob/transfer资源 profile，保留基础32KiB附件片段与64KiB消息 | 长度/摘要/范围关联、对象权限、租约/代次、分块累计预算、背压、取消/释放与完整性；引用不含宿主地址/秘密 | 已有片段与部分私有传输；通用公共 profile 未实现。业务数据放有界类型payload可兼容，突破上限不可静默扩大旧接口。 |
| 跨插件服务／异步依赖 | 独立 service/combined profile，在批准slot和当前实例授权交集内路由 | 深度/循环/死锁规避、根deadline/cancel传递、共享累计预算、调用原字节证据、关闭后旧代次拒绝；不得在持DB锁时等待另一域 | 同步slot依赖已有实际factory/原包；异步/服务依赖尚未稳定。不同类型会话不能仅把同步回调延长。 |
| Codex执行／PTY／原生能力 | 独立批准的 native/exec backend 与任务资源 profile；普通 Wasm不增加任意WASI/OS权限 | 工作目录/文件/命令/网络/凭据等授权边界、完整进程树和必要句柄、UI/host/supervisor各故障行为、稳定错误/终态、原Unknown保留 | 现有监督不等于任意原生程序隔离或 exec授权。安全/完整执行器权限模型仍需单独明确；具体Codex实现暂停，不因原源码缺口阻塞基础ABI。 |
| 采集／卡片UI | 有界内容命令和新业务Transform；必要时独立 capture/projection 或 UI2 | 来源身份/摘要、审批和对象scope，generation/revision/serial、未知必需控件明确拒绝；专业绘制/媒体/持久draft另有合同 | 基础UI1与content已有回归；专业UI/完整采集API未全部公开。旧6节点UI不承诺Flutter Widget API或任意窗口。 |

无需为上述未来插件现在增加另一套 supervisor、SDK语言或数据库权威。现有边界能容纳独立 opt-in 扩展，但完整目标 SDK 仍欠对应公共合同、授权与生产验证。发布、native sandbox 权限扩大或新系统组件不由本文件授权。

## 本轮资格与仍未完成

`S013/extension-contract-001` 三个 fixture在真实当前 Package/Runner 上通过；它们是拒绝／隔离／字节保留证据，不是 future functionality。profile-query-001 的真实新 host／tool、未知参数拒绝、双路65536／65537输出、超时和非零退出通过。原643项runtime、SDK91项Rust（含重叠旧兼容/IO项）、12旧基础原件与工作台4项各有自己的范围，不能相加成全面通过率。

基础 ABI 可在明确 Windows 矩阵和当前三语言新旧包资格完成后独立冻结；完整目标 SDK 还必须完成上表共享 stream/events/cursor/transfer/组合或执行器合同中实际使用的部分。本轮保持SDK源码test.50、profile标记candidate。版本/路线的扩展规则和负测解决未来新增时的兼容策略，未声称未来需求均已实现。
