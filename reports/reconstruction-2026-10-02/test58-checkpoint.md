# Morrow test58 重建检查点

日期：2026 年 10 月 2 日 UTC

应用版本：`0.1.9-test.58+62`

本检查点完成了共享 SDK、channel runtime 和 Workbench 调用侧的有界重建。
Linux 范围明显小于丢失的候选版本：受保护 SQLite 打开对象证明、受保护调用点、可信宿主监督和 GTK 生命周期尚未重建。历史输入列出的 73 处范围仍待重新逐项审计。
Windows 原生执行器的两处退出边界仍待运行验证。本检查点不能作为完整 Linux 产品或 Windows 原生所有者验收。

## 来源和同步

- 从精确基线 `925fb8ca6563dcb7de38db8fbdf6b005f9ad5420` 的 test57+61 独立重新实现
- 丢失的 116 文件版本没有提交或推送；这里没有恢复原始脏工作区字节，也没有沿用旧通过结论
- 已完成阶段分别保存本地提交、累积源码补丁和 Git bundle，并同步 GitHub 开发分支与私有 Google Drive 备份；Library 另存累积源码备份
- 版本递增前检查了八个远端分支的精确提交，最高应用版本为 test57+61；main 仍为 test56+60
- 阶段 10 同步提交为 `d90a247b825448b86aefdf680053f7d74b7e97a4`，树为 `b13d1ec02e8c6dcbda452fca30931d494cfa4187`
- 检查点沿用开发分支 `codex/linux-sdk-reconstruction`；没有合并 main、创建 tag/Release 或运行 Actions/CI

## 已完成的重建范围

1. runtime 在认证原始所有者后，将终止失败绑定到原始共享 Control。清理在队列锁内保留首个 Revoked、Expired 或 Unknown 原因，生产者 panic/drop 不覆盖已有原因；实际 join 与业务结果分开记录，不重放原任务
2. SDK 提供有界可复用 WasmClient。一次提交只编码和散列一次请求，冻结输入和摘要且不与输出别名；原生 callback 的整个输出容量在每条退出路径清零。公开错误优先级、规范编码检查和 ABI/schema 保持原状，C FFI 去除重复编码/散列
3. 新 channel Rust 项目采用 opt-level 3，仍保留 20M fuel、16MiB、16 次宿主调用；旧客体、不可变包和 schema/pins 不覆盖
4. 调用侧 001 使用 generation/key fence，晚到 status/Close 和旧 refresh finally 不能覆盖新 prepare；在校验、监听器和 await 前冻结调用者 frame 列表
5. 调用侧 002 根据真实规范编码做 256 字节单端点 Directory 容量预检，早于 live grant。Dart 只筛选对应的合法 source-only 元数据入口；65 字节私有旧路由保留
6. 调用侧 003 在任何 append/run 前检查返回 kind、六个 ceiling、单端点身份、生命周期、总量、数量和每个冻结 frame。允许合法收窄，包括 maxRequests=1；已识别原 key 保留给显式 status/Close，不自动 Close、重试、拆分或重放
7. discovery 要求相应约束为正确类型且精确 false。实验 channel codec/transport 和有限本地 bytes/events 已存在；正式公开 Workbench binding、SSE/WS 网络适配、Cloud 内容变化源及异步依赖仍未获得资格
8. canonical host Cargo.lock 只补 audit 的 async-io/futures-lite 两个直接依赖边，包版本和 checksum 不变

## Linux 安全基础的边界

已实现并测试受保护 key provider 的有界基础：实际 Unix peer UID、NoAutoStart、唯一 owner 及每次材料调用前重新验证、getpwuid home、无明文 seed 引用和失败关闭策略。
O_PATH/fstatat 元数据检查覆盖目录和 SQLite sidecar 的模式、UID、单链接和 NOFOLLOW；实际 SQLite/WAL fcntl 锁保持测试通过。

这些元数据检查不能证明 SQLite 真正打开的对象。路径事后检查仍有替换/ABA 风险，不能先进行 SQL/migration 再验证路径。
因此四个 private Store 入口在文件创建和 SQL 前明确拒绝；generic Store 的原有可移植行为保留，不作为受保护存储资格。
受保护 SQLite 实际 inode/VFS 证明和生产读写、status、snapshot、backup、recovery 等入口尚未完成。历史重建输入列出 73 处审计义务；当前 grep 结果包含测试位置，尚不是重新核实的去重生产入口计数。

