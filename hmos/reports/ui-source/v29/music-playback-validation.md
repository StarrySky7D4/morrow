# v29 音乐播放平台层限定验证

2026-10-09：新增实际 `MusicPlayback.ets` 与 `PlatformMusicPlayer.ets`，未修改 Index、MusicFiles、MusicLibrary、附件播放器、Rust/C++、SDK 工程或设备。此记录仅覆盖这两个新增源及其专项工具；曲库、文件适配器、页面连通、Native 和产品打包由对应负责人另行验证。

实际源执行 **50/50 PASS，0 fail/cancelled/skipped**，三份 suite、六项产品 ETS/tool 输入前后哈希一致，耗时 **601.0096ms**。见 [结果](music-playback-result.json)、[完整 TAP 日志](music-playback-tests.log)、[精确输入](music-playback-inputs.json)。测试读取并执行产品 ETS，不另写虚拟队列或用静态文本开关代替产品代码；AVPlayer、文件和曲库选中提交接口为可控 host provider。平台层与协调器的组合也执行实际新增源，不能把可控 callback 视为设备播放 ACK。

## 页面与文件合同

- `new MusicPlayback(hooks)`；`view()` 返回独立 `MusicPlaybackView` 快照；`select(identity, play=false)`、`play/pause/toggle(token='')`、`seek(ms,token='')`、`next()`、`previous()`、`pauseForBackground()`、`foreground()`、`close()`、`retryClose()`、`dispose()`。
- `MusicPlaybackIdentity`、`MusicPlaybackSource`、`MusicPlaybackDescriptor` 为 exported 无参 classes。Identity 为 `trackId/libraryRevision/importOperation/byteLength:number/sha256`；Source 仅另加 `token`；Descriptor 为 `token/fd/byteLength/sha256`。Source 不含 path 或 uri；Native `source_uri` 不作为文件地址。
- Hooks：`admitSelection(identity)` 必须等待实际曲库选中提交并返回确认的曲目完整身份及最新 revision；`acquire(identity)` 返回私有注册的原对象；另有 `ownsSource/createPlayer/releaseSource/isCurrentOwner/adjacent/changed`。Manual、next、previous、结束切歌都在 acquire 前经过该 selected admission；提交失败或 Unknown 不由播放器自动重发。
- `createMusicPlayer(source, files)` 接 `MusicPlaybackFiles.ownsSource/open/ownsDescriptor/close`。Files 返回实际注册的 FD descriptor；平台还检查实际 `stat(fd).isFile()/size` 与完整长度，才交给 AVPlayer `fdSrc {fd,offset:0,length}`。不接受任意路径、provider URI、internal URI 或单独 Native JSON 作为播放资格。

## 已实现与检查的边界

实际播放指令、异步创建、加载、准备、切歌与关闭串行执行。只有平台 state/time/duration callback 更新播放状态和位置，不把 seek Promise 或曲库政策 proposal 当作平台成功。每个原会话检查 generation、view owner、原 Source 完整身份及注册对象；旧曲目、旧 token、关闭后的 callback 和失效 owner 不控制新播放器。

libraryRevision 是 acquire 时的 CAS 观察。重新排序、歌词或显隐修改后的同一 stable track（trackId/importOperation/length/hash）保留原 owned source、player 和 FD，不因 revision 变化重开。下一首/上一首使用曲库最新 complete snapshot 提供的 exact identity；选中提交后用实际新 revision 获取资源。

背景立即撤销播放意图；已发 play 完成后再串行 pause。foreground 不自动恢复；需要用户新的显式 play。结束切歌仅限当前已请求播放、前台且 owner 有效的会话；重复或迟到 completed 不再推进。播放指令失败且已有真实 playing callback 时，尝试停止已确认播放，失败状态保持，原资源仍需关闭。

关闭按 **AVPlayer release 成功 → 同 descriptor 的实际 FD close 成功 → releaseSource** 顺序。任何失败保留原会话及租约，阻止下一曲获取、普通 close 自动重试或重新 mint token。`retryClose()` 只重试原 retained resources；若 FD close 失败，不再次释放已成功 release 的 AVPlayer；若 Source 清理失败，不重开平台会话。dispose 撤销新控制，仍允许显式清理失败资源。

覆盖了延迟 acquire/factory/load、owner 失效、并发关闭、队列中替换遇到释放失败、原身份参数被异步 hook 修改、同 track admission 中租约失效、后台期间迟到 prepared/play、latest play 对排队 pause、真正协调器与适配器组合的释放失败重试。

## 资格范围

本专项 **没有调用 SDK build、Native Store/export、HDC、安装、codec 或设备播放**。Library admission、FD export 的真实提交/字节证明须由实际 MusicLibrary/MusicFiles/Native 证据补足；host hooks 不授予任何 Native effect。SDK、HAP、设备声音/后台行为、重启曲库恢复、完整 Flutter parity 在本记录中均未验。歌词解析/显示/导入、在线搜索、封面、加密音频解密不属于本播放平台层，不能据此宣称音乐功能或完整产品验收完成。

参考实际 Flutter `build/win-cloud-20261005/lib/music/music_controller.dart` 的 serial transport、selected revision、complete-next 和 blocked pause；参考现有 HMOS `AttachmentPlayback.ets`/`AttachmentMediaPreview.ets` 的 AVPlayer/FD 释放模式。此次新增独立音乐来源及选中提交边界，不借附件 receipt 当音乐 Store 或运行证据。
