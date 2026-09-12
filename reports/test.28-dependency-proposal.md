# test.28：显式插件依赖与授权结果提案

日期：2026-09-13。应用 `0.1.9-test.28+33`，审计／工作台宿主 test.28；核心仍为 test.22（内容格式6），执行后端和SDK仍为 test.11，既有工作台guest仍为 test.13，随包清单 test.28。新增独立Rust提供者示例 test.28。第一方 AGPL-3.0-only；仅本地开发，没有推送、标签、Release 或上传。

## 本轮变化

- 新增 Dependency：可信宿主按真实 HostRuntime、双方连接代次、包ID／版本／摘要、注册处理器、类型、scope及期限显式绑定单级调用。连接被撤销或替换后旧依赖不可复活，复制包名或使用同包新实例均不能替代原连接。
- A 已获授权的固定 Mapping 通过临时租约交付 B，只允许纯Wasm转换；B没有从路由获得内容能力。临时授权及映射使用RAII释放，成功、协议失败、取消、拒绝和额度失败均保留原调用者输入所有权，不错误退休原对象。
- 入场先做不改变时钟的身份、真实Mapping、scope、原生产者状态及大小检查；实际分享授权时再次检查原输入租约。结果交付用同一个最终时间值核验输入与路由，避免两次取时导致短输入租约过期后仍交付。
- 不可直接构造的 DependencyOutput 保留路由及双方撤权信号。路由撤销或Drop、双方实例撤权、结果期限到期均使后续提案失效。原输入租约只约束执行和首次输出交付；已经交付的输出有独立路由生命周期，不能宣称退休输入可收回既有字节。
- 新增 EditProposal：卡片ID、预期修订、操作ID、标题、预览与接受类型由可信工作流给出，插件输出只填正文，不解码为命令。提交继续使用唯一核心、原内容能力及具体卡片授权，保留附件与修订冲突检查。
- 核心 edit_content_guarded 将附加约束放入每次 Store 授权回调，覆盖最终提交和幂等回执交付。guard与原授权使用同一真实时钟值，不能替代内容权限；提供者或依赖在最终边界撤权时事务拒绝，已成功提交的记录不回滚。

三个子代理分别实现依赖路由、核心guard与文档、独立权限和提案回归；主代理实现提案接口、真实双Rust模块资格、集成与Windows验收。没有增加原生unsafe范围，也没有改变guest ABI v2、任务契约v3、运行期v7或持久内容格式。

接口与边界见 [依赖设计](../docs/PLUGIN_DEPENDENCIES.md)。

## 实际验证

| 范围 | 结果 |
| --- | --- |
| 核心提交约束 | 新增6项与原内容修改4项通过；涵盖最后guard拒绝、幂等重复授权、原权限不可替代、guard触发撤权、真实时钟及跨线程单向撤权；核心严格Clippy和Wasm库编译通过 |
| 依赖与提案 | 16项通过；真实Wasm逐字节检查完整Invocation，错误包／同包替换实例／宿主／scope、短原租约分享与交付到期、最终撤权、临时map额度失败、未来时间污染、提案权限／类型／修订／幂等／已提交保留均覆盖 |
| 执行底座 | 全特性104项通过（包含上述16项及既有3个子进程入口），全目标严格Clippy通过；0个文档测试示例 |
| 工作台宿主 | 18项通过，覆盖自动封存、启动恢复、资料读取、插件管理失败、升级停用及在线UI |
| Flutter实际宿主 | 5项通过，覆盖内容与偏好、管理库恢复及在线文字工具；本轮未重复整个82项FlutterUI套件 |
| 两个独立Rust插件 | 最终代码下实际A将abc变成ABC，宿主固定后实际B生成B:CBA；原输入缓冲改动不改变执行内容；A显式提案提交到修订2，重复提交同一回执，撤权后拒绝重交；关闭重开核心库后结果保留，只有seed＋edit两条内容事件 |

独立审查修复了两处时间边界：外来broker或错误scope不得先推进合法路由时钟；原输入租约比路由短时，最终交付必须使用同一次时间样本校验两者。随后补齐实际分享授权时的原租约检查。对应独立用例与父端全量回归均通过，没有用恒定时钟成功示例替代这些边界验证。

Windows Release 构建及实际运行已通过。最终证据目录为 `build/workbench-host/test28-runtime-9a370949079c488eae66540468909463/`，含result.md、result.png、日志与全新合成资料；产品版本 `0.1.9-test.28+33`，活动库登记存在，进程退出码0。实际程序完成工作台渲染、原生背景API、静音WAV解码／时钟／跳转、声音互斥与恢复不自动播放检查。此检查不代表新依赖流程已经接入UI，也不是背景像素比较或新物理交互验证。

包内宿主与最终独立Release宿主SHA-256均为 `4ea8c22feca86a212a8d495854d49aa8e71895bc55c8a1d870310b756d1633dd`；随包插件为 `f74fabbc27886d14556c761d149d213f21b42eb58dceb1ee0f3aed1c13b2543b`。实际程序位于 `build/windows/x64/runner/Release/morrow_studio.exe`，运行需保留整个Release目录。

真实资格使用两个字节内容不同、分别编译的Rust模块，宿主没有手写中间转换结果：

- A：`morrow_example_transform.wasm`，SHA-256 `6946d9a7e24389fbe2b826b890f10ab59054e453370b478889106fbb8b2122b0`。
- B：`morrow_example_chain_provider.wasm`，SHA-256 `1a53d73ecc1e6720f47893c3d0fcecd544133762254689cedcecfa5073ef9c91`。

主要复现（仓库根目录）：

```powershell
cargo test --offline --locked --manifest-path core/Cargo.toml --test content_guard --target-dir build/content-guard
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --all-features --target-dir build/dependency-parent
cargo build --offline --locked --manifest-path sdk/examples/rust-transform/Cargo.toml --release --target wasm32-unknown-unknown --target-dir build/chain-guests
cargo build --offline --locked --manifest-path sdk/examples/rust-chain-provider/Cargo.toml --release --target wasm32-unknown-unknown --target-dir build/chain-guests
cargo run --offline --locked --manifest-path plugin_runtime/Cargo.toml --features packages --example qualify_dependency --target-dir build/dependency-parent -- build/chain-guests/wasm32-unknown-unknown/release/morrow_example_transform.wasm build/chain-guests/wasm32-unknown-unknown/release/morrow_example_chain_provider.wasm
```

## 尚未完成

这是可复用的宿主显式单级路由和结果提案，尚未接入默认工作台UI。调用A、选择B、批准路由与提交目标仍由可信宿主工作流决定；尚无guest动态嵌套调用、依赖清单／持久锁文件、全局依赖图、自动版本协商、跨进程Wasm执行或三语言通用依赖SDK。

现有核心事件证明内容事务，不包含完整A/B包、输入、依赖及执行事实。完整宿主审计、证据留存、隔离重放、持久恢复、全局资源限制、真实旧资料迁移及各平台验收仍待推进。Windows映射和核心Wasm编译不代表其他平台共享后端完成。附加guard最后通过后与实际数据库提交之间仍存在已接纳操作区间，不宣称与外部撤权实现分布式原子事务。

M3–M7和0.2.0完整退出门槛保持未完成。