真实 SecretService/GNOME 正向验证仍受平台 socket EPERM/home EROFS 阻塞。没有重试被拒绝的操作、变更权限、用 fixture 替代真实服务或声称 Linux protected owner 可用。
pidfd/boot ticks、编译期宿主 SHA pin、sealed memfd exec、PDEATHSIG、可信 Wasmi 宿主内 no_new_privs/seccomp、资源释放证明和 GTK close/heartbeat/EPIPE 也仍待重建与验收。

## 本检查点的新鲜验证

固定 Rust 1.96.0、Capnp 1.5.0；Flutter 3.44.0 精确 `559ffa3f75e7402d65a8def9c28389a9b2e6fe42`，Dart 3.12.0。
Rust 使用 locked/offline；Flutter 使用 no-pub 和已解析的锁定依赖。`CI=true` 仅用于官方 bot 检测短路，排除已拒绝的 metadata 访问，不运行 CI/Actions。
Flutter 的独立 HOME 仅存 analyzer 缓存；core 合成 authority 测试仅用新 XDG_STATE_HOME，生产保护身份仍来自 getpwuid。

| 检查 | 新鲜结果 | 范围 |
|---|---:|---|
| SDK native | 91 通过 | 包含 8 个新增 transport 回归 |
| SDK Wasm guest/C | 两项编译通过 | C 路径编译不等于真实 Wasmi C ABI 验收 |
| runtime controls/compat/faults | 40 通过 | 原始 Control、首因、真实线程 join |
| Directory 容量 helper | 4 通过 | 实际 Windows live-grant 路由未在 Linux 验收 |
| Linux 元数据 | 7 通过 | 包括真实 DB/WAL 锁保持和 private 入口无副作用 |
| core 合成 unit | 42 通过，1 忽略 | 独立 XDG_STATE_HOME，不是受保护所有者证明 |
| audit 默认套件 | 35 通过 | 不把子进程 helper 重复算作额外测试；真实 SecretService 正向未运行 |
| canonical host | locked/offline 编译通过 | Linux 编译，不是 Windows 所有者验收 |
| Python 工具/SDK/pins | 52 通过 | 冻结基线、模板、约束和 source-lock 回归 |
| Dart 调用/catalog/UI | 97 通过 | fake-backend unit 范围 |
| Dart analyzer | 9 文件无问题 | 限定修改路径 |
| real Wasmi 003 严格比较 | 3 测试通过，4 次实际运行 | 两种 transport × bytes/events 均完成 |
| real Wasmi 001/002 stress | 6 通过，2 失败保留 | 真实 fuel 边界，不能称全绿 |

1945 个相关仓库输入文件在各段运行前后完全一致。最后 source manifest SHA256：
`e382cb9c816ae4378878d3efcca556202ececf47e4f21ca53b022f86ca5f1db8`。
全部命令、显式环境、执行日志和前后源码清单保存在本检查点备份的 `logs/test58-checkpoint/final`。

初始 Directory filter 匹配到 0 测试，初始 Python 命令因缺 tool/tests import 路径失败；更正的是命令，不是源码。原始零执行/失败日志保留，后续实际 4/52 通过才计入上表。
最初 1799 文件的清单未包含既有 plugins/workbench 和 host path-dependency 等目录；补齐完整路径闭包后，同样整组测试再次执行，1945 文件无漂移。该子集不被当作完整输入闭包。
更早同名负控共享构建缓存碰撞、源码漂移和编译未运行日志也保留；最终负控使用独立包身份，不能拿早期结果替代固定源码验证。

## real Wasmi 的精确结论

003 one-shot 客体源码与不可变 Directory example 逐字节一致，可复用客体仅有两处 transport 替换。
两者 bytes/events 四次实际运行均 `Ok(0)`，11 次 metered channel call，消费 5×32768=163840 字节，显式 Close，实际原生产者 join，原 ACK/hash/cursor 校验通过。
payload SHA256 为 `12a8659000a14e107b88bc40ee759d70c4749f68c01ee54157537b71008d6d16`。
两种 transport 都被同样的严格成功断言约束；one-shot Limits 不能让 aggregate 假绿。

001 可复用和 matched one-shot 的 2×65536 字节通过；4×65536 在原 20M fuel 内真正 Limits，保留为负边界。
002 使用相同 5×32768 字节但额外执行 Query 和 owned-vector/equality scans，两种 transport 都在 15 次调用、ACK5 后 Limits，未到 Close/completion。
002 的两个可复用成功期待测试继续为红；不能由它推断原 Directory 操作失败。001/002 one-shot 测量控制必须结合精确 receipts，而不能仅看 aggregate。
没有扩大预算、删掉 stress 检查或声称所有尺寸都合格。清理后 ProducerOutcome::Unknown 保留，不伪造 EOF 或业务成功。

