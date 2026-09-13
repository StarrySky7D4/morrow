# test.45：固定归档游标与全库暂存准入

日期2026-09-13。应用 `0.1.9-test.45+50`，核心／宿主／审计及打包清单test.45；运行时仍test.44（本轮链接核心45全量验证），SDK test.11、guest test.13。数据库格式12，新增可重建SQLite索引，不改变业务PB、原件及签名契约。第一方AGPL-3.0-only，无新增unsafe，仅本地开发。

## 实现与实际接入

新增`ReadArchiveCursor`在独立只读WAL事务中钉定原归档，按调用方预期根、Manifest原件与audit identity检查逻辑来源，完整验证所有分片与发布关联后才允许首片交付。每页按operation_id和ordinal索引扫描，原PB＋容器预算最多32 MiB、最多128片；预算不足不推进，结构／SQL错误使游标失效。完整遍历无需每页重复验整链，8 MiB原件仍可完整读取。

游标完成EOF与释放读取事务分开；finish检查完整消费，close报告ROLLBACK失败，Drop兜底。Store owner绑定不是权限，Store被释放后游标仍持有独立视图；关闭reader不冒称释放Windows Session应用租约。Web／exclusive明确拒绝，尚无其对应游标后端。

真实查询恢复资格已改为同一cursor逐页读取原包、全部冻结来源与执行观察，保持130卡过滤／分段排序／归并和空库Filter的完整实际Rust重放。预期root来自读取观察；审计专项另从实际Ed25519验证的事件取根，源目录和原凭据删除后由备份恢复，并核对原片字节与reader／Session各自租约。

全库所有subject的未发布归档共用32档／8 GiB硬准入限额。计入Manifest原PB＋容器与所有Part累计原PB＋容器；begin／append在同IMMEDIATE事务检查占用和修改，目录增量按新旧成本差额计费。`with_admission_budget`仅允许此次调用更低额度，非持久库配置；精确无增长重试不重复收费。发布／中止释放暂存占用，已发布原件不被清理。

旧超过32档的库仍保留原件并可打开、逐档发布／中止收口；usage和增长准入返回Limit。档数合法但旧字节超过8 GiB时usage仍返回真实用量，新增增长拒绝。索引`read_archive_preparations(published,operation_id)`避免每次为pending查询扫描已发布行；写模式打开时可重建，独立只读不写库。

## 审查与失败记录

三个子代理分别实现游标、独立核心回归、审计及宿主真实回放资格，并交叉审查根代理的暂存配额。未发现确定的事务或计费绕过。根审查补了32 MiB单页硬限，避免调用方给出usize::MAX时可能一次构建GiB级页面；该限制不降低整体归档容量。

暂存初轮8项中7PASS／1FAIL：并发测试在同时打开两个Store时遇到合法StorageBusy，尚未进入要验证的准入竞争。修正为先顺序打开两个真实Store，再通过Barrier同时执行准入事务；没有屏蔽或修改生产StorageBusy语义。新增并发追加最后字节额度与旧超额暂存保留测试后10／10通过。初次失败日志保存在`build/test45-read-archive-budget-initial.log`。

## 验证

| 验证 | 结果 |
|---|---|
| 核心release + fault-injection全量 | 331项PASS；新增cursor12项、暂存配额10项 |
| 宿主release + fault-injection全量 | 95项PASS，包含真实130卡／空库固定游标回放 |
| 运行时44链接核心45，release + packages全量 | 214项PASS |
| 审计release + fault-injection全目标 | 65项PASS |
| 核心／宿主／运行时／审计全目标严格Clippy | 四部分PASS，-D warnings |
| 核心普通Wasm／web-storage编译 | 两种--lib check PASS；普通codec-only有4组store-only辅助方法dead_code警告，web-storage无警告；不代表浏览器运行验收 |
| Flutter与Windows实际构建 | Flutter39项PASS；Windows Release48.6秒构建PASS，实际test.45+50四项自检PASS、退出码0 |

计数来自独立--list，包含故障测试入口，不重复计算子进程嵌套输出。cursor实际验证首片前拒绝后段损坏、固定视图、8 MiB完整原件、错误pin／subject／owner、页参数超界与无推进重试；WAL checkpoint在reader存活和EOF后均busy，close／Drop后解除。范围SQL的实际EXPLAIN使用复合索引，257片分页完整遍历；性能证据不等于隐藏调用计数、基准吞吐或全平台保证。

暂存验证包括精确目录与分片字节临界、跨subject／跨Store／重开库共享占用、两个真实Store抢最后名额或最后字节预算、幂等重试、发布／中止释放、旧超額状态收口、损坏目录拒绝和索引重建。

## Windows实际产物

实际截图已查看：`build/workbench-host/test45-final-1644670052f6463abb91a6a7cccd9bb6/result.png`。正常显示侧栏、设置及Rust工作台卡片，无加载占位或Flutter错误画面。四项自检为原生合成blur 0/1/12/40与关闭API、真实静音WAV解码时钟、seek／播放互斥／恢复不自动播放、实际工作台渲染；其中透明效果是API检查，未做桌面背景像素比较。

自检库格式12、SQLite integrity_check=ok，暂存索引存在；6份内容Evidence／6次引用、34个唯一块／144次块引用，缺块与孤块均0。归档三表均0，明确不是默认查询自动存证的证据。5个产物SHA256和Windows内host/plugin与本轮生产打包副本一致性已重新核对。元数据在`build/test45-final-verification.json`，测试和构建日志在`build/test45-final-flutter.log`、`build/test45-windows-{build,qualification,inspection}.log`。

| 文件 | SHA256 |
|---|---|
| `morrow_studio.exe` | `8d2bee7c6ebfe71b910435f54f502b86d048e2a31461744ec66dcd1c8b0d950e` |
| `morrow-workbench-host.exe` | `5fb80d02d75636a26d5d5bbfdf22319573282e90fed614c0bd5c572bb405628f` |
| `workbench.morrowplugin` | `a47e9daa8e079061f04706666e637f384fea3a9c0e3ff7263c89e583fb1f082a` |
| `morrow-content-replay.exe` | `2482c19de69a87bdc7ebebbbc4c3652d83a2dd3248f3ebafd0760300f944f8be` |
| `morrow-audit-check.exe` | `bd54bcb6af868c9b545976703b1bbcc3b3e43887fa5599dda0867e4c115e27c1` |

## 仍待完成

默认Workbench::query仍使用一致来源读取，尚未自动持久化全部查询原件。本阶段游标接入的是实际恢复回放资格。暂存计费扫描有界Manifest与关联，信任经Store验证的累计成本；它不是抗外部任意SQL改造的磁盘配额，也不预留文件系统空间。已发布历史、WAL、其他内容和凭据不计入该暂存额度。

下一步需持久查询上下文、未完成意图恢复、取消／过期、已发布历史总配额及保留／GC，随后接入默认查询全部实际调用、全程与交付当前权限及历史修订结果。独立查询CLI、Web来源和游标后端、完整依赖图及全平台验收仍未完成。阶段结论PASS_SCOPED，完整重构目标保持继续。

日志：`build/test45-{core,host,runtime}-full.log`与对应Clippy／list；`build/test45-audit-full*.log`、`build/test45-read-archive-cursor-page-limit*.log`、`build/test45-read-archive-budget-{initial,final,clippy}.log`、`build/test45-{audit,query}-cursor*.log`。契约见[分片归档](../docs/PLUGIN_READ_ARCHIVE.md)，后续顺序见[路线](../docs/FUTURE_ROADMAP.md)。
