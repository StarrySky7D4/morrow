# test.30：插件主动调用、三语言SDK与聚合结果提交

日期：2026-09-13。应用`0.1.9-test.30+35`，审计／工作台宿主test.30；核心crate仍为test.22、内容格式6，执行后端／SDK crate仍为test.11，默认工作台guest仍为test.13，随包清单test.30。第一方AGPL-3.0-only。仅本地开发，无推送、标签、Release或上传。

## 实际交付

1. 新增固定Cap’n Proto依赖请求／响应v1。请求只携带call ID、声明slot和输入，响应绑定完整实际请求帧SHA-256，不从字段重编码替代。核心与Rust SDK独立编解码；C接口和C++所有权封装验证相同原帧。
2. 包必须显式声明`dependency-calls-v1`并提供Manifest字段17协议摘要，且同时具备任务和转换／依赖特性。普通Runner拒绝新import，新模式必须有可信依赖回调。
3. Manager路由当前批准锁和显式提供的真实实例。零次调用仍验证调用者Manager归属、原连接与启用状态。每次调用冻结输入、临时授予映射、运行提供者、返回相关响应；所有失败路径回收本次发布对象和映射。
4. `RoutedOutput`保留调用者最终加工结果及所有使用过的依赖证明，进入EditProposal后仍在最终核心提交时核对完整依赖集合、调用者、取消与期限。SDK输出不直接成为内容命令，卡片目标、操作ID、预期修订与内容权限仍归可信宿主／核心。
5. C、C++、Rust各有真实发起依赖请求的Wasm示例；同一个Rust提供者返回数据后，各调用者继续加工，再经核心内容授权提交。

当前是单层同步纯转换，默认8次，可信策略可设置1..16次且仍受包host_calls上限约束；输入1..64KiB，成功输出0..64KiB，帧128KiB。递归provider、新WASI或任意import不被接受。每个guest分别消耗其fuel预算，不宣称整链共用一份fuel。详细设计见 [主动调用](../docs/PLUGIN_DYNAMIC_DEPENDENCIES.md)。

## 发现与修正

- 独立实际Wasm测试发现：被拒绝的普通内容exchange返回码可被guest忽略，原实现仍接受最终成功输出。未产生实际内容写入。现新增不可清除的失败标记，拒绝后续依赖调用和最终输出；测试同时核对正文、事件、操作回执不变。
- 独立协议审查发现：C响应接口最初从请求描述符重建帧，无法正确验证合法但布局不同的实际请求。现传递原始请求字节与长度，C/C++ transport保存并使用实际发送帧。新增含合法未使用段字的回归，原帧通过、重编码替代帧拒绝。
- 初次新增包特性测试使用无Wasm头的夹具，2项测试失败；已改为合法最小Wasm头后通过。首次Windows自检后的摘要采集写错插件子目录；实际程序已退出0，改为`Release/plugins/workbench.morrowplugin`后读取成功。这些修正不作为产品功能缺陷统计。

## 验证证据

| 范围 | 实际结果 |
| --- | --- |
| 固定协议 | 核心9项＋独立SDK9项通过；覆盖版本、schema、UTF8、非对齐输入、大小、尾部、未知指针环／别名放大，以及完整原请求关联 |
| 包特性与原依赖锁 | 新增2项、既有声明10项、注册表12项通过 |
| Runner导入边界 | 新增11项通过，覆盖模式、顺序、完整输出区、重叠、取消、返回长度与共享导入预算 |
| Managed动态路由 | 新增11组真实Wasm通过，逐字核对Invocation及相关响应；覆盖0调用绑定、两提供者全链撤权、默认8／显式9／17拒绝、重复ID、错误实例、取消、失败清理、递归特性拒绝、最终提交guard与提交后不回滚 |
| Runtime全量 | 全特性138项通过，包含上述新增22项及既有子进程入口；全目标严格Clippy通过 |
| C ABI | 4项边界／所有权／实际原帧回归通过；另1项用于生成native smoke夹具，不算额外业务验证；C与C++ native smoke编译运行通过 |
| 可移植编译检查 | core与SDK全目标严格Clippy；core Wasm库与SDK wasm-guest编译通过，不是浏览器产品运行证据 |
| 工作台集成 | 真实宿主18项通过；Flutter／实际宿主5项通过；Flutter analyze无问题。未重复整个旧FlutterUI套件，本轮无新默认界面功能 |
| 三语言真实链 | Rust／C／C++调用者分别调用同一个Rust提供者，全部通过最终整合后的`qualify_dynamic_dependency` |

