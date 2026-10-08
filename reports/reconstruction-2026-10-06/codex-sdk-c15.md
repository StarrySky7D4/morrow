# C15 工作台完整会话插件管理接线与限定验证

2026-10-06，本地增量，基线 `e83cdf1d3506b001d07ffceaaa9f98b688bcc150`，tree `abcdee6e582ce18954118430e514f1d0805ce095`。
本阶段没有 commit、push、main、tag、Release、CI、封存 Wasm 重建或系统权限变更。

## 实现与边界

新增独立 `agent-catalog-admin-v1` Cap’n Proto 契约和 `agent-catalog-owner-v1`。
请求、回执绑定一次性 ID、规范请求 SHA、动作、catalog/Manager 双 revision；UInt64 保留
大于 JavaScript 精度范围的位。单段固定分配，有界帧、页、路径、批准子集和 cursor；
异常 mutation 回执保持 Unknown，不再次执行。旧 host.capnp 与 Core/IO 契约保持。

CatalogOwner 借原 Manager，复用 C14 的真实持久目录，不创建第二个 Core Store/runtime。
完整包装与基础包的安装、选择、批准、启停是分别明确的决定。review 核全包装、基础摘要、
双 schema、有限会话和执行域；选择文件安装时重新读字节。普通绝对本地文件有大小、
namespace/ADS/reparse 限制；这不证明原生 picker 时刻或祖先来源，也不是 OS sandbox。
256 个 mutation ID 上限只属于当前 owner，不宣称跨重启耐久 exactly-once。

WorkbenchState 持有懒打开的目录 Slot；新管理帧只走原顶层 native 管道，guest/service
嵌套入口拒绝。busy、丢失 owner、恢复与原 ProductGate 门禁保持，mutation 使用原存储
维护步骤；维护后、执行后、编码后重新检查同一 gate。控制器丢失时只撤原目录并交付
相关 Unknown，不重做请求。检查是边界采样，不保证最终采样后物理 pipe write 的原子性。
失败打开 sticky；finish 先撤目录再执行原收尾。目录撤销不等于实际进程 exit/EOF/资源回收。

插件设置已接独立完整会话插件管理界面，展示双摘要/双 schema、允许能力子集和状态。
每步单独确认；刷新不会把 Unknown 解锁，也不会自动批准或启用。页面使用原主题设置。
文件 Inspect 预览同时只读展示声明的五类会话和七类进程能力，不在预览中批准或启用。
新增文案当前为中文/英文，其他语言回退英文，没有完成新增文案的九语种审校。
此管理入口没有启动/执行按钮；WindowsExecutionPort 与 ProtectedSession/worker 接线仍缺。
Web/设备通道不声明支持此 native profile；UI 源码接线和 widget 测试不等于真实桌面资格。

## 当前实际验证（各组独立，不累计历史）

| 范围 | 结果与限域 |
| --- | --- |
| 新 admin 契约 | 10 PASS；非法帧零调用、关联/预算、双 revision、UInt64、单次执行和 Unknown |
| 新 CatalogOwner | 18 PASS；Windows 普通合成 SQLite/临时文件、独立批准、重开、撤权和 Unknown；WAT 只作逻辑入口，不算 OS 执行 |
| C14 host 回归 | catalog18 / managed14 / route13 / schema1 通过；沿用原件 Rust Wasm，重复不算新方法 |
| Dart | 18 PASS：codec6 / client5 / widget5 / 实际 Rust 向量互验1 / 原管道路由1；widget新增Inspect全部能力展示且零隐式安装/审批/启用 |
| Rust/Dart 互验 | Rust 原 example 实际输出的五对请求/回执逐字节 decode/reencode，十个 golden SHA 核验；UInt64 MAX 与 >2^53、Unicode 路径覆盖 |
| 原 Dart 管道路由 | 真实普通 Python 子进程模拟受控回执：原 service business 仍等待时，顶层 admin 收到 Busy；没有嵌套管理请求，最终 exit0、EOF/drain；不是实际 Rust 授权或服务进程验收 |
| 静态/格式 | 新 admin/owner all-targets strict Clippy 通过；原 runtime 的19条依赖 dead-code warnings保留。Dart fatal-infos 无诊断；14个 Dart 与限定 Rust owned 格式检查通过 |
| Windows 宿主 | x86_64 lib/bin locked/offline cargo check 通过；原8条 dead-code warnings保留，仅编译检查 |

每次最终运行均核源前后不变、原始日志 SHA 和退出码；准确命令、方法及输入见
[validation](codex-sdk-c15-validation.json)。首次失败均保留，没有以成功重试覆盖原记录。
Flutter 001 未启动测试且退出4294967295不记功；002的16/17不当作最终资格。
003的17项限定通过与当时文档/源码前像保留；独立审查发现Inspect预览缺能力名称后，
只补只读展示和一项widget验证，004的18项是最终当前源码资格。

327 SDK、57冻结输入、旧 contracts/Linux 源码及 C12/C13/C14 原封存包保持原字节。
独立审查是静态限定范围；不以测试数量、编译检查或审查通过关闭完整 SDK26/G04。

## 下一实际开发门槛

1. 将实际 native process port 改为借原 HostRuntime；保留原受保护 Session 的 key/lease，
   completion/EOF/facts 回同一 owner。有界队列、停止期限、取消和 Unknown 需完整覆盖。
2. 独立 AgentWorker 接原 StateSlot/Executor，完整移动原 WorkbenchState；不得借旧 IO
   carrier 伪造批准，或另建 Store/runtime。补实际工作台执行和收尾入口。
3. 完成生产保护 owner、GUI、沙箱/网络与认证、原生交互控制、耐久文件能力及平台矩阵。

这次管理接线减少一个产品缺口，完整 SDK26/G04 仍 OPEN；未生成发布安装包。


## 公开发布路径说明

链接的 validation 是保留原测试结果的公开路径派生记录，不能称原字节或当前源码新资格；原始完整 SHA、派生 SHA 和替换边界见 [公开历史记录说明](PUBLIC_VALIDATION_PROVENANCE.md)。原始失败和 NOT_RUN 范围不变。
