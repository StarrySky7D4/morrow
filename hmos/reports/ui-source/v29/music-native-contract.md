# v29 Native 离线音乐基础合同

2026-10-09。本合同对应 [冻结输入](music-native-inputs-final.json) 的实际 Rust 实现。提供独立 development 曲库、真实 Core 原件导入/导出和纯歌词/播放决策；音乐页面、实际 AVPlayer 音频、设备/protected 宿主、完整 Flutter parity 尚未据此验收。

## 传输与身份

普通 JSON 外层为 `{action:"music",music:{action:INNER,...}}`。FD 导入外层为 `{action:"music_import",music:{action:"import_file",request:ImportRequest}}`，FD 导出为 `{action:"music_export",music:{action:"export",library_revision,track_id,import_operation,byte_length,sha256}}`。只通过已有 import/export FD 桥接传音频；普通 request 路由拒绝这两个 FD 动作。外层旧业务/editor/draft 字段必须空，不能借业务 Card 或草稿身份。旧回复增加可省略 `music`；旧动作中不生成此键。

`ImportRequest` 必填且拒绝未知键：`schema_version:1,track_id,operation_id,expected_revision,name,byte_length,sha256`。revision/length 为规范十进制字符串，hash 为完整小写 64 hex。track ID 长 1..64、operation 长 1..128，只允许 ASCII 字母/数字/`_.-`，禁止 `morrow-host-` 前缀。name 为 1..512 UTF8 bytes，禁止控制字符与路径分隔符。稳定 `source_uri=file:///morrow-music/<track_id>` 是显示/持久身份，**不作为可打开路径或播放资格**。

Native 仅接标准扩展 mp3/wav/flac/m4a/aac/ogg/opus/wma/ape/aif/aiff/alac/wv/dsf/dff/mp2/ac3/dts/au。真实原件必须满足完整长度/hash；本段没有 codec sniff、metadata extraction 或加密音频解密。

## 实际动作

| INNER | 必填参数 | 行为 |
| --- | --- | --- |
| read | after:string, limit:number 1..16 | 按 track_id 字典序分页全部 Pending/Ready/Retired；每页重复完整 order/selected/current revision，调用方核同一 revision 后聚合。缺失库返回完整空库 revision 0，不创建库。 |
| track_read | library_revision,track_id | CAS 当前 revision，只读一条完整 track。 |
| import_begin/import_inspect/reconcile | request | 固定完整原导入请求；begin 只提交 Pending，inspect 只读，reconcile 仅以实际 retained 原件发布 Ready。 |
| import_file | request | FD-only 原件存储，先要求真实 Pending 历史。 |
| export | library_revision,track_id,import_operation,byte_length,sha256 | FD-only 当前 Ready identity、原历史和原件 owner 完整核对，流式校验后输出 FileReply。 |
| set_lyrics | library_revision,operation_id,track_id,lyrics,lyric_source | 完整原文写入，拒绝超预算；不裁剪。 |
| reorder | library_revision,operation_id,order:string[] | 必须是当前 Ready 集合的完整唯一排列，保持 selected track 身份。 |
| remove/select | library_revision,operation_id,track_id | remove 提交 Retired 墓碑并从 order 移除；select 持久选择当前 Ready。 |
| show_lyrics | library_revision,operation_id,flag:boolean | 持久歌词显示偏好。 |
| lyrics_read | library_revision,track_id,position_ms | 返回完整原文、共享 parser 全部时间行和 active_index；无时间行时 untimed=true。 |
| policy | library_revision,index,playing,blocked,position_ms,duration_ms,music_action,value,flag | 复用真实共享 Playback，返回纯 transport 决策，无 AVPlayer 执行或存储收据。 |
| match | title,artist,duration:number,candidates[] | 共享 lyric_match 纯候选匹配，不请求网络。candidate 为 title/artist/duration/synced/has_lyrics。 |
| import_policy | byte_length | 共享 import_policy 纯策略校验。 |

policy 的 index/position/duration 为非负十进制字符串，时间不超过 `1<<40` ms，value 为可解析 signed i64 字符串；music_action 为 restore/block/toggle/seek/select/next/previous/remove。

## 完整回复

