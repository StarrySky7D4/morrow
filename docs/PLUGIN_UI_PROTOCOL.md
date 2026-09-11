# 声明式 UI 消息与视图事件

基于 0.1.9-test.10。新增独立 Cap’n Proto UI 契约 v1，与内容消息 v6、任务契约 v3、guest ABI v2 分别管理。当前已实现 Rust／Dart 消息与 Rust 视图事件校验；尚未实现 Flutter 控件渲染、C／C++／Rust guest UI 构造器、包 UI 扩展点注册或真实插件到视图的调度桥接。

## 描述与边界

`core/schemas/ui.capnp` 为唯一 UI schema，Rust 与 Dart 从同源生成绑定并校验版本及摘要。Document 是按父节点在前排列的扁平节点列表，节点 ID 唯一；第一个节点必须为根 column，后续节点必须引用已出现的 row／column。文本和交互节点不能充当父节点。前向引用、自环、重复节点和额外根均拒绝。

| 节点 | 当前字段 | 宿主未来呈现方式 |
| --- | --- | --- |
| column／row | ID、父节点 | 宿主有界行列布局 |
| text | 文本、normal／muted／emphasis 色彩角色 | 主题映射的普通文本 |
| button | 标签、动作 ID、启用状态 | 宿主按钮 |
| textInput | 标签、文本、动作 ID、启用状态、maxBytes | 宿主输入控件；UTF-8 字节上限 |
| toggle | 标签、布尔值、动作 ID、启用状态 | 宿主开关 |

色彩只通过主题角色表达，未开放任意颜色／样式／表达式。文本首期按普通文字处理，不解析 Markdown。其他组件、可选节点退化、资源引用、专业渲染和 UI 增量更新仍待实现；未知必需枚举拒绝。输入、焦点和动画仍应留在 Flutter，不能逐帧经核心往返。

固定实验限额：每个消息至多 64 KiB，1–128 节点，树深度至多 8；单个文本至多 4096 字节、标签至多 512 字节，全部节点的 ID、父 ID、标签、文本和动作总 UTF-8 字节数至多 32768。即使原消息帧没有超限，也必须检查展开后的文本预算。输入框 maxBytes 为 1–4096，初始和事件文本均不得超出。普通文字允许换行／制表，拒绝其他控制字符；标识符沿用核心规则。

容器与普通文本不能携带动作；只有 toggle 可以带 checked；只有输入框可以带 maxBytes；交互节点需要非空标签与动作。无意义的混合字段拒绝。Dart 解码后返回不可变节点列表，Rust Document 的节点由私有容器持有，修改后须重新验证。

## 宿主拥有视图状态

可信宿主创建 `Session(view, generation)`，generation 非零；当前原型不生成唯一 ID，生产调度器负责分配和绑定实际插件连接。Document 自身不能指定视图身份、权限或代次。

`replace(base_revision, document)` 必须匹配当前修订，由宿主推进修订；失败不改变已有文档。当前是完整快照替换，没有 patch。`close()` 永久关闭当前 Session 并丢弃描述；新视图需要新的宿主代次，不能恢复旧会话。

Event 包含契约身份、view、generation、revision、serial、node、action、事件类型与有界值。三种事件为 activate、editText 和 setToggle；禁止混用文本与布尔载荷。Dart 使用 BigInt 保留完整 UInt64，不经 JSON／浮点数转换。

`Session::accept` 检查视图、代次、当前修订、连续序号、节点、动作和启用状态，并检查事件类型与节点匹配、输入文本上限。序号从 1 开始，仅接受上一已接纳序号加一；拒绝不消耗序号，替换文档不重置序号。旧修订、重复、跨视图和关闭后的事件拒绝。

接纳只产生待路由的事件数据，不修改表单、卡片或资料库，也不执行动作。调用层仍须绑定实际插件连接、检查权限并创建有界任务；动作 ID 不是权限。生产事件确认、队列背压、多视图生命周期和持久草稿尚未接入，不把 Session 当作完整授权或恢复系统。

## 跨语言与浏览器证据

`core/examples/ui_vectors.rs` 生成中文表单与 UInt64 最大代次事件。Dart 客户端读取 Rust 原字节，编码新事件；Rust Session 实际读取 Dart 输出并验证，同时拒绝重放和关闭后的事件。

浏览器探针将同样流程放入真实 Chrome 的 Dart/Wasm，导出浏览器生成的事件原字节，再由原生 Rust 验证器检查。此证据覆盖浏览器消息编解码，不表示浏览器中运行了插件或 Rust UI Session，也不表示已经绘制 Flutter 表单。

```powershell
pwsh -File tool/verify_core.ps1 -Web
pwsh -File tool/verify_plugin_ui.ps1 -Web
```

UI 验证脚本重建 Rust 向量、比对已提交的二进制样本、运行 Dart 测试及原生双向探针，并在 -Web 下编译 Dart/Wasm、运行无界面浏览器、回收其进程后校验浏览器事件。具体结果见 [验证记录](../reports/plugin-ui-protocol-validation.md)。

下一步：三语言 guest UI 构造器和包级 UI 类型契约 → 实际任务输出交付 UI 文档 → Flutter 有界渲染与主题／窄屏／输入法测试 → 用户事件绑定任务及核心权威提交。长期 UI 设计与 M6 门槛继续保留。