## 仍然存在的基线失败

当前 core `evidence_chunks` 为 4 通过/6 失败，`file_content_receipt_store` 为 3 通过/4 失败。
同样 10 个名称在精确 test57 源码的独立包负控中复现，属于保留的基线失败；本检查点没有修复它们，不能称完整 core 全绿。

evidence_chunks 六项：

- audited_readonly_v7_does_not_migrate_and_v8_preserves_existing_signature
- legacy_migration_preserves_raw_unknown_fields_exact_container_and_commit_bytes
- format_eight_readonly_then_migration_preserve_unknown_v1_bytes_and_signed_history
- schema_two_hidden_in_format_eight_is_rejected_without_version_repair
- format_nine_preserves_legacy_maximum_intent_and_signature_across_migration
- expanded_intent_roundtrips_in_ten_but_cannot_be_smuggled_into_nine

file_content_receipt_store 四项：

- v22_import_without_event_room_rolls_back_and_succeeds_with_room
- existing_v22_content_is_marked_legacy_import_on_migration
- receipt_survives_pending_seal_snapshot_and_pinned_readpoint
- live_receipt_after_cancel_is_rejected_but_legacy_import_after_cancel_is_valid

初次 core unit 的另外 9 个失败是只读 HOME authority-lock 环境问题；在获准的隔离 XDG_STATE_HOME 合成环境下，42 unit 通过。这个结果不解除真实受保护环境阻塞。

## Windows 阻塞和未运行项

Windows 在独立 stage07 pin 上已完成 SDK 84、native C/C++ 7、runtime Control 15 和 Job 4；这些不是本检查点新 SDK 的 Windows 回归结论。
原生 private-output-cap 与 finish_io maintenance 失败后的原始 Control 停止，已有测试准备，实际 executor 没有运行，拟议修复没有应用，也没有并入共享检查点。

获准隔离 DPAPI 诊断只建立了 4/12 个合成 DB，0 个发布 key。Workbench 原始错误为 `SessionError::Key(KeyError::Protection)`。
另一个同账户 CryptProtectData 诊断返回 Win32 2，与缺 ProfileList/user hive 的观察一致；这个 2 不是从 SessionError 解包出的 Win32 码。
已停止进一步 DPAPI 操作，没有修改账户、profile、registry、ACL 或借用其他身份。须获得有效同账户环境后再做原始负控、最小修复和独立 review。

正式 product-owner、WM_CLOSE/窗口、监督进程配对故障、catalog 原生 UI、真实 SecretService、网络 adapter/Cloud source 都未获得本检查点资格。
SDK 仍为可继续开发的实验状态，没有冻结。

## 目录丢失复查

重建前确认 repo/tools/build/deliverables 不存在。当前 kernel boot 为 00:55:05 UTC，workspace/shared birth 为 01:03:22 UTC，初始 overlay 使用量约 138MB，明显小于旧约 29GB。
这些证据强烈指向重新初始化/替换的文件系统视图或旧状态恢复/挂载失败；精确触发者、生命周期动作和责任方没有 audit 证据，不能确定。
相同路径、文件系统类型和容量不能证明是同一持久磁盘实例。已保存清理后的事件证据，并把完成阶段放到 GitHub、Drive 和 Library，核对下载字节，降低再次丢失源码的风险。

## 后续阶段

1. 先设计并验证 owned/documented SQLite VFS 的 xOpen 前实际对象 attestation，包含 sidecars 和任何 SQL/header read 前的拒绝；证明不会释放原进程的 fcntl 锁
2. 重新建立完整生产入口清单并逐项封闭受保护 Store 路径（历史目标为 73 处，须重新核实），使用明确 typed/required protection 契约；不允许悄悄落回 generic open
3. 在获准且可用的 Linux 同 UID 环境完成真实 SecretService 和受保护 owner 验收，再重建有 SHA/sealed memfd/pidfd 证明的可信宿主及 GTK 生命周期；限定已测 Debian13 amd64
4. 在有效 Windows 同账户 profile 环境完成 DPAPI 原始负控与两个 native executor seam 的真实 red/green/join/recovery 验证，再同步可验收 Windows 阶段
5. 将正式公开 Workbench binding、网络 SSE/WS、授权 Cloud change source 和 async dependency 分别验收后才改变 discovery authority；CCSwitch/Codex 全面集成留待后续

每个完成阶段继续核对 GitHub commit/tree 与私有 Drive 下载 SHA；源备份保留 base、累积 bundle/patch 和日志，排除 secret、媒体和构建缓存。
