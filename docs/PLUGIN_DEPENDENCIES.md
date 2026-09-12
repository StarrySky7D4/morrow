# 宿主路由的插件依赖与内容提案

本页描述 0.1.9-test.28 的阶段性底座：可信宿主把调用者已经获准读取的冻结输入，临时交给一个确定的提供者执行纯转换，再将返回值放入显式内容提案。对应实现为 `plugin_runtime/src/dependency.rs`、`proposal.rs`，资格入口为 `plugin_runtime/examples/qualify_dependency.rs`。

这是宿主显式选择的单级路由。现阶段没有持久依赖锁文件、guest 动态发现或嵌套调用、默认工作台 UI 接入，也没有完整的依赖证据审计与确定性重放。下文记录接口约束和资格步骤，不预先声明整轮测试通过。

## 接口与身份

这些接口属于启用 `packages` 的原生 Rust 宿主库，不是 guest 可以调用的授权接口。

| API | 作用与约束 |
| --- | --- |
| `Endpoint { package, connection }` | 同时提供实际 `PreparedPackage` 和实际 `Connection`；只传包 ID 不足以建立身份。 |
| `Spec { handler, input_type, output_type, scope, expires }` | 可信宿主选定一个处理器、精确类型、共享范围与绝对期限。 |
| `Dependency::bind(host, caller, provider, spec, now)` | 为当前宿主及两个确定连接建立临时路由；固定双方包 ID、版本、摘要和连接身份。 |
| `Dependency::run(objects, host, caller, provider, input, task_id, clock, cancel)` | 接纳现有 `Mapping`，临时授权提供者，执行实际 Wasm 任务并校验结果；同步返回 `DependencyOutput` 或错误。 |
| `Dependency::revoke()` / `Drop` | 单向撤销路由，后续执行和保留结果的授权校验失效；不撤回已复制的字节或已提交事务。 |
| `DependencyOutput::bytes()` / `output_type()` / `input_descriptor()` | 读取拥有的输出与原输入描述符；描述符仅供关联，不是新的读取授权。 |
| `DependencyOutput::validate(host, caller, now)` | 校验原宿主、原调用者、双方就绪状态及路由存活状态。 |
| `DependencyOutput::validate_liveness(now)` | 检查路由和双方撤销信号、期限及单调时钟；可在核心最终授权回调内调用，不需要再次借用宿主。 |
| `EditProposal::new(output, target)` / `commit(host, caller, clock)` | 将真实输出绑定到宿主选择的修改目标；经核心原有内容授权和额外 guard 后提交。 |

`bind` 要求双方为不同的实际连接且均处于 Ready，连接中绑定的摘要必须对应所给包；提供者须使用 guest ABI v2，且注册了精确匹配的 Transform 处理器及输入／输出类型。`scope` 非空、最多 256 字节且不含控制字符，`expires` 必须晚于当前时间。以后再次传入同包的新连接、其他宿主、不同版本或摘要，都不能替代最初绑定的端点。

## 授权与执行顺序

包的能力声明、宿主批准的能力上限、当前连接状态、具体内容授权与共享对象租约是不同的检查。建立依赖不会自动授予 `ReadContent` 或 `EditContent`，也不会从调用者向提供者继承这些能力。宿主若从卡片取得原始内容，仍先通过核心读取授权；本接口只接收已经发布并持有有效租约的输入。

一次调用遵循以下边界：

1. 读取宿主时钟，检查固定端点、路由存活状态，以及调用者输入 Mapping 的真实租约、分配对象、范围和发布者状态，并检查输入长度不超过提供者声明及任务层上限。只有预检通过后，才推进路由与共享对象管理器的时钟。无效或不属于该范围的 Mapping 不能利用一次较大时间值污染后续合法调用的时钟。
2. 在实际共享授权时重新取时钟，复核端点、路由与原调用者输入访问，防止原租约在接纳后、共享前到期。随后以固定范围与路由期限，为提供者创建临时 lease 并映射同一冻结分配；临时 lease 不是调用者原租约的转移。
3. 通过 `SharedObjects::run_transform` 与真实 `PreparedPackage` 执行提供者任务。处理器、类型与配额沿用包声明和已有任务校验；原生映射复制到 guest 线性内存，不向 guest 交付原生指针。
4. 执行结束后，用新的时钟值重新检查路由、双方端点和原调用者输入租约。即使提供者已经算出结果，原租约过期或撤销、输入退休、连接失效或路由失效仍会阻止交付。
5. 只接纳无核心调用、无核心响应、成功退出且符合固定输出类型的纯转换结果。执行故障、业务失败、协议异常或授权失败分别返回错误；不会把任意返回字节直接当作内容修改。
6. 交付拥有独立字节缓冲区的 `DependencyOutput`，释放临时提供者访问。

时钟来自可信宿主 `FnMut() -> u64`，期限比较使用 `now >= expires`，倒退时间拒绝；guest 不负责提供有效时间。上述无副作用预检专指无效输入的接纳边界，不意味着所有失败操作都会回滚已经合法观察到的时间。取消与执行期限沿用现有任务后端；此接口不增加队列、自动重试或多级调度器。

## 临时访问与输出生命周期

临时资源在建立 lease 后立即由 `TemporaryLease` 托管。成功返回、映射失败、任务失败、取消或最终校验失败的正常 Rust 返回路径，都会先撤销该临时 lease，再释放临时 Mapping。它只清理本次提供者访问，原调用者的 lease 与 Mapping 仍由原所有者管理。撤销限制后续授权，不能令此前已经取得的只读借用或复制字节自动消失；进程崩溃也不依赖 Rust `Drop` 执行。

输出与输入具有独立生命周期：

