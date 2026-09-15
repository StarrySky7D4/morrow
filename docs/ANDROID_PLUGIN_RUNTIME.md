# Android 插件执行域与新核心接入方案

日期：2026-09-15。状态：设计／待验证，不是已实现的 Android 新架构或 APK 交付声明。来源为用户的 [ds.md](../ds.md)，整体排期见 [ROAD-12](ROADMAP_UPDATE_2026-09-15.md)。本文件补充原讨论的实际接入前置条件、系统版本差异与失败验收；保留 [旧 Android 构建记录](ANDROID.md)。

## 1. 代码起点与首要缺口

核对源码提交 `0e93e49f56dd2534db5a0cd8fb31e9d5c705dfa7`，本次未构建或运行设备测试。

| 当前入口 | 事实 | 必须完成的适配 |
| --- | --- | --- |
| [bootstrap_native.dart](../lib/plugins/bootstrap_native.dart) | 非 Windows 不启用新 Rust 工作台 | 接通 Android 唯一可信内容宿主；新架构不能继续与旧 Dart 存储双写 |
| [main.dart](../lib/main.dart) | 其他平台保留旧 SharedPreferences 路径 | 新配置显式启用，旧数据先保留；迁移经副本恢复验证，不直接批量切库 |
| [workbench_native.dart](../lib/plugins/workbench_native.dart) | 桌面桥使用 Process.start | Android 使用随包 Rust 库及薄 JNI／FFI 桥，不套用桌面可执行文件启动路径 |
| [worker.rs](../plugin_runtime/src/worker.rs)、[package.rs](../plugin_runtime/src/package.rs) | 当前执行路径持有 HostRuntime | 提取不含 Store、账号、审计密钥的 RunnerEngine，宿主交换受控且有界 |
| [shared_memory.rs](../plugin_runtime/src/shared_memory.rs) | 非 Windows 映射后端明确不支持 | Android 新后端独立实现、验证和声明；不能继承 Windows mmap 通过状态 |
| [build.gradle.kts](../android/app/build.gradle.kts) | minSdk 来自 flutter.minSdkVersion | 每次资格报告记录解析后的 minSdk、targetSdk、ABI、页面大小；旧 API 24 记录不保证新后端支持 |

## 2. 采用的进程与职责划分

同一 APK 包含 Flutter 界面、可信 Rust 核心、薄 Kotlin/JNI 服务适配和经过审核的 Wasm 执行引擎。代码打包在一起不要求运行在同一进程。

可信宿主保留唯一资料库、Registry／Manager、当前批准、审计与平台资源代理。按需隔离 Service 只负责执行获准程序，接收固定输入并返回提案；不启动第二个 FlutterEngine，不打开正式内容库，也不加载账号或签名能力。