真实链输入为`abc`：caller大写为`ABC`，自行发起slot `reverse`请求；提供者返回`B:CBA`，caller校验响应后加工为`A[B:CBA]`。核心将修订1变为2，显式重复提交返回同一回执，只有种子创建与一次编辑两个事件；发布对象／授权／映射／计费字节归零。重开注册表恢复批准，缺必需锁拒绝连接，provider升级撤销旧实例与旧提案，重新批准后才可创建新实例。升级测试使用同一提供者Wasm的新包版本，不是新算法验证。

实际Wasm SHA-256：

| 文件 | SHA-256 |
| --- | --- |
| `build/chain-guests/wasm32-unknown-unknown/release/morrow_example_dependency_caller.wasm` | `8d27dde13213dd33df242a5ad048cc500aac644140d2071004532ed80732920e` |
| `build/dynamic-sdk/wasm/c_dependency_caller.wasm` | `525e1474599185e9f85f6a8726e4b5b6167bd7027e238c649ea345c663ad3f00` |
| `build/dynamic-sdk/wasm/cpp_dependency_caller.wasm` | `71f0218084add6ae933f6208c9450f2acc646d4735066f7d54efae71c65106be` |
| `build/chain-guests/wasm32-unknown-unknown/release/morrow_example_chain_provider.wasm` | `1a53d73ecc1e6720f47893c3d0fcecd544133762254689cedcecfa5073ef9c91` |

## Windows实际构建

Windows Release已编译。实际程序在全新合成资料目录完成工作台渲染、原生背景API、静音WAV解码与播放时钟、跳转／声音互斥／恢复不自动播放自检，进程退出0，产品版本`0.1.9-test.30+35`，活动库登记存在。本轮未做新背景像素比较，也不表示默认工作台已经提供依赖管理或调用界面。

最终证据目录：`build/workbench-host/test30-final-89c9031497744bf2b4f1fa0413f5aa47/`，包含result.md、实际窗口截图result.png、日志与全新合成资料。自检基于最终Release目录，未单独重做截图像素分析。

- 程序：`build/windows/x64/runner/Release/morrow_studio.exe`；运行须保留整个Release目录。SHA-256：`c3a4fa47288ec88090fdda5b3508f7b2de5970b3f95efb4902fe0633aa7610c5`。
- 独立宿主与包内宿主SHA-256均为`18bde2d536f13798d22ce254bbe7c544a924a0fa7ace6d55f94528dce650de29`。
- `Release/plugins/workbench.morrowplugin`的SHA-256为`80adcd24796c7c85f782b7edc90fc7fb35d3b58669ffe458095355b38ef61067`。

## 复现与下一步

```powershell
cargo test --offline --locked --manifest-path core/Cargo.toml --test dependency_call --test dependency_call_package --test package_dependencies --test dependency_registry --target-dir build/dependency-registry
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --all-features --target-dir build/dependency-parent
cargo build --offline --locked --manifest-path sdk/examples/rust-dependency-caller/Cargo.toml --target wasm32-unknown-unknown --release --target-dir build/chain-guests
# C/C++需要已有clang与WASI sysroot作为链接工具；最终guest仍不得导入WASI。
./tool/build_plugin_dependency_wasm.ps1
cargo run --offline --locked --manifest-path plugin_runtime/Cargo.toml --features packages --example qualify_dynamic_dependency --target-dir build/dependency-parent -- build/chain-guests/wasm32-unknown-unknown/release/morrow_example_dependency_caller.wasm build/chain-guests/wasm32-unknown-unknown/release/morrow_example_chain_provider.wasm
```

将上条资格命令的caller路径换为表中C／C++路径可复验对应实际guest。提供者可用`sdk/examples/rust-chain-provider/Cargo.toml`构建至同一`build/chain-guests`。

后续继续多层有界调度与等待环、自动实例启动／故障传播、通用版本化接口、默认UI接入、完整输入／程序／依赖事实与审计封存及隔离重放。其他平台共享后端、跨账户恢复／轮换、旧资料迁移与规模证据仍未完成。帧哈希关联与当前锁不是批准历史、签名执行证据或防回滚系统。M3–M7及0.2.0退出门槛保持未完成。