- 接纳、实际共享及结果交付时必须仍能授权访问原输入；提供者临时 lease 的期限不能替代这些检查。
- 成功交付后，输出保留路由状态、输入描述符和独立 `Vec<u8>`，不保留原 lease 或 Mapping。
- 随后仅释放原 lease、退休原输入或释放其映射，不会单独使已签发输出失效。输入描述符是关联信息，不是持续占有输入的承诺。
- 输出仍受原宿主／调用者身份、双方连接状态、双方撤销信号、路由撤销和期限约束。宿主应保留 `Dependency` 直到预览及提交结束；提前 drop 路由会使保留的提案拒绝提交。
- 输出的普通字节副本无法被远程撤回。调用者不能以“已经拿到 bytes”为由跳过后续授权。

底层冻结映射目前由 Windows 原生实现提供；其他原生平台返回 Unsupported。此依赖路径使用同宿主管理的映射并向 Wasm 复制输入，没有在两个插件进程之间实现零复制句柄交付。独立进程 reader 的能力不等于此依赖路由已经接入跨进程插件执行。

## 内容提案与最终 guard

`EditTarget` 中的操作 ID、卡片 ID、预期修订、标题、预览与接受的输出类型全部由可信宿主流程决定。guest 输出只贡献正文，不能选择修改对象、扩展权限或伪造事务命令。`EditProposal::new` 检查输出类型和 `ContentChange`，附件设为 `None`，沿用现有内容修改规则。

`commit` 首先验证输出的宿主、调用者与当前端点，然后进入 `HostRuntime::edit_content_guarded`。每次 Store 授权回调都读取一次真实时钟，同一个值同时用于附加 guard 与原有 `EditContent` 范围授权。附加 guard 检查提供者、调用者及路由撤销信号与期限，不替换包能力上限、连接检查或内容授权。

回调覆盖事务接纳、最终提交前检查以及幂等回执交付。最终 guard 拒绝时，本次事务不提交；重复请求同样需要授权，不能通过已有回执绕过撤权。已成功提交的事务不会因为之后撤权而回滚。固定操作 ID 用于显式幂等重试，预期修订仍防止覆盖较新的内容；提案不会自动重跑 guest。只有核心返回的事务回执表示提交成功，插件输出或 UI 预览成功均不表示内容已保存。

该 guard 通过既有撤销信号和事务边界检查状态，不宣称撤销发生后可以追溯撤回已经提交的结果，也不建立独立的分布式撤权事务。

## 真实双 Rust 资格流程

`qualify_dependency` 必须接收两个分别编译、字节内容不同的真实 Rust Wasm 模块。当前示例使用 `sdk/examples/rust-transform` 的 A 与 `sdk/examples/rust-chain-provider` 的 B，宿主分别构造独立包和连接；不以手工拼接阶段输出替代实际执行。

资格程序要求验证以下流程：

1. 在临时 Store 创建正文为 `abc` 的卡片，调用者取得显式读取／编辑批准及具体卡片授权；提供者没有内容能力。
2. 冻结输入后修改原生产者缓冲，实际 A 仍将原字节转换为 `ABC`。
3. 宿主发布 A 的实际输出并取得调用者 Mapping，显式路由到实际 B，得到 `B:CBA`；返回的输入描述符对应中间对象。
4. 提供者临时访问释放，资源用量回到调用前；创建提案前卡片正文仍为 `abc`。
5. 宿主选择卡片和修订，显式提交后得到修订 2；重复提交得到相同回执，不新增内容事件。
6. 撤销路由后，保留提案再次提交被拒绝；此前提交的结果保留。
7. 退休输入并释放真实映射后，映射计费归零；关闭并重开 Store，检查持久结果及完整性。

独立 `plugin_runtime/tests/dependency.rs` 与 `core/tests/content_guard.rs` 用于检查身份替换、范围、期限、撤权、错误清理、原租约与输出生命周期、内容权限、修订冲突、幂等回执及最终提交拒绝。测试用局部 Wasm fixture 的通过不能替代上述双 Rust 资格。

以下是从仓库根目录运行的复现入口，要求已有本地 Rust／Cap’n Proto 工具链和 `wasm32-unknown-unknown` target；命令本身不是执行成功记录：

```powershell
cargo build --manifest-path sdk/examples/rust-transform/Cargo.toml --target wasm32-unknown-unknown --release --target-dir build/dependency-qualification/a
cargo build --manifest-path sdk/examples/rust-chain-provider/Cargo.toml --target wasm32-unknown-unknown --release --target-dir build/dependency-qualification/b
cargo run --manifest-path plugin_runtime/Cargo.toml --features packages --example qualify_dependency --target-dir build/dependency-qualification/host -- build/dependency-qualification/a/wasm32-unknown-unknown/release/morrow_example_transform.wasm build/dependency-qualification/b/wasm32-unknown-unknown/release/morrow_example_chain_provider.wasm
cargo test --manifest-path plugin_runtime/Cargo.toml --features packages --test dependency --target-dir build/dependency-qualification/host
cargo test --manifest-path core/Cargo.toml --test content_guard --target-dir build/dependency-qualification/core
```

本页不登记尚未完成的整轮通过数量。真实资格成功只能证明该 Windows 本地示例及被检查的边界；不能推广为全平台验收、默认 UI 接入、任意第三方依赖兼容性或完整插件系统完成。

## 后续边界

后续仍需设计持久依赖声明与锁定、版本选择和升级撤销、宿主显式批准流程、guest 调用请求协议与有界嵌套、故障恢复、完整输入／输出证据链及独立重放。当前普通核心事件和 Store 完整性检查没有记录完整依赖链证据，不能称为依赖审计或确定性重放。默认工作台及托管表单也尚未自动接入本提案流程。
