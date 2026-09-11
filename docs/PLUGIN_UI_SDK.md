# C／C++／Rust 插件界面 SDK

基于 0.1.9-test.10，UI 契约 v1、任务契约 v3、guest ABI v2。三种语言现在可生成受限表单，读取宿主 UI 事件，并通过现有纯任务通道返回完整界面快照。首期仍不提供 TS／JS guest 或动态 Dart guest；Flutter 负责宿主控件。

## 接口与所有权

| 语言 | 入口 | 用法 |
| --- | --- | --- |
| C | `sdk/c/include/morrow_plugin_ui.h` | `mp_ui_node_v1` 数组 → `mp_ui_document_encode`；`mp_ui_event_decode/get/free` 读取事件 |
| C++17 | `sdk/cpp/include/morrow_plugin_ui.hpp` | 拥有字符串的 `morrow::ui::node`、`encode` 与只可移动的 `event` |
| Rust | `morrow_plugin_sdk::ui` | `Node::new`、`Document::new/encode/decode`、`Event::decode` |

C 编码时指定 ABI=1、节点结构的精确 sizeof，布尔值只接受 0／1，enabled 必须显式初始化为 1。所有指针和跨度在调用中必须有效、对齐且互不重叠；空字符串可为空跨度，非空跨度不得为 NULL。输入只在调用期间借用，编码结果不引用原字符串。失败时长度为零，输出缓冲区不写入。

C 事件解码得到本地拥有的句柄，get 提供只读借用跨度，free 后失效；不是宿主能力或可持久化身份。失败输出句柄为 NULL。C++ 事件采用 RAII，移动后原对象不能继续读取；节点拥有字符串，编码时临时构造 C 跨度。Rust 文档私有持有节点，字段或树变更后须重新构造校验。

SDK 独立构建，不链接可信核心、Store 或授权接口。UI schema 与版本从核心同步为固定快照，构建生成 Cap’n Proto 绑定并计算独立摘要；复现检查快照一致。C 与 C++ 使用 Rust 实现的同一局部编解码库，不让 STL、Rust 对象或原生指针进入消息。

## 有界表单

首批节点是 column、row、text、button、textInput、toggle；主题角色 normal／muted／emphasis。规则与宿主协议一致：消息至多 64 KiB，1–128 节点，深度至多 8，父节点在前且只能是容器；总展开文本至多 32768 字节。单个正文 4096、标签 512、标识符 256 字节；输入框 1–4096 字节，并校验初始文本。控制字符、混合字段、未知枚举、重复 ID、非法父引用均拒绝。

Event 解码保留完整 UInt64 generation、revision、serial，并检查契约和字段组合。**解码不证明当前视图、权限、节点动作或序号有效。** 可信宿主必须先通过 Session 接纳，再按绑定的实际插件连接提交任务；guest 自己读到的身份不能授予权限。SDK 不提供宿主 Session 或序号分配。安全性仍来自宿主校验，不能依赖 guest 善意。

## 真实示例与任务交付

`examples/c-ui`、`cpp-ui`、`rust-ui` 提供相同中文表单，包含标题、置顶与应用按钮。示例的两个处理器通过已有不可变包的纯转换声明交付：

| 处理器 | 输入类型与上限 | 输出类型与上限 |
| --- | --- | --- |
| `ui.form` | `text.utf8`，32 字节 | `morrow.ui.document.v1`，65536 字节 |
| `ui.edit` | `morrow.ui.event.v1`，65536 字节 | `morrow.ui.document.v1`，65536 字节 |

类型名目前是精确匹配的实验声明，不代表版本协商或 UI 扩展点注册。通用执行器验证包的处理器与输入／输出类型、上限及完成消息关联；可信 UI 接入层仍须独立解码结果。类型名正确不能把任意字节变成可渲染文档。

`ui.form` 将输入文本放入标题；`ui.edit` 读取已经由宿主检查的标题编辑事件，返回新快照。无效 UTF-8、控制字符和坏事件返回关联的 InvalidInput 业务错误。表单只演示局部 UI 变化，按钮与开关没有在 guest 中实现保存／置顶业务，完整状态输入与动作分派需下一阶段设计，不能将快照中的值当作已保存内容。

两个处理器都没有内容权限或核心调用。示例不根据事件触发存储，不自动重试；改名、保存、网络等操作仍需核心授权与事务。主应用自动发现面板、启用管理、多视图绑定、队列确认、持久草稿及错误来源显示尚未接入。

## C++ Wasm 初始化约束

WASI libc++ 的 out-of-line string 实现会间接使命令退出清理进入链接；默认命令导出包装进一步引入时钟和 stdio 导入。宿主仍拒绝这些导入。

构建脚本现在将 C++ 用户 `morrow_run` 编译为 `mp_guest_run`，公共入口由 SDK 提供；显式调用 `__wasm_call_ctors` 后执行用户入口。导出构造器避免链接器生成 WASI 命令退出包装。每次任务仍创建全新的 Wasm 实例，宿主按原有 fuel／内存／期限限制执行；没有新增系统导入或宿主 ABI 权限。

正常作用域内的 RAII 析构仍执行，所有全局构造器在一次调用前初始化。此执行后端不运行进程退出／全局析构／atexit 清理，实例退出后由宿主回收内存；插件不得依赖这些回调保存内容或释放宿主资源。仍须显式通过既有协议提交必要结果，资源租约后续由宿主管理。分配测试验证每个新实例构造一次及局部析构，原有 abort／OOM 测试继续证明陷阱退出。

## 复现和下一阶段

```powershell
pwsh -File tool/verify_plugin_ui_sdk.ps1 -Python python
pwsh -File tool/verify_plugin_runtime.ps1 -Python python
```

工具链和本地测试字体要求同 [Flutter 渲染器](PLUGIN_UI_RENDERER.md)。第一条命令核对固定契约，检查原生／Wasm SDK，运行边界测试和实际 C++ 动态库测试，生成真实 Flutter 编辑事件，打包三语言模块并运行独立宿主校验，最后用 Flutter 渲染三种 guest 的初始／更新文档。第二条回归原有三语言业务任务、后台队列、分配、取消和异常终止。

此实验通过磁盘原字节在验证阶段间交付，原始 Flutter 编辑来自同一份 Rust 标准表单；三种 guest 的初始输出与该表单相同。它不是主应用在线插件会话，也不是 Web 插件全链验证。详见 [实际验证记录](../reports/plugin-ui-sdk-validation.md)。下一阶段建立真正的包 UI 扩展点与宿主实例绑定、状态输入和动作分派，再接通生产确认与核心内容提交。
