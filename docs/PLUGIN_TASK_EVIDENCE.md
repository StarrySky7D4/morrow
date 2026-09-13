# 纯转换历史证据与隔离重放：test.34–test.35

test.34 建立实际捕获、可保存原件和独立执行验证；当前应用版本为 `0.1.9-test.35+40`，已增加[原子内容关联](PLUGIN_COMMITTED_EVIDENCE.md)。捕获本身不提交内容或签名；可信宿主显式提交建立原子关联，后续封存再形成签名关联。默认工作台 UI 尚未自动捕获，完整多包流程仍不可由此重放。

## 证据和完整性

固定 `core/schemas/task_evidence.proto` 使用 Protobuf 原字节与有界 LZ4 容器，包含实际包 archive、原始 Cap’n Proto Invocation、实际相交后的 fuel／内存／调用预算、执行后端标识、Runner 保留的原始 completion、退出码或故障、实际调用数和剩余 fuel。

Evidence 保留原始 PB、容器和摘要，未知可选 PB 字段不因解析重编码丢失。decode 必须传入外部固定的原始 PB SHA-256；它与压缩文件 SHA-256 不同。容器不自行建立信任，摘要也不证明是谁执行了任务。core 的 encode 是格式工具，调用者可以构造观察数据；只有 runtime 的捕获入口自行执行并形成不可直接构造的 CapturedTransform，导出文件本身仍无签名；test.35 的已封存原始 Commit 可提供宿主签署的摘要关联，但不证明任意调用者构造的观察数据来自真实执行。

包和 Invocation 都再次解析校验；任务必须是 ABI 2 纯转换，存在依赖声明或动态依赖调用功能时拒绝。预算必须在实际包声明和运行时硬上限内。压缩前后长度、字段数量、重复已知字段、字段类型、子消息、成功 completion 的相关性和注册输出上限均检查。故障可以保留 Runner 实际暴露的有界 completion；完成后陷阱时 Runner 会丢弃 completion，记录故障与空 completion，不重建不存在的成功输出。

## 实际捕获

`Pool.record_transform(manager, host, session, invocation)` 先验证实际 Host／Manager／根会话与当前批准，捕获前验证计划，使用该根的包、实际预算与取消信号执行。执行前后维护池，真实陷阱／任务协议／执行限额停止根，外部撤权后的纯输出不再交付。不会接收任意外部 TaskReport 并把它包装为实际捕获。

CapturedTransform 提供实际 TaskReport 与 Evidence。它不提供核心提交证明、对象授权或可复活句柄，捕获本身不写入内容库；test.35 的显式内容接口可将其证据纳入同一提交事务。读取历史证据不依赖原注册表、模块文件、运行实例继续存在。

## 隔离执行与结果比较

`replay(evidence, policy)` 仅使用嵌入的包和任务字节，每次创建新的 Runner。该接口不接受 HostRuntime、Connection、Store、路径回调、凭据或恢复授权；没有 WASI。任何普通核心 exchange 都拒绝，即使 guest 忽略错误后返回 completion，也不能报告成功。依赖任务在执行前拒绝。

本阶段后端 profile 为 `wasmi-1.1.0/morrow-pure-v1`，对应当前 ordinary-task Runner 配置和此模块的纯转换验证语义；影响结果的后端或策略改变时必须更换 profile。其他 profile 返回 UnsupportedBackend。它不是跨平台、跨版本结果一致性的承诺。

运行使用记录的实际预算，policy 只限制是否允许，不默默提高额度。支持比较成功输出、结构化业务失败、Trap、TaskProtocol 和 Limits；Limits 是实际运行故障类别，不全都称作 fuel 耗尽。外部 Cancelled／Deadline、撤权或准备状态不能仅靠历史纯输入确定性复现，返回 UnsupportedOutcome。

比较完整原 completion、退出码／故障、调用数和剩余 fuel。返回 matches=false 时保留此次实际报告，不把完整性通过误称为执行结果匹配。匹配也不证明业务结论正确或具有提交权限。

## 独立 Windows 工具

```powershell
build/replay-tool/release/morrow-transform-replay.exe <evidence-file> <raw-evidence-sha256>
```

只读打开一个有界证据文件，使用默认可信执行上限。退出 0 表示精确观察匹配，2 表示不匹配，1 表示格式、完整性、策略或后端拒绝。标准输出不打印用户输入／输出正文。

## test.35 原子内容关联与后续任务

可信宿主已可通过 `HostRuntime::create_content_with_evidence`／`edit_content_with_evidence`，把有序证据集合与内容、回执所据的原始 Commit 和事件放入同一 Store 事务。签名封存覆盖原始 Commit 中的摘要引用；库完整性检查与快照核对原件和全部关联。重试必须匹配原命令与有序摘要，不能用后来重跑得到的证据替换已提交原件。格式、限额、迁移和实际资格链路见[内容关联说明](PLUGIN_COMMITTED_EVIDENCE.md)。

默认工作台 `run/transform` 仍主要返回解码数据，创建／编辑随后构造核心内容命令；设置保存还存在多次分页任务对应一次提交。下一步需要保存从捕获到提交的稳定意图与有序原件，处理重试，再接入默认 UI 自动捕获。还需记录版本化宿主投影、前置内容与最终命令，才能验证完整内容重建。

多层 A/B 调用证据、宿主响应录制、全局证据配额、保留与 GC、完整隔离重放及六平台验收继续推进。历史纯任务验证见[test.34](../reports/test.34-transform-replay.md)，内容关联验证见[test.35](../reports/test.35-committed-evidence.md)。
