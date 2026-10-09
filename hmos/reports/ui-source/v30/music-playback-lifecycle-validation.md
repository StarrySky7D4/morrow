# v30 音乐播放器前后台生命周期专项

2026-10-09：修复实际 `MusicPlayback.ets` 的前后台控制和晚到资源释放；公开 API 与 hooks 签名保持兼容。`PlatformMusicPlayer.ets` 未修改。本专项没有修改 MusicFiles、MusicLibrary、Index、Rust/C++、SDK 工程、Git 远端或设备。

实际源码执行 **77/77 PASS，0 fail/cancelled/skipped/todo**，五份 suite，耗时 **723.3258ms**。十项源/工具/固定 Store DTO 输入以及 Node、SDK TypeScript 三项外部运行输入前后哈希完全一致。见 [结果](music-playback-lifecycle-result.json)、[输入清单](music-playback-lifecycle-inputs.json)、[完整 TAP 日志](music-playback-lifecycle-tests.log)。Node `v24.14.1`，本机 SDK TypeScript `4.9.5-r4`。

产品源冻结：

- `MusicPlayback.ets`：21,827 bytes，SHA-256 `BB659BF5FB6C528C6171AA492AF7416003341D916FA0394902CB485B55FEACC0`。
- 未修改的 `PlatformMusicPlayer.ets`：5,959 bytes，SHA-256 `07446422A8376BA1885ED496BACDC8AE75DEA661289C649FDBE40F81C9C23A65`。
- 日志 SHA-256 `2B9F637E21543BA55CE18186EC0E53D65AF9677CC57660FD06EA12C9E5CED96D`。

## 修复行为

原 `pauseForBackground()` 通过 `active()` 检查 caller 的 owner flag；caller 先把前台置 false 时，已播放的 owned session 会跳过实际 pause。本版将用户控制和原资源停音分开：停音检查同一个 current session、完整原 Source 身份和 Files 原对象所有权，允许在 owner 已 false 时串行暂停该实际播放器。页面实例仍有责任在永久离页时调用 `dispose()`。

`isCurrentOwner()` 应表示页面实例所有权，例如 pageAlive；前台状态单独由 `pauseForBackground()` 和 `foreground()` 报告。后台转换立即清空原播放意图并推进内部 lifecycle epoch；实际 pause 等待先前已接受的平台命令完成。返回前台只开放新操作，不自动恢复播放。

选曲的 admit、acquire、create、load 在进入时绑定 epoch。经过离开再返回，即使 caller flag 已恢复 true，也不继续原 autoplay 或获取后续资源。实际已取得的晚到 Source 或 Player 按原对象关闭。factory/load 在 epoch 失效后 reject，同样关闭本次已取得的 Source，不能只隐藏错误而遗留 loading 租约。已拒绝的 cleanup 不在 catch 中重试。

已完成 load 的稳定 player/source 可继续在后台完成 prepare，prepared 不再触发旧 autoplay；原 pending next、EOF、play、seek 不穿越 epoch。先前已接受 play 的晚到 playing callback 即使在返回前台后到达，也要求实际 pause。用户新的显式 play 可以在该停音屏障后播放，沿用已有 FD，不重开来源。

player release → 同 descriptor FD close → 原 source release 顺序、cleanup_failed 原资源保留、显式 `retryClose()`、实际 selected CAS admission、同 stable track 在排序 revision 改变后保留 FD 的规则继续通过原 suite。

## 检查范围

新增 lifecycle suite 18 项，含 owner=false 后真实 pause、in-flight play/queued seek 的离开返回、旧 admission、late acquire/create/load 成功和拒绝、原 cleanup 失败不自动重试、稳定 prepared 后显式播放、EOF 不自动前进。实际协调器与未修改平台适配器 composition 另加三项，检查同 FD 停音、真实适配器 delayed open 的释放顺序、平台 late playing callback 后实际 pause。原 model、platform、固定实际 Store DTO suite 一并执行，未以静态源码字符串替代产品控制逻辑。

可控 host provider 提供 admission、Files、AVPlayer 方法和 callback。固定 Store DTO 读取 v29 的原 fixture 并校验其 SHA-256，仅证明 DTO 身份和完整字段的匹配；没有运行新的 Native Store/export。此记录不资格授予 SDK 编译、HAP、音乐 UI、设备声音、codec、后台停音、应用重启或完整 Flutter/Windows parity。设备后台行为仍须实际安装候选后验证。

## 复现

使用本机 DevEco Node 执行 `hmos/tool/music-playback-lifecycle-check.cjs`。默认标签 `music-playback-lifecycle` 对已有证据拒绝覆盖；复查时传入新的 ASCII 标签。该工具只读取实际输入、执行五份 suite 并写本专项证据，不构建、安装或发布。
