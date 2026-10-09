# v28 音乐功能实际源码审计

日期：2026-10-09。Root 报告的起点为已推送的 v27 `e4891f89`；本审计没有运行 Git。本文件描述读取到的实际源码和下一块交付范围，不把建议 API、已有测试文件或附件播放能力算作音乐功能已经接通。

## 结论与证据范围

HMOS 音乐页仍是占位界面。可以直接复用的 Rust 音乐策略、歌词解析和偏好编解码已经位于 HMOS 的共享 workbench 源码中；另一个实际 Rust 工程提供纯音频容器解密。下一块有独立用户价值的交付是：授权导入本地音频、独立持久播放列表、真实播放/暂停/切歌/定位、离线歌词与页脚歌词。仅增加解密函数或歌词解析路由不能称音乐功能“大部分实现”。

本轮只读核对 Flutter `build/win-cloud-20261005`、HMOS 实际源码以及外部 `C:/Users/Administrator/Desktop/CodeXProjext/um` 的代码。没有打开用户音乐、酷狗数据库、密钥或凭据，没有请求歌词网络服务，没有运行测试、构建或设备操作。55 个代码/协议/测试源文件的当前路径、字节和 SHA256 见 [music-parity-source-inputs.json](music-parity-source-inputs.json)；它是源码审计快照，不能替代构建输入清单。该快照 SHA256 为 `4D00D09D6A1D65F725C99F4D2B6AD5F3584C1FB53B1A4E384F83626F7D2D4A56`。

关键当前源码：

| 源文件 | 字节 | SHA256 |
| --- | ---: | --- |
| Flutter `lib/main.dart` | 221628 | `2CB2A519E31AC982D3A8638EB7DE95FE63D5421ED3D1B6ACDA507CD142169F06` |
| Flutter `lib/music/music_controller.dart` | 19569 | `D5390E8C0435FB9B0F5D677B30B4F338971F459BB8187AA1AECE7D2F2CF13DF2` |
| Flutter `lib/music/music_panel.dart` | 31013 | `6DF33C27CDE8054D3F70B5D5EBAC00831D78E212D515574CAD764D64E72D694D` |
| HMOS `shared/plugins/workbench/src/services.rs` | 19030 | `5551924E63A942B35CD8A0D9E8EE513EE9351B61347E5757C9132CA1BA593065` |
| HMOS `shared/plugins/workbench/src/preferences.rs` | 17744 | `85C764733A9E4ACBE92C0D364ABF1CD16579C6B496EADD0BC71612042C99FC25` |
| 外部 `um/crates/um-decrypt/src/lib.rs` | 7153 | `BE94E34F81AEF52D5C0AA97D6735B0AABACD11D612E98153F9EB9BB4A3339097` |

## Flutter 已实现的用户流程

实际入口为 `lib/main.dart:1183` 的 `MusicController` 恢复：tracks、index、showLyrics、onlineLyrics 与 `onSave: persist`；`main.dart:1257` 保存 `music.toJson()`。`main.dart:1005` 和 `1086` 将音乐与背景视频声音互斥，`1913` 把音乐带入页脚。

