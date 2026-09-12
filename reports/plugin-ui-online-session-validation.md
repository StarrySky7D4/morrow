# 在线声明式插件 UI 会话验证

2026-09-12，Windows 原生 Rust 宿主。实现位于 `plugin_runtime/src/ui_session.rs`，尚未接入主应用界面。

## 已实现

`UiSession` 将固定包摘要、实际插件连接、`core::ui::Session`、单独后台 Worker 及固定 `ui.form` / `ui.edit` 处理器关联为一个持续在线的宿主对象。初始化会核验任务 ABI 和处理器输入输出声明；generation 与 view 由可信调用者提供，不接受客体自报身份替换绑定。

宿主先按当前文档检查节点、动作、事件类型、generation、revision、serial；每次最多接纳一个在途 UI 任务。忙碌时不接纳且不消费序号。接纳之后，即使队列提交或执行失败，也通过带同一 Ticket 的 `Failed` 返回，并保留已经消费的事件序号。不会静默重放事件。结果回执与实际任务关联后再次进行 UI 文档解码，只按该任务的基线修订更新文档。失败保留原文档与修订，后续有效事件可继续。

取消会丢弃任务句柄和结果，即使后台已完成但尚未消费，也不能更新视图。原任务实际退出前继续背压，不能用取消绕过容量限制。关闭清理视图并停止工作线程；`try_finish` 只在实际退出后交回核心所有权。`execution_pending` 仅表示后台任务是否退休，与结果是否消费分开。

此通道严格使用纯转换任务，不向 guest 提供内容提交。`Update::Document` 只代表受检 UI 快照，没有“已保存”或“已提交”属性。完整内容修改仍需另一条核心授权与事务路径。

## 当前证据

- 全目标、全特性严格 Clippy 通过。
- 运行时全特性测试 **41 项通过**，其中新增在线会话边界测试 3 项：真实 WAT 客体返回非法 UI 文档、返回错误任务关联、无限循环耗尽资源；额外验证超大事件在复制及接纳前拒绝。
- 本轮从源码重新编译 Rust UI Wasm；C/C++ 使用既有构建模块，本轮没有重新编译这两个模块。
- 三种客体各在同一在线 `UiSession` / Worker 内完成 **27 个有效 UI 快照**：初始表单、24 次连续 Unicode 编辑、业务失败后恢复及最后一次编辑。
- 每种客体另通过单事件背压、4 种身份/动作/修订拒绝、按钮被接纳后返回明确业务失败、取消结果丢弃、实际完成后未消费结果丢弃、关闭后的结果丢弃验证。
- 全部事件和结果在同一进程内在线传递，没有通过磁盘阶段拼接；输入由原生资格验证程序编码，**不代表 Flutter 控件已在线接入**。
- 三语言资格验证结束后读取实际核心：原卡片标题仍为 `untouched`，outbox 仍只有初始化事件，并通过存储完整性检查。

| 模块 | SHA-256 |
| --- | --- |
| 本轮 Rust UI | `b5644fc822443c8250a46d3e8385f4ff90ec26ad6bbc88db94aea26d68e4a9ce` |
| 既有 C UI | `98e8834288755630e4715c3f656e82f1b9bc790290e7a788e29c729e66474bfa` |
| 既有 C++ UI | `369bbba75ac48b51e87481c223f11af2a05d9cc9ccf2a6e369b54b3afab3c3be` |

## 复现

```powershell
cargo build --offline --locked --manifest-path sdk/examples/rust-ui/Cargo.toml --target wasm32-unknown-unknown --release --target-dir build/plugin-ui-session/guest
cargo clippy --offline --locked --manifest-path plugin_runtime/Cargo.toml --target-dir build/plugin-ui-session --all-targets --all-features -- -D warnings
cargo test --offline --locked --manifest-path plugin_runtime/Cargo.toml --target-dir build/plugin-ui-session --all-features
cargo run --offline --locked --manifest-path plugin_runtime/Cargo.toml --target-dir build/plugin-ui-session --features packages --example qualify_ui_session -- build/plugin-ui-session/guest/wasm32-unknown-unknown/release/morrow_example_ui.wasm build/plugin-c-guest/c_ui.wasm build/plugin-c-guest/cpp_ui.wasm
```

后两个 C/C++ 模块不存在时可只提供 Rust 模块路径。重建 C/C++ 的既有入口为 `tool/build_plugin_c_wasm.ps1`，需其指定 WASI 工具链。

## 明确未完成

当前是可复用在线原生会话底座，不能宣称已完成主应用的自动发现、包 UI 扩展点注册、启停管理、跨视图分派或 Flutter 在线控制器。每个会话独占一个现有 Worker 与核心所有权；生产多视图需要接入共享调度与生命周期协调。generation 仍要求可信宿主分配，尚无跨会话全局路由注册表。

固定示例只有标题编辑；置顶和应用按钮尚无保存业务，不自动推导全部表单状态、保存草稿或恢复插件内部状态。文档渲染、用户确认、核心事务、持久恢复、Web / 移动执行后端与生产界面错误展示仍需独立接入和验收。本轮未修改主应用、宿主协议、核心 schema、版本、发布状态或远端。
