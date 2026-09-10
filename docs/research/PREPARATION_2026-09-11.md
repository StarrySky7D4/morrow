# Morrow 前期调研准备

日期：2026-09-11
范围：M0／M1 的准备与少量隔离技术探针；不是核心实现、插件安全验收或全平台资格声明。
依据：[完整架构基线](../ARCHITECTURE_BASELINE.md)、[未来路线](../FUTURE_ROADMAP.md)。本次未改业务代码、应用依赖、用户资料或发布包。

## 1. 当前结论与首轮候选

建议保留 Flutter，继续按既定顺序验证内容契约、可靠提交、共享对象／权限、审计封存、受限重放，再迁移真实业务。此次调研没有发现需要推翻该路线的证据，但不能直接锁定全部实现库。

| 决策点 | 研究结论 | 下一步 |
| --- | --- | --- |
| Cap’n Proto | Rust 工具已可用；Dart 有新旧两组候选，不能假定能力等价 | 先验证生成器、普通／分段／畸形消息、未知字段及 Web 数值边界 |
| Flutter↔Rust | 薄 C ABI 作为对照方案；FRB 作为桥接候选 | 保持 Cap’n Proto 业务契约，计量生命周期、复制与取消，不把桥接自带 codec 当作默认业务协议 |
| Protobuf | **实测 prost 0.14.4 普通类型化往返丢弃未知字段** | 暂不批准它直接承担需要无损编辑的长期记录；比较保留未知字段的实现或受控动态消息方案 |
| 原生存储 | SQLite 值得进入下一轮候选；合成数据的内容＋事件事务恢复通过 | 用实际 Rust 绑定、正式二进制载荷及附件提交继续验证 |
| Web 存储 | IndexedDB 适配与 SQLite WASM／OPFS 需对照测试 | 单一核心语义，平台事务实现分别证明；不宣称与原生同等耐久性 |
| 插件后端 | Wasmi、Wasmtime／Pulley、浏览器 Worker 内 Wasm 分别验证 | 不锁定全平台唯一后端，不把解释执行等同于分发渠道许可 |
| 共享对象 | 原生 mmap 仍是 IPC 主路径；固定内容与释放证明先于优化 | 创建不可变发布和跨权限复用探针，不能仅靠代次或租约到期回收 |
| 审计与证据 | 验证原始字节规则得到实测支持 | 将固定字节、待封存事件、独立检查点和隔离重放列为后续硬门槛 |

以上是进入原型的候选，不是生产选型批准。比较时固定包版本、生成器、依赖锁和平台；网页的 latest 页面仅代表本次查阅结果。

## 2. 本机与源码核对

| 项目 | 本次结果 | 实际含义 |
| --- | --- | --- |
| 应用 | 0.1.8+9；HEAD ee8c5744b3fff8e6015fa6a04917502d953d28d0 | 工作树仍有未提交修改，HEAD 单独不能代表当前源码 |
| Rust / Cargo | 1.95.0 / 1.95.0 | 本次成功编译并运行隔离 Rust 探针 |
| Rust 已安装目标 | x86_64-pc-windows-msvc | 尚未由本环境证明 Android、Web、Linux 或 Apple 核心交叉构建 |
| Cap’n Proto 编译器 | 1.4.0，在 PATH 中 | 只检查版本；尚未执行 Dart／Rust schema 互操作 |
| Dart | 3.12.0，直接调用 Flutter 缓存内 dart.exe | 本次未重跑 Flutter 全量构建或测试 |
| Node | v24.19.0 | 可用于浏览器探针编排，本次未运行新的浏览器核心探针 |
| protoc | PATH 未发现 | 不等于整机未安装；下一轮选定生成链后再补齐并固定版本 |
| Rust 缓存 | capnp 0.24.1、capnpc 0.24.0、prost 0.14.4／0.11.9、rusqlite 0.31.0 | 缓存命中不代表已经选用或兼容全平台 |
| Android | 发现 JDK 17.0.20.1+1 路径、ADB、NDK 28.2.13676358 与 SDK 31/34/35/36/37.0 目录 | 此处为安装路径检查；不替代新核心的编译及真机运行 |
| macOS / iOS / Linux | 本次没有这些平台的新运行证据 | 需对应 runner／工具链和设备，不阻止 Windows 隔离原型继续 |

设计与源码读取锚点（SHA-256，仅覆盖列出的文件，不是完整工作树快照）：

