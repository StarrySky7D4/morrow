# v29 音乐基础分支交付限定验证

2026-10-09，基线 `af1bb1d1927413846cb0de66528b44accd68331a`。本轮交付独立 Native 音乐库、完整原件 FD 流、严格曲库协调器、播放器生命周期和私有 spool/cache 的基础能力。**Index 音乐 UI 尚未接入**，本轮仍不是音乐产品闭环或完整 Flutter/Windows 功能等价。交付仅用于 `codex/ArkTsUI`，不并入主线；版本保持 **dev19 /1000019**。

## 实际实现与保留边界

Native 复用真实 Core Card/retained blob 事务和 shared workbench 的 preferences unknown-preserving codec、Playback、parse_lyrics、lyric_match、import_policy。独立私有音乐库从普通业务 list/query 排除；不借 idea/draft 身份。固定 operation/track/CAS revision/name/完整 length/hash 和原 FD 请求先持久 Pending，再保存 retained 原件，最后发布 Ready；三个步骤是独立事务，没有跨事务原子承诺。Unknown 按真实历史/owner 和同 literal 显式 inspect/reconcile，不盲重导或盲 GC。

已实现标准本地原件导入/导出、完整页读取、选曲、排序、退役、离线歌词读写与定位、共享纯播放/候选匹配策略。新增 `MusicLibrary.ets`、`MusicFiles.ets`、`MusicPlayback.ets`、`PlatformMusicPlayer.ets`，以及必要 C++ NAPI/Workbench 接线。播放器只通过真实 Files registry 的 owned source/descriptor 取得 FD，AVPlayer release 后才关闭 FD/释放来源；测试 host callbacks 不代表设备 ACK。Native `source_uri` 不是可打开路径。standard extension+length/hash 验证不代表 codec 或可听音频。

累计 512 entries（含 Pending/Retired）、Native 单件 150 MiB、累计逻辑 512 MiB，保留 Core 既有硬额度；音乐独立 prepare 150 MiB，附件 prepare 仍为 64 MiB。歌词完整 raw/parsed/reply 预算超限拒绝、不截断。Retired 原件与历史 pins 保留，逻辑预留不释放；durable release/GC 仍 OPEN。详细合同见 [Native 合同](music-native-contract.md)。

## 当前精确验证

| 项目 | fresh 实际结果 | 证据 |
| --- | --- | --- |
| Rust default library/binary/doc | **200 library +3 attachment binary PASS**；库 20 explicit ignored；self-check/doc 为 0 tests；90.49s/6.33s | [Native final result](music-native-result.json)、[完整 default 日志](music-native-default-final.log) |
| 音乐 Rust 子集 | **8 PASS**、0 fail、3 explicit ignored；属于 default 范围，不重复累计 | [专项日志](music-native-scoped-final.log) |
| 真实进程中断/重启 | **13 个实际 child exit86 边界 PASS**，1 matrix；仅 host fault-injection scope | [crash 日志](music-native-crash-final.log) |
| 实际 Store DTO | exact exporter **1 PASS**；最终 65127B、SHA `DA6D8DF4F50F59555A4064AB29C9B302CE29B85A843DEB03F2F5C9EB0D83B9B2` | [完整 DTO](music-store-fixture.json)、[导出日志](music-store-fixture-final.log) |
| 实际 ETS/tool 全量 | **1021/1021 PASS，0 fail/skip/cancel**；43 suite 文件、143 项实际输入前后一致；32,783.3028ms | [final 结果](models-final-result.json)、[完整日志](models-final-tests.log)、[before](models-final-inputs-before.json)、[after](models-final-inputs-after.json) |
| 曲库独立限定子集 | **28/28 PASS**；含实际 Store DTO 读取，11 项输入零漂移 | [曲库结果](music-library-result.json)、[final 日志](music-library-final.log) |
| 播放器平台层限定子集 | **50/50 PASS**；6 项输入零漂移；controlled Files/AVPlayer seams | [播放结果](music-playback-result.json)、[限定验证](music-playback-validation.md) |
| 播放器 actual DTO 消费子集 | **6/6 PASS**；5 项输入一致；只读 Ready/current/selected/export/policy 身份 | [结果](music-playback-store-fixture-result.json)、[限定验证](music-playback-store-fixture-validation.md) |
| 新双 ABI Native release | **ARM64/x64 PASS 并采用**；283 项 Native 来源，shared 冻结复核；非 v27 archive 复用 | [精确 Native inputs/archives](native-build-inputs.json)、[ARM64](native-arm64-build-result.json)、[x64](native-x64-build-result.json) |
| 完整隔离 API26 产品 SDK | **SUCCESS /30.098s**，34/34 tasks 执行、0 up-to-date；319 复制/392 仓库输入前后一致 | [final SDK result](sdk-final-result.json)、[build log](sdk-final-build.log)、[copy manifest](source-copy-manifest-final.json)、[repository inputs](build-inputs-final.json) |
| 独立 Music SDK probe | **SUCCESS /23.545s**，34/34 tasks 执行；319 项 probe 输入一致；四音乐模块实际列入 filesInfo 并 emit | [final probe result](music-sdk-probe-final-result.json)、[build log](music-sdk-probe-final-build.log)、[inputs](music-sdk-probe-final-inputs.json)、[module coverage](music-sdk-module-coverage.json) |
| 产品 HAP 原生库核对 | **4/4 PASS**（两 ABI 的 libmorrow/libc++） | [final package check](native-package-check-final.json) |

