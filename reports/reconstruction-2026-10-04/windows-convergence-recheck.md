# 云端汇合输入复核与当前 Windows 限定验证（C01）

2026-10-04，本地 Windows 开发树 `codex/windows-sdk-qualification-20261003`，HEAD `63f38d4a8a5bf453248dc7532197bee6980dc86f`。本轮保留现有修改，完成来源复核、当前源码验证与有界恢复交付。SDK 仍处于候选阶段。

## 汇合输入与源码

汇合 ZIP 为 98,404 bytes，SHA256 `53e91e775bf32cd8dae8f8724aaaf0257c7720075f758c889982bcca612d4fa3`。18 个成员、16 份 payload、自带清单及 CRC 校验通过。补丁基线 commit/tree 与本地一致；逐 hunk 从真实基线 blob 重建的 17 个 Linux 最终文件，与当前工作树全部相同。

该源码增量已由 [W11](../reconstruction-2026-10-03/linux-windows-convergence.md) 汇入。此次复核需要应用的新增源码为 0；未重复应用补丁。5 个 native 修改、2 个 native 测试与 10 个独立 Linux bridge 文件保持原字节；共享 SDK、Core contracts、ABI 和原 pins 未由汇合输入改写。

执行前另存 95 项当前 dirty/untracked 源码及完整 tracked patch，锁定 827 项明示源码／构建输入、SDK327 和 frozen57。所见相关旧会话为空闲或未加载；启动前针对本项目的 Cargo／宿主进程查询成功且未发现占用。未清理未知目录或覆盖未提交内容。

## 本次实际 Windows 执行

Windows 11 `10.0.26200`、AMD64；Rust `1.95.0`、MSVC `14.44.35207`、Windows SDK `10.0.26100.0`。五条 Cargo 命令均为原生 Windows Release、`--locked --offline`、单线程测试，使用既有 Runtime 独立可再生成 target，日志与合成 TEMP 使用新目录。原 frozen guest/provider 未重建、重打包或重封存。

| 套件 | 本次命名方法 | 结果 | 未计入通过的筛选项 |
| --- | ---: | --- | ---: |
| sdk_frozen_dependency | 3 | passed | 0 |
| sdk_frozen_compat | 9 | passed | 0 |
| shared_memory::windows::tests | 7 | passed | 14 |
| shared_objects | 14 | passed | 0 |
| remote_reader | 9 | passed | 1 |

共 42 个唯一 `(suite, method)`，5 条原始 exit0，失败／忽略／zero-match 均 0。14＋1 个 filtered 项保留原摘要；两个 mapping helper 和一个 reader helper 不增加通过数。三组 dependency 使用原 Rust/C/C++ guest 与原 rust-provider，经既有 dynamic dependency、SharedObjects、真实 Windows FrozenRegion 路由执行。base 9 项覆盖旧 task/transform/UI 协议；没有 Flutter 渲染验收。

只读写入父方法已通过；另从本次实际运行的未改 harness 直接执行隐藏合成 helper，记录 marker `mapped-readonly`、实际 OS exit `0xC0000005`、0.016 秒。该预期故障子例不另计方法，也不表示业务成功。

每条命令保存 argv、UTC、白名单环境、PID、原始 stdout/stderr、退出码、命名方法、工具及前后输入哈希。实际 stderr 指向的五个测试二进制和 reader 辅助二进制另存哈希与副本；历史同前缀 target 产物不计作本次执行。5 组 TEMP 零文件残留。SDK327、57 原件、827 输入与实际 HEAD/index 前后恒同。执行经提权编排，实际 child token 未采样，不构成普通用户 token 资格。

## 本轮审查与保留边界

三个 GPT-6.1-sol high 内部子代理分别审查输入来源、Linux 差异和 Windows 原件／计划；执行脚本及 42 方法结果另经独立于生产者的内部复核。审查无阻塞，均未改源或执行额外测试。这是内部限定审查。

包中 Linux 199 方法、51,318 路径完整恢复与私有 Close 结论是外部汇总；本 ZIP 缺完整原始运行和恢复材料，未独立复现，不计入 Windows 通过。原丢失第16阶段整体材料仍未恢复。

protected Storage／IoWorker／DPAPI 的后两组 9 方法保持 NOT_RUN；本轮只使用普通合成、无密钥数据。production public owner binding 保持 false，生产 channel Directory003 原件、GUI、其它平台、真实 TLS／外部 API 和完整 SDK 冻结仍未完成。

当前 W15 WebSocket 的授权入口、原生适配器、三语言 fixture 源码和 19 个新测试方法已保存；独立 schema codegen 已通过静态布局核对。上述 19 方法与 WebSocket transport feature 未执行，本次 42 方法只建立原件兼容与映射等限定资格。

## 后续工作

1. 完成 W15 成熟协议库适配器的源码审查、实际三语言通道执行、背压／撤权／期限／Close／ACK／双 join 验证，保留原 Accepted 与 Unknown 含义。
2. 对公开生产 owner binding、Directory003 输入和 protected owner 资格分别补齐前提与明确授权，不以普通 Store 或 Linux fixture替代。
3. 按原 26 需求、10 门槛继续网络／文件／异步组合／重启核对和各平台验证；冻结决策仍为 OPEN。

本轮源码、原始日志、失败历史链和未完成草稿写入本地单个可恢复交接包。HEAD、真实 index 与原主工作区保持；未 commit／push／CI／tag／Release，公共文档不包含私有存储标识、链接或凭证。
