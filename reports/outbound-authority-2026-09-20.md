# 持久出站批准与 Windows 凭据验证

结论：`PASS_SCOPED`。原数据库已能保存出站端点批准和系统保护凭据，关闭重开后重新校验实际插件身份、权限及政策，再签发活动 HTTP 授权。撤销与到期检查贯穿外部请求和结果交付。这是宿主底座进展，尚未接入主应用的批准／凭据管理界面，不代表插件系统或全平台资格完成。

## 实现范围

- Store v20 在原数据库增加有界、严格 Protobuf＋LZ4 出站记录，保留原授权身份，沿用共享容量、修订比较、原 worker 和授权锁。凭据与端点更新保守撤销现有入站／出站恢复授权。
- Windows 当前用户 DPAPI 保护凭据；独立保护域绑定引用、修订与时间元数据。复用既有受控原生调用，无新增 unsafe 块，也不创建另一套明文配置或密钥文件。
- 恢复时先验证原 Store、实际实例、插件包身份／摘要、HttpRequest／CredentialUse、规范 origin、网络 profile、TLS 根和额度，再解密凭据。拒绝 URL 库会重写的地址别名。
- 凭据仅在受管传输中注入。持续检查原数据库租约、共享 UTC 高水位和单调到期时间；禁用、到期或时钟倒退后，不恢复旧权限，不交付旧结果，不自动重发已发生的远端效果。
- 冻结的 SDK／guest 原件保持不变。应用版本仍为 `0.1.9-test.52+56`。

## 验证证据

Windows 本机、临时数据库和合成测试主体。统计每个顶层套件最终结果，排除嵌套子进程的重复输出；专项复跑不累加到完整回归总数。

| 检查 | 结果 | 本地日志 |
| --- | --- | --- |
| core 完整，fault-injection／locked／offline | 557 通过，0 失败，7 ignored | `build/outbound-authority-core-full.log` |
| plugin_runtime 完整，all-features／locked／offline | 392 通过，0 失败，2 ignored | `build/outbound-authority-runtime-full.log` |
| network_node 完整，plugin-adapter／locked／offline | 100 通过，0 失败 | `build/outbound-authority-network-full.log` |
| audit 完整 Release，all-features／locked／offline | 91 个顶层入口通过，0 失败 | `build/outbound-authority-audit-final.log` |
| workbench_host 完整 Release，all-features／locked／offline | 116 个顶层入口通过，0 失败 | `build/outbound-authority-host-full.log` |
| core／runtime／network／audit 全目标严格 Clippy | 均通过 `-D warnings` | `build/outbound-authority-{core,runtime,network,audit}-clippy.log` |
| core 默认 wasm32 库检查 | 通过，既有 dead_code 警告 | `build/outbound-authority-core-wasm.log` |
| core 含 web-storage 的 wasm32 库检查 | 通过 | `build/outbound-authority-web-storage.log` |
| runtime 默认 wasm32 库检查 | 通过 | `build/outbound-authority-runtime-wasm.log` |
| 冻结 SDK 原件检查 | 36 个固定文件、13 对原 Wasm／包通过 | `build/outbound-authority-sdk.log` |
| 新增／实质修改 Rust 文件格式 | 19 个文件通过 | `build/outbound-authority-format.log` |

相对前置提交新增 30 项有效测试：core 14、runtime 3、network 5、audit 8。ignored 为父测试调用的崩溃／竞争子入口；audit／host 的顶层入口数包含既有无参数时直接返回的子入口，不将入口数宣称为独立产品场景数。

专项覆盖真正关闭重开数据库、Windows DPAPI 解密并完成真实 HTTP 头注入、未批准／错误实例／过期／禁用／证书错误在解密回调前拒绝、凭据轮换／撤销、发送后的撤销和 Ready 交付检查，以及迁移与提交边界崩溃。请求原件验证不含注入的凭据。

完整审计首次运行发现旧库测试夹具保留了后来新增的表，触发既有完整性校验；仅修正夹具，未放宽生产检查，完整复跑通过。初次日志保留于 `build/outbound-authority-audit-full.log`。

严格检查随后要求调整函数位置、合并条件，并将 worker 更新消息装箱。这些结构调整后重新通过严格检查及 runtime HTTP／service 专项（5＋18 项）和 managed HTTP 专项（18 项），日志为 `build/outbound-authority-runtime-final.log`、`build/outbound-authority-managed-http-final.log`。完整回归计数不重复包含专项。

工作台宿主使用既有 Rust guest，SHA-256：`a0eaadc910c086fb46f46125e9c0e720bf9d29e32393651bd44f2653f4ab67ab`。未重建或重新打包冻结 guest。

## 验证边界与后续

本轮证明数据库重开与新宿主授权路径，不宣称完成主应用重启体验、真实账号、浏览器 OPFS、Linux／macOS／移动端或安装包验证。wasm 检查仅为编译证据。Windows 当前用户保护不抵御同一用户下的恶意进程；HTTP 库内部副本不承诺清零，远端回显的秘密可能进入响应材料。

下一步接入批准与凭据录入／轮换界面，继续完成其他平台提供者、OAuth／多账号、路径级政策、持久授权配置审计链、可信时间恢复、Unknown 核对、因果证据及退休流程。完整文件系统接口和 C／C++／Rust SDK 产品验收仍按原路线推进。

合同：[持久出站批准与系统保护凭据](../docs/PLUGIN_OUTBOUND_AUTHORITY.md)。任务入口：[开发看板](../docs/DEVELOPMENT_BOARD.md)。本轮仅本地开发、验证和提交，未推送或发布。
