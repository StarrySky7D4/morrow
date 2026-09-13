# test.46：持久捕获状态与默认查询存证

日期2026-09-13。应用 `0.1.9-test.46+51`，核心／宿主／审计与打包清单test.46；运行时仍test.44（本轮链接核心46验证），SDK test.11、guest test.13。数据库格式13。第一方AGPL-3.0-only，本轮未增加unsafe，只进行本地开发。

## 实际接入

Windows默认查询现在保存原始意图、固定来源readpoint、实际包与全部调用原件。每次Filter／SortRun／Merge执行真实Rust Wasm guest并保存Observation；全部类型的Card原件与来源事实进入分片，不把不属于Idea的卡片漏出来源census。结果最多4096个ID，资源超限明确失败，不截断候选或删除观察后声称完成。

数据库格式13新增持久read_captures。Preparing保留尚未完成意图，Ready与分片发布、读取观察、根关联及outbox同事务提交；Failed／Cancelled／Interrupted保留原Plan/context并释放未发布原件。随机owner与revision控制状态修改，终态不能用同ID重新绑定来源，普通内容／记录／归档入口不能绕过。Manifest绑定不可变意图及owner，删除一边也不能降回无跟踪归档。旧DB12及更早原件、既有签名保持原字节。

每状态原PB＋容器及额外4 KiB在begin计费，保留小终态更新空间；最多4096状态／256 MiB。Preparing状态也计入全库32档／8 GiB暂存。较低Budget是此次准入政策，不是持久配置。list每页最多128状态且32 MiB，必须读到空页才认定结束。

稳定operation贯穿Flutter、现有Cap’n Proto字段与Rust。相同条件、相同Ready ID重试读取原已提交IDs，不重新访问当前来源或执行guest；条件改变、内容generation变化或替换backend生成新ID。明确终态重试新ID，未知结果保留ID。现有query错误uiCode=100表示明确终态，不更改Cap’n Proto schema/digest。全局已占用ID冲突也明确告知需新查询，避免相同ID反复失败。

宿主启动时仅将自己query subject/context的Preparing标记Interrupted，不能换新快照续跑。每次来源读取、调用、提交前和交付检查当前许可，Idea按对象临时授予并撤销ReadContent。Ready后的封存、权限或交付失败不降级已提交结果；Ready不是用户已收到或展示的证明。

## 并行实现与审查

子代理分别负责核心生命周期、独立故障／迁移资格、Dart协调器及真实查询回放。根代理负责实际默认查询适配、协议错误分类、集成和最终产物。旧夹具更新只添加新表降级清理与当前格式13断言，保留故障中间格式。唯一unsafe模块白名单仍为plugin_runtime/src/shared_memory.rs，运行时全量再次覆盖真实Windows只读映射权限与写保护。

独立核心初次编译遇到测试子进程OsString转换Path类型错误，修正测试后运行16/16通过；生产首次strict有needless_borrow/collapsible_if两个lint并已修正。未通过降低错误等级或禁用测试解决。Ready后封存故障使用仅单元测试＋fault-injection构建存在的实例一次性开关，不新增unsafe或全局环境变更。

## 验证

| 范围 | 结果 |
|---|---|
| 核心release + fault-injection | 全量347/347 PASS；新增16项含1 child入口，14个真实进程退出边界 |
| 宿主release + fault-injection | 原全量含首批5项共100 PASS；新增2项后生产查询7/7再验PASS，最终清单102项全部覆盖 |
| 运行时release + packages | 全量214/214 PASS |
| 审计release + fault-injection | 全量65/65 PASS |
| 四部分all-targets严格Clippy | 全部PASS，-D warnings；宿主最终增量再次PASS |
| 核心普通Wasm／web-storage | 两种--lib check PASS；普通codec-only有6组store-only dead_code警告，web-storage无警告；不代表浏览器运行 |
| Flutter实际宿主集成 | 9文件40项PASS，包含同ID跨内容修改及进程重开保留结果 |