| 流程 | 实际源码与行为 | HMOS 当前缺口 |
| --- | --- | --- |
| 多选音频导入 | `music_panel.dart:58`；标准/加密扩展验证，逐曲导入；`MusicTrack.import` 准备音频、读取元数据、复制到应用拥有的 texture 存储，再清理解密临时文件。 | 无 picker、持久曲目身份、导入/失败状态或真实列表。 |
| 持久素材 | `media/texture_storage_native.dart` 把文件复制到应用 support/textures；resolve 校验存在后给播放器 URI，remove 限于该拥有目录。 | 附件 preview 是临时租约，不能拿来替代曲库。 |
| 播放控制 | `music_controller.dart:124` 的 transport 提供 playing/completed/position/duration/error 流；`474` 选择、`522` 切换、`568`/`569` 上下首、`589` seek。 | 空页按钮无播放动作。 |
| 选择与异步隔离 | controller 的 `_pending` 串行 transport；selectionIntent、revision 阻止旧 resolve/open/歌词结果覆盖新曲目；EOF 完成切下一首。 | 需要独立音乐 controller 与实际播放器回调。 |
| 列表整理 | `music_panel.dart:468` 起选择、删除、排序；controller 重排保留同一曲目和同一 transport，不重新打开/定位/播放。 | 仅有“播放列表还是空的”。 |
| 本地歌词 | 多选的同名 LRC 优先；手选 LRC/TXT 非空、至多 1 MiB；元数据读取还可取内嵌歌词。 | 无歌词持久字段、解析、全文查看或时间轴。 |
| 歌词展示 | `music_panel.dart:196` 全文可选文本；`little_tips.dart:149` 页脚根据 playing && showLyrics 展示当前时间轴歌词，没有时间轴时显示明确提示。 | 只有静态提示文字。 |
| 封面/元数据 | 选封面、持久 source；元数据获取 title/artist/duration/内嵌 lyrics；缺失标签不阻止音频播放。 | 当前没有该读取与展示流程。 |
| 在线歌词 | `lyrics_service.dart:34` 的实际请求为 LRCLIB search，12 秒超时、最多 30 候选；精确标题/歌手和时长 ±2 秒，歧义要求人工选择。controller 不覆盖已有/手选歌词，并检查曲目身份和请求代次。 | 网络、候选选择、取消、重试未接；本审计没有验证服务可达性。 |

HarmonyOS 文件选择取得的授权 URI/FD 不等于 Windows 路径，也不自动授权扫描父目录。Flutter `track_metadata_native.dart` 的相邻 LRC 自动扫描只能迁移为明确授权的多选音频/LRC 配对或手动选歌词，不能根据一个音频 FD 推定有权读取其邻居。

## 已有 Rust 可直接复用的函数

下列函数均在 `hmos/shared/plugins/workbench`，现有 `hmos/rust/Cargo.toml` 已依赖该 crate，不必复制算法或增加解密依赖。`src/lib.rs:28` 已公开 services 模块。

| 实际函数 | 已有能力与预算 | 复用边界 |
| --- | --- | --- |
| `services::Playback::validate` / `apply`，`services.rs:113` / `128` | 唯一合法 ids、至多 1024；Restore、Block、Toggle、Seek、Select、Next、Previous、Remove；循环选曲、seek clamp、Restore 不自动播放。 | 返回 `none/play/pause/seek/open/stop` 是 transport 指令。不是 AVPlayer 已成功，也不是持久写入收据。没有 reorder 动作；重排由稳定曲目身份的 controller 处理。 |
| `services::parse_lyrics`，`210` | UTF-8 输入至多 49152 字节、输出至多 1024 行；多时间戳、offset、分数毫秒、排序。 | 1 MiB 文件选择上限大于原 Rust 歌词字段预算。必须明确拒绝超预算或保留独立原文后显式报告解析失败，不能截断后称完整导入。无时间戳文本应保留全文并显示“无时间轴”。 |
| `services::lyric_match`，`259` | 至多 30 候选、有限时长；规范化标题/歌手、±2 秒、非空、同步歌词优先、歧义不自动选。 | 纯匹配，无 HTTP/授权/网络重试。第一块离线交付可不开放在线入口。 |
| `services::import_policy`，`302` | 非附件音频/视频 150 MiB；其他 25 MiB；可验证 HTTP(S) URL 无账号密码。 | 这是大小/字段策略，不证明文件权限、完整复制、音频可解码；非空输入和实际长度还需 trusted 导入层验证。 |
| `preferences::validate` / `decode_persistent` / `encode_persistent`，`74` / `333` / `400` | 偏好整体至多 4 MiB、512 tracks；每曲歌词 49152 字节、时长有限非负；持久编码按 source.location 保留 retained track 的未知字段，递归保留未知设置。 | 本地 file URI 语法有效不证明拥有该文件。更新音乐不能重建/覆盖 appearance 等其它已知设置；必须在实际完整前版上窄更新。曲库实际上限应受 512 tracks 而非 policy 的 1024 限制。 |
| `preferences::validation_pages`，`426` | 每个 guest 验证输入至多 65536 字节，曲目逐项分解，trusted adapter 仍核整体数量/index并保存原 aggregate。 | 这是验证分解，不是已实现的 HMOS read/list 分页合同。现 native JSON 通道为 512 KiB，不能整包返回 4 MiB，也不能把音频 base64 塞入 JSON。 |

