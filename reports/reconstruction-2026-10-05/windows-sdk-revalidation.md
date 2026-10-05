# C02 Windows SDK 复验与最小修复

2026-10-05。云端输入提交为 `468ef2e912ac74e5f97f0016a8729b7d5c1f5399`，树为 `7352d48321d64a72d38a40b951e0cda81d0b9b29`。独立候选保留了原 Windows 开发工作树及其 `20669f67` 提交。本阶段先执行云端原始源码，再对发现的 Windows 问题作最小修复并复验；没有以 Linux 历史通过代替 Windows 实测。

## 结论与范围

**PASS_WINDOWS_REVALIDATION_SCOPED**。1258 项相关源码输入及 11 项本地修复已在下一 SDK 阶段前独立封存。原 SDK327、冻结57及原 Rust/C/C++ dependency、base、provider 均保持原字节，没有重建、重打包或重新封签。

原生验证采用 Rust 1.95.0、Windows x64 MSVC、Capnp 1.4.0、Release、锁定且离线构建；每条命令保留原始输出、退出码、工具和实际执行文件摘要。Python 使用已安装的 3.12.14。仅运行合成文件、临时普通 Store 与本机无密钥服务。

| 验证组 | 最终 Windows 实测 | 计数边界 |
|---|---:|---|
| 原 dependency / base / mapping / shared objects / reader | 3 / 9 / 7 / 14 / 9 通过 | 42 个唯一方法；映射和 reader 的辅助／过滤方法不计入 |
| 网络 transport / SSE / WS / managed adapters | 96 通过 | 九个实际执行的原生 harness；失败、忽略、过滤为 0 |
| changes metadata Core | 18 通过 | 独立 integration target |
| 原 Core channel / sealing 与 receiver liveness | 5 / 4 / 1 通过 | liveness 为定向 unit，53 个过滤方法没有通过资格 |
| changes source runtime | fault 构建 32 通过，production 构建 30 通过 | production 30 是同一 32 的子集，不能合计为 62 |
| 原 runtime 回归 | 57 通过 | 六个指定 target，不是整个 runtime 全量验收 |
| 独立 changes Rust SDK / preparation policy | 3 / 7 通过 | preparation 是工具策略测试，不能代替 guest 执行 |
| 同源 codec corpus | Rust / C / C++ 各 209 个判定 | 各 6 接受、203 拒绝；这些是向量，不计为 627 个测试方法 |
| 诊断工具 Python | 178 个方法中 176 通过、2 POSIX skip | 包含 176 个旧方法及 2 个新增文件身份回归；skip 不算通过 |
| 原生 discovery / preflight | 7 / 7 通过 | Unix-only 方法没有 Windows 资格 |
| 实际 host / 独立诊断包 / developer CLI | 30 个预期命令结果 | 静态 prepared / rejection / 参数结果，不计为业务执行方法 |

三语言 NEW WS 与 NEW changes guest 均实际构建并运行；其工具、Wasm 和包摘要另行记录。SSE 重用历史 NEW 三语言夹具的原字节，并在运行前后核对摘要。上述夹具与被冻结的 SDK 原件分开计量。

## 复现与修复

1. **Windows 文件身份误判**：Python 3.12 的路径 stat 与 fd stat 的 `ctime` 含义不同；路径 stat 还按 `.exe` / `.cmd` 名称添加执行位。诊断消费者与 source-only 打包器改为在 Windows 比较一致的 birth time，并仅屏蔽合成执行位。设备、inode、文件类型、读写位、长度、mtime、字节摘要、regular/reparse 与读取限额检查保留；POSIX 规则保持原样。新增正反回归，测试夹具使用 Windows 可执行入口和明确的子进程退出交接。
2. **超时首因被清理取消覆盖**：expired delivery 触发自动 cancel 后，尚未发布终态的 worker 返回 Cancelled。修复不在原 deadline 已到期后额外自动取消，让实际原 timer 返回 Timeout 并完成 join；没有事后改写 completion，也没有改变显式取消的优先级。定向测试阻止当前线程发布 timer 结果后先执行 delivery，再核验实际 worker 终态；显式到期前取消作为对照。
3. **背压测试假设系统缓冲容量固定**：原测试要求至少八次 64 KiB flush。Windows 实测三次 flush 后已真实阻塞。测试保留原 send future，核验取消瞬间仍 Pending、真实写入字节、取消中断及实际 join，不以 quota 或错误替代背压。最终记录 3 次 flush、196650 wire bytes；本轮取消／join 记录 0 ms，不作为一般延迟保证。
4. **注册表发布失败夹具移动了被锁定目录**：Windows 正确拒绝移动仍有 owner lease 的 registry 目录。改为仅阻塞实际 `selection.morrow` 发布目标，保持 lease 不动；证明发布失败前旧批准已失效、revision 不变、无 ACK、恢复后旧批准不复活，并核对恢复的 snapshot 原字节与 SHA256。生产 registry 实现未修改。
5. **新 changes guest 工具假定 Unix 工具布局**：保留原 WASI 布局默认值，补充 Windows `.exe` 发现及成组的显式 clang / clang++ / sysroot 选项。缺少任一显式参数或工具就拒绝；没有安装工具链、网络 fallback 或旧 SDK 修改。

首轮网络 94 通过／2 失败、changes fault 31 通过／1 失败及 production 29 通过／1 失败、Python 首轮错误和所有重试均保留。记录脚本的计数／编码／汇总错误与功能失败分开保存，没有删除旧结果。

最初网络构建失败来自长缓存路径中的 MSVC include 路径。相同 compiler 和相同 ring 输入的独立 probe 在长路径退出 2、短路径退出 0；crate archive 与 header 摘要正确。新建短目录并逐项核对 13810 个公共 registry 文件后构建通过，未修改旧缓存或 dependency lock。

实际旧 Windows host 仅收到 `--sdk-capabilities`，新 preflight 被消费者拒绝；source-only 诊断包在源码目录之外执行并核对前后清单。没有将静态 prepared 当作安装、guest 执行、授权或生产路线验收。

## 保留的边界

- protected owner 9 项、真实数据库、DPAPI、账户密钥：**NOT_RUN，用户当前范围排除**。
- 实际子进程普通用户 token：**NOT_SAMPLED**；管理员编排不等于普通用户资格。
- GUI、生产 catalog / owner 绑定、系统选择器、TLS／外部 API／真实账号：**NOT_RUN**。
- Linux 重新资格、Android／Web／其他平台、断电与恶意文件系统隔离：**NOT_RUN**。
- 全目标 SDK、生产通道、长期 changes watch／完整 cursor 恢复等门槛：**OPEN**。
- 本阶段没有提交、推送、main / tag / Release、CI 或安全设置调整。旧 stage16 丢失的增量没有恢复。

下一阶段补充独立版本的 changes payload discovery，保持旧 envelope、四类 IO discovery 和权限规则；能力信息必须继续区分编译存在、静态准备与生产绑定。
