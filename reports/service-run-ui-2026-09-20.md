# 有限 API 服务运行面板验证

日期：2026-09-20。结论：**PASS_SCOPED**。基线为 `507aba9af933ff6da4ca4abfe0f360a726ec5ce0`，版本保持 `0.1.9-test.52+56`。本报告限定本轮界面、控制会话和本地验证；不声明完整插件系统、SDK 冻结或全平台资格。

## 可操作路径

插件设置中的 `ServiceRunManager` 使用既有服务配置与发布授权启动有限运行。候选必须匹配配置摘要/修订、发布引用/修订、插件摘要和 Registry 修订，且插件已启用、具备已批准的 Listen/Publish 能力和对应处理器。禁用、过期、摘要不符、TLS 及当前适配层不支持的非回环 HTTP 条目不能启动。配置管理仍负责授权写入，运行面板不创建授权。

面板提供运行时长、累计任务预留次数和字节额度，以及每项作业调用/字节额度、请求/响应/请求头大小、并发数及超时参数。所有数值先由共享 Dart 校验，再由原宿主按插件声明和已批准能力准入。选择与修订绑定；刷新记录或更换语言不会隐式把旧选择绑定到新修订。数值草稿保留，包括无效输入。

启动、运行、停止、退出、绑定、监听、监督及内容库回收状态分别展示；实际退出后显示执行、断连和维护结果。停止不等于退出，只有带真实退出回执的 Reclaimed 才允许确认；RecoveryRequired 仅开放修复。状态核实失败时保留最近观察并标注未核实，禁止使用不可信旧状态执行控制。

`ServiceRunSession` 按后端组合复用，不属于可收起的页面。写前重新观察原任务，控制始终绑定原 key/submission；页面卸载不丢在途启动或 Unknown，也不会重发。未知启动只能按原 submission 查询；若新观察为本地无任务，可显式结束本次尝试并保留 Unknown 记录，不把它解释为未执行或回滚。历史保留最近 5 项，不新增累计服务启动配额，不宣称无限历史去重。

计时器仅由可见页面持有，只对已核实的活动服务轮询。页面卸载取消计时器；失败、陌生 HTTP 任务和 Unknown 后需显式刷新，不持续重试。服务运行控制不等待普通元数据刷新完成。运行面板只订阅配置元数据，不增加一次性令牌查看者计数，最后一个真正的管理页面离开仍会清除令牌。

同一服务活动及待确认期间，短 HTTP 面板提示使用服务控制，隐藏不适用的读取、取消和确认控件。服务实际回收并确认后，原 HTTP 草稿及控制器继续使用。

## 本轮证据

- 最终 14 个文件组合回归：**144 通过、0 失败、0 跳过**，日志 `build/service-run-ui-regression.log`。
- 新控制会话单元测试：12 项；覆盖复用、未知启动、陌生任务、写前身份更替、失败后控制门槛、真实退出、修复/确认和 dispose 后不发新写入。
- 新运行面板测试：19 项；真实点击选择与启动、完整预算交付、修订变化、10 类不可准入候选、语言切换与无效草稿、慢元数据刷新期间停止、卸载/重挂、令牌清理、320px 中英文高级参数布局，以及两个面板的草稿和控制联动。
- 原 Windows 原生集成测试改为通过新控制会话启动/观察/停止/确认；真实 debug Rust 宿主、原 Rust 工作台 guest 和有限服务 WAT 测试包共同完成两个真实 HTTP 请求、期间卡片创建/编辑/读取及语言保存、回收确认与重开后的持久数据读取。独立运行 1/1 通过，日志 `build/service-run-ui-real.log`，也包含在最终 144 项中，不重复计数。
- 受影响 7 个手写 Dart 文件的 Flutter 分析无问题，日志 `build/service-run-ui-analyze.log`；格式及差异空白检查通过。
- `tool/build_i18n.py --check` 验证 5 个中英文目录/语言包产物；官方 `flutter gen-l10n` 生成访问器。`tool/generate_workbench_client.py --check` 通过；冻结 SDK 基线 36 个固定文件、13 对原 Wasm/包通过。
- 原生宿主 release 离线锁定构建通过，日志 `build/service-run-ui-host-build.log`。本轮没有 Rust 生产修改，没有把历史 Rust 测试数重复计入本轮。

