# v30 播放器回调类型 SDK 诊断修复

2026-10-09：集成构建 a1 报告 ArkTS object literal 不接受 `MusicPlaybackHooks` 的 method-signature 字段。本次仅把八个 hook 改为同名 function properties，参数、返回值与行为保持一致；没有执行 SDK 或设备命令。之前 [生命周期证据](music-playback-lifecycle-validation.md) 保留为修改前记录；当前源使用本次新标签结果。

当前 `MusicPlayback.ets` 为 **21,859 bytes**，SHA-256 `9F1E29260AADBFA8E51216DAEF323C2E19A8B39CC5B95CB17A33BCC6C1D571DA`。`PlatformMusicPlayer.ets` 未修改，仍为 `07446422A8376BA1885ED496BACDC8AE75DEA661289C649FDBE40F81C9C23A65`。

实际五份 suite **77/77 PASS**，0 fail/cancelled/skipped/todo，**719.9923ms**；十项实际本地输入及三项运行时输入前后哈希一致。见 [结果](music-playback-lifecycle-sdk-hook-result.json)、[输入](music-playback-lifecycle-sdk-hook-inputs.json)、[TAP 日志](music-playback-lifecycle-sdk-hook-tests.log)。日志 SHA-256 `1181D1E6AC4577AD5B24C1F6FE10CBC8EA4D571A103F274240E0BAE2E0C51FB2`。

此结果执行实际播放器/平台代码并使用可控主机 provider，不授予新的 Native Store、SDK 编译、HAP、codec、UI 或设备播放资格。实际 SDK 诊断是否消失由集成构建另行确认。