核心资格含owner/CAS、通用API绕过、双向标记、已签名DB12迁移、状态和暂存预算边界、begin／append／Ready／end四类真实SQLite COMMIT失败及14个实际崩溃点。状态是持久控制事实，未宣称提供已签名的完整状态转换历史。

宿主真实多阶段资格保存130 Idea加2个其他类型来源，独立快照census和132份原卡字节作为断言oracle。删除原库目录与凭据后，从备份恢复的Store取得原包、全部分片与观察，经固定归档游标逐条实际replay_observation，并复用版本化query_plan重放全部过滤／排序／归并，核对最终130 ID。不是仅验证容器可解包，也不是未来独立查询CLI已经完成。

新增故障资格证明：Ready写入成功后的一次封存失败返回非终态错误，原State和ReadObservation容器、counter保持；同ID重试返回原结果，不再执行guest，随后显式维护可封存原事件。恢复Preparing与Cancelled使用正式核心API构造持久状态；真实进程崩溃原子性由核心14处子进程测试证明，不把宿主模拟状态等同强杀正在执行的guest。

## Windows产物验收

实际Windows Release编译80.6秒，版本 `0.1.9-test.46+51`、退出码0。四项自检通过：原生blur 0/1/12/40及关闭API、静音WAV解码播放时钟、seek／播放互斥／恢复不自动播放、实际Rust工作台渲染。玻璃效果仍为API级检查，未做桌面背景像素比较。

实际截图已查看：`build/workbench-host/test46-final-767a7879f90e41e08c4b1416a8cde98b/result.png`，侧栏、设置与卡片正常，没有加载或Flutter错误画面。实际自检库格式13、integrity_check=ok；1个Ready状态、1个已发布归档、10片、1条提交根，证明默认查询已经真实持久化。仍有6份内容Evidence／6引用、34个唯一块／144引用，缺块／孤块及缺失捕获关联均0。

5个SHA256已重算，Windows内host／plugin与本轮生产打包副本一致。详情 `build/test46-final-verification.json`；构建、自检和检查日志为 `build/test46-windows-{build,qualification,inspection}.log`。

| 产物 | SHA256 |
|---|---|
| `morrow_studio.exe` | `ff806d0f5d447221ecbbfe6c338f17b2debb2fc94410a37ecb0c4f78a004f1c5` |
| `morrow-workbench-host.exe` | `85788489681ca9928a3a8b1b407671315154b0d973f06a48c721a9ac0e2b73e7` |
| `workbench.morrowplugin` | `d5adaa42852cd685b5bf01865e4f57c952299b469b29858da2e2df39f96e1ce0` |
| `morrow-content-replay.exe` | `4ffadb10253b4a42fe6f9f9333bba2cae2e05020c824192786993a7c2a649cf8` |
| `morrow-audit-check.exe` | `bf205950adfaa84d049771e459c74981810561378f3a3708403d8e2fa8cd78d2` |

## 后续范围

下一阶段优先完成已发布历史总字节准入、显式保留／GC、取消／过期与历史结果修订展示。当前4096状态限额不是完整磁盘配额；已发布归档、WAL和其他内容仍未纳入统一物理空间政策，达到硬限不会自动删除旧内容。默认查询总fuel100亿，分片65536／4 GiB；已定义超限失败，不保证任意规模资料库均能完成。

当前同步宿主尚无UI实时取消，自动重连不恢复授权；超时后需要显式重开。ID映射的是当前UI缓存，完整历史修订投影仍待接入。128 KiB响应帧超限是交付错误，不撤销Ready。Web／Android没有接入本轮Windows持久查询；独立查询CLI、依赖图证据和全平台后端验收仍在推进。完整重构目标保持继续，阶段验证不能视为插件系统完成。

日志：`build/test46-core-{full,clippy,list}.log`、`build/test46-host-{full,query-final,clippy-final,list-final}.log`、`build/test46-{runtime,audit}-{full,clippy,list}.log`、`build/test46-read-capture-{final,clippy-final}.log`、`build/test46-final-flutter.log`。契约见[读取归档](../docs/PLUGIN_READ_ARCHIVE.md)，后续见[路线](../docs/FUTURE_ROADMAP.md)。
