# 插件依赖声明、持久锁与重新绑定

本页描述 0.1.9-test.29 在 [宿主依赖路由与内容提案](PLUGIN_DEPENDENCIES.md) 之上增加的底座：包声明所需转换接口，可信宿主明确批准一个已安装、已选中的提供者，将双方包摘要一起持久保存；恢复后重新建立实际实例和运行期授权。声明、持久批准与运行期权限是三个不同层次。

实现入口为 `core/src/plugin_package.rs`、`core/src/plugin_package/registry.rs`、`plugin_runtime/src/manager.rs`。本页记录已落盘的行为与验证入口；实际执行结果和范围以 [test.29 验证报告](../reports/test.29-dependency-locks.md) 为准，不在本页预填通过数量。

## 包声明与契约检查

固定预编译 `plugin_package.proto` 在 Manifest 字段 16 增加 `dependencies`。每条 `DependencyRequirement` 包含：

| 字段 | 含义 |
| --- | --- |
| `slot` | 调用者包内唯一的依赖槽位。 |
| `handler` | 提供者应注册的纯转换处理器标识。 |
| `input_type` / `output_type` | 必须精确匹配的输入与输出类型标识。 |
| `provider_version` | 提供者**包版本**的 SemVer `VersionReq`，不是独立接口版本协议。 |
| `optional` | 未配置或不可用时，是否允许调用者通过启动检查。 |

有依赖的包必须使用 guest ABI v2，并在 `required_features` 标记 `dependencies-v1`。最多 16 条，槽位不能重复；槽位和接口标识沿用既有 identity 检查，版本范围须非空、最多 128 字节、无控制字符且可解析。未知必需特性和重复特性拒绝加载。它可以与 `transform-handlers-v1` 共存；默认构造的旧包仍没有依赖。

`Package::dependency(slot)` 查找声明，未声明即报错。`check_dependency(slot, provider)` 检查实际提供者包为 ABI v2、包版本符合范围，并以空输入查询其注册处理器和精确输入／输出类型。可选声明不跳过兼容性检查。该检查不证明 guest 实现正确，不分配权限，不建立新的通用 interface SemVer 系统。

## 同一快照中的选择与依赖锁

`LockedDependency` 保存 `caller_id`、`caller_digest`、`slot`、`provider_id`、`provider_digest`。摘要对应不可变包归档，因此同时固定版本、声明和模块内容。版本范围只用于判定所选包是否兼容；运行前不会在范围内自动改选另一个版本。

锁与原有 Selection 共用 `selection.morrow`，使用固定 Protobuf＋LZ4 契约和同一递增 revision。没有独立锁文件与选择快照之间的双写窗口。Registry 构造完整候选快照，写临时文件并同步文件内容，再原子替换目标；成功后更新内存状态。所有管理变更要求预期 revision 的 CAS，批准依赖同时核对双方当前摘要，迟到的批准不能套用到另一个版本。

读取仍有容器上限、完整性检查及规范编码检查。未知或非规范状态拒绝打开，不静默删除后重写；已有不含依赖锁的旧快照仍可读取。快照恢复检查锁的包摘要、声明、版本和接口，但允许“尚未补齐必需依赖”的配置存在，以便用户逐步批准。

持久化恢复的是选择和批准。快照不含进程身份、连接、内存句柄、共享 scope、TTL、lease 或可直接执行的运行期授权。原子快照不等于已经完成跨进程内容事务、完整断电恢复资格或依赖审计重放。

## 必需依赖、可选依赖与环

`resolve_enabled` 从目标包检查完整的必需依赖闭包：每个必需槽位必须已有精确锁，涉及的包均须启用且仍可加载。仅设置 `enabled=true` 不表示已经可启动；缺锁或必需提供者禁用时，`Manager::connect` 拒绝创建连接。

可选依赖未绑定或提供者不可用，不阻止调用者启动；但要实际使用该槽位，`resolve_dependency` 仍要求调用者与所选提供者各自的必需闭包可用。可选槽位没有锁时不能凭声明创建路由。

批准和恢复都拒绝纯必需边构成的启动环，包括必需自环。检查采用有界迭代遍历，不依赖递归堆栈。含可选边的环不因此触发必需启动环错误；这只是配置图规则，不意味着已实现可递归执行的 guest 调用。当前 route 仍要求两个不同的实际连接。

## 变更前撤权与升级清锁

可信宿主通过独占 Registry 的 Manager 执行管理动作。Manager 在改变持久状态前，先按**旧锁图**找到受影响包的传递必需调用者，再撤销这些实例及被修改包的实际连接权限，然后请求协作取消。审批上限放宽也要求新连接，不修改存活连接的能力上限。