| 文件 | SHA-256 |
| --- | --- |
| docs/ARCHITECTURE_BASELINE.md | b70f7802aa6c0a956cc35c1635f62a3626c1d01d21e97cb8988ec6cd6d692cb1 |
| docs/FUTURE_ROADMAP.md | f1bea8649e10a3454bf7addab72d840ceb1652f2d709ed3d69e412c5b86602fd |
| lib/storage.dart | c1726effe2bf4355b724a7156ebc271264e13bd8aec7dc3c5cf3de40ee2f3aa2 |
| lib/main.dart | c5a9b63eef07b462ce936d4783afcd5fdd7a9995b6f277c50fa08783fa44131c |
| pubspec.yaml | 11131916da79edcf79419b1f27ef73ae6d077ea819f2b333a47e27e5f45ec232 |

M0-01 的完整工作树归档、恢复演练和最终产物绑定仍未完成。已有 UI、平台适配等修改继续保留，各自归属不因此次调研改变。

## 3. 序列化与桥接调研

### Cap’n Proto：区分旧 Dart 包与新候选

旧 [capnproto 0.2.0](https://pub.dev/packages/capnproto) 的发布者说明明确列出不支持编码、枚举／union 等能力，因此不能直接承担双向业务契约。新 [capnproto_dart 0.1.0](https://pub.dev/packages/capnproto_dart) 声明支持读写，并配套 capnpc_dart 生成器。它是独立候选，不能因为名字相似就继承旧包或 Rust 实现的验证结论。

Rust 的 capnp／capnpc 与官方 capnp 编译器先作为参照。首轮不引入通用网络 RPC 或将 Cap’n Proto capability table 直接当作 Morrow 的权限系统。对象授权、租约和身份由宿主管理。

Dart／Web 重点向量：空列表、默认值、union、跨段指针、Unicode、字段演进、截断与循环式恶意结构、资源预算，以及超过 JavaScript 精确整数范围的 64 位 ID／修订。身份可选 bytes 表示，具体 schema 由 M1 决定，不能靠转换为 JSON 回避问题。

[flutter_rust_bridge 的 codec 文档](https://cjycode.com/flutter_rust_bridge/guides/miscellaneous/codec)说明其 SSE 等编码机制。研究建议是把它作为调用和资源生命周期工具比较；若采用，证明业务消息仍符合既定契约，并测量 byte buffer 的实际搬运。先用薄 ABI 做可测对照，不立即执行整项目 integrate 或改写现有插件结构。

### Protobuf：无损编辑与原始证据是两项保证

本机缓存中的 prost 0.14.4 已做实际编译验证；详见第 6 节。已知结果：

- 普通生成类型丢弃示例中未知字段 2，不能直接承诺长期记录无损编辑。
- 未知枚举整数能够保留，但这不等于未知字段保留。
- 即便没有未知字段，合法原件也可能在重编码后改变字节。
- 将原件放入 bytes 字段可保留该载荷，但外层新增字段仍可能丢失，不能据此宣布整套迁移兼容。

[prost-reflect DynamicMessage](https://docs.rs/prost-reflect/latest/prost_reflect/struct.DynamicMessage.html#method.unknown_fields) 明确描述未知字段往返保留，可以进入对照组。若使用，descriptor 来自构建期固定、安装前校验的契约；不能让插件运行时任意注入特权 schema。动态消息再转换为普通生成类型仍需防止丢字段。

[rust-protobuf](https://github.com/stepancheg/rust-protobuf) 虽有 UnknownFields 能力，但仓库已声明接近生命周期结束，不建议仅为该能力直接锁定为新项目长期底座。[官方 Rust Protobuf](https://protobuf.dev/reference/rust/rust-generated/) 也列入候选，需实际验证生成器／kernel、Cargo 构建、移动与 Web 兼容，不沿用旧 crate 名称推断维护状态。

下一轮比较完整流程：新版本写入→旧版本读取并修改一个已知字段→保存→新版本再读；同时覆盖嵌套、oneof 和未知枚举。证据验证始终使用保存的原始字节，不以这些编辑测试替代签名原件规则。

## 4. 存储、执行与共享内存

### 原生 SQLite 与 Web 存储

SQLite 的事务和恢复机制适合作为内容＋待封存事件的候选基础；其原子提交机制见 [官方说明](https://sqlite.org/atomiccommit.html)。本次探针使用 Python 自带 SQLite，不是 rusqlite，也不是最终应用后端。

建议先比较两条 Web 路径：

| 路径 | 如何保持统一权威 | 待证风险 |
| --- | --- | --- |
| Rust 核心＋异步 IndexedDB 适配 | 核心产生提交计划；平台适配以一个事务提交内容和事件 | 多标签页协调、事务自动结束、配额、标签页退出及浏览器清理 |
| Rust 核心＋SQLite WASM／OPFS 适配 | 复用核心业务规则，比较实际数据库适配成本 | VFS、Worker、锁、浏览器覆盖、包体和部署头要求 |

SQLite 官方区分不同 OPFS VFS：普通 opfs 路径依赖相应跨源隔离条件；opfs-sahpool 不要求同样的 COOP／COEP 头，但有并发取舍。不能把“OPFS 必然要求共享缓冲”或“浏览器有 SQLite 就与原生相同”当作前提。[SQLite WASM 持久化文档](https://sqlite.org/wasm/doc/trunk/persistence.md)

原生 WAL 也不是无限并发写入或任意文件系统耐久保证；具体提交参数和故障恢复按 [WAL 文档](https://www.sqlite.org/wal.html)及目标环境验证。内容与事件同事务不能同时使外部附件文件发布原子化，Blob 暂存、引用提交和回收仍需单独状态机。

业务记录使用 Protobuf＋LZ4，附件保持原字节，索引由引擎管理。浏览器后端差异不允许引入第二套 Dart 内容逻辑或 JSON 正式保存旁路。

### 插件后端候选

| 候选 | 文档依据 | 原型重点 |
| --- | --- | --- |
| Wasmi | 项目定位为解释器，支持 fuel 计量 | 受限纯任务、包体、内存上限、宿主调用取消、各目标构建 |
| Wasmtime／Pulley | 有原生编译与解释后端，平台支持按目标组合区分 | 桌面执行性能、可执行内存要求、预编译载荷信任、解释模式成本 |
| 浏览器原生 Wasm＋Worker | 作为 Web 平台候选承载机制 | 终止 Worker 后迟到消息、资源释放、宿主授权、配额和不同浏览器差异 |

[Wasmi 项目](https://github.com/wasmi-labs/wasmi)、[Wasmtime 平台说明](https://docs.wasmtime.dev/stability-platform-support.html)提供能力依据；本次未编译或安全验收这些候选。宿主函数的文件／网络／数据库阻塞不能仅靠 Wasm fuel 解决。执行隔离、能力授权与撤权都仍是 Morrow 宿主职责。

Apple 平台须把技术可运行性与分发资格分开验证。其审核规则 2.5.2 与 4.7 对可下载功能和特定插件形态有约束，不能仅因使用解释器就承诺任意动态 Wasm 插件获准分发。[Apple 官方规则](https://developer.apple.com/app-store/review/guidelines/) 这是平台调查项，具体产品形态及分发渠道仍待验证。

### Windows 共享对象

[文件映射权限说明](https://learn.microsoft.com/en-us/windows/win32/memory/file-mapping-security-and-access-rights)和 [UnmapViewOfFile](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-unmapviewoffile)是原型依据。设计上分别追踪句柄和各进程视图；不能将关闭句柄、租约到期或代次变更当作旧映射已释放的证据。

拟验证：宿主准备对象→固定内容→受控只读交付；不可信输入存在可写别名时，先可信复制。测试旧视图尚在时对象退休、进程异常退出、重复释放、跨权限范围再分配，以及检查与执行间篡改。当前只有设计与官方 API 调研，没有 Windows mmap 运行验证。

## 5. 现有数据到新模型的准备清单

此次只读取源码，没有读取实际用户资料库。

| 现有表示 | 候选归属 | 迁移约束 |
| --- | --- | --- |
| Idea.id、title、description、attachments | Card 身份、正文与 Blob 引用 | 保留 ID 和原始附件；内容校验与文件校验分别进行 |
| category、stage、hypothesis、conclusion、todos | 业务类型载荷 | 不把中文阶段字符串固化到核心枚举 |
| favorite、展示图标／颜色 | 用户元数据／视图呈现，需明确多视图语义 | 收藏到底全局还是工作区级须由模型决策，不能迁移时猜测 |
| completed | 条目清单状态与顶层日常状态两种来源 | 分开识别，不能仅按字段同名合并 |
| theme、glass、background、texture 等 | 持久配置及资源引用 | 保留配置语义；UI 即时状态与持久配置分开 |
| time='刚刚' 等展示文本 | 旧展示信息 | 不伪造精确创建时间；缺失真实时间明确表示 |
| 本机路径、IndexedDB key、远程 URL | 平台存储定位及外部资源引用 | 公共 BlobId 不直接等于绝对路径；远程仅引用不等于已下载 |

下一轮 M0-03 准备至少六类合成迁移样本：完整条目、未知扩展字段、失踪附件、重复引用、损坏快照、旧展示时间／缺省字段。先建立字段映射和预期校验，不修改当前应用以适配尚未冻结的 schema。

本次未完成资料恢复演练、迁移器、1千／1万／10万条 UI 性能基线。它们仍是 M0 的未完成项。

## 6. 已运行的隔离探针

### P-01：Protobuf 行为刻画

入口：[Cargo.toml](../../tool/research/protobuf_probe/Cargo.toml)、[main.rs](../../tool/research/protobuf_probe/src/main.rs)。依赖固定 prost=0.14.4，附 Cargo.lock；使用本机缓存离线构建。4 项刻画检查均通过，但其中“未知字段丢失”是确认了候选限制，不能记为兼容性通过。

在项目根目录复现：

~~~powershell
cargo run --offline --locked --manifest-path tool/research/protobuf_probe/Cargo.toml --target-dir build/research/protobuf-probe
~~~

仅覆盖一个简单未知字段、一个未知枚举、重复标量编码及 bytes 包装。没有验证嵌套演进、Cap’n Proto、LZ4、跨语言、Web 或最终证据协议。

### P-02：SQLite 提交恢复

入口：[sqlite_atomicity_probe.py](../../tool/research/sqlite_atomicity_probe.py)。Python SQLite 3.53.1，WAL＋synchronous=FULL；生成测试专用数据库，由子进程在指定位置直接退出，再由父进程重开检查。

| 注入位置 | cards / outbox 行数 | 结果 |
| --- | --- | --- |
| 写入卡片后，尚未写事件 | 0 / 0 | 未提交事务恢复，无孤立卡片 |
| 内容与事件均写入，尚未 commit | 0 / 0 | 一同回滚 |
| commit 返回后立即退出 | 1 / 1 | 一同恢复 |
| 恢复后查询同一操作 ID 再重试 | 1 / 1 | 已提交结果可识别，不重复插入 |

~~~powershell
& 'C:\Users\Administrator\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 tool/research/sqlite_atomicity_probe.py
~~~

可换用带 sqlite3 的 Python。临时目录在 build/research 下，脚本结束清理自己创建的数据库，仅向终端输出诊断；不访问真实资料。BLOB 使用合成字节，不代表已实现正式 Protobuf＋LZ4 容器。

这证明本机 Python SQLite 在所测进程崩溃点的行为；**不证明断电、损坏文件系统、磁盘写满、Rust 绑定、浏览器或跨文件附件事务**。重试示例只证明查到已提交操作，不是完整多客户端幂等实现。

## 7. 下一轮探针执行单

| 编号 / 对应路线 | 状态 | 输入与验收 | 决策出口 |
| --- | --- | --- | --- |
| P-01 / M1-02 | 本轮完成有限刻画 | 上述 4 项固定字节检查 | 普通 prost 类型不能无条件承诺未知字段无损 |
| P-02 / M2-02 | 本轮完成前置小试 | 上述 3 个崩溃点＋1 个重试 | SQLite 留在候选，不视为 M2 完成 |
| P-03 / M1-02、M1-03 | 待执行，P0 | Rust↔Dart／Web 相同 Cap’n Proto 向量；Protobuf 新旧字段往返对照 | 决定绑定库、表示及生成链 |
| P-04 / M1-03、M2-02 | 待执行，P0 | Rust 原生存储与两种 Web 适配执行相同提交、配额、重开、竞争向量 | 决定后端和能力差异 |
| P-05 / M3-01、M3-02 | 待执行，P0 | mmap 固定发布、旧映射回收、身份伪造、撤权及迟到结果 | 决定共享对象实现和必要复制位置 |
| P-06 / M1-03、M3-03 | 待执行，P0 | 同一纯 Wasm 模块在候选后端执行、死循环终止、内存和宿主调用限额 | 决定首轮 A/B 执行后端，不提前锁定全平台 |
| P-07 / M0-03、M6-01 | 待执行，P0 | 六类迁移样本、工作树归档与恢复、全链性能基线 | 固定模型迁移边界和测量基线 |
| P-08 / M4、M5 | 待执行，P0 | 固定原件签名、幂等封存、独立检查点、隔离重放 | 完成 A/B 贯通验收最后一段 |

P-03、P-07 是下一轮最先推进的工作。P-04、P-05、P-06 随契约冻结逐步进入验证；P-08 依赖提交与共享对象语义。此执行单不改变总路线的阶段门槛。

开始下一轮前补齐的是可复现工具清单：选定 protoc／descriptor 生成方式，增加所需 Rust 目标，准备 Apple／Linux runner 和实际浏览器矩阵。按需安装并固定到探针环境，不先升级全局 SDK 或把研究依赖加入应用 pubspec。

首轮范围仍是两个受限测试插件的贯通链；本次不扩展到市场、云平台实现或全量应用重构。
