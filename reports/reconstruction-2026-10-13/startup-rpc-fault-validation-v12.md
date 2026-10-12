# 启动 RPC 故障夹具与限定验证 v12

本轮将已编译、审核的四个 Rust 源文件同步到开发分支。新增 `c28-qualified-startup-rpc-fault-fixture` 显式 feature，默认关闭，正常资格 feature 不会启用它。真实资格 owner 校验之后，首次有效的 `PageVersioned(limit=1)` 返回固定错误；三条私有服务通道仍走原分派，不消耗故障标记。七项合成测试覆盖一次性状态、请求校验、原错误编码和路由隔离。

## 已通过

- 独立源码副本完成四阶段 Windows AMD64 编译：故障 feature 关闭／开启各一份库测试目标、各一组 host 与 supervisor。四次原 Cargo 和外层均退出 0、双流 EOF，四次严格 HOME 后验通过；源码和 20 份锁守卫通过。不是 Flutter GUI 或 Release 构建。
- 关闭故障 feature 的 345 项清单仅执行原 14 项；开启后的 352 项清单仅执行 21 项。每项 1 通过、0 忽略、退出 0、双流 EOF。合计 **35 次执行、21 个不同测试名称**；其余条目只列出未运行。原日志及闭合记录另经独立复核。
- 为后续原生验收选定的故障 host 和普通 supervisor 保存副本，由主代理和独立代理分别进行两遍完整 SHA256、文件身份及 AMD64 检查，零漂移。程序未执行；后续字节核对未重读全部源码、HOME 或 vendor 数据。

当前生产 Dart 保持原字节；此前 23 项 Flutter/Dart 内存回归保持其历史限定范围，本轮未重跑。

## 未完成与下一步

真实 RPC 失败的 Dart 测试仅作为 [源码候选](source-candidates/startup-rpc-fault-native-r001/README.md) 归档，未编译或运行。数据准备与来宾采集代码只完成静态审核、仍禁用；宿主采集／checker 最新草稿尚未封存，最新补丁未完成审核、接口文档待同步。后继绑定仅保存作者源码，未生成完整材料。这些执行工具和未完成草稿不包含在本次推送中。

下一步完成精确绑定和独立审核，生成本地有限验收包并逐项回读，再做新的 VM 就绪检查与真实 `RustWorkbench.open/catch` 异常隔离验证。当前组合 native／VM 验收为 **NOT_RUN**，先前正常验收不能替代当前组合；原 native **Unknown 保留、不重放、不终止**，历史失败保持原范围。

SDK26／G04、namespace、祖先目录持续 reparse、生产沙箱与网络仍 OPEN，`release_eligible=false`，SDK 未冻结。Agent 会话层与安全执行层完成后暂停准备预览，扩展执行层延期。版本、锁、工作流、main、标签与 Release 不变；本次不运行 CI。

见 [机器可读证据摘要与精确测试名称](startup-rpc-fault-validation-v12.json)。其中本机原始证据的哈希用于核对归档身份，原始日志不随本次提交公开；不要将哈希清单视为额外运行验收。