组合文件为：`host_request_test.dart`、`service_run_control_test.dart`、`service_run_transport_native_test.dart`、`service_control_test.dart`、`io_task_control_test.dart`、`service_session_test.dart`、`workbench_close_native_test.dart`、`service_business_routing_native_test.dart`、`service_business_routing_real_native_test.dart`、`service_run_session_test.dart`、`service_run_manager_test.dart`、`service_manager_test.dart`、`http_task_manager_test.dart`、`plugin_library_test.dart`，均位于 `test/`。

独立 `dart analyze` 在本机 analysis server 关闭时遇到 perf 管道文件删除错误；最终采用 `flutter analyze --no-pub` 成功完成，不把异常退出视为通过。测试开发中一次点击未命中来自测试输入后的光标滚动未稳定，修正测试等待并验证 hitTestable 后通过。

## Windows 构建

首次构建因 media_kit 的 libmpv 下载文件完整性校验失败而中止，日志 `build/service-run-ui-windows-build.log`。使用主检出已有的 libmpv 与 ANGLE 压缩包，逐一核对依赖 CMake 固定 MD5 后复制到本次隔离构建目录，保留原校验逻辑。两份缓存 SHA-256 分别为 `dce982222d7a23e4a1c6f0fb6cc39f6e899a6714624b95ea49cff6558ee97572` 和 `cc5911bb15d596fd5a2b362613ad35b7093b427117269a7359054a65746a5f9a`。

依赖恢复后的首次重试虽完成编译，却暴露了 CMake 的重试缓存问题：首次配置在插件下载阶段中断，缓存保留 `C:/Program Files/morrow_studio`，下次配置已没有 `CMAKE_INSTALL_PREFIX_INITIALIZED_TO_DEFAULT` 标志，完整产物被安装到系统目录。首次随附组件测试因此缺少预览目录中的宿主/插件而跳过 1 项；该次不计为通过，也不视为可交付预览。

已修改 `windows/CMakeLists.txt`，在插件配置前固定默认 Flutter bundle 位置，并识别恢复该历史默认缓存，同时保留其他显式目的地。重新配置后检查所有生成的资源清理路径位于本次隔离构建目录，再执行构建。最终 `flutter --no-version-check build windows --release --no-pub` 成功（20.1 秒增量构建），日志 `build/service-run-ui-windows-build-final.log`；安装清单全部位于 `build/windows/x64/runner/Release/`，已确认主程序、AOT 数据、Rust 宿主和工作台包存在。此前写入 `C:/Program Files/morrow_studio` 的产物已告知用户并保留，未擅自删除可能已有的内容。

随后使用最终预览目录内的 release 宿主与随附工作台包重跑真实服务会话/HTTP/卡片及重开持久化测试：**1/1 通过，0 跳过**，日志 `build/service-run-ui-release-real-final.log`。这与上述 debug 组合回归是不同二进制配置的验证，不能将其描述为 145 个不同测试。

本地入口为 `build/windows/x64/runner/Release/morrow_studio.exe`，须与同目录数据、DLL 及 plugins 一起使用。主要产物 SHA-256：

| 产物 | SHA-256 |
| --- | --- |
| `morrow_studio.exe` | `95d5dcb22fb1f860aa51397dda0b8649ffd02a15338c2673815b9e66731ac6c0` |
| `data/app.so` | `f527b8a3714f438ae17ce3050fe21d19ae56789d6e44862e69d37e62733960b2` |
| `morrow-workbench-host.exe` | `489d954afc49a68e0a48741cd7329e51735725d46ff51a0351f690b076a9d3ac` |
| `plugins/workbench.morrowplugin` | `d068b1d8e87e219a74b5446fc14d71c4fabaeddc72977e8f5e3c76d5a052ce1f` |

哈希清单保存在 `build/service-run-ui-sha256.txt`。本轮没有制作安装程序、ZIP、签名或 Release。

## 尚未证明与下一项

Widget 布局和交互测试不等于真实 Windows 窗口的视觉、输入或长时间运行验收；原生集成使用测试 WAT 服务，不代表任意 Rust 插件已具备完整三语言 SDK 支持。本轮不发布 Release、不推送、不修改版本。

下一切片应增加实际界面/原生端口冲突、明确准入拒绝的可操作诊断、授权撤销、到期、丢回执、慢回调停止与封存修复流程，并扩大内容/UI/capture 和大请求分段验证。长 IO 可暂停、TLS、出站资源、文件系统、Unknown 持久核对、三语言 IO SDK 与其他平台继续原门槛。IO-D2b/IO-E2 尚未整体完成。
