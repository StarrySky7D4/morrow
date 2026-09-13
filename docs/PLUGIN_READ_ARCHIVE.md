# 分片读取归档契约（test.47）

`core::read_archive` 保存有序不透明分片，`Store` 在内容库格式12管理准备与发布。它提供字节完整性和原子引用闭包；应用适配器另行证明EOF、来源、实际执行、授权与结果语义。普通卡片及其修订不因归档改变。

## 生命周期与提交

`begin_read_archive(Plan)` 固定操作ID、subject、请求类型／原件、响应类型及预算；准备仅占归档命名空间，不能阻止普通内容或读取先提交同一全局操作ID。此冲突在最终发布时拒绝。`append_read_archive` 接受连续序号与固定type；相同原件重试幂等，跳号或不同数据拒绝。暂存不会生成读取事件。

每片是预编译Protobuf＋LZ4，包含版本、序号、type、data原件及SHA256、前驱摘要。初始链是原Plan PB的SHA256；后续链取原Part PB的SHA256。最终Manifest保存原Plan、数量、逻辑字节、尾链、适配器元数据及响应摘要；根是原Manifest PB的SHA256。读取保留原始PB与容器，不能靠重编码未知字段验证摘要。

`finish_read_archive_local_authorized` 在一个IMMEDIATE事务内完整验证分片、关联及终结参数，保存schema2读取观察、永久操作索引、Evidence引用、最终目录、根引用和待封存事件，最后调用当前授权回调并提交。核心不将调用方声明的EOF当成独立来源证明。已提交重试核对原目录与观察，重新调用当前guard后返回原Receipt；不会重跑最新查询。提交错误为CommitUnknown，调用方应查原ID再决定恢复。

`abort_read_archive` 只删除未发布暂存；发布后拒绝。即使损坏库把目录降为pending并删除根引用，只要已存在schema2操作仍拒绝清理，保留原件供修复。普通内容／辅助记录／schema1读取抢先占用provisional ID，不阻止清理原暂存。

## 容量政策

| 项目 | 上限与计费 |
|---|---|
| 单分片data | 8 MiB |
| 单归档分片数 | 65536，可选择更小预算 |
| 单归档逻辑字节 | 4 GiB，可选择更小预算；每片原始PB＋压缩容器累计 |
| 请求、响应、最终元数据 | 各4 MiB，独立有界；封装有固定总量限制 |
| 原TaskEvidence | 每操作最多16份、原PB＋容器64 MiB；单证据PB24 MiB、batch最多1024观察，保持旧规则 |

超过预算明确拒绝，暂存仍未发布；不删观察或截断候选后声称成功。分片没有引用绕过旧Evidence计费，它是独立类型化归档政策。物理去重不降低逻辑计费。test.45加入全库暂存准入硬上限，test.47另加全部归档总容量；保留期限与GC仍未建立。test.46 已将 Windows 默认查询接入下述持久捕获。

既有`read_archive_manifest` 与 `read_archive_part`仍在各自事务内完整核验，逐片遍历会重复扫描。test.45的`open_read_archive_cursor(subject,operation,expected_root)`用于完整读取：仅接受已发布归档，将调用方预期根与原始Manifest、逻辑来源及audit identity核对，在独立只读WAL事务中先验证全部分片和关联，再允许返回任何一片。不能把未验证的目录自报根当成外部可信根，也不宣称完成OS文件身份认证。

游标的`manifest()`保留原件；`next_page(max_parts,max_bytes)`每页1～128片，原PB＋容器成本最多32 MiB。首片预算不足或参数超界返回Limit而不推进，可增加预算重试；结构、SQL等执行错误使游标失效。8 MiB原件仍可完整单片读取。分页使用`operation_id+ordinal`范围索引，只解码当前页，不再每页完整验链；一次全验证加全遍历不再形成O(N²)的重复工作。这里的性能证据是源码与实际EXPLAIN索引计划，不是隐藏调用计数或吞吐承诺。

`finish()`只检查已消费到EOF，不释放读取事务；调用方应显式`close()`并处理ROLLBACK错误，Drop为清理兜底。游标存活期间固定原始视图，后续发布、修改或外部损坏不会让它混入新数据。owner token用于同Store接纳检查，不是插件授权；Store被Drop后已建立游标仍有自己的连接。关闭游标只释放该WAL读事务，Windows Session仍持有其独立应用租约。Web／exclusive存储明确拒绝，尚需对应读视图后端。

## 全库暂存准入（test.45）