曲库和播放平台子集包含于全量模型范围，不相加为额外验收数字。模型读取并执行 actual ETS，部分 Files/AVPlayer/native lifecycle 回执由可控 host 提供；完整 Store DTO 本身来自真实 Rust Store，不由 Node 构造。以上模型资格不替代 SDK strict ArkTS 或设备 transport/FD/codec 资格。

实际 Index 尚未引用新增音乐模块，final 产品 entry graph 中 MusicFiles/MusicLibrary/MusicPlayback/PlatformMusicPlayer 的 filesInfo/emit 均为 false。独立 final Music SDK probe 在改动的未安装 test entry 中显式引用四模块，四项 filesInfo/emit 均为 true；提供严格 SDK compile/type 资格，**不是 final 产品 HAP 的音乐页面或运行资格**。probe 不进入交付产品 HAP，音乐 UI/页面和真实音频设备运行仍未验收。两个 entry graph 的准确结果见 [module coverage](music-sdk-module-coverage.json)。

## 准确原生与产品身份

| Artifact | Bytes | SHA256 |
| --- | ---: | --- |
| ARM64 `libmorrow_hmos.a` | 58,110,214 | `DD86DF956795BC04CDF98BC3184625C6FAD9D0382E041E86DEAF99C44028744A` |
| x64 `libmorrow_hmos.a` | 56,515,718 | `E9C66A4974D32CD9B6C68559504F56EBA7D10CC27F1DB3CCFDC31B4FDDA4BF1E` |
| final unsigned HAP | 30,842,790 | `4C4292881291C769B3039E01F895BF75C6C526070DB063AB78224698743015E1` |

最终包在 ignored `.build/artifacts/dev29-music-foundation-final/entry-default-unsigned.hap`；准确身份见 [artifact-final.json](artifact-final.json)。包 **unsigned/uninstalled**，设备验收 **NOT_RUN**。Native 原件/FD 测试在 Windows host 使用真实 File 和流式 reader/writer；新 OHOS raw descriptor ABI 经双 ABI 构建，但没有设备运行资格。本轮没有安装、签名或 protected 宿主批准，也不借既有已安装 v27 的限定 UI 记录赋予 v29 产品资格。

## 历史与冻结

Native 5 项 source/test 准确 path/bytes/hash 见 [冻结清单](music-native-inputs-final.json)，最终 Root FD/NAPI/Files 精确冻结见 [Root inputs final](root-music-inputs-final.json)。Native 原始冻源清单在 default run 期间生成、结束后复核相同；283 项 build capture 才是本轮独立 Native 构建来源资格。最终 MusicFiles 为 **28429B /`C4472E6A9C3F4E04149A08C2AFCD54DA10AD52BB96F56780FA19AB218FB2B92D`**，只有 explicit interface/typed envelope 的 SDK 要求窄修；Native 来源和 archives 未随此窄修变化，无重复 Native 构建声明。早期 invalid query 条件的测试失败已修为既有 caller，未放宽 query 规则；stage1 DTO 与 Library 调试阶段日志保留为历史，不混作 final qualification。

[首轮 Native capture](native-capture-attempt1.json) 在编译前拒绝未登记的 Root bridge 变化，`FAILED_BEFORE_BUILD`；登记准确 Root inputs 后才重新 capture/build/adopt。未覆盖此历史拒绝，也不把它算作最终构建失败。

首轮 foundation 模型 **1021/1021 /28,745.8251ms**、产品 SDK **27.251s** 和 `F68809C5…` HAP 保留在 [旧模型结果](models-foundation-result.json)、[旧 SDK 结果](sdk-foundation-result.json)、[旧 artifact](artifact.json)，仅资格对应窄修前 MusicFiles `58DA1029…`。其未引用的四模块不能借旧产品 SDK 获得 strict type 资格；[首轮 Music SDK probe](music-sdk-probe-result.json) 的实际失败也完整保留。完成两处 typed 窄修后，使用独立 final 目录重新运行全量模型/产品 SDK/probe，只有以上 final 文件用于当前 MusicFiles/source qualification。新旧 v29 子报告均保留；此前各轮历史叙述不改，三份顶层文档当前音乐/分发表更新到本轮准确范围。

## 继续追平范围

下一阶段接实际 Index 音乐控件、文件选择/播放列表/歌词/footer 流程并取得设备准确输入、可听声音、seek/上下首/后台/restart 和真实 FD 清理证据。在线歌词、加密音频解密、metadata/封面、Retired 原件 durable GC、protected 宿主和完整 Flutter/Windows parity 均 **OPEN**；当前通过数与新 archive/HAP 不能把这些项目改为完成。