- 更换所选包版本或移除选择时，同一快照删除该包作为调用者或提供者的全部锁；包文件和用户内容保留。即使新版本仍满足原版本范围，也须重新明确批准依赖。
- 更换或移除某个依赖锁时，先撤销调用者及其传递必需消费者，再保存锁变更。
- 提供者禁用、批准集改变、升级或移除，会停止其必需消费者。可选消费者不因该边被整实例停止，但旧 route 和输出持有提供者撤权信号，因此不能继续授权使用旧结果。
- revision 或摘要冲突在撤权前拒绝。其他校验或保存失败可能留下更严格的停止状态；旧持久选择和批准不会因此被隐式放宽，也不会自动复活已撤销实例。取消不是已经完成的内容提交的回滚。

这里的传递撤权对应 Manager 的持久控制变更。当前没有通用的提供者进程崩溃通知、自动级联停止及自动重启协议；不能把管理操作的测试推广为这类故障传播已经完成。

## 从持久批准建立新路由

`Manager::bind_locked_dependency(host, caller, provider, slot, scope, expires, now)` 每次重新解析当前锁及可用闭包，检查两端属于此 Manager、摘要和提供者身份与锁相同，然后调用既有 `Dependency::bind`。底层进一步固定实际 Host、双方 Connection、包版本和摘要、处理器、类型、共享范围和期限。

恢复后必须创建新实例，并由可信宿主重新提供本次 `scope` 与绝对到期时间，重新授予具体内容和共享对象访问。沿用相同的范围字符串不等于恢复旧 lease；另一宿主、另一 Manager 或同包的新连接不能替代旧 route 原先绑定的实例。持久批准本身不授予 `ReadContent`、`EditContent`，提供者也不继承调用者的内容权限。

`ManagedInstance::parts_mut` 为可信宿主授予内容访问保留可变连接接口。Control 额外固定创建时的 `ConnectionBinding`：Manager 的归属检查以及 `run`、`run_task`、`close` 均核对它。通过替换或交换连接造成错配时，执行与绑定被拒绝；`close` 先停止原控制对象，再报错，不误断开替换进来的另一条连接。这防止可信适配器把撤权信号和实际连接接错，不是对恶意可信宿主的安全隔离。

运行期临时分享、原输入最后交付检查、输出独立生命周期、最终内容 guard 和取消边界继续遵循 [依赖路由说明](PLUGIN_DEPENDENCIES.md)。已成功提交的内容保留；后来撤权可以阻止旧提案继续取得回执或提交，不能撤回此前已经复制的字节。

## 验证入口与当前范围

真实 Rust 资格程序 `plugin_runtime/examples/qualify_locked_dependency.rs` 接收分别编译且字节不同的 A、B Wasm 模块。它使用临时包目录、Registry 和 Store，要求按以下流程检查：

1. 安装并选择双方包，批准调用者内容能力；必需锁尚未批准时，调用者连接失败。
2. 批准精确双方摘要，释放并重开 Manager，核对 revision 和锁；创建新实例及本次具体内容／共享授权。
3. 实际执行 A 将 `abc` 转为 `ABC`，通过锁建立的路由调用实际 B，取得 `B:CBA`；使用原核心内容提案提交并核对幂等回执。
4. 升级提供者，核对双方旧实例撤销、旧锁移除、旧提案拒绝。只启用新提供者仍不足以重新启动调用者。
5. 显式重新批准后创建新实例和新路由；旧调用者不能替换新端点。检查已提交内容保留、没有重复内容事件，释放实际映射并检查 Store 完整性。

此示例目前重开的是依赖 Registry；不应将它表述成进程崩溃恢复或完整内容数据库重放。测试文件 `core/tests/package_dependencies.rs`、`core/tests/dependency_registry.rs`、`plugin_runtime/tests/managed_dependencies.rs` 分别覆盖声明、持久锁与管理器集成边界；局部 Wasm fixture 的通过不能替代实际双 Rust 执行。

从仓库根目录复现真实资格的入口如下，命令不是已通过记录：

```powershell
cargo build --manifest-path sdk/examples/rust-transform/Cargo.toml --target wasm32-unknown-unknown --release --target-dir build/locked-dependency-qualification/a
cargo build --manifest-path sdk/examples/rust-chain-provider/Cargo.toml --target wasm32-unknown-unknown --release --target-dir build/locked-dependency-qualification/b
cargo run --manifest-path plugin_runtime/Cargo.toml --features packages --example qualify_locked_dependency --target-dir build/locked-dependency-qualification/host -- build/locked-dependency-qualification/a/wasm32-unknown-unknown/release/morrow_example_transform.wasm build/locked-dependency-qualification/b/wasm32-unknown-unknown/release/morrow_example_chain_provider.wasm
```

本阶段不包含动态 guest 调用协议、自动拓扑启动、自动崩溃传播、完整依赖证据审计与重放、默认工作台依赖配置页。持久选择属于原生 Registry；Windows 映射路径的资格也不等于全平台插件依赖系统验收。
