# v29 实际 Store 身份消费与文件层只读复审

2026-10-09：此补充记录保留 [原 50 项实际播放源检查](music-playback-validation.md) 和其日志、结果、输入文件，未覆盖原证据。音乐生产源码保持原冻结 SHA：`MusicPlayback.ets` **4F330274CA8C716CEA42AA576FEA1672B04636040547A6B307A470C2DB5BEC25**；`PlatformMusicPlayer.ets` **07446422A8376BA1885ED496BACDC8AE75DEA661289C649FDBE40F81C9C23A65**。

新增 `music-playback-store-fixture.test.cjs` **6/6 PASS，0 fail/cancelled/skipped**；五项 actual ETS/tool/Store fixture 输入前后一致，耗时 **334.9317ms**。见 [独立结果](music-playback-store-fixture-result.json)、[TAP 日志](music-playback-store-fixture-tests.log)、[精确输入](music-playback-store-fixture-inputs.json)。日志 SHA256 **E6393EEABD63C199BD2F64232B74B7585C38CEC93A89EA82FCF126507AB3C6AF**。

消费的是 Native 负责人导出的不可变 [实际 isolated Store fixture](music-store-fixture.json)，**65,127B** / SHA256 **DA6D8DF4F50F59555A4064AB29C9B302CE29B85A843DEB03F2F5C9EB0D83B9B2**。该 Store 使用合成音频字节；其 Ready 和完整字节证明不代表可解码格式、codec 或声音成功。

## 新增检查

- 原真实 Ready 曲目中的 `track_id/import_operation/byte_length/sha256/library_revision` 映射为实际 `MusicPlaybackIdentity`，通过生产验证函数；importOperation 保持原导入操作，不能替换为内部 ready operation。原 registered request_json 的 SHA256 与 Native 标记相同。
- 真正 selected second 曲目与后来 current second 曲目仍是同一 stable blob identity；读取实际 DTO 的不同 revision，当前为 **26**，不从操作数量推算。生产 `musicPlaybackSameTrack` 接受其稳定来源；`musicPlaybackSameSource` 不把不同 acquire CAS 混为同一个来源。
- 真正 16+2 页中保留 18 个按 lexical 顺序返回的 all-entry records；两条 Ready 正好覆盖真实 order，其余 pending/retired 不被当成新的来源令牌。
- 实际 export_request/export_reply 的长度、hash 和曲目身份与 current track 对齐。该 JSON 没有 token/FD，生产来源验证仍拒绝它。
- 原始 Ready track、内部 `source_uri`、只含身份的对象和 export JSON 都不能绕过 actual `PlatformMusicPlayer` 的 `MusicFiles.ownsSource` 门禁；没有文件 open、stat、AVPlayer 创建或 FD close。
- 实际 next/seek/restore 等 Native playback proposal 仍为只读指令，未把其 playing 字段或 transport_effect 变为平台 ACK。

这些测试不构造可播放租约，不 mint token 或实际 FD，不重新发送 Native 操作，不解码 fixture，也不运行 SDK 或设备。原 50 项的可控 host provider 状态检查和此次六项只读 DTO 消费是不同证据范围，均不能称为音乐 UI 或设备播放完成。

## Files 修复后只读检查

最终审阅 `MusicFiles.ets` SHA256 **58DA1029415AF4B2C34472219323F01FBB737DF7FB2574ECAEED83C17E2D4844**。本审阅没有修改 Files，也未重新运行 Files suite。Root 的 15 项 actual ETS 受控检查由 Root 单独记录。

`writeSmall` 在 metadata/request 写入前要求已有节点为普通文件，使用 NOFOLLOW；其 close 拒绝会由 `closeOwned(parent, actualFile)` 保留原 FD。partial player open 的失败清理也走该原 FD 保留路径。export 写 FD 已关闭并获得成功确认后才登记新的 Source；capture 的两个 FD 用嵌套 finally 都尝试关闭。

存在 pending FD 的目录禁止再次 open/import 和删除；`retryPendingIo` 只显式重试原实际对象。若 request 写入完整但 close 拒绝，磁盘已有原 literal 时拒绝更换操作，恢复关闭后仅允许显式同原 literal 重写/fsync。活跃播放器 descriptor close 失败时保留原 file/descriptor，由平台层和协调器显式同会话重试；Source 释放只在真实关闭后进行。

inactive 播放缓存清理仅在显式调用时处理受控命名目录，排除 static active/pending；未知文件、symlink 和非法目录被保留。preview 预算计入真实缓存字节，而不只统计活跃对象。在这些已审的关闭、原 literal 和清理边界内 **未发现阻塞缺陷**。这不替代 SDK/filesystem provider/device 验证。

## Library 只读协同检查

审阅当时 `MusicLibrary.ets` SHA256 **7317B45A6E853FF78BD0FE44D39B3B8957DA3DAB4CA16E014903AAC70095DF36**。两个先前发现的问题已修：每次新 dispatch 将 mutation effect 置 Unknown，新失联结果不能继承旧 known-not-committed 的 discard 资格；final page 完整 ready/order 校验使用候选 aggregate，通过后才安装，失效回复不污染同 wire 重试。

对固定 import literal/hash/内部 ready operation、all-entry 分页与 Ready identity、异步 owner、固定 CAS 重试和 Native pure proposal 门禁的限定只读复审未发现其他阻塞。未改 Library，也未运行其 suite；最终 Library 结果和可能后续源身份由其负责人单独冻结，不能把此时 hash 当成其他版本的测试资格。

## 发布范围

本轮交付为 **离线音乐基础接口、曲库/文件/播放协调模型和实际 AVPlayer 适配器源码**。Index 音乐 UI **尚未接入**，现有页面未因此获得可用的音乐导入/播放/歌词闭环。SDK、HAP、Native 当前最终 suite/双 ABI、安装和设备播放由 Root/Native 对各自实际输入单独验证。本记录不证明设备声音、后台音频、重启恢复、加密音频解密、在线歌词、封面或完整 Flutter parity。
