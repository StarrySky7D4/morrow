# test.17 Windows 受保护密钥与封存入口

日期：2026-09-12。audit 模块为 `0.1.9-test.17`，核心仍为 test.16（新增读取链尾的宿主 API）；Windows 应用包仍为 test.14。未推送或发布 Release。

## 实现

新增 Windows 当前用户 DPAPI 密钥文件，私钥与日志身份来自系统随机源，不使用固定测试种子。密钥明文只用于受控内存处理；文件内容是保护后的密文与版本化 Protobuf＋LZ4 容器。额外文件校验检测编码损坏，OS 保护和内层公钥一致性分别校验密文与密钥内容。

明确区分创建与加载。已有目标一律不覆盖；缺失、损坏或解密失败不会生成替代身份。临时密文同步后无覆盖发布，读取已发布文件复核。工具不输出私钥，私钥类型不提供 Debug 或明文导出。

独立 Sealer 服务从核心已封存链尾恢复，以批次数、事件数、段字节上限约束执行。大批次自动缩小，保持事件顺序。确认仍调用核心原子封存 API，不新增插件写入或签名旁路。

## 当前验证

- 实际系统随机密钥创建两次获得不同公钥／日志身份，保护文件重载保持相同身份。
- 缺失、损坏、超限文件拒绝；重复创建拒绝且原文件字节保持不变。
- 初次损坏测试发现某种编码字节变化仍能被读取，已加入完整文件校验。后续测试还重算文件校验再损坏密文，或重新保护不匹配的私钥／公钥，均拒绝。
- 130 个事件分两批封存，第一批处理 128 个，重启加载原密钥后处理剩余 2 个；错误密钥不能改变已有链或清理剩余事件。
- 发布前、发布后、首批封存后 3 个真实子进程中断，退出码 86。发布前没有最终目标；发布后只能加载既有密钥；首批已提交后恢复仅处理余下事件。
- 35 MiB 真实不可压缩合成正文使整组超过读取上限，服务将 5 个事件拆成 4＋1，保持序号，不跳过或重复事件。Release 定向测试通过。
- 真正的 Release 封存工具使用随机受保护密钥跨进程加载，首次封存 1 段／3 个事件，第二次为 0 段／0 个事件。独立只读验证器核验成功，核验前后数据库 SHA-256 相同。
- 严格 Clippy（含故障注入）通过；Wasm 可移植验签库编译通过，Windows 密钥后端不被编入 Wasm。

随机密钥样本目录：`build/audit/protected-key-149fa60e6e46480db9bd955a1d0900bf`。该目录只含全新合成数据；公钥与日志 ID 为测试核验材料，未使用用户数据库。保护文件本身不作为对外分发附件。

工具：

- `build/audit/release/morrow-audit-seal.exe`，SHA-256 `961b911f6544718f137fcdf3d64fdaef16940df3a3de3212094fa33d09463e0b`。
- `build/audit/release/morrow-audit-check.exe`，SHA-256 `7fefd671837be9ea36b8393f093011ae3453101caf1e374f6e7d659ca5eda2e6`。

复现入口：`tool/verify_audit.ps1`、`audit/tests/keys.rs`、`audit/examples/qualify_keys.rs`。

## 边界与下一步

尚未把创建密钥或 Sealer 接入工作台启动／保存流程，因此应用没有自动开启审计封存。下一步需要确定首次身份绑定、已有库初始化、密钥丢失时的恢复入口及错误状态，避免把“文件缺失”误当成“首次使用”。

同一系统账户之外的恢复、账户迁移、轮换、其他平台保护、设备断电、硬件故障和后台调度未验收。文件校验不是鉴权机制；同一账户内完全控制宿主的攻击者不受此机制隔离。没有独立检查点时不宣称历史未回滚或获得见证。

参考：[CryptProtectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)、[CryptUnprotectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptunprotectdata)、[BCryptGenRandom](https://learn.microsoft.com/en-us/windows/win32/api/bcrypt/nf-bcrypt-bcryptgenrandom)。
