# v29 Native 音乐基础限定验证

2026-10-09。实际实现独立 development 曲库、固定 Pending 原件请求、Core retained audio import/export、持久选择/排序/退役/歌词偏好，以及共享歌词定位、播放决策、候选匹配和 import policy。本次 **Native 基础 PASS_SCOPED**；音乐页面尚未接入，不能称音乐产品闭环或完整 Flutter parity 已完成。合同见 [music-native-contract.md](music-native-contract.md)。

## 冻结范围

准确 source/test path、bytes、SHA256 见 [music-native-inputs-final.json](music-native-inputs-final.json)，5 项复核零漂移：

- 新 `hmos/rust/src/music_bridge.rs`、`music_bridge_tests.rs`。
- `lib.rs` 必要 Request/Reply 和 music/FD 路由、普通 list/query 私有库隔离、独立 music prepare FFI。
- `file_stream.rs` 共用 copier 的独立 150 MiB music cap，保留原附件 64 MiB。
- `editor_business.rs` 只有既有 Reply literal 增加 `music: None`，无业务协议/校验逻辑变化。

其余 Reply 初始化在 `lib.rs` 同步增加可省略字段，旧 JSON 动作不输出 music。未修改 shared、C++、ETS、Index、SDK、Git 或设备。使用独立 `hmos/.build/music-v29-target`，没有共享 target 并发写。该输入清单于 default suite 运行期间生成，结束后再次核对相同；不冒称它是另一次全仓库 prelaunch inventory。双 ABI/SDK/package 的完整 capture 由主任务单独出证据。

## 最终运行

准确结果和日志 hash 见 [music-native-result.json](music-native-result.json)。所有命令使用当前实际 `hmos/rust/Cargo.toml`、`--locked --offline`。

| 运行 | 实际结果 | 证据 |
| --- | --- | --- |
| `cargo test --lib music_bridge::tests` | 8 PASS、0 fail、3 explicit ignored，0.77s | [scoped final](music-native-scoped-final.log) |
| `cargo test`（default 全库/binary/doc） | 库 200 PASS、0 fail、20 explicit ignored，90.49s；attachment binary 3 PASS，6.33s；self-check/doc 为 0 tests | [default final](music-native-default-final.log) |
| exact ignored `actual_process_crash_reopen_reconcile_uses_original_pending_and_owner`，依赖 feature `morrow-core/fault-injection` | 1 matrix PASS，13 真实 child 进程 exit86，2.96s | [crash final](music-native-crash-final.log) |
| exact ignored `actual_store_fixture_export`，显式输出路径 | 1 PASS，1.21s | [fixture export final](music-store-fixture-final.log) |

三项 music ignored 分别是需要显式输出路径的 DTO export、需要 fault feature 的 matrix、只由 matrix 调用的 crash child；后两项上述显式运行。原有其它 ignored 环境资格没有借本轮补做或改变状态。

## 实际验证覆盖

- 真实隔离 SQLite Store：空库只读不创建；begin Pending 持久化后 drop/reopen；实际原件 retained 和 Ready；再 reopen 全长/hash export；精确旧 import 不读取替代 reader。普通 list/query 不显示私有库，业务写/task/favorite/delete 等 reserved identity 拒绝。
- 错 hash、短读、额外尾字节拒绝，原 Pending/history/revision 保留；同 operation 改 name 请求拒绝；stale export revision 与 changed identity/hash 拒绝。实际 Windows `File` reader、短 writer、失败 writer 执行；测试中破坏 Snapshot owner 后 export/read fail closed。
- 真实 retained 后/Ready 前 reopen，inspect 为 Pending+bytes_retained=true；同原 literal reconcile 发布 Ready；remove 提交 Retired，再 replay 原 import 只给历史 Ready receipt+当前 Retired，不复活、不借旧 revision 回滚。
- 真实排序保持 selected ID、select 后完整 order；歌词保完整中文原文、共享多 timestamp/offset 行与 active_index；无时间歌词保原文；raw UTF8 超限、parsed reply 展开预算超限、CAS/page limit 拒绝。未来 root/retained track protobuf unknown fields 经真实 Store 写与 reopen 保留。
- 三个 150 MiB Pending 在真实 Store 预留 450 MiB，第四个超过逻辑总额拒绝且不保存音频；单件 150 MiB+1 和加密扩展拒绝；512/513 cumulative entries 为结构验证，不冒称 512 曲目真实音频压力验收。
- 共用 streamed copier 实际传过 64 MiB+1 到 sink，music 准许、旧 attachment cap 拒绝；无需分配整个歌曲。prepare raw Unix/OHOS FD ABI 未在 Windows执行，未把 Windows File 测试冒称此资格。
- 共享 pure Playback 的 seek clamp/next/restore/blocked、候选歧义匹配和全 pages snapshot consistency 执行；这是决策 DTO，不能视为 AVPlayer transport ACK。

13 crash 点分别为 stage-after-allocation/payload/chunk、stage-retained-before-commit、stage-before-commit、stage-after-commit，以及 Ready after-begin/card/operation/blob-references/event/before-commit/after-commit。前五点 reopen owner=false/revision1，只读 reconcile 保 Pending；其余实际 owner=true，原 literal reconcile Ready；最后 Ready commit 后 reopen revision2，返回历史精确结果。完整 per-boundary 状态在日志。

## 可公开实际 DTO

[music-store-fixture.json](music-store-fixture.json) **65127 bytes / SHA256 `DA6D8DF4F50F59555A4064AB29C9B302CE29B85A843DEB03F2F5C9EB0D83B9B2`**，全部通过真实 Engine/Store/原 Reply serializer 导出。含完整空库、Pending/Ready、歌词、选择/排序、retained Pending/reconcile/Retired、18 entries 的 16+2 lexical read pages、当前 selected Ready 身份、seek/next/restore、拒绝 reply、actual export FileReply。最终当前库 revision26，selected=`fixture-second`，order=`fixture-second,fixture-track`；客户端应读取 DTO，不从历史 receipt 推算当前状态。

夹具原件是明确公开的 synthetic bytes，不是可解码、可听见或设备播放样本；没有 provider/private path、密钥或账号凭据。原 [stage1 DTO](music-store-fixture-stage1.json) 保留作为早期限定模型输入，未覆写、不代替本冻结 final DTO。首次 scoped run 有一个新 query 测试使用非法条件的失败；改为既有真实中文 caller 条件后才完成上述冻结测试，未放宽生产 query rules。早工具输出/初轮结果不混用为 final qualification。

## 仍未完成

UI/Index MusicPanel 连通、设备可听音频/codec/后台和实际 HMOS FD ownership、protected host/签名/安装、metadata/封面、在线歌词网络、加密音频与解密、Retired 原件及历史 pins 的 durable release/GC、设备完整库性能与完整 Flutter parity 均保持 OPEN/NOT_RUN。Pending→retained→Ready 是独立事务，不承诺跨事务原子；Unknown 按真实 owner/history reconcile，不盲重导或盲 GC。外部 Windows DLL/密钥路径未访问，未将其包装为可用 HMOS 解密能力。
