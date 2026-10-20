# 2026-10-20 结构性修复分支

基线为 `abae8bfd568d95d5230de57082149e8b645e90b6`，分支为
`codex/structural-blockers-20261020`。当前机器可读状态为
[current.json](current.json)。历史报告维持原结论，当前仍不是发布候选。

## 启动令牌契约

原生启动过去将 `LogonUserW` 的原输出直接视为最小权限令牌。普通账户的
组策略可以附带额外权限；禁用权限仍可重新启用，因而不满足原来的准入。
新实现由同一个 `Attempt` 管理两个独立槽：登录源令牌与启动令牌。

1. 登录源必须仍是新建、非管理员、未过滤的实际 managed user；身份、会话、
   IL、logon SID、认证身份、授权掩码与稳定查询窗口继续检查。
2. 仅在登录源合格后执行一次 `CreateRestrictedToken`，显式删除除
   `SeChangeNotifyPrivilege` 外的权限；不设置 `DISABLE_MAX_PRIVILEGE`、
   `SANDBOX_INERT`、`LUA_TOKEN`，不改变组和 restricting SID。
3. 检查输出身份确实不同、原认证身份和非权限安全上下文一致、权限数精确减少、
   保留权限属性一致。`TokenHasRestrictions` 的变化仅在该受控派生关系内接受；
   任意外来 filtered token 不能因此获得启动授权。
4. default DACL 初始化、启动前检查及实际 R 读回全部使用该启动令牌。
   原令牌及派生令牌均留在原 owner 内，部分输出、失败和 Unknown 不自动重试。
   原生调用计数改用具名操作，派生与默认 DACL 初始化各有独立的一次预算；
   deadline 仍封存资源观察，不能转入登录拒绝的有限关闭路径。

`asuser_a1_target.rs` 同时提供准入和拒绝原因，原 31 项拒绝回归继续执行。
额外派生检查同样通过唯一 evaluator 产生错误原因。纯模块无句柄、无序列化能力，
本身不能产生 admission capability。外部 Wasm/R2 协议未改。

本变更是新的内部启动契约，需要新产物、C/R/S pins 和实机验收；不能沿用旧 hash
或将原 `TargetPolicyGuard` 失败追认为已确定由某个权限导致。
Microsoft API 依据：
[CreateRestrictedToken](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-createrestrictedtoken)。

## 结果与资源

`AttemptOutcome` 分开记录 runner 是否进入创建、观察是否丢失、资源状态。
登录失败而尚未进入 transport/helper 时，已明确持有的 token/query 句柄有一次
有限关闭机会。失败关闭、调用在途或丢失观察不被解释为释放证明；第一错误保持。

这还不是完整事务退休：账户、factory、路径 pin、上层 Claim/registry 的所有权
继续保留，旧 Unknown 不会改写。不能依据“未派发 runner”释放整个执行配额。
后续需在原 owner 中补齐这些资源的实际退休证明，而非取消 Unknown 限额。

## 构建边界

`workbench_host` 的 catalog review 在所有原生平台可用，因此其依赖不能只声明在
Windows target 下。修复后 Linux 宿主单元测试可编译并执行。原生 crate 的两个 Windows 导出也补齐平台条件，
使共享策略与诊断测试可在 Linux 构建。

原 derived workspace 的锁文件来自不同的依赖图，漏掉该工作区的测试依赖，
所以 `--locked` 直接失败；从其他 workspace 用 `-p` 运行它的测试也漏掉 dev
依赖。现在从该 crate 的 manifest 进入，锁定所需测试依赖，移除不属于其图的
包。共有包的版本和 git 来源未升级；额外 `ctor 0.6.3` 来自原 workspace 已声明的
测试依赖。历史报告与 frozen upstream 不改写。外层产品 workspace 仍用自己的锁。

## 可复现命令

要求 Python 3.12+、Rust 1.96.0、Cap'n Proto 编译器。

```sh
python tool/verify_structural_repairs.py --mode portable
```

在专用、可丢弃的 elevated Windows 测试机中运行：

```powershell
$env:RUSTUP_TOOLCHAIN='1.96.0'
$env:MORROW_DISPOSABLE_WINDOWS_TEST='1'
python tool/verify_structural_repairs.py --mode windows --disposable-windows
```

第二条入口创建并删除一个随机测试账户，明确加入按 SID 解析的 Users 组，
检查真实系统登录令牌、派生令牌、对象描述符和默认 DACL 初始化；
不修改 LSA 权限，不启动业务 runner，不授予完整 47 步资格。普通 Windows 回归可
省略 `--disposable-windows`。报告留在 `build/structural-acceptance/result.json`，
记录命令、日志、提交和工作树源码摘要。原生构建输出统一位于已忽略的
`build/structural-native-target`，无需本机私有 Git 忽略规则。L0 必须实际执行且恰好一项通过，零测试
退出 0 不算通过。

```sh
python tool/verify_structural_repairs.py --gate build/structural-acceptance/result.json
```

所有必需门槛均须具有同提交、同源码摘要的完整 PASS；本地单测、交叉编译、
历史 PASS 和 NOT_RUN 都不能替代 Windows 完整验收。当前该命令应返回 2。
CI 工作流只构建、测试和保存日志，不部署。根据用户要求，整体修复完成前的提交
带 `[skip ci]`；目前 CI 尚未运行。

## 本地限定验证

- Python 75 项（原 71 项，加 4 项精确源码门槛测试）通过。
- Rust 宿主 118 项通过，3 项 ignored 不计为通过。
- 原生 crate 的跨平台 A1 策略/诊断 72 项通过，包含新的 12 项纯策略与状态测试；
  不将这 12 项的独立运行重复累计。
- Windows 目标的原生库及测试目标 `cargo check --locked` 通过；这是交叉编译，
  实际 Windows L0 测试仍 NOT_RUN。
- 完整门槛检查应拒绝发布。没有触发 CI、创建 Release 或改变 main。

## 仍需继续的工作

- 原 47 步源驱动与其固定输入未找到；清单在 `current.json`，必须补回实际源码。
  检索到的是历史报告，不能从报告编造相同验收结果或脚本。
- 交互 Windows 上验证实际初始化、握手、一次业务、join/ACK、owner/factory/account
  退休，再运行全 47 步与九项门槛。
- 正式编辑器仍需完整接入工作台 owner、附件 provenance、富捕获证据和 S1→S2
  持久交接。现有独立组件测试不能代替该接线；不得直接打开 deferred ack 清理。
- 本地 Flutter 启动被自动审批拒绝：它尝试访问云实例元数据地址。本分支没有绕过
  拦截；Flutter 运行验证尚未完成。修复后的 CI 可提供受控测试环境。
- SDK26/G04/SDK_FREEZE 仍开放，且五平台资格未由本轮原生修复自动完成。
