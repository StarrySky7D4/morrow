# TLS 受保护身份封装与原生加载

2026-09-21；基线 `b625540`，应用版本仍为 `0.1.9-test.52+56`。这是证书持久保存/轮换的前置实现，尚未把受保护引用接入服务启动或 UI。

## 已实现

- `core::tls_identity` 定义独立版本化 Protobuf＋LZ4 密文封装，绑定资料库身份、随机引用、正修订号、证书 PEM SHA-256、保护提供方与禁用标记。固定字段按顺序预检后解码，拒绝未知/重复/非规范字段、超限数据与损坏容器。没有 Debug 实现。
- Windows `audit::tls_identity` 在独立 DPAPI 用途域中加密证书链和私钥。内部密文也认证上述身份/修订/证书摘要。解封要求来自调用方的预期资料库、引用和修订，不能通过修改外层元数据重新绑定。
- 明文 protobuf、压缩输入、解压输出与返回材料释放时清零。字段上限在解码分配前检查；单份 PEM 仍为 65,536 字节。TLS 专用 DPAPI 上限增加到 144 KiB，以容纳两个最大材料和封装；原审计密钥及 HTTP 凭据保持原 64 KiB 边界，三个用途域互不解封。
- `TlsSelection::protect` 从冻结的明确文件选择重新读取、重验摘要/配对/证书格式再封装；`load_protected` 从密文恢复并重新检查配对和实际有效期。它们不写数据库、不启动监听、不代替原服务批准；非 Windows 返回明确不支持。

当前用户 DPAPI 通常要求相同登录凭据及计算机，但漫游配置存在例外；这里未使用机器范围保护。它不防御同用户恶意代码或任意备份回滚，也不能替代资料库修订与 live authority 检查。依据 [Microsoft CryptProtectData 文档](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)。

## 验证证据

| 范围 | 结果 |
| --- | --- |
| core lib | 31 通过、1 个原有子进程专用 harness 忽略；其中新封装 2 项 |
| audit lib | 14 通过；其中新保护/解码 4 项，旧域隔离测试扩展 |
| workbench host lib + service protocol | 84 + 7 通过；新增原生保护加载 2 项 |
| 静态检查 | core lib、audit lib/tests、host lib 严格 Clippy 通过 |
| 可移植格式 | core 的 wasm32-unknown-unknown lib 编译检查通过，7 项既有未使用代码警告；不代表 Web 密钥保护已实现 |

实际调用 Windows DPAPI，使用合成材料验证：移除源 PEM 后仍恢复正确身份；不配对私钥、损坏 PEM、改变已检查文件、跨资料库/引用/修订替换、禁用/篡改及错误用途域全部拒绝；过期证书恢复真实旧有效期而不续期；两份最大不可压缩材料可往返。数据级测试不是跨 Windows 账号/设备验收。

完整宿主测试仍有原有 `prepare_write` 测试配置 dead-code 警告；生产 lib Clippy 无警告。日志为 `build/tls-protection-*.log`。本轮无界面改动，未重建整套 Windows 应用、未推送、未发布。

## Bridge 使用与审核

原授权过期，确认无运行/排队任务后通过官方 manager 分别申请 8 次有界任务授权并 reload；没有修改账本或供应商限额。

GLM-5.3-flash/max `task_a636563a900521d2f6e99c9f` 首稿错误使用 prost API 和 Error 变体，拒绝。给予真实 API 后，`task_fed045f1ad3096a3f3b9f85d` 可用的字段比较/切片部分经修正采用；主代理补回必需字段顺序、完整性、schema/revision 精确比较，并消除错误 wire type 导致的 unreachable 分支。新增回归覆盖这些缺陷。执行成功不等于答案可直接合并。

DeepSeek-flash/max `task_e4305961a9c2d3e5b072452e` 完成独立审阅。其关于私有 prost Clone 和有界解压分配的推测未构成确认缺陷；秘密类型不从模块导出，Drop 对所有实例生效，输入上限在分配前校验。没有据此放宽边界。

## 后续必须完成

1. 原 Store 新增受保护 TLS 记录：Schema 迁移、逻辑额度、完整性校验、精确 CAS、禁用墓碑、分页与快照/备份路径都必须覆盖。使用已有 `service_authority_identity`，不另开数据库，也不伪装成 HTTP 凭据。
2. 为 TLS 身份新增独立 `ServiceAuthorityResource`，在原协调器内建立依赖；替换/禁用只撤销相关活动服务。封装本身不能判断旧版本新鲜度，必须从原 Store 当前记录恢复并核对修订。
3. 私有控制命令只接收路径或已选引用并返回脱敏元数据；私钥明文不经过 Dart 或 guest。启动冻结引用、修订和摘要，原拥有者重验后收窄共同有效期。
4. 显式轮换采用停止、检查新身份、CAS 保存、重新批准/启动的可核对流程；失败保留旧持久记录但不自动重播 Unknown 启动，不承诺热换证书或 ACME 自动续期。
5. 完成真实服务撤销/缓存交付、故障恢复和 Windows 窗口验证，再推进 Unknown 持久核对、完整文件系统与 C/C++/Rust IO SDK。SDK 仍未稳定。
