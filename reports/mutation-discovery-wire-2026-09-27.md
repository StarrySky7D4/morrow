# 原计划发现私有协议与 Dart 客户端

日期：2026-09-27。分支：`codex/io-safety-refactor`，基线 `b9225f64f6c62584ad7243e30249d8a088bcb155`；本轮为未提交的增量，保留已有修改。

## 已完成

- 将原生有界计划发现接入私有 Action 134；新增 Discover 请求、NextPlans 命令及 Plans 结果，重新生成 native/web 绑定与宿主 schema 摘要。公开 guest IO 契约和持久化 schema 未变。
- 启动身份绑定包、Registry revision、主体、操作范围、扫描限额和期限。合法准入失败也保留提交身份；相同重试只取回原任务，不重新开游标。Next 重试只取回原命令，不推进下一页。
- 发现任务与目标选择分离；仅分页／释放，禁止写入执行；已有目标任务拒绝 NextPlans。Release、停止、实际 join 和 ack 复用原 owner 生命周期。
- Dart 增加 `MutationDiscoverRequest`、`startDiscovery`、`submitNextPlans`，接入真实 Windows Workbench 私有通道。严格校验列表及字节边界、混合字段、任务／命令身份和 done／terminal 一致性；结果在回帧清理前复制成不可变列表及字节。
- 外层调度识别新动作；服务业务嵌套拒绝。构造及发送缓冲执行现有尽力清理。客户端不自动重试任何请求。

## 本轮验证

| 检查 | 结果 | 证据 |
|---|---|---|
| 新增 Windows 真实宿主发现协议测试 | 4 通过、0 失败 | `build/mutation-discovery-wire-tests.log` |
| 宿主完整 lib 回归及全部真实 Rust 回帧重导出 | 171 通过、0 失败、0 ignored；124.85 秒 | `build/mutation-discovery-wire-host-full.log` |
| Dart 变更／文件任务组合回归 | 46 通过、0 失败、1 既有文件任务夹具跳过 | `build/mutation-discovery-wire-dart.log` |
| Flutter 帧清理和受控子进程传输 | 26 通过、0 失败 | `build/mutation-discovery-wire-flutter.log` |
| Dart 指定源文件／传输测试分析 | exit 0，JSON diagnostics 为空 | `build/mutation-discovery-wire-analyze.log` |
| 宿主 Clippy lib | exit 0，12 条既有其他模块警告 | `build/mutation-discovery-wire-clippy.log` |
| 私有绑定重新生成一致性 | `generate_workbench_client.py --check` 通过 | 本轮工具输出 |

Dart 七类 Mutation 真实 Rust 夹具均已启用并通过；唯一跳过的是另外依赖 MORROW_FILE_WIRE_FIXTURES 的既有文件任务帧测试，不是发现回帧。四项新增宿主专项包含在 171 项全量中，不应重复相加。Rust 指定文件格式与 git diff --check 均通过。

Clippy 沿用 `-A dead_code -A clippy::collapsible_if`，不宣称全仓零警告。Dart 分析使用工作树内 DART_DATA_HOME，避免已知系统 perf 清理故障。宿主测试使用既有真实 Wasm 夹具，本轮未重新编译这些插件。Flutter 使用受控 Python 子进程，证明通道路由及帧生命周期，不等于真实 Rust 子进程崩溃恢复或 UI 验收。

原计划页跨语言夹具 `build/mutation-wire-fixtures/rust-mutation-plans.bin` 为 880 字节，SHA-256 `4EC82E29BF9DFC9F64E59A1B819CF2D1F27AE02E0767CDB63ABD602B9799D759`。夹具含本次会话新建目标引用，重跑后摘要会改变。

## 保留边界与下一步

发现只返回受保护的原始计划，不能替代完整历史核对，也不证明文件效果。空页可能尚未扫描完；完成或失败均不能继续使用同一游标。回帧在宿主领取后若传输丢失，该页不可再次领取，客户端必须明确处理，不能换令牌自动跳页。

worker 游标尚无预算耗尽后的可续接位置；重新打开仍从起点扫描。下一步优先实现独立 Dart 恢复会话及丢回执状态，再验证真实宿主跨进程重启闭环，并设计受权限约束的续扫。恢复 UI、完整 IO SDK、Windows 条件替换与其他平台验收继续开放。

本轮没有安装包、UI 性能验收、提交、推送或 Release，SDK 尚未冻结。