同一数据库所有subject的未发布归档共用32档和8 GiB硬上限。每档成本为原Manifest PB＋完整压缩容器＋累计Part原PB和容器；目录变化使用新旧成本差额，不重复计费。新增或追加在同一IMMEDIATE事务内读取全库占用并写入，两个Store不能重复消费最后一个名额或同一段剩余预算。

默认begin／append始终使用硬上限。`*_with_admission_budget`允许本次调用选择更低额度，并拒绝大于硬上限或零额度；它不是持久化的每库政策，更小额度需要宿主在每次相关准入继续传入。合法的精确无增长重试不再次占用容量，即使本次较低预算已不足也可核对原件；错误policy和不同原件仍拒绝。

发布将数据移出暂存计数，abort只清理未发布数据。已有旧库超过32档时保留原件，usage／增长准入返回Limit，仍允许逐档发布或中止收口；档数合法但总字节超过8 GiB时usage返回实际用量，增长准入拒绝。打开库不把超额本身当成损坏或静默删除数据。可重建索引`read_archive_preparations(published,operation_id)`在写模式打开时确保存在，数据库版本仍12，独立只读不修改库。

计费扫描有界pending Manifest并核对关联，信任经Store完整性校验的累计Part成本，避免每次追加重新解码所有历史分片。它不是抵抗外部任意SQL改造的磁盘配额安全边界，也不预留真实文件系统空间；完整数据由库打开、发布和游标入口核验。已发布历史、WAL保留、凭据、其他内容和外部文件不在此暂存准入额度内。

下一步需要持久查询上下文、未完成意图恢复与过期策略、已发布历史的保留／总配额／GC，再把完整捕获接入默认查询。该暂存硬限不等于整个系统的资源生命周期已闭环。

## 实际执行原件与回放

`Pool::record_transform_observation` 检查当前管理器与实例绑定，在新runner实际执行一个纯转换，返回包摘要、TaskReport及保留原始PB的ObservationRecord。它不复制整个包，不接受宿主调用，失败或撤权不返回成功记录。每次fuel取当前限制与调用方剩余预算的较小值；完整序列总预算由适配器累计。逐调用Observation仍只记录成功执行；捕获整体的失败／取消／中断由test.46持久状态保存，不伪造成功调用。

`task_evidence::decode_observation` 在Protobuf分配前限制帧长、字段数、嵌套预算、重复字段和u32范围。`replay_observation` 使用单独归档的包在隔离runner重放原输入，比较完成原帧、退出状态、host call与剩余fuel。调用方须验证包和归档根，并按自己的策略限制整个序列燃料；结构合法不等于历史真实执行。

查询资格适配器保留原请求、包、冻结Card原件与来源事实、每次过滤／排序／归并观察、EOF census和最终宿主结果。宿主有序结果使用独立PB，不能用单次guest最多128项的响应解码器读取跨页合并结果。源库与其凭据删除后，签名快照恢复并从恢复库读取所有回放输入；内存中的旧ID只作为结果比较基准。

## 证据边界与后续

签名证明既定身份封存了这些原始读取事件；分片哈希证明字节关联，census重算证明所给有序材料一致。它们不独立证明从未遗漏来源、不恢复历史授权，也不证明用户收到结果。独立audit CLI核验签名及完整数据库引用；尚无独立查询回放CLI。

下一步：结果历史修订映射、UI实时取消／过期、已发布历史GC、独立查询回放CLI与Web来源适配。第一方代码保持AGPL-3.0-only，本阶段无新增unsafe。


## 持久捕获状态与默认查询（test.46）

数据库格式13新增 `read_captures`，保留版本化原Plan、context、owner nonce、CAS revision和阶段。Preparing只表示未完成意图；Ready与原分片发布、读取观察、根关联和审计outbox在同一事务提交。Failed／Cancelled／Interrupted保留意图，原子释放未发布目录和分片。Ready不等于结果已收到或已显示。

核心接口 `begin_read_capture`、`append_read_capture`、`finish_read_capture_local_authorized`、`end_read_capture` 强制owner／revision及状态转移；`lookup_read_capture`和有界`list_read_captures`用于确认与恢复。owner是可信宿主的随机32字节会话标记，不是插件权限。终态ID不可复用；普通归档及其他内容／记录写入口不能占用tracked ID。Manifest的capture_binding钉定原意图及owner，与State双向检查；旧无标记归档保持原件不变。