`Reply.music` 固定必填：`schema_version,kind,library_revision,order,selected_track_id,show_lyrics,online_lyrics,tracks,next_after,repeated,operation_id,operation_revision`。kind 为 library/track/lyrics/playback/match/import_policy；可选 payload 为 lyrics/playback/match_index。online_lyrics 当前始终 false。

TrackView 必填：`track_id,library_revision,import_operation,name,byte_length,sha256,phase,source_uri,title,artist,duration_ms,lyric_source,lyrics_byte_length,bytes_retained,request,request_json,request_sha256`。phase 为 pending/ready/retired。`request_json` 是真实持久完整 FD import 外层 literal；SHA 绑定其完整 UTF8。`bytes_retained` 来自实际 Core Snapshot owner 和全文 hash 校验；Pending 可能是 false 或 true。初始 title=文件名 stem、artist 空、duration 0，当前没有 metadata 写 API。Retired 不授予 export/重新激活。

LyricsView：`track_id,text,source,lines:[{time_ms:string,text}],active_index:number,untimed:boolean`。PlaybackView：`track_id,index:string,playing,blocked,position_ms,duration_ms,transport_effect`，effect 为 none/open/play/pause/seek/stop。

只读 top effect=`not_committed`，top receipt_revision 为空。inspect 仍携带原 Pending operation 上下文，**不是写入收据**。写入 operation_revision 是实际历史收据 revision；library_revision 永远是实际当前状态，禁止用旧收据推算当前。Ready 的固定内部 operation ID 由 Native domain hash 导出；它只用于 reply 的实际收据，不授权客户端构造 reserved 请求 operation。

## 存储、Unknown 与预算

独立私有 Card `morrow-host-music-library-v1` 是 Core 存储 primitive，type=`hmos-music-library`，format1；不是业务 idea/draft。普通 list/query 明确验证后跳过，普通 get/save/delete/task/favorite 与导出等保留 host identity 拒绝。坏 private schema 不做读回退。偏好使用现有 preferences bounded decode/unknown-preserving encode，保留根与 retained track 的未知 protobuf 字段；私有 JSON schema 未知键拒绝。

Pending、retained Snapshot 原件、Ready Card 发布是**三个独立持久事务**，没有跨事务原子承诺。begin 固定原 track/operation/CAS revision/完整 length/hash/name/FD literal 后才可保存原件。Unknown 仅 inspect 原同 literal/owner 并显式 reconcile；未发现原件时保持 Pending，不盲读替代内容或自动重导。原件已 retained 的显式 import 重试不读取替代 reader。retire 保留不可复用 identity，旧 import 精确重试返回原 receipt 加当前 Retired 状态，不复活。

累计最多 512 entries（包括 Pending/Retired），音频单件 150 MiB、累计逻辑 length 512 MiB；真实 Core 现有 200 MiB 单 blob/2048 blob/2 GiB 总量和 8 MiB Card 预算继续生效。当前不回收 Retired Snapshot 或历史 event pins，逻辑预算不释放；明确 durable cleanup/GC 仍 OPEN。

歌词 raw ≤49152 UTF8 bytes、source ≤4096 bytes、最多 1024 parsed lines，并对重复 timestamp 展开后完整 JSON 预算预检；超出完整 Reply 预算拒绝写入，不截断。整体 Reply 仍 ≤512 KiB。title/artist 字段沿共享 preferences 验证，match 文本 ≤16384 UTF8 bytes。完整库读取验证真实原件的开销尚无设备性能资格。

## 独立音乐 FD prepare

新增 `morrow_hmos_music_prepare(source_owned_fd,destination_owned_fd,max_bytes)`，max 1..150 MiB；两 owned descriptor 在所有路径消耗，同 descriptor 不双关。要求常规非空 source、常规 destination、不同 inode/dev；source rewind 后完整流式短读/短写/hash，destination 截断、rewind、sync_all 后才成功。返回既有 FileReply `ok,error,byte_length,sha256`。附件 prepare 继续 64 MiB；未扩大附件额度。当前 Windows 测试覆盖真实 File reader 和流式 copier，**不能替代 OHOS raw FD ABI、设备 codec 或 player ACK**。
