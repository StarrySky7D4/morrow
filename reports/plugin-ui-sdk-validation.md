# 三语言 UI SDK 与实际任务验证

2026-09-11，Windows x64，基线 0.1.9-test.10；UI v1／任务 v3／guest ABI v2。结果来自本机真实构建、Wasm 解释执行和 Flutter 控件测试，不外推全平台或主应用上线状态。

## 已通过

- 独立 SDK 固定契约检查，原生及 wasm32 Clippy；21 项 SDK 测试（原有 15 项和新增 6 项 UI／C ABI 边界测试）。覆盖树形、节点／深度／UTF-8／展开文本预算、截断和尾随数据、契约损坏、C 失败缓冲区与句柄状态、字段枚举／布尔值及 UInt64 最大值。
- 原生 C++ 程序链接本次 SDK DLL，验证节点字符串拥有权、只可移动事件、完整 UInt64、中文、错误输入与模型限制。此新增步骤在三语言链验证通过后单独执行通过，已纳入统一脚本。
- 运行器全部 35 项测试、全目标 Clippy 和旧三语言实际执行资格检查通过。原有重命名／授权／撤权／取消／持久化检查、包绑定／后台队列、动态任务、纯转换和业务错误均通过。
- 三语言 UI 插件均编译为实际 Wasm，打包为不可变 `.mplugin`，零内容能力。每种语言 3 次处理器／类型／大小拒绝发生在 guest 运行前，不消耗 fuel；随后每种运行 8 个真实任务，共 24 个：4 种合法初始标题、2 种非法文本、1 个坏事件及 1 个已接纳编辑事件。
- 每种语言返回 5 个合法表单和 3 个关联 InvalidInput 业务失败。输入覆盖空文本、中文／emoji、32 字节 ASCII 和 32 字节 emoji；非法 UTF-8、控制字符和坏事件被拒绝。宿主逐字段比较独立核心解码的文档，三语言初始与更新输出各自逐字节相同。
- 先由实际 Flutter TextField 产生“从 Dart 编辑🌈”事件，可信测试适配器编码，再由 Rust Session 检查完整 UInt64 代次、修订、序号、动作及节点。接受后将原始事件字节交给三个真实 guest 的 `ui.edit` 任务。输出再经独立 Rust 解码并替换为修订 2；重复／旧修订事件拒绝。
- 三份实际 guest 初始／更新文档进入 Dart 解码和 Flutter 渲染，3 项新增控件测试通过；更新快照显示正确文本且不伪造编辑回调，按钮仍能产生宿主意图。原有 8 项控件测试同时通过。
- 三种 UI 插件全程零核心调用。关闭 worker 后重新打开独立测试数据库，原卡片未改变，待封存事件仍只有初始种子事件。

UI 主验证日志：`build/plugin-ui-sdk/verification.log`。旧运行器回归日志：`build/plugin-ui-sdk/runtime-regression.log`。复现与开发指南见 [UI SDK](../docs/PLUGIN_UI_SDK.md)。

## C++ 问题与修复证据

初始 C++ UI 包被准备阶段拒绝（UnsupportedAbi）。实际模块导入表含 WASI clock_time_get／fd_close／fd_seek／fd_write。反汇编显示 out-of-line string 库链接后，命令出口包装调用 `__wasm_call_dtors`，进一步引入 stdio 退出清理及锁／时钟路径。

构建入口改为显式调用构造器和用户任务入口，导出构造器阻止链接器合成命令退出清理。修复后的 UI 模块仅导入 `morrow_task_v1.read_input` 与 `complete`，通过原有未放宽的宿主准备规则。每个任务仍使用新实例；实例内存由宿主回收。该模式不运行全局析构／atexit；不得在这些回调中保存内容。

新增分配用例检查每个新实例的全局构造一次及局部析构执行，旧 SDK guest／包／worker 各路径均通过；C++ abort 与 OOM 仍是无核心调用的本地 trap。没有添加系统接口模拟或放宽导入权限。

## 产物摘要

以下为本次通过 UI 资格检查的包与原字节；后续旧回归输出位于其他独立目录。guest-1／guest-2 分别为 C／C++，其初始／更新文档摘要与 guest-0 相同。

| 文件 | 字节 | SHA-256 |
| --- | ---: | --- |
| `build/plugin-ui-sdk/packages/7c26c06293004aeb970d3d0f39010ade/rust-ui.mplugin` | 61799 | `0fa88aa9a1d46fc22efe6b55def0d0cab54ab4ca1fa30901501a41db8773b140` |
| `build/plugin-ui-sdk/packages/7c26c06293004aeb970d3d0f39010ade/c-ui.mplugin` | 53147 | `bdd857191222be91b2e7fa299314b32d3063fd1014857b300e7809e75f3f230d` |
| `build/plugin-ui-sdk/packages/7c26c06293004aeb970d3d0f39010ade/cpp-ui.mplugin` | 57587 | `d1a93428258cf8bd83b54edbc6dc1994bb48ef84b929a0ed7b8be8c327286b8b` |
| `build/plugin-ui-sdk/artifacts/guest-0-form.capnp` | 592 | `77a2345d8f199ce100431ae4812f5ea37a727159256d02fb5ec34d7ac333b4df` |
| `build/plugin-ui-sdk/artifacts/guest-0-updated.capnp` | 600 | `7551e4a0a259d5c7a2db03fc196e007a1292619775c73f489d5b2a9ff5ee8289` |
| `build/ui-protocol/widget-event.capnp` | 176 | `1e2a7c412a548b29a45e08b6e4b6553ccd9bfa1d78eae9f0cf18f3775af8b6a6` |

## 尚未完成

当前为独立测试宿主串联，阶段间通过原字节文件交付。Flutter 初始编辑基于独立 Rust 标准样本，三种 guest 的初始表单与该样本相同；这不是主应用中已经运行的在线插件会话。包使用通用纯转换声明，不等于 UI 扩展点注册或自动启用。

未接入 UI 会话与实际插件实例的生产绑定、快照状态输入、通用动作调度、序号确认／背压、持久草稿和核心内容事务。按钮／开关仍为表单演示。Web guest、其他系统／设备、完整读屏／输入法及主应用玻璃主题整合需分别验证。本轮未迁移用户数据，未推送或发布 Release，完整插件系统目标继续推进。
