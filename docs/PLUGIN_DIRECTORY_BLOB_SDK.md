# 目录观察与分段字节验证 SDK

当前C10检查点（2026-10-05）：本次开发分支更新收录C08–C10。C10已新增独立 `fs-directory-request-v1`、严格单import、包feature及只读discovery，复用原已批准selection和owner，不导出路径、句柄或新授权。Windows新Rust34（codec12/helper4/profile9/owner9）、既有回归191及frame5分别通过；Python33和两个原生C/C++消费者另计。新Rust Wasm仅编译通过，真实新Rust guest、C/C++ Wasm、Workbench产品／GUI、受保护Session与其他平台仍未验收。旧Core IO FileList保持Unsupported，SDK26／G04仍OPEN，无新Release。 详见[接口与实测边界](../reports/reconstruction-2026-10-05/directory-request-sdk.md)。

当前开发范围与完整项目状态见 [项目状态](PROJECT_STATUS.md)。C06提供独立目录/blob载荷库；C07在Windows普通合成Store／临时目录上接入原IoWorker目录命令、合作取消与idle清理。C08已新增原工作线程生成秘密的可信宿主入口，验证结果为 `PASS（Windows限定）`，见 [阶段说明](../reports/reconstruction-2026-10-05/directory-secret-factory.md)。公开guest FileList及conditional Replace仍Unsupported，SDK26／G04保持OPEN。

这两个独立实验库提供 Rust、C 和 C++17 数据接口：`fs-directory-v1` 表示有限的目录观察，`blob-transfer-v1` 验证有界分段字节流。本轮 G04 和 SDK26 仍 OPEN。它们的 codec 身份不是包 required_feature、公开 import、文件路径授权或生产能力。

Windows Release 的库内方法已完成：目录库 13 个、blob 库 23 个，均无失败、忽略或过滤。独立 Windows native Rust/C/C++ 消费者与原生目录 broker 的限定资格通过：138 wire vectors（目录76、blob62）、原生 broker16方法；另有7个独立Rust方法及19个去重检查families，C++复执行8个已有C families不另加方法。原件42与网络100已按当前接线重新实际执行；这些数字分别计数，详情见 [本轮记录](../reports/reconstruction-2026-10-05/directory-blob-sdk.md)。本轮新增消费者均在 Windows native 执行，没有新增目录/blob Wasm 客体或 guest 协商。原件42按定向parent名单计数，其过滤与child helper排除另记，不能写整体零过滤。此前冻结 SDK 与历史客体未因此重建。

## 目录数据和观察状态

目录契约 version 1，原始 schema SHA256 为 `ade60daee77497056a3fe618b38616331b8d5de61bdb494f703592e74ec3d5f7`。每页含 nonzero 32-byte `selection_epoch`、从 1 开始的 `page_sequence`、entries 和 terminal。每条 entry 含 nonzero 32-byte `entry_id`、完整 name bytes、UTF-8 或 UTF-16LE 编码、File/Directory/Other 类型以及可选逻辑长度；只有 File 可带长度。

名称是平台表示的数据，不是可拼接或重新打开的路径。UTF-16LE 保留孤立 surrogate，调用者不能以有损 Unicode 转换代替原字节。空名称、`.`、`..`、NUL 和路径分隔符拒绝。epoch 和 entry ID 只关联同次观察，不授予 FileRead、递归、rename、delete 或未来重新选择的权限。目录名也不暗示目录内容可读。

每页最多 32 条、名称合计 16,384 bytes，wire 独立限制 65,536 bytes，traversal 16,384 words、nesting 8。默认 allocator 保持不变；字段有效并不保证重新编码的 wire 仍在上限内。`DirectoryState` 固定 epoch，核验连续页序、跨页 ID 唯一性和终页；累计最多 1,024 entries、1,024 pages、1 MiB name bytes 与 1 MiB admitted wire，可由可信调用者收紧。

实际 wire 费用先 admission，随后 malformed、foreign、duplicate、terminal 等语义拒绝保留已收费用，接受状态不推进。release 丢弃 resident IDs、关闭 helper，保留费用和计数。helper 没有原 broker 的 liveness/deadline 检查，也不能恢复原 IoBinding 的信用或权限。

## Windows 原生目录接线边界

原bare-File宿主侧 `DirectoryBroker` 消费可信宿主已经选择并打开的 directory `File`，直接对持有的 Windows handle 查询身份与有限条目；不使用公开 guest 路径，也不重新解析路径。原 IoBinding、精确 managed instance 和原 host 继续负责 FileList admission、预算与各边界检查。同步 OS 查询必须由原 owner 在 UI 线程之外调用，合作式取消不能中断正在进行的同步查询。

原bare-File观察实现拒绝 root/child reparse，查询期间保留 root handle 并在批次及交付边界核验对象身份；不会跟随子目录、symlink/junction 或以 entry 名称打开子文件。hardlink 的目录项观察不授予链接目标读取或写入。持有 root object 只证明这个对象，不能证明它的祖先路径、picker 来源或父级选择策略；这些证据仍由可信选择 adapter 保管。并发目录编辑可能被观察到，本接口不保证文件系统原子快照。

