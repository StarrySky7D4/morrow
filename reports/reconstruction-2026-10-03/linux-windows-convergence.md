# Linux 汇合源码本地整合（W11）

2026-10-03，用户提供 `Morrow_Windows_convergence_inputs_after_Linux_Close_UNPUBLISHED_20261003.zip`，98404 bytes，SHA256 `53e91e775bf32cd8dae8f8724aaaf0257c7720075f758c889982bcca612d4fa3`。基线 commit `63f38d4a8a5bf453248dc7532197bee6980dc86f`、tree `a8a74bc4563fae735e9b695f50db4e979ac1393d` 与本地一致。

包的 18 个普通成员安全检查通过，16 payload 与 manifest 逐字节/CRC 核对。source-only patch 共 17 路径：5 个现有 Linux native 文件修改、2 个新增 native 测试、10 个新增独立 bridge crate 文件。补丁 SHA256 `c2861b14f34b672c789360762f9060606b8723d74cb057e7a152101e875362d3`。

所有原目标等于基线，新增目标不存在，与先前 25 项 Windows dirty/untracked 无同路径冲突。独立内存重建、ROOT 隔离 Git check/apply、最终 17 个 SHA256 与 Git blob 均匹配后，按原字节合入本地工作树。25 项本地修改逐文件保留，真实 Git index/HEAD 不变，diff check exit0；合入后当前 42 个源码增量尚未提交，后续本轮状态文档另记于有界源封存。

初次隔离目录位于另一个 Git 工作树子目录，`git apply --no-index` 的 subdirectory prefix 静默排除全部 patch 路径，外层 check/apply 均 0；最终文件核验正确拒绝该 no-op，未写真实源码。第二次显式设置 Git ceiling，先证明不处于任何仓库，再 check/apply/逐文件验证通过。两次原日志保持，不将第一轮外层 0 当恢复成功。

Linux native 继续使用现有 exec-status/controller loss guard，增量补足 pending/loss/EOF 与回收测试。新增 bridge 使用普通 Store 和合成声明，不构造 Windows protected Storage/DPAPI/OwnerBinding。bridge 是独立 Linux-only crate，没有加入通用 workspace；显式在 Windows 构建其二进制不受支持，不能用 cfg 编译排除或零测试建立 Windows 资格。原 ABI/schema/runtime 协议、共享 SDK、原锁和 pins 均未被本次 patch 修改；new bridge Cargo.lock 属于新增 crate。

云端 Close 199 方法和 51318 路径恢复零差异为包内外部汇总记录；ZIP 没有完整实际测试 stdout/stderr、逐命令方法记录或完整恢复源码，本轮不能独立复核这些数值。Windows 本轮没有执行 Linux 测试，也不把云端 Linux 记录计作 Windows通过。此次是提供的 17 路径源码恢复与整合，不等于恢复先前丢失的完整第16阶段材料。

Windows 静态对账：双 frozen root、13 对原 guest/provider、5 contracts、30 stable source 与 7 组测试输入按外置记录核对。计划前 5 组 42 个命名 Windows 方法已有先前同输入原始运行证据；本次汇合 receipt 新执行为 0，不重复计数。后 2 组 9 个 protected Storage/IoWorker/DPAPI 方法超出当前无密钥执行范围，保持 NOT_RUN。production_public_binding_available 保持 false。

下一步分别推进生产 channel/GUI 与公开 owner 接入门槛，以及完整 SDK 的网络流、文件/blob、异步组合、重启核对和其他平台证据。Linux shared backend 不能替代实际 Windows 映射/owned Child/受保护 owner 验证。私有检查点 metadata、链接、标识与凭证没有加入公共源码；本轮没有 commit/push/CI/tag/Release。

同日文件合入后另行执行原冻结依赖3方法，实际 Windows 路由全部通过；这是应用 receipt 之后的新限定运行，见 [汇合后依赖复验](windows-convergence-dependency-refresh.md)。旧 receipt 的执行数0与历史42方法保持原身份，不叠加计算。
