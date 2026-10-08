# C13 原插件管理器接线与 Windows 限定资格

2026-10-06，本地增量，基线 `e83cdf1d3506b001d07ffceaaa9f98b688bcc150`，tree `abcdee6e582ce18954118430e514f1d0805ce095`。
本轮没有 commit、push、main、tag、Release 或 CI；C12 历史封存工件未重建。

## 当前结果

新增 `Manager::connect_agent_session_process` 与 `ManagedPreparedPackage`，使用原
Catalog/Registry、原实例表、原 Control、128 个共用实例限额和同一 `Arc<Connection>`。
预算取 Manager 与 manifest 的交集；运行中继续核验当前 selection/revision、原 host 与取消。
基础包 selection 不批准 wrapper；完整 wrapper SHA、双 schema、会话与进程权限子集仍独立批准。
没有第二个 Core Store，也没有为同一 managed 实例创建替代连接；旧 factory 未放宽，Unknown 不自动重放。

修正时序：`live_tool(clock)` 后再次核验 managed 实例；时钟采样发现取消即停止原 Control
并传播 Core revoke；Create/Append 的最后提交前取消不写原 SQLite。效果后失权保留 Unknown。
新运行期 `HostIdentity` 绑定首次使用的进程宿主，禁止注册/执行时换宿主；关闭遇到外来
ProcessHost 不误报成功，仍停止并退役原连接。R2 issuer 失效时仍请求可信 provider 清理，
返回原错误。清理请求不等于真实退出/EOF；仍由原 owner 观察并 `finish`，不释放未核实资源。

## 已运行，分别计数

| 范围 | 实际结果 |
| --- | --- |
| 新 managed host | 14 方法 PASS；含封存真实 Rust Wasm，但这些 provider 是逻辑测试实现 |
| 新 Manager profile | 6 方法 PASS；原 production `packages` feature；128 总实例与预算、生命周期 |
| 新进程宿主身份 | 3 方法 PASS；移动保持身份、外来宿主拒绝、销毁失活 |
| 原 host | schema 1 与 route 13 PASS；不新增计数 |
| 原 Manager | 11 与 storage 1 PASS；未知发布、保存失败不复活 |
| strict single-import | 6 方法 PASS；旧入口仍拒绝新 profile，不与旧权限混用 |
| process 原回归 | host 18、protocol 5 PASS；不新增计数 |
| 实际 Windows managed Wasm | 2 方法 PASS；合成子进程；helper 被明确排除 |
| 新 host Clippy | all-targets、`-D warnings`，exit 0；原依赖 feature 的 19 个 dead-code warnings 单独保留 |
| 限定格式 | 新/修改的 8 个 owned 文件 `--check` exit 0；原 Manager 只新增两行，未格式化其既有 import 差异 |

Windows 两场景使用同一普通合成 SQLite/runtime/原 executor Arc：封存 proposal Rust Wasm
→原审批/Claim→原 Windows provider 真启动→封存 process Rust Wasm 读写/终止。
验证真实 stdout/stderr、事件序列、耐久终态、退出与 EOF，再由原 owner 清理、finish 并核实
native registry 空。另测 Manager 停用后 guest 被拒绝而可信收尾保留。sandbox 实际为
`None`，这不证明生产 OS 隔离、PTY、托管网络、认证 API 或安装界面。二进制与 Wasm 的
精确 SHA、执行日志 SHA、前后源码输入 SHA 见 [validation](codex-sdk-c13-validation.json)。

## 失败与修复记录

首次 Manager 编译时其他新文件仍在编辑，Cargo exit 0 但全局源码输入变化，不能作为最终
固定输入资格；最终 `packages` 同输入复验通过。首轮 host 缺锁定 cc 1.6.0 离线缓存，
其后 `--locked` 要求新增 package-management 的 prost 依赖边。只从本机已有缓存复制
120 个 checksum 核实的公开 crate，不联网；由 Cargo 正常解析两个新候选 lock，版本、
source 与 checksum 全部不变，仅 runtime→prost 依赖边新增。C12 lock 前像仍在封存包。
Clippy 的一处新布尔表达式已等价修正并复验。原 Manager 全文件格式检查失败涉及新导出
排序和既有 import 风格：只修新行排序，保留既有代码，限定 owned 格式检查通过。
Windows 元数据初次因受限 Git 缓存与全平台 Android 缺项失败，保留原始 exit；经审查的
固定缓存离线 Windows 平台检查通过，没有下载或修改系统权限。所有失败日志保留在有界交接。

## 仍开放的完整目标

这一步接入原 Manager，并非正式产品安装链完成。完整 wrapper 的持久 catalog/review
审批尚未实现；当前原 Registry 保存的是 base selection，wrapper 仍由可信调用显式批准。
Workbench 新容器路由/GUI、原 ProtectedSession 的 native owner 借用/工作线程桥、认证
Codex transport、真实 sandbox/managed network、native close-input/PTY resize/interrupt
以及其他平台仍 OPEN/NOT_RUN。真实 handles 不跨重启恢复，Unknown 不自动再次执行。
完整 SDK26/G04 继续 OPEN；继续原批准链与 owner 接入，再补 blob 耐久/恢复、完整文件系统
与平台矩阵，不能把冻结标准缩为本轮通过的方法。

327 SDK、57 冻结输入与旧 wire/schema/封存 archives 字节门禁保持。新候选有独立摘要，
不继承旧冻结 pin。本轮源码写回前后还须核实当前分支/HEAD、原未提交工作与全部前像。


## 公开发布路径说明

链接的 validation 是保留原测试结果的公开路径派生记录，不能称原字节或当前源码新资格；原始完整 SHA、派生 SHA 和替换边界见 [公开历史记录说明](PUBLIC_VALIDATION_PROVENANCE.md)。原始失败和 NOT_RUN 范围不变。