C07的可信宿主入口是原IoWorker的 `capture_directory(file, limits, fresh_host_secret)`、`next_directory_page(session, request)` 和 `finish_directory(session)`；原Manager／HostRuntime／ManagedInstance／IoBinding负责实际FileList准入。每个selection只允许一个pending／未读结果，broker在页编码与最终核验后提交cursor；调用方只有成功领取后才可据回执请求下一页；foreign session/epoch/cursor不能擦除他人selection，Unknown不自动重放。global8包含queued／resident／retired tombstone，原root／lease／spool真正drop后才释放额度；累计费用不退，`directory_usage().1`是metadata allowance，不是总RSS上限。

原clock的采样与对应验证在同一短step完成；native identity/Buffer查询、私有解析、编码、取消谓词和实际drop在clock锁外。谓词只读取Ticket取消标记，不能授予权限，也不能插入native内部双查询／解析或抢占同步OS。idle维护沿原clock，停止信号、EOF或终态回执均不能替代原owner实际join。

C07九组115方法（已含7取消／14owner／16native）、原件42与network100分别通过；过滤、child helper与各层join范围见 [C07记录](../reports/reconstruction-2026-10-05/directory-owner-sdk.md)。这些历史结果不代替C08复验，也不是生产protected owner资格。

C08新增 `IoWorker::capture_directory_fresh(file, limits)`，在原owner工作线程经原身份、FileList及clock检查后调用固定 `getrandom 0.4.3` 的OS随机源；随机调用及派生均不持原clock、Control或Ticket锁，生成后再次核验原取消、时钟与授权。非零worker/session serial参与版本化域分离，checked exhaustion不回绕。失败、部分填充或全零输出关闭，不以时间、路径、旧key或重试替代熵；原Unknown与不重放规则保持。

生成缓冲、派生key及broker持有key由 `Zeroizing` 管理，在普通Drop／unwind清理。此范围不保证SHA内部状态、OS／编译器临时副本、abort或进程退出后的擦除。原 `capture_directory(file, limits, secret)` 和 `DirectoryBroker::new(secret)`保留，legacy caller仍负责秘密新鲜性和自己的副本。C08实际结果为 `PASS（Windows限定）`；它没有新增C／C++／Wasm目录入口。这些C08结果不提供picker或祖先来源证明；C09新增下面限定的宿主相对选择接线，C10已新增独立Dir request/import/profile，真实三语言guest及产品资格仍待验证。

## C09 可信anchor相对选择（Windows限定）

C09已完成限定Windows Release／locked／offline资格，见 [C09阶段说明](../reports/reconstruction-2026-10-05/directory-selection-owner.md)。可信宿主 `capture_directory_under(anchor, relative, limits)`只保留并核验原opened anchor到relative leaf的raw UTF-16句柄链，复用原worker FileList、原时钟、取消和预算；root加N个分量共享原8资源，32段语法上限不是可用深度。新selection_path8＋directory_selection12、C08 factory14、原owner九组115和原件42分别当前实际PASS；原件42为base9／dependency3／region7／reader主9／shared14，reader raw10含child helper1不加方法，region保留84过滤。17个credited测试进程合191 meaningful方法（raw192含child1），zero-match失败进程保留且不计功；这些数字不能作为SDK冻结。Workbench第二次Release x86_64 `--locked --offline --lib` check通过，首次缺offline asn1-rs0.7.2的exit101保留；只是编译检查，ProtectedSession／GUI／picker以上provenance和non-Windows产品执行NOT_RUN。C09 network100和Clippy明确NOT_RUN，不继承C08历史通过或lint结果。这不证明picker时刻、anchor以上来源或传入anchor的sharing策略，不增加guest FileList、目录guest或公共UI，blob耐久后端仍缺。SDK26／G04仍OPEN，公开FileList及conditional Replace仍Unsupported。C08/C09历史报告保留当时状态；本次开发分支更新收录C08–C10，无新Release。

原 `DirectoryRelativePath::new(Vec<Vec<u16>>)`保留精确raw UTF-16，不做Unicode转换、规范化或路径重开。每分量最多255 units，合计长度及保留分量容量最多8192 bytes，外层段数及容量最多32；空段、`.`／`..`、NUL、控制字符、分隔符、Windows保留设备名及末尾空格／点关闭。孤立surrogate不被转成替代字符。这只是可信宿主输入的语法边界，不是grant。

worker先在原IoBinding中为整个链预留FileList资源，然后以父句柄为RootDirectory逐段只读打开；NtCreateFile的FILE_OPEN不创建或截断，NoReparse设置和返回句柄的目录／reparse／volume-file identity检查共同关闭重解析路径。所有已持有祖先和leaf在native查询、分页的前后复核。查询及打开在原clock锁外，采样和对应授权验证仍在同一短step；取消只能否决，不能抢占同步OS调用。已admitted bytes费用不退，失败／取消／结束实际drop整链后才归还资源额度。通常N最多7，其他原IO资源会进一步减少可用深度；额度不足在native打开前关闭。持有链不等于文件系统原子快照。

