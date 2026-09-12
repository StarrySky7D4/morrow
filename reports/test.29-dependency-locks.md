# test.29：依赖声明、同快照批准锁与管理器恢复

日期：2026-09-13。应用 `0.1.9-test.29+34`，审计／工作台宿主 test.29；核心crate仍为test.22（内容格式6），执行后端／SDK仍为test.11，默认工作台guest仍为test.13，随包清单test.29。第一方AGPL-3.0-only，仅本地开发，无推送、标签、Release或上传。

## 实际变化

- Manifest字段16新增最多16个依赖声明：唯一slot、处理器、输入／输出类型、提供者包SemVer范围及optional。使用guest ABI v2，有实际依赖必须声明必需功能dependencies-v1；未知功能、重复slot、畸形标识／版本范围均拒绝。声明只描述所需契约，不授予权限或自行指定已批准提供者。
- 可信宿主将caller摘要、slot、provider身份及摘要批准为LockedDependency。锁和包选择、启用状态、能力批准共用既有selection.morrow及revision，规范Protobuf＋LZ4同次原子发布，不另建第二份持久权威。旧无锁快照兼容；损坏、未知语义、错摘要和非规范数据拒绝打开且不重写。
- 全部必需依赖逐次检查已批准锁、当前摘要、声明契约、版本及启用闭包；缺少必需依赖拒绝连接，可选缺失不阻止caller。纯必需启动图以迭代算法拒环，避免依赖深度变成Rust递归深度；可选／混合环不因此当作强启动环拒绝。
- 升级和移除包时，同一次快照更新清除源自或指向该包的锁；停用保留配置。Manager在变更前读取旧图，先撤销传递必需消费者及目标实例，再持久发布。保存失败不前移登记内存，旧实例保持已停止；过期revision／摘要确认在撤权前拒绝。
- Manager.bind_locked_dependency只接受本管理器登记、摘要匹配且仍活跃的真实实例，按包声明派生运行接口。重启恢复的是批准选择；实际HostRuntime、实例、scope、期限和对象租约必须新建，文件中没有可复活的运行句柄。
- 审查修复了既有ManagedInstance的控制对象与连接误配风险：Control钉定最初ConnectionBinding。即使可信适配器通过parts_mut错误交换同包连接，owns／run／run_task／close均拒绝错配；close仍停止原控制对象，不错误断开被换入的另一个连接。该保证针对可信接口误配，不宣称隔离能任意操作宿主的代码。

接口细节见 [依赖锁设计](../docs/PLUGIN_DEPENDENCY_LOCKS.md)。三位子代理分别完成包声明与审查文档、登记持久层、独立管理器回归；主代理完成Manager接线、连接钉定修复、真实双Rust恢复资格与应用集成。

## 验证

| 范围 | 本轮结果 |
| --- | --- |
| 包声明 | 新增10项与既有包11项通过；feature／ABI、可选声明、SemVer范围、类型和slot／容量边界覆盖 |
| 登记持久层 | 新增12项、既有登记10项、规范格式单元2项通过；实际1024锁重开、第1025条原子拒绝、required环、optional／混合环、共享子图、升级双向清锁、CAS及真实保存失败覆盖 |
| 坏快照 | 未知字段（含嵌套）、错误哈希、尾字节、排序／摘要错误与伪造必需环拒绝打开，不改写原文件 |
| 管理器依赖 | 新增12项通过：持久锁重开、provider四类变更、三级必需链、可选消费者、保存失败、stale确认、外来Manager同包实例、锁移除／重批、最终提交时撤权，以及两项同包连接交换回归 |
| 执行底座全量 | 全特性116项通过（含新增12项与既有3个子进程入口），全目标严格Clippy通过；0个文档测试示例 |
| 核心可移植检查 | Windows全目标严格Clippy通过；声明扩展后Wasm库编译通过，非Web事务／浏览器运行证明 |
| 实际工作台 | 宿主18项、Flutter／真实宿主集成5项通过；覆盖封存、恢复、内容与偏好、插件启停／升级及在线工具。本轮未重复整个82项FlutterUI套件 |
| 两个Rust插件 | 实际不同模块A／B在登记重开后按批准锁重新绑定，ABC→B:CBA并授权编辑到修订2；重复提交同回执；提供者升级撤销旧实例／提案并清锁，重新启用本身仍不能恢复依赖，显式重批和新实例后才能绑定 |

真实资格为 `qualify_locked_dependency`：A使用既有rust-transform，B使用独立rust-chain-provider，实际模块字节不同。宿主明确批准A的内容读取／编辑上限并给具体卡片授权；B无内容能力。提供者1.1.0使用同一真实模块重新封装成新版本包，验证不可变包升级和重批路径，未声称算法在升级中变化。所有数据在新临时目录创建，未读取真实用户库。

A模块SHA-256为 `6946d9a7e24389fbe2b826b890f10ab59054e453370b478889106fbb8b2122b0`，B为 `1a53d73ecc1e6720f47893c3d0fcecd544133762254689cedcecfa5073ef9c91`，沿用test.28分别编译的真实模块，依赖声明与批准在本轮新包／登记中实测。

Windows Release 构建和实际运行均通过。最终证据目录 `build/workbench-host/test29-runtime-da14567028f24413b12895aae502289d/` 包含result.md、result.png、日志和全新合成资料；产品版本 `0.1.9-test.29+34`，活动库登记存在，退出码0。实际核对工作台渲染、原生背景API、静音WAV解码／时钟／跳转、声音互斥与恢复不自动播放。此结果不表示默认界面已有依赖配置，也不是背景像素比较或新物理交互测试。

最终独立宿主与包内宿主SHA-256均为 `c582dbb837daeabaf5d04591632bbfc4f036917461cb3128b9c82c16361603e9`，随包插件为 `fe76fb5fd50525619a5a8178b585e4f6785edab8f844bff7184b64f1c7ea0ae6`。程序为 `build/windows/x64/runner/Release/morrow_studio.exe`，运行需保留整个Release目录。

主要复现（仓库根目录）：

```powershell
cargo test --offline --locked --manifest-path core/Cargo.toml --test package_dependencies --test dependency_registry --target-dir build/dependency-registry
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --all-features --target-dir build/dependency-parent
cargo run --offline --locked --manifest-path plugin_runtime/Cargo.toml --features packages --example qualify_locked_dependency --target-dir build/dependency-parent -- build/chain-guests/wasm32-unknown-unknown/release/morrow_example_transform.wasm build/chain-guests/wasm32-unknown-unknown/release/morrow_example_chain_provider.wasm
```

## 仍需推进

默认工作台没有依赖配置界面，也不会自动按图启动提供者。当前解析只检查已选包／批准与契约，随后由可信宿主建立真实实例；通用版本化服务接口、动态guest请求及C／C++／Rust调用SDK、有界嵌套／等待环、运行实例崩溃传播与恢复预算仍未完成。控制变更的传递撤权不等于所有进程故障的自动传播。

锁是当前批准配置，不是完整批准历史、签名审计或执行重放证据；不保存scope、对象授权、期限与连接身份。普通核心事件继续证明内容提交，完整输入／程序／依赖事实、封存、独立验证与隔离重放仍需贯通。1024锁容量测试不证明大规模响应时间、全局RSS或全平台性能。

同账户协作独占和文件原子替换沿用既有边界，不构成跨账户授权、抗回滚或恶意宿主隔离。内容库、任务v3、guest ABI v2、运行期v7及获批unsafe范围未改变。M3–M7和0.2.0退出门槛保持未完成。