已有 `tests/services.rs:34`、`:66`、`:87` 覆盖歌词、播放策略与 import budget 的源文件可以作为后续回归起点。本审计未运行它们；它们也不证明 HMOS 音乐 UI、存储或音频输出。

## HMOS 平台基础与实际空页

`entry/src/main/ets/pages/Index.ets:3529` 的 MusicPanel 仅有标题、图标、占位说明、无 onClick 的上一首/播放/下一首、无 picker 的加号；唯一交互是 playlistOpen 切换空列表。`hmos/rust/src/lib.rs` 与 C++ bridge 尚无 music/lyrics/playback/um_decrypt 接线。

可以复用的平台经验已经真实存在：

- `pages/AttachmentMediaPreview.ets:10` 的 `PlatformAttachmentPlayer` 用 AVPlayer；授权 private source 上以 READ_ONLY | NOFOLLOW 打开 FD，核 regular file 和长度，设置 fdSrc；实际 time/duration/state/error 回调及 play/pause/seek；player.release 成功后才关 FD。release 失败保留资源等待显式清理。
- `model/AttachmentPlayback.ets:64` 的 session/controller 约束 token、旧异步创建、serial work、释放失败和独立状态。可复用其平台接口和资源顺序，音乐需要新的长寿命 owner；不能复制“组件不可见即背景暂停”的全部 preview 行为，因为折叠音乐面板不应被当作切换曲目/释放音乐。
- `model/AttachmentFiles.ets:716` 的 preview 导出有实际 FD/length/hash 和受控清理。它是临时 preview，不是持久音乐库。现附件导入预算 64 MiB、预览上限 200 MiB；编辑草稿另有 64 MiB 活跃总额。音乐的 150 MiB 单曲策略不能通过放宽这些原有额度、复用 card/draft id 或把 preview 租约当作永久曲目来实现。

## 下一可独立交付功能块

建议第一块闭环为“离线本地曲库与随身听”：先导入平台实际可解码的标准音频，保留完整自有音频，持久列表及稳定曲目身份；选择、播放/暂停、循环上下首、seek、完成自动切歌、排序/删除；导入明确授权的 LRC/TXT、完整歌词查看、时间轴与页脚开关；恢复列表时不自动播放。未具备的在线歌词、自动内嵌元数据和加密容器能力应在界面如实呈现。

建议实施顺序与 API 范围（均为建议，尚未实现）：

1. Native 有界纯策略 DTO：`music_policy` 调真实 `Playback::apply`，`music_lyrics_parse` 调真实 parser，`music_import_policy` 调真实 policy；保持旧 Card/Editor 协议。返回有界、严格 DTO 和独立 `transport_effect`，纯策略请求保持 not_committed，不生成虚构 Store receipt。
2. 独立 durable 音乐 library：稳定 source URI/track id、完整实际 owned 文件、长度/hash/owner 与 bounded track 元数据。窄接现有原件/FD/hash 基础时核清长期引用和清理权；不能直接挪用附件临时 cache。偏好读写使用完整前版和 unknown-preserving codec，明确 Store CAS、operation reconciliation、整体文件/内存额度及 read 分页；缺文件恢复为不可播放记录，不能静默换成另一个源。
3. ArkUI 音乐 controller 和 UI：串行播放器、曲目身份+代次保护、真实 state/time/duration/completed 回调；late import/resolve、后台阻塞、重排不断播和选择/删除交叉操作。Root 后续接 Index、MusicFiles 和 NAPI 文件路径；此报告不把路线等同已接通。

先只交付纯 parser/policy 可以作为真实 Native 基础，但必须标为子集；达到上面的导入、持久库、实际播放器、列表、离线歌词闭环后，才有充分依据称音乐主要的本地工作流已实现。

## 解密模块的独立后续块

外部 `um` 的 workspace 声明 MIT，并有 LICENSE。实际 core `crates/um-decrypt/src/lib.rs` 禁止 unsafe，接受调用方 bytes/keys；没有文件系统、网络、播放、转码、数据库或标签改写。候选入口：

