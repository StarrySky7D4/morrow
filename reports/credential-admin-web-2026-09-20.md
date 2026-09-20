# 主应用凭据管理与 Web 构建验证

结论：`PASS_SCOPED`。Windows 主应用已接通受保护凭据的新建、替换、停用和重启恢复；Flutter Web JavaScript Release 的生成代码精度阻断已修复。实际主应用 HTTP 任务、具体端点批准界面、文件系统后端、完整 IO SDK 与全平台资格仍未完成。应用保持 `0.1.9-test.52+56`。

## 实现范围

- 原 Store v20 增加出站记录有界列表：每页最多 16 条、整个表最多 512 条，在同一 SQLite 读事务校验全部记录并形成引用／修订／原容器摘要快照。页外损坏、快照漂移和未知游标拒绝返回；不增加数据库格式、不解密、不改变期限或活动权限。
- 宿主私有协议增加凭据列表、保存和停用，只返回引用、修订、创建／到期时间及停用状态。共享表过滤出的空页仍保留前进游标；不会返回秘密、HTTP 头内容或密文。
- Windows 录入复用原 DPAPI 保护域和原 Store，期限 1–30 天；新建生成随机非零引用，替换需要原引用、修订及新秘密。停用保留引用并增加修订。写入经过原审计会话准备、CAS 与撤权协调器，不创建第二份配置或独立密钥文件。
- Flutter 插件库新增中英文凭据面板，使用现有按钮风格；明确提交、替换和停用，不自动启用插件。冲突或未知结果需要明确刷新，不能自动重发；迟到结果与旧后台会话隔离。
- 原生输入由 zeroize 管理，Cap'n Proto 借用原请求帧；启用 `unaligned` 保持任意有效字节偏移兼容。Dart 在序列化后清空消息构建区的全部分段，在传输写入完成或失败后清空序列化帧。不可变 Dart 字符串、依赖内部 UTF-8 临时副本及系统管道副本不承诺清零。
- 核心 Dart 生成器为原生保留完整反射，为 Web 移除不可精确表示的可选 int 反射，并导出十六进制／BigInt 精确身份侧表。消息字段与摘要不变。UiEvent 的 generation／revision／serial 使用两个 UInt32 读写完整 UInt64，不截断或舍入原类型编号。

## 验证证据

使用 Windows 本机、合成凭据和临时数据库；未连接真实账号。专项复跑不累加到完整回归数量，嵌套子进程输出不重复累计。

| 检查 | 结果 | 本地证据 |
| --- | --- | --- |
| core 出站授权专项，Release／fault-injection／locked／offline | 20 通过、0 失败、1 ignored 子入口；含 6 项新列表测试 | `build/credential-admin-core-tests.log` |
| workbench_host 完整 Release／全部特性／locked／offline | 126 个顶层入口通过、0 失败；包含既有 3 个无参数时直接返回的子入口 | `build/credential-admin-host-full.log` |
| 宿主最终凭据专项 | 7 通过、0 失败；在完整回归后新增错误类型与过滤空页两项测试，未重复累计 | 子代理定向执行回执；`workbench_host/tests/credential_control.rs` |
| Flutter 凭据组件与请求缓冲区 | 13 通过、0 失败（9 组件＋4 缓冲区） | `build/credential-admin-dart-tests.log` |
| Flutter 原生凭据、既有 IO 与插件库回归 | 19 通过、0 失败（2 凭据原生＋1 IO 原生＋16 插件库） | `build/credential-admin-native-tests-final.log` |
| Dart VM 原 UI 与跨平台向量 | 9 通过、0 失败 | `build/web-ui-vm-tests.log` |
| 真实 Chrome JavaScript 精确身份与 UInt64 向量 | 4 通过、0 失败 | `build/web-ui-chrome-tests.log` |
| Web 生成适配器正负测试 | 4 通过、0 失败 | `build/web-ui-generator-tests.log` |
| 完整 Flutter Web JavaScript Release | 通过，产物 `build/web` | `build/web-ui-release-final.log` |
| 主应用相关 Dart 分析、核心 UI／绑定分析 | 无问题 | `build/credential-admin-analysis-final.log`、`build/web-ui-analysis-final.log` |
| 宿主全部目标／特性、core 出站测试严格 Clippy | 通过 `-D warnings` | `build/credential-admin-{host,core}-clippy.log` |
| core 含 web-storage 的 wasm32 库检查 | 通过 | `build/credential-admin-core-web-storage.log` |
| 核心及私有协议生成、中英文资源一致性 | 通过 | `tool/generate_core_client.py --check`、`tool/generate_workbench_client.py --check`、`tool/build_i18n.py --check` |
| 冻结 SDK 原件 | 36 固定文件、13 对原 Wasm／包通过 | `build/credential-admin-sdk.log` |
| 新宿主凭据与修改的核心 Rust 文件格式、差异空白检查 | 通过 | rustfmt 定向检查及 git diff 检查回执 |

原生测试使用原 Rust guest，SHA-256 `a0eaadc910c086fb46f46125e9c0e720bf9d29e32393651bd44f2653f4ab67ab`；未重建或重新打包冻结插件。

原生凭据测试覆盖 Windows DPAPI 实际解密、创建后关闭重开、替换和过期修订冲突、停用后重开、显式输入新秘密恢复、错误类型引用、空凭据页跨重启续读。真实 Flutter 输入控件经实际宿主保存和停用；缓冲区测试覆盖待写入时保持有效、写入失败、配置失败及超大帧拒绝。宿主还验证有效请求的 8 种字节偏移、尾部数据拒绝、响应不含秘密或密文。

独立复核发现借用读取的对齐要求和可变请求缓冲区残留，均已修正并增加回归。辅助模型本轮没有可用的 SubagentBridge 调用接口，未将 DeepSeek 计作已执行审查。初次原生组合运行误用了不存在的既有测试文件名，凭据两项当次通过但组合退出失败；修正选择后最终 19 项全部通过，初次日志保留。分析过程中 Windows 文件锁导致的旧失败与一条测试格式提示均未计作通过，最终定向分析无问题。

## 边界与后续

Web 验证仅覆盖上述身份、UiEvent 字段及 JavaScript Release 构建；不等于全部 runtime UInt64 字段、OPFS、Web IO 或 Wasm Release 产品验收。未构建 Windows 安装包，未进行移动端／Linux／macOS 运行验证，未更改版本或发布。

下一步保持原 Storage／审计 Session／HostRuntime／Pool 的唯一所有权，完成网络作业接线；禁止另建数据库或运行时绕过所有权。之后提供端点批准与短响应 start／poll／read／cancel，并验证主应用真实 HTTP 效果、撤销与重启核对。文件系统后端、API 节点管理、Unknown 核对、因果证据、三语言 IO SDK 继续按原退出门槛推进。

合同：[主应用管理接口](../docs/PLUGIN_IO_MANAGEMENT.md)。状态：[开发看板](../docs/DEVELOPMENT_BOARD.md)。本轮为本地开发与验证，不代表已推送 GitHub 或发布 Release。
