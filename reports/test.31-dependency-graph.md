# test.31：有界多层依赖执行

日期：2026-09-13。应用`0.1.9-test.31+36`，审计／工作台宿主test.31；核心crate仍为test.22、内容格式6，运行时／SDK crate仍为test.11，默认工作台guest仍为test.13，随包清单test.31。第一方AGPL-3.0-only。仅本地开发，无推送、标签、Release或上传。

## 本轮实现

新增可信`run_graph`入口，在现有Manager批准锁与固定依赖调用协议上逐层解析真实提供者。根调用者深度0，默认最大边深度4、全图16次尝试，硬上限8／64；每个Invocation原有局部调用额度仍独立保留。活动包ID栈拒绝同包重入，即使使用同包另一个实例；完成的分支退出活动栈后，可以顺序复用同一提供者。

每条边沿用Dependency源授权与provider临时映射流程。仅crate内部执行回调允许真实嵌套任务；原公开Dependency.run仍要求零导入计数，原dynamic_dependencies::run保留单层语义。共同交付路径额外复核provider临时租约和注册输出大小，不能因为改用嵌套执行回调而漏掉原约束。

根RoutedOutput保留所有真实父子边及节点取消信号。中间边验证自己的原始父／子连接与宿主，根调用者单独验证；保存提案在核心最终guard统一检查完整集合。深层失败不产生部分结果，临时对象／映射随作用域回收。已提交数据不会因后续撤权回滚。

接口与限制见 [图执行设计](../docs/PLUGIN_DEPENDENCY_GRAPH.md)。未修改运行期协议、guest ABI、SDK契约或获批unsafe范围；没有新增JS／TS插件或Dart插件要求。

## 测试与审查

- 既有依赖16项和单层动态调用11项先行回归通过。最终全特性Runtime全量150项通过，原始日志`build/test31-runtime.log`；全目标严格Clippy通过。
- 新增多层图测试最初12项通过，逐字核对每个实际Wasm Invocation和相关响应：三节点链、顺序diamond复用、边深度／全图计数、配置硬上限、可选依赖环、同包不同实例重入、局部重复call ID、深层陷阱／输出超限、叶子停止与最终交付／提交检查、提交后不回滚、旧单层入口不放宽。
- 另追加2项实际运行上界测试：8条边链成功，第9条边拒绝；根8分支且每分支7次共享叶子调用共64条边成功，第65条边拒绝。没有调大原guest内存／fuel，所有请求与响应仍逐字验证，失败后资源归零。最终图测试共14项，运行时唯一测试总数152（全套150项加上述追加2项）。
- 宿主18项、Flutter／真实宿主集成5项通过。本轮没有UI源代码变化，未重复完整旧FlutterUI套件或将test.30的analyze结果冒充本轮新增结果。
- 独立审查确认跨层scope、输入冻结、映射授权、计数、活动包拒环和失败清理。审查建议保留所有节点的取消信号，已实现；公开leaf.stop的取消＋撤权已实测，内部单独cancel token未通过integration API独立触发，不将其声称为独立运行测试。
- 初次Cargo检查在新example文件尚未落盘时被清单引用阻断；暂时移除该声明，文件完成后恢复。一次全目标Clippy恰逢独立测试夹具只写了一半而报告未使用项，待测试完成后通过；没有以allow屏蔽警告。

## 真实三包与三语言根调用者

新增Rust中间插件`rust-graph-middle`，使用同一SDK调用slot leaf。Rust根A、中间M、叶B均为不同实际Wasm模块：输入`abc`→A大写为`ABC`→M调用叶子→叶子返回`B:CBA`→M返回`M[B:CBA]`→A返回`A[M[B:CBA]]`。

同一资格入口还实际运行了test.30的C、C++根调用者与上述Rust中间／叶子插件，两种混合语言链全部通过。不是仅原生编译或协议mock。

各链均核对持久批准锁重开、每图2次实际调用、临时objects／leases／mappings／charged_bytes全0、转换完成尚未修改内容；提案只把修订1改为2，重复提交返回同一回执。保留另一未提交提案后升级叶包，根／中间／叶旧实例均撤销，已提交和未提交旧提案都不能再写入；原提交正文和两个事件保持，重开Store仍一致。升级沿用同一叶Wasm的新包版本，不证明新算法行为。

本轮中间Wasm：`build/graph-guests/wasm32-unknown-unknown/release/morrow_example_graph_middle.wasm`，SHA-256 `cd4ea09383df499ccab6e0a6568d7b14f0b4f99bb68a99532920377a0ef0c0f1`。中间插件release编译及Wasm严格Clippy通过。根Rust／C／C++与叶模块复用 [test.30已记录产物](test.30-dynamic-dependencies.md)，实际路径分别传入本轮资格执行。

## Windows最终产物

Windows Release已编译并运行实际程序自检。证据目录：`build/workbench-host/test31-final-9ea2a24644714b579c4149edc455d174/`。产品版本`0.1.9-test.31+36`，进程退出0，活动库登记存在。新合成资料自检通过实际工作台渲染、原生背景API、静音WAV解码与时钟、跳转／声音互斥／恢复不自动播放。未单独重做截图像素分析，也不宣称默认界面已接入多层依赖配置。

- `build/windows/x64/runner/Release/morrow_studio.exe` SHA-256：`67a84bd8e2d5c99fcc22024c312670113ce6ad5c5ef81a77fff66d8e821a6b96`。运行须保留整个Release目录。
- 独立与包内`morrow-workbench-host.exe` SHA-256均为`038c41ae556fc403650d061d312a280126508becccdc3b3722cb110b36cd3d0d`。
- 包内`plugins/workbench.morrowplugin` SHA-256为`772d3c60bf9a06e6b7dd47ee07280ae586723d2932326ad0b1428c0242266704`。

## 复现与剩余门槛

```powershell
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --features packages --test dependency_graph --target-dir build/graph-review
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --all-features --target-dir build/dependency-parent
cargo build --offline --locked --manifest-path sdk/examples/rust-graph-middle/Cargo.toml --target wasm32-unknown-unknown --release --target-dir build/graph-guests
cargo run --offline --locked --manifest-path plugin_runtime/Cargo.toml --features packages --example qualify_dependency_graph --target-dir build/dependency-parent -- build/chain-guests/wasm32-unknown-unknown/release/morrow_example_dependency_caller.wasm build/graph-guests/wasm32-unknown-unknown/release/morrow_example_graph_middle.wasm build/chain-guests/wasm32-unknown-unknown/release/morrow_example_chain_provider.wasm
```

用test.30的C／C++根路径替换资格命令第一个Wasm即可复验混合语言链。根和叶构建方法见test.30记录。

同步活动栈拒环不等于完整跨Worker等待图、事件循环收敛或异步调度。自动实例启动、崩溃传播、故障重绑定与恢复预算、通用服务接口、默认UI、多任务全局额度、其他平台应用接入、完整审计／证据封存和隔离重放仍未完成。M3–M7及0.2.0退出门槛保持未完成，不用该阶段测试数量替代系统完成证明。