## Blob frame 与接收状态

blob 契约 version 1，原始 schema SHA256 为 `941db7c662815f8963b46bbb65a5c143d90f9e7217937e84c05d1b8f11024543`。所有 frame 固定 nonzero 32-byte `transfer_epoch`、`object_ref` 和 `operation_id`。四个独立 action 为：

| action | 字段与意义 |
|---|---|
| Descriptor | total_length 与 whole_sha256；对象最多 16 MiB |
| Chunk | sequence 从 1 连续、offset、最多 60 KiB 的 payload 与实际 chunk_sha256 |
| Receipt | 原 request wire digest、sequence、offset、length、chunk_sha256；Accepted 或 Existing |
| End | 与原 Descriptor 相同的 total_length 和 whole_sha256 |

每帧 wire 最多 65,536 bytes，traversal 32,768 words、nesting 16；拒绝尾随消息字节和 capability。owned 分配前检查有界字段与范围，chunk digest 以真实 payload 验证。Rust `FrameRef::decode` 通过无 allocator 的 Capnp parsing 取得 payload，再验证它位于调用者原 wire 内，返回原字节的 checked sub-slice；不借用临时 owned segment，不创建未对齐原生引用。库显式启用固定版本 capnp 的 `unaligned` 支持。

`Receiver` 固定 Descriptor 和三个 ID，要求连续 sequence/offset、checked arithmetic、总长度上限和各费用上限，并以 streaming SHA256 更新每条首次接受的 payload。它同步处理请求，无排队 pending 工作，只保留最近一条原 wire（最多 64 KiB），不保留完整对象。

仅同一 live 状态的最近请求、相同原 wire digest、原字节与全部字段都一致时，重复可返回 Existing。不同编码的相同语义、旧 chunk、foreign IDs、gap、overlap、错误摘要和提前 End 均不能推进接受状态。重复不再计算 payload/对象摘要，但仍支付请求 wire、计数和准确编码的 receipt 费用。已 admitted 的费用不因失败、cancel 或清理退回；超过 admission 上限的请求拒绝且不新增费用。

End 必须等于真实已收到的长度及 streaming whole digest。成功状态只叫 `VerifiedBytes`，不是 Store 持久化、FileCreate、上传完成或业务 commit；空对象仍需真实空串 SHA256。verified/cancelled 状态拒绝新接受，cancel 释放最近请求而不退费。默认 helper 上限为 payload 16 MiB、request 32 MiB、response 4 MiB、合计 wire 36 MiB、request/receipt 各 1,024；可收紧，硬上限 wire/request/response 各 64 MiB、计数各 4,096。

## Rust、C、C++17 接口

Rust 的目录 `FsDirectoryPage`/`FsDirectoryPageRef` 与 blob `Frame`/`FrameRef` 提供 owned/borrowed 表示。两个独立 crate 均输出 rlib/staticlib，固定 registry 依赖版本来自原 SDK 允许集合，不依赖 Core/runtime 提供权限。

C 使用各库的 opaque owning handles 与小型 checked views，不复制整个 64 KiB 容器到调用栈。view 的名称或 payload 借用 owner，到 owner 释放或替换时失效；输入需符合 header 的 NULL、长度、对齐、reserved 和 alias 合约。错误保持输出 slot、buffer 和 length 不变，状态 helper 可能保留已 admitted 费用。调用者必须提供有效原生内存、live handles 和独占可变访问；检查不是指针沙箱，普通 allocator OOM 不承诺可恢复。

C++17 wrapper 是 move-only owner，销毁时释放 handle；encode 先生成临时结果，只在成功时替换 vector。blob 的 Debug 只输出类型、长度、序号/offset及有限阶段摘要，不输出 payload、opaque IDs 或 digest 数组；Snapshot/Acceptance 的嵌套表示也遵循此规则。状态成功、快照或 receipt 均不包含授权证明。详细接口见 [目录 README](../extensions/fs-directory-v1/README.md) 和 [blob README](../extensions/blob-transfer-v1/README.md)。

## 仍开放的产品能力

blob真实大对象后端、上传与Store效果记录、blob原owner接线和公开guest import仍未完成；目录的watch／rename、授权选择来源及公共入口也仍开放。已有独立mutation Create/Delete不能补作目录/blob公共能力。conditional Replace 仍 Unsupported；不能用普通覆盖或先删除再写代替条件操作。来源或外部效果不确定时保留 Unknown，不能凭 receipt、VerifiedBytes、EOF、清理或新建 helper 自动重放。Linux、GUI、账户、真实数据库、DPAPI、CI、Release 与完整 SDK 资格未由本轮库检查覆盖。