- `Decoder::prepare`（125）、`info`（153）、`decrypt_chunk`（160）、`decrypt`（173）：解析容器及密钥状态、验证解出的音频头、提供 payload 起点和输出长度。chunk offset 是音频 payload 相对偏移，读取原容器的 `audio_offset + offset`，支持不对齐独立块，越界先拒绝再修改。
- `Format::from_filename`（71）只便利映射扩展，实际支持由 bytes/header/version/key 验证得出。NCM/QMC/KGM/KWM/TM/Xiami/Ximalaya 的具体版本/slot 有明确限制；不能按枚举数声称每个变种支持。
- `key_hint`（190）仅给 KGM v5 hash/QMC musicex 标识，是不可信查找 id，既不是 key 也不是可打开的路径。`Key::EKey`/`Decoded` 由应用供应；MissingKey(6)、UnsupportedFormat(4)、UnsupportedVersion(5) 必须明确失败。
- `derive_qmc_key`、`sniff_audio` 可复用；NCM parser 跳过 metadata/cover，没有提供同等封面/元数据 UI。

重要边界：prepare 不保留整个源，也不对源做内容 hash；decrypt 仅核同长度，不能识别另一个同长度源。HMOS host 必须先 capture 实际授权源到自有不可变快照，绑定 FD/完整长度/hash/operation/owner，再从同一 snapshot 解密。原 core 初始化需要完整容器，chunk API 不自动消除初始化的整曲内存开销；需给输入/输出、并发、总内存和临时磁盘分别定额。生产输入/输出应保持 150 MiB 音乐策略，不能沿用 FFI 的 512 MiB buffer 上限。

Flutter `audio_import_native.dart:15` 明确仅 Windows，并加载 `um_decrypt_ffi.dll`；MissingKey 的 KGM fallback 调 Windows 酷狗数据库查询。这些宿主流程不可直接迁到 OHOS。优先在 HMOS Rust 内直接复用纯 core，由正式 authorized FD worker 管捕获、解密、验证、private output 和资源释放；不借 Windows DLL、WASM 或数据库路径绕过文件权限。此次没有导入该 crate，没有更新 dependency/lock，没有 OHOS 双 ABI、package 或设备资格。导入前需固定全模块及查表二进制源码、保留许可证、核依赖/构建兼容和真实 byte vectors。

现有 external `tests/compatibility.rs` 与 `tests/formats.rs` 包含 QMC 参考向量、qtag/musicex/KGG caller key、非法 range、QmcPayload footer、坏输入和 NCM/其它容器测试源码；均未在本轮运行。解密得到 FLAC/Ogg 等输出也不意味着 HarmonyOS 当前播放器支持其 codec，需分别验证。

## 真正交付门槛

| 层级 | 必须得到的实际证据 | 本审计状态 |
| --- | --- | --- |
| Native 策略/歌词 DTO | 当前源真实 Rust tests；边界/溢出/非法 action；actual DTO 与 ETS strict model 对接；指令不冒称 committed。 | 未新增、NOT_RUN。 |
| durable library/导入 | 真实 Store/CAS/reopen、crash/Unknown reconcile；源完整性、拒绝坏 FD/越界/hash/owner；不丢未知字段；同 track 重排；配额与清理失败可见。 | 未实现、NOT_RUN。 |
| ArkUI 页面/model | picker 取消/失败/多选、late selection/歌词、重启、删除/排序、blocked、无时间轴、超歌词预算；完整文件不截断。 | 音乐仍占位，NOT_RUN。 |
| 包和平台 | 新源码 manifest、双 ABI、fresh SDK/HAP 内实际库与资源；正式宿主文件授权与 codec 能力。 | 音乐新块 NOT_RUN；旧 v27 包不能替代。 |
| 设备音乐闭环 | 真正可听音频、position/duration 前进、pause/seek/EOF next、切换音源、与背景/附件音频互斥、恢复不 autoplay、player/FD cleanup。 | NOT_RUN。 |
| 解密/在线/全 parity | 真实解密向量及 OHOS 导入输出、合法 key 接口；网络候选/取消/失败/超预算；metadata/cover/UI 布局与 Flutter 逐项比对。 | OPEN，未计入第一块通过。 |

源码审计结论为 **PASS_SCOPED（已定位实际可复用模块与真实缺口）**。音乐功能实现、音乐设备资格、protected 宿主与完整 Flutter parity 继续 OPEN。后续 Native 实施必须另留当前测试/DTO证据，不能回写本只读审计为运行通过。