最多4096个持久状态、256 MiB状态计费；每状态在begin保留原PB＋容器及额外4 KiB，支持容量满时写入有界终态原因。Preparing状态费用也计入全库32档／8 GiB暂存。低预算接口仅允许更低的此次准入，非持久策略。状态与已发布原件不自动删除：已发布总容量现按test.47执行，完整保留、过期与GC仍未实现；达到硬限需明确拒绝，不能静默覆盖旧结果。

Windows `Workbench::query` 默认创建唯一operation；`query_with_operation`允许保留调用者ID。源快照readpoint、调度器版本、实际包摘要与100亿总fuel预算先保存；随后记录原包、全部类型的原Card PB与来源事实，以及每次实际Filter／SortRun／Merge执行观察。单个任务预算仍受运行时限制，总归档65536片／4 GiB；这些是显式资源限制，不是候选截断。结果最多4096个ID，超限结束失败，无部分成功。

每次来源读取、实际调用、最终提交之前和结果交付检查当前插件选择／ReadContent许可与会话状态；读取Idea仍逐张授予并撤销对象权限。未宣称跨进程的持续撤权、原生实时抢占或UI已收讫。每次重试的授权来自当前会话，绝不从原owner或原包恢复旧grant。

同ID同条件的Ready重试返回原读取观察中的结果，不重新读取当前卡片或执行guest；改条件必须新ID。同步宿主重启时把自己的Preparing标记Interrupted；再次遇到丢失快照的Preparing也中断，不能换到新来源继续。同一ID的终态保留，明确终态重试使用新ID。任何写入或最终交付结果未知时先查询持久状态，不能把Ready改为Failed。

Flutter查询协调器在条件、内容generation或backend变化时新建128位随机ID；未知传输失败重试保留ID，宿主明确终态则新ID。私有Cap’n Proto保持原schema，query复用已有operation；仅query错误的uiCode=100表示明确终态，其余错误保守视为未确认。宿主响应帧仍128 KiB，超大结果交付失败不撤销Ready。超时后无自动重连，也不恢复旧授权。ID结果目前映射当前UI缓存；完整历史修订展示、UI实时取消、Web对应存储和独立查询CLI仍待接入。


## 全部归档的保留容量（test.47）

格式14新增派生 `read_archive_costs` 与 `read_archive_totals`，共同记录Preparing与published归档的逻辑原件成本。全库硬上限8192档／64 GiB，与32档／8 GiB暂存限制、4096状态／256 MiB状态限制分别成立。计费是Manifest原PB＋完整容器与各Part原PB＋完整容器，物理去重不折扣；不包括已独立限制的State、普通Card、读取事件/签名日志、WAL、SQLite索引和其他应用数据，不宣称预留磁盘空间或限制全部物理占用。

begin、append、finalize和abort与账本增减同一写事务提交，published也继续占用；最终Manifest的新增元数据必须纳入正增长准入，不能只在开始时检查后无上限发布。精确无增长重试、减少费用的操作不因当前更低政策被阻止。账本是派生的加速信息，原PB／分片仍是成本依据，完整性检查及快照验证核对二者，不以SQLite缓存取代完整原件验证。

`RetentionBudget { max_archives, max_bytes }`、`set_read_archive_retention_budget`只收紧当前Store的运行期政策，既有普通与tracked归档写入口都执行，不新增旁路append API。另一Store默认仍执行硬上限，双方共享同一数据库事务账本；较低policy不会自动传播或成为全库用户配置。`read_archive_retention_usage`报告实际计费，包括已超限旧库。迁移回填账本，不改变旧PB或签名，也不因超限拒绝只读历史；正增长会返回类型化ArchiveCapacity。

宿主把ArchiveCapacity与任务/帧Limit分开。开始前容量失败不产生半条捕获，部分捕获容量失败尝试原子结束并保存query_archive_capacity原因。若终止无法确认，保留Preparing和原operation，不伪称失败已完成。query专用uiCode101表示容量且明确终态，102表示容量且结果未确认；100和0保留旧含义，Cap’n Proto schema/digest不变。Flutter显示容量已满、已有内容保留、当前版尚不支持清理历史；只有用户点击重新检查才重试，不暗示备份本身会释放容量。

Ready原结果在预算收紧后仍可按当前读取权限交付，同ID不重新执行guest。自动清理、历史删除或State压缩尚未实现。后续需要合法清理记录、材料可用性与签名验证分开报告；精确设计见[归档保留与清理](PLUGIN_ARCHIVE_RETENTION.md)。
