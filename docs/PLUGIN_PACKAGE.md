# 实验插件包与 SDK 开发流程

基于 0.1.9-test.10；包 schema v1、guest ABI v1、运行消息 Cap’n Proto v6 分别管理。本阶段使 C／C++／Rust 示例经过统一打包、不可变安装和受限执行，尚未建立完整插件管理器。

## 开发者入口

三种语言继续使用 [SDK](../sdk/README.md) 构建 Wasm。模块字节与版本化 manifest 统一封装为 `.mplugin`，持久格式为 Protobuf＋LZ4。界面接口按 [SDK 与 UI 设计](PLUGIN_SDK_AND_UI.md) 推进：插件提交有界界面描述与动作，Flutter 渲染。首期没有 TS／JS guest；第三方不需要编写 Dart，动态 Dart SDK 不阻断 0.2.0。

```powershell
# 先按 SDK 说明构建 C 示例。目标文件必须不存在；工具不会覆盖已有包。
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.10 --example plugin_package -- pack build/plugin-c-guest/c_rename.wasm build/example.mplugin org.morrow.example.c-rename 0.1.9-test.10 rename
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.10 --example plugin_package -- inspect build/example.mplugin
cargo run --locked --manifest-path core/Cargo.toml --target-dir build/core-test.10 --example plugin_package -- install build/example.mplugin build/example-catalog
cargo run --locked --manifest-path plugin_runtime/Cargo.toml --target-dir build/plugin-runtime --features packages --example qualify_package -- build/example.mplugin
```

最后一条是合成重命名示例的资格工具，仅适用于返回约定测试状态的示例，不能作为任意业务插件启动器。它使用隔离临时资料库，不修改 Flutter 现有资料。

打包工具接受 `rename,summary,operation,attachment` 的逗号列表或 `none`。模块最多 4 MiB；检查前有界读取。默认 fuel 为 2000 万、内存 16 MiB、宿主调用 16 次；其他实验限额可通过核心 Package API 构造 manifest。当前 CLI 不提供依赖、资源或 UI 清单输入，不把未实现字段伪装为有效功能。

## 校验、安装与执行

| 阶段 | 当前实现 | 不代表什么 |
| --- | --- | --- |
| 容器校验 | 固定 magic／版本、长度、LZ4 输出上限、原文 SHA-256、Protobuf 字段预算 | 摘要不是作者签名或信任证明 |
| 清单校验 | ID、SemVer、显示名、入口、guest ABI、运行版本、两份消息 schema 摘要、模块摘要、能力和预算 | 合法清单不授予权限，也不证明 Wasm 可运行 |
| 不可变安装 | 宿主管理目录中暂存、同步、无覆盖发布；文件名由整个包的 SHA-256 生成；加载重新校验 | 没有启用状态、版本指针、升级事务或断电持久性资格 |
| 运行准备 | 验证 Wasm、禁止 start、检查固定导入和入口；将清单预算与宿主上限取较小值 | 编译／准备本身不执行插件；初始化内存限额在实例化时强制执行 |
| 连接与调用 | 每次连接生成新实例，绑定包摘要与能力上限；实际调用仍检查对象授权 | 同名或新版本不继承旧实例权限；更换包必须重新连接 |

原始 manifest 与整个归档字节被保留，未知可选字段不因解析丢失。未知必需语义必须声明在 `required_features` 中；当前不支持任何此类功能，遇到非空列表拒绝。未知能力、重复能力、缺少预算、错误摘要或版本均拒绝。后续依赖、入口扩展等不能仅追加未知字段并让旧宿主静默忽略。

`Catalog` 只接受宿主提供的根目录；包内名称不参与路径拼接。相同包重复／并发安装收敛到同一文件，已存在内容损坏时拒绝且不覆盖。这里假定目录归可信宿主管理，不宣称能隔离一个可任意修改宿主目录的外部进程。

运行库默认仍独立于内容核心。启用原生 `packages` feature 后提供 `PreparedPackage`，拥有已校验归档与编译模块；其 `run` 只接受可信 HostRuntime、Connection 和宿主时钟。包摘要不一致时在执行前返回 PackageBinding，宿主调用为零。低层 Runner 仍用于受控适配与故障测试，不是第三方自行选择宿主连接的入口。

能力声明是上限，实际授权是另一层：例如只声明 rename 的包不能被授予摘要查询或附件读取；声明了 rename 也必须获得具体卡片授权才能修改。安装、连接、包升级都不自动产生授权。断开后核心拒绝旧连接的新操作，PreparedPackage 还会在 guest 执行前检查连接的真实宿主与 Ready 状态。已增加原生后台任务队列，见 [任务与生命周期](PLUGIN_TASKS.md)；进程／浏览器 Worker 强制终止仍待完成。

## 验证与后续任务

复现命令：`tool/verify_core.ps1 -Web` 与 `tool/verify_plugin_runtime.ps1`。后者创建独立输出目录，将 Rust、C、C++ 与 C++ 分配器示例打包为四个 `.mplugin`，安装、重新加载后运行；同时保留独立核心消息对照和提交后故障测试。实际结果见 [插件包验证记录](../reports/plugin-package-validation.md)。

下一阶段按以下依赖推进：

1. 在已验证的原生后台队列上实现版本化任务输入／结果、多实例调度、实例停止与撤权协调；替换固定示例任务。
2. 包注册／启用状态、作者信任、签名与撤销、依赖接口和锁定；更新不能复活旧授权。
3. 声明式 UI schema、事件代次、Flutter 有界渲染器及三语言 UI 构造器；先贯通同一编辑表单，再扩展专业渲染。
4. 共享对象租约、审计封存、证据和 A/B 贯通，逐平台完成资格验证。

上述工作继续纳入 M1-05、M3-03／04／06、M6-06，M5 仍是首轮贯通门槛。Windows 上的包执行和 Web 核心编译不能代替浏览器插件安装、设备运行或全平台支持。