`android:process=":runner"` 仅指定另一个进程，不能当作权限隔离；`android:isolatedProcess="true"` 才是本方案的隔离 Service 配置。设 `exported=false`，宿主使用显式组件绑定，真实连接代次绑定包摘要、执行域和回调端点。manifest 本身仍需通过实际安装与权限拒绝测试。[Service 配置](https://developer.android.com/guide/topics/manifest/service-element)

宿主验证包与固定字节后交付受控缓冲／FD；不要求隔离进程直接读取宿主私有目录。检查最终合并 Manifest、Application、自动初始化组件及 JNI 加载，证明隔离进程没有意外初始化 Flutter、数据库或后台账号任务。

第三方 native 插件是另一个执行配置，不因随包加载引擎 .so 而开放任意下载动态库。可信短函数可以在宿主工作线程执行，但必须明确属于可信代码；进程内 Wasm 仅作为单独、较弱故障边界的配置研究，不能因内存紧张自动降级到主进程。

## 3. API 分层与内存保证

下表是待验收的配置，不是支持列表。当前工程实际最低系统要求与各能力 API 门槛同时生效。

| 系统范围 | 候选实现 | 验收／拒绝规则 |
| --- | --- | --- |
| API 24–25（若构建仍支持） | 有限声明式隔离服务槽；有界副本 | 没有 ASharedMemory；要求映射特性时明确不支持，不伪装为零复制 |
| API 26 | NDK ASharedMemory；有限服务槽 | Kotlin/JNI 与 FD 转移单独验证 |
| API 27–28 | 可使用 Java SharedMemory；有限服务槽 | 只读保护和旧可写映射行为实际验证 |
| API 29+ | bindIsolatedService 多实例执行域 | 每次绑定使用新的实例／域代次，任务数和在途量有界 |
| API 34+ 的共享隔离进程选项 | 首期不启用 | 不能以省进程为由合并必须隔离的信任域 |

API 级别依据：[NDK 共享内存](https://developer.android.com/ndk/reference/group/memory)、[Java SharedMemory](https://developer.android.com/reference/android/os/SharedMemory)、[Context 服务绑定](https://developer.android.com/reference/android/content/Context)。旧系统服务槽耗尽时排队或拒绝，不隐式回退到 Flutter 进程；不预先承诺具体槽数。

发布输入顺序：宿主独占写入 → 释放所有已知写映射 → 降低保护并检查返回值 → 固定并校验同一份字节 → 交付。setProtect 只限制之后建立的映射，不能撤销已有可写映射；若不能证明写者已消失，则复制到宿主独占对象再固定。执行器返回的不可信结果默认先复制到宿主独占缓冲，校验后才提交或签名。[SharedMemory 保护语义](https://developer.android.com/reference/android/os/SharedMemory)

固定、业务撤权和物理回收分别记账。关闭宿主 FD、释放一次映射、收到 ACK 或解除服务绑定，都不足以单独证明其他读者已释放。记录所有交付引用及代次，无法确认时保守计费，禁止把仍有旧读者的页面复用为新敏感内容。

## 4. 控制通道、身份与生命周期

Binder／AIDL 只承载控制、通知、取消及 Cap’n Proto 消息，大对象走有界共享对象或明确副本配置；不再创建另一份 AIDL 业务模型。Binder 的事务缓冲在进程内共享，不能用“每帧小于上限”替代总量背压。限制连接级在途任务、总字节与 FD 数，给取消和终态响应预留预算；oneway 不作为可靠送达证明。[AIDL](https://developer.android.com/develop/background-work/services/aidl)、[事务失败说明](https://developer.android.com/reference/android/os/TransactionTooLargeException)

在 Binder 入口校验并捕获真实调用身份及通道关联，再把任务放入有界执行队列；不能在工作线程上重新读取 getCallingUid() 当作原调用者。回调只做校验、接纳与投递，不运行长计算或持有数据库锁。[Binder 身份](https://developer.android.com/reference/android/os/Binder)

连接状态机至少处理：绑定返回失败、连接超时、onNullBinding、启动崩溃、onServiceDisconnected、onBindingDied、重复／迟到回调和主动关闭。记录 bind 请求是否被系统接纳、是否仍持有绑定注册及解除状态；不能以 onServiceConnected 已到达作为清理前提。已接纳后即使创建阶段崩溃或连接超时也要核对并解除注册，避免遗漏或重复 unbind；onNullBinding／onBindingDied 后显式解除绑定。断连可能保留绑定并自动重连，但新连接必须取得新代次和重新批准的短期资源。旧结果只允许核对原操作，不能恢复旧执行权限。[ServiceConnection](https://developer.android.com/reference/android/content/ServiceConnection)

关闭流程：禁止新操作 → 撤销当前执行／交付权限 → 取消与排空 → 核对原操作状态 → 释放可证明释放的资源与绑定 → 对新域重新分配身份。服务取消或 IPC 响应丢失不代表未提交；按同一 operationId 查询，不自动重发外部效果。

系统可终止进程且不保证调用 onDestroy；宿主不能假定有权通过 killProcess 终止不同隔离 UID。逻辑撤权必须独立成立，物理终止与内存回收另作真实设备证据。若某配置无法强制停止无响应代码，应报告保证不足并拒绝要求该能力的任务，不显示“已完全回收”。[进程生命周期](https://developer.android.com/guide/components/activities/process-lifecycle)、[Process](https://developer.android.com/reference/android/os/Process)

后台执行与隔离分开实现：可延期作业评估 WorkManager，用户可感知持续工作按平台前台服务规则处理。先持久保存意图／检查点，重启按原操作核对；不承诺后台常驻。调度与前台服务资格在目标系统和实际渠道配置上复核。

## 5. AND-01–06 实施任务

这些工作包属于 ROAD-12；共享身份／协议由主线集中整合，平台探针可独立进行。全部尚待本轮实施验收。

| ID | 依赖与交付 | 退出证据 |
| --- | --- | --- |
| AND-01 能力与引擎边界 | 从 ROAD-01 同步开始；核对工具链、API、JNI／加载、引擎与可信宿主依赖，设计跨域交换 | 构建／能力清单；无 Store 的引擎接口样例；全部随包原生依赖的对齐／页面假设检查；安装包进程与初始化审查。仅编译不标集成 |
| AND-02 Android 唯一内容宿主 | AND-01；接入核心、资料库、授权、审计和身份保护；确定 UI 桥 | 独立合成资料的创建→提交→封存→重启核对；无双写，权限与恢复失败可解释；旧资料另经迁移门禁 |
| AND-03 单隔离域 | AND-01 接口固定后可与 AND-02 做独立探针；集成验收依赖 AND-02 | 真实 UID／连接代次、受控包交付、拒绝直接文件／网络访问、所有绑定失败状态；runner 崩溃不拖垮 UI |
| AND-04 固定对象与回收 | AND-03；按 API 支持映射或有界副本 | 4／16 KiB 环境实际映射、写者残留、旧引用、FD 泄漏、结果篡改、解除绑定后引用、未确认回收计费；输入校验与执行字节一致 |
| AND-05 多域依赖与调度 | AND-04＋ROAD-05 的公共接口 | A/B 域协作、根预算、域内串行与全局限额、等待环、取消；终止一域时另一域仍可用，不静默合并隔离域 |
| AND-06 后台恢复与分发 | AND-02–05＋ROAD-08 的相关证据 | 调用前、提交后、响应前被杀的恢复；旧代次拒绝、Unknown 核对；不同 API／页面大小／真实设备与渠道配置独立报告 |

页面资格从 AND-01 的依赖／对齐检查开始，AND-04 验证实际映射，AND-06 汇总 4 KiB／16 KiB 页面配置下 APK 全部原生依赖的 ELF／ZIP 对齐及实际运行；映射与分配不得硬编码 4096。只升级 NDK 不代表所有库通过。[Android 页面大小指南](https://developer.android.com/guide/practices/page-sizes)

具体 Wasm 运行时、消息编解码桥、服务槽数、内存预算、后台方案及旧系统最低保证仍待探针。渠道允许分发的代码类型与设备上技术可执行性分别验收；Wasm／解释执行名称不能自动证明商店批准。实际发布前复核 [Google Play 代码与网络政策](https://support.google.com/googleplay/android-developer/answer/9888379?hl=en)。

## 6. 交付记录

每份平台报告包含源码／APK／插件摘要、已解析 SDK 配置、设备系统／ABI／页面大小、实际 UID 与进程关系、配置能力、测试入口与失败证据。标明仅编译、探针集成、真实应用、设备恢复、渠道资格分别达到哪一级。

未运行的平台向量保留未测；不能以 Windows 核心测试、旧 APK 或 Android service 壳启动代替本方案验收。后续实现保持当前安全边界要求，新 FFI／共享内存代码按具体所有权和生命周期审查，不继承其他模块的 unsafe 许可。
