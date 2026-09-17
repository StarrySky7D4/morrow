# Track A 定向整合验收（2026-09-16）

状态：Windows 范围验收通过（PASS_SCOPED），可以定向合入当前开发线。用户已授权将本轮定向修正合入当前开发线并推送源码；后续任务见 [编码看板](../docs/DEVELOPMENT_BOARD.md)。

## 范围与来源

- 当前开发线基线：`680787b17c8f48fbcb3eaebc33530eaa2ccff532`（test.52）。
- 隔离分支：`codex/integrate-track-a-current`。
- GitHub 候选参考：`track-a/w1-io-contract`，固定提交 `86211b32cac3e4904b1f169bab50f6f535b8a792`。
- Drive 本地副本：`C:/Users/Administrator/Desktop/temp/morrow-track-a-review`，参考其 W2 文件读取调用链；原下载文件未修改。

采用现有协议重新适配候选实现。不能整体合并候选分支：两条线的 IO schema 与 manifest 字段定义不兼容，Git 自动解决的文本也可能产生重复字段。既有 `io.capnp`、`io_manifest.proto`、包 manifest 字段 18、DB16 持久化规则及冻结 SDK 原件均保留。

## 本轮可审查的实现

1. **协议编解码**：按当前 Cap’n Proto 契约支持 Read/Finish/Cancel。关联原始请求摘要、call ID 与 schema 摘要；限制帧、负载、遍历、嵌套及偏移，拒绝尾数据、未知标签和畸形指针。其余已知操作返回 Unsupported，不执行外部操作。
2. **底层 Wasm 调用**：加入实验性 `morrow_io_v1.call`；IO 与核心交换共用 host-call 预算，限制输入输出内存范围及调用顺序，取消后不交付迟到结果。raw Runner callback 仅供可信宿主使用，本身不授予权限。
3. **真实管理链路**：文件代理使用 Manager/Registry 的实际批准、ManagedInstance、HostRuntime、IoBinding，支持 Pool 管理的根实例。IO handler 独立声明，普通任务入口继续拒绝 IO；不复用纯转换注册。
4. **资源与额度**：宿主传入固定字节，客体只接收不透明引用。原文件租约、当前调用绑定及结果交付均校验；实例停止、审批变更或过期后拒绝。空闲资源不占用并发作业，释放不退还累计字节；读取按完整请求帧和响应帧计费，重绑定及多 broker 共享实例额度。
5. **回收与时钟**：宿主维护调用 `reap` 清理失效文件；身份、能力及文件归属预检不推进共享时钟。保留原资源交付证明，防止用较长的新绑定跨越原文件期限；成功交付更新单调时钟。

## 明确边界

这是 Windows 原生宿主中的固定字节读取基础能力。每次 managed `run` 携带一个准确的 IO 请求，客体只允许一次相同 IO 调用，并以实际响应完成；核心内容交换被拒绝。它不是任意任务输入和多次 IO 调度的最终 ABI。

没有接入 OS 文件选择器、目录枚举、写入/覆盖/删除、持久证据库或 Flutter UI。网络 HTTP 客户端、对外 API 服务节点、WebSocket/QUIC 的真实适配也未在本轮接通；下载目录的网络/QUIC 状态模型与草稿不作为已实现后端合入。后续外部副作用需要沿当前 DB16 预约及证据链继续实现，不能直接把状态模拟器接到产品入口。

IO 新接口仍为实验性，不纳入 guest-v1-rc1 冻结承诺。网络、移动端、Web、UI 与完整产品构建不是本轮测试结论。

## 调用约束

新 IO import 的输出缓冲容量固定为 128 KiB，输入帧最多 128 KiB，单次文件负载最多 64 KiB；输出与输入区间不能重叠。客体先读取 task input，再调用 IO，最后完成任务。底层传输失败返回 `-1`，授权/配额等业务拒绝使用当前协议的响应状态。生产宿主必须提供单调时间、每个 broker 独立生成的秘密值以及闲置回收调度。

## 验证

所有日志位于本隔离工作副本的 `build/`，以最终日志为准。

| 检查 | 结果 | 日志 |
| --- | --- | --- |
| 核心 release 全量默认功能测试 | 396 passed / 0 failed / 0 ignored | `io-merge-core-full.log` |
| 运行时 release 全功能测试 | 271 passed / 0 failed / 0 ignored | `runtime-integrated-final.log` |
| 新 IO codec / raw Runner / managed broker 回归 | 15 / 12 / 15 项，已包含在以上全量结果中 | `io-codec-tests-final.log`、`runtime-integrated-final.log` |
| 冻结 Rust/C/C++ 原始插件执行 | 9 项兼容 + 3 项依赖调用通过，已包含在运行时 271 项中 | `runtime-integrated-final.log` |
| 核心 / 运行时全目标严格 Clippy | 通过 | `io-merge-core-clippy.log`、`runtime-integrated-final-clippy.log` |
| wasm32 默认功能 release 编译边界 | core / plugin_runtime 均通过；未在浏览器运行 | `io-merge-core-wasm-check.log`、`io-merge-runtime-wasm-check.log` |
| SDK 核验工具回归 | 14 项通过 | `sdk-integrated-tests.log` |
| 冻结 SDK 执行后完整性 | 36 个固定文件、13 对原始 Wasm/插件包通过 | `sdk-integrated-final-integrity.log` |
| 下载原件与协议保留检查 | 19 个下载文件 SHA256 与前次清单一致；当前协议及 build.rs 无修改 | 本次实测，未重打包冻结 guest |

完整回归覆盖真实 Manager/Pool 管理、预算共享、过期与撤权、90,007 字节分块读取和最后时刻拒绝交付。新增并列违规 guest 场景验证重复 IO、伪造完成数据和禁止的核心交换都不能形成成功结果。

首轮新增违规 guest 测试因 WAT 输出容量使用 65,536 而触发已有 ABI 的 131,072 字节约束；修正测试夹具后 15 项通过，再跑最终全量 271 项通过。首次失败保存在 `managed-file-io-protocol-final.log`，复验在 `managed-file-io-protocol-verified.log`。该失败不是生产实现修改的理由。

wasm32 默认 profile 的 core 编译有 7 项 read_archive/read_capture 辅助代码未使用警告；运行时无警告。这是编译边界检查，不等于 Web IO 后端或浏览器执行验收。

未启用核心 fault-injection 特性，因此不将条件编译排除的崩溃测试算作通过。没有构建 Flutter 应用、安装设备、连接真实网络服务或重建冻结 SDK guest。

## 合并方式

只合并本隔离分支的定向修正；不得将远端候选的旧 schema、旧授权对象或 Drive 网络占位模型连带覆盖当前线。合入前复核目标仍基于上述 test.52 提交；若目标发生变化，重新检查差异和相关回归。本轮按该范围合入 `codex/refactor-test.51-network` 并推送同名远程分支；不整体合并旧候选分支，不创建 Release。
