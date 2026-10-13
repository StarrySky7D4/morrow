# Windows runner 初始化失败的真实证据（2026-10-13）

当前完整安全执行链仍未通过；SDK26／G04 OPEN，release_eligible=false。

## 已通过的限定构建

613 原 harness／runner／setup 均实际 Cargo 退出 0、原外层退出 0、双流 EOF。原 570 源图的 2393 叶加 603 单叶诊断构成当前 2394 叶；20 原锁保持。依赖缓存为独立复制复用，所选三个 bin 实际重建（Cargo fresh=false），不称全新依赖构建。

626 独立读回实际退出 0，报告位于原先预留的 618/readback001，明确 reader_stage=626；核原 API 链、PID／四 Popen 时钟、完整双流、原命令与环境、原／保存 PE、源图／锁、缓存守卫及最终严格 HOME。旧 618 源码发现的公共工具白名单阻断已在新 626 修正，旧源码保持不变。

| 角色 | 字节数 | SHA256 |
| --- | ---: | --- |
| harness | 95575040 | `b4dc88f1c0db0b55fd9d0419d18fd519dca1c4ee82bc462b834f351286dbe288` |
| runner | 24376320 | `67a91832ad5e34e7f1b25829f923f633764473451549f6c2fbdc316944c60754` |
| setup | 37720064 | `54b56c7776aa4d59bedbca76acb9bdeebc2551c79cf8a2d20415229738c9ac1f` |

独立构建报告 SHA256：`9aaa092bc42b74d8594d6be458a93722ee7e930d657d43f0b5f3bd400182b4c5`。详细构建历史见 [613 记录](windows-runner-diagnostic-build-613.md)。

## 新 619 campaign 的实际结果

旧 590 已保留 Standard 内存检查点，并显式恢复指定基线。该恢复不是原 owner 的正常 cleanup，旧 Unknown 不升级、不重放。

新包传输原外层／子进程退出 0、双 EOF，传输连接明确关闭。新实际执行完成 39 个命令，覆盖固定 setup、原 owner／factory、Wasm 会话、proposal／review 及一次 Claim。原 journal 已保留一次 native Start 尝试；未获得启动 ACK，随后停止新业务并保留原现场。

603 新增观察确认同一个 runner 在库内既有 TerminateProcess 之前已退出，原始退出码为 `0xc0000142`。创建与退出观察相隔约 15.007 秒。这里只证明观察点前已退出，不证明退出的具体 DLL、自然退出原因、业务成功或正常回收。后续只读连接返回 0 并关闭；44 份固定公开失败记录已经前后 SHA 对照归档，不含数据库或密钥。

微软将该状态定义为 [STATUS_DLL_INIT_FAILED](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-erref/596a1078-e883-4972-9bbc-49e60bebca55)。[CreateProcessWithLogonW 文档](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createprocesswithlogonw)说明创建函数可在 DLL 初始化结束前返回，并要求调用方处理目标账户的窗口站与桌面访问。由此需要核查启动上下文；不能从该退出码单独认定权限就是根因。

## 确认的源码缺口与下一步

静态检查已确认首跳 runner STARTUPINFOW.lpDesktop 未绑定已经选择的私有桌面；后续 LaunchDesktop 还硬编码 Winsta0，可能与非交互会话实际窗口站不符。runner 直接导入 USER32.dll；下游收到 SpawnRequest 后的桌面设置不能代替首跳初始化。正在准备仅修这两处接线的候选：持有私有桌面和 UTF16 缓冲至创建／握手结束，读取真实窗口站名，保留现有批准策略和 ACL。候选尚未审核、应用、编译或复验，不修改已有交互／服务窗口站权限。

完整 47 步、basic 文件／SID／网络见证、join／ACK、owner finish、factory release、正常 owned cleanup 仍待验。原 555 正常 supervisor 路径未重跑，565 仍仅保留原限定资源隔离结论；生产／跨平台资格、namespace／持续 reparse、完整后代树和 SDK 冻结继续 OPEN。当前版本保持 0.1.9-test.58+62；本轮无 Git 提交、推送、CI、标签或 Release。
