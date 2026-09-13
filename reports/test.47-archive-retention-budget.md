# test.47：全部归档容量与可审计清理设计

日期2026-09-13。应用 `0.1.9-test.47+52`，核心／宿主／审计与包清单test.47；运行时仍test.44（链接核心47验证），SDK test.11、guest test.13。数据库格式14，业务PB及原签名不改。第一方AGPL-3.0-only，无新增unsafe，仅本地开发。

## 实际变更

全库Preparing与published归档共用8192档／64 GiB逻辑容量。每档成本为Manifest原PB＋完整容器与所有Part原PB＋完整容器；已发布历史继续计费，finalize新增元数据也计入。它与32档／8 GiB暂存、4096状态／256 MiB控制状态分别成立；未宣称限制Card、读取事件、签名日志、WAL、索引或整个磁盘占用。

格式14新增read_archive_costs每档费用与read_archive_totals汇总两张派生表。四个归档写入位置在原IMMEDIATE事务内更新原件及账本：begin、append、finalize和abort。失败、当前授权拒绝或COMMIT结果未知都不能留下脱离原件的永久预扣。日常账本准入读取总账及受影响行，不重新解包全部历史Manifest；完整性与备份验证仍流式核对原目录、完整分片及派生费用，账本不是抗任意外部SQL篡改的独立安全边界。

Store运行期RetentionBudget可收紧额度，所有tracked/untracked归档入口执行；打开另一Store恢复硬上限，低预算不是持久库配置或插件授权。多Store仍在同一个数据库事务账本上争抢实际剩余容量。精确无增长重试和暂存减少不因更低政策受阻。旧库先验证原件、流式回填账本、升级格式再完整验证；原超额库可读取并清理pending，任何正增长仍受限，finalize增长也不例外。

宿主使用类型化ArchiveCapacity区分总归档容量与任务自身Limit。新准入失败不留半条capture；部分捕获失败保留query_archive_capacity终态原因并原子释放暂存。终止无法确认时保留Preparing与operation，协议报告未知容量失败。Ready历史不降级，同ID仍经当前权限返回原结果。

Flutter显示“查询历史容量已满”和已有内容保留提示，明确当前版尚不支持历史清理。只在用户点击“重新检查”后重试；明确终态使用新ID，未知状态复用原ID。现有query专用uiCode101／102表示容量且明确／未确认，100／0原语义不变，Cap’n Proto schema/digest未改变。没有通过错误文本猜测容量，也没有自动删除历史或暗示备份本身释放容量。

## 独立实现与审查

核心、独立测试和清理设计／只读审查由三个子代理并行处理；根代理接实际宿主错误、Flutter界面与最终集成。审查核对了FK顺序、原件和费用同事务回滚、finalize成本、checked整数转换与低预算Ready重试，未发现确定生产缺陷。

独立预算测试首轮15/15通过（14项实质场景＋1个child入口），包括4类真实SQLite COMMIT失败与10个实际子进程退出点，两个真实Store分别抢最后档数和字节预算，签名迁移／快照原件保真。超额迁移样本为1份真实published加8192份合法旧pending归档，不是8193份published，也不是64 GiB物理实测。

初次核心check有内部begin漏传新预算参数，初次生产strict有collapsible_if风格lint，均已修复。完整核心首跑遇旧read_archive_budget夹具在当前DB14直接SQL插行却不造账本，正确返回Integrity；修复夹具为旧格式13构造后走14迁移，不放松生产校验。初次日志均保留，最终结果以下表为准。

## 验证

| 范围 | 结果 |
|---|---|
| 核心release + fault-injection | 完整重跑362/362 PASS；独立清单同362 |
| 宿主release + fault-injection | 全量105/105 PASS；查询专项10/10 PASS |
| 运行时release + packages | 全量214/214 PASS |
| 审计release + fault-injection | 全量65/65 PASS |
| 四部分all-targets严格Clippy | 全部PASS，-D warnings |
| 核心普通Wasm／web-storage | 两种--lib check PASS；普通codec-only有7组store-only dead_code警告，web-storage无警告；不代表浏览器运行验收 |
| Flutter实际宿主集成 | 9文件41项PASS；容量状态及重试交互专项5项PASS |

计数均由独立--list核对，包含故障child测试入口，不重复计入嵌套子进程输出。

宿主新增三个真实容量测试：历史已满拒新capture但保留原Ready读取；真实包追加越界后终态与费用释放；通过真实SQLite触发器阻止清理，协议返回102且Preparing保留，移除故障后相同ID中断并回收暂存。容量错误协议测试调用实际Cap’n Proto respond；Flutter独立交互测试验证对应UI和ID策略，两者不冒称用产品配置实际填满64 GiB。

## Windows实际产物

实际Windows Release编译86.5秒，版本 `0.1.9-test.47+52`、退出码0。四项自检通过：原生blur 0/1/12/40与关闭API、真实静音WAV解码时钟、seek／播放互斥／恢复不自动播放、实际Rust工作台无Flutter错误渲染。玻璃效果仍是API检查，未进行桌面背景像素比较。

截图已查看：`build/workbench-host/test47-final-19bff1ab355c4ae3bd21ed11974e5a31/result.png`，正常侧栏、设置和卡片，无加载或错误占位。自检库格式14、SQLite integrity_check=ok，1个Ready/1个published归档/10片/1个提交根；费用行及总账均1档、1,587,168逻辑字节。保留6份内容Evidence／6引用、34个唯一块／144引用，缺块／孤块及缺失捕获关联均0。

5个产物SHA256已重算，Windows内host及插件包与本轮生产打包副本一致。元数据 `build/test47-final-verification.json`；实际构建及验收日志 `build/test47-windows-{build,qualification,inspection}.log`。

| 产物 | SHA256 |
|---|---|
| `morrow_studio.exe` | `c79d3d2fa9455e75a04324630a72facff243c50a7963c56dc9baa549585bd949` |
| `morrow-workbench-host.exe` | `2b307a1cc37276554d6d37c770e7ac96f79cf34133e85e6611ec140f03d51957` |
| `workbench.morrowplugin` | `ab7f7f54997bc5daa54822f1a8327386167617359b40f6b77fd5cd2de0e33b82` |
| `morrow-content-replay.exe` | `0673e6e9f6fb907481a2107adf96565edf598de9700ad5765f817aa88b723268` |
| `morrow-audit-check.exe` | `82f90bf977ee8dee2b7f83733f47af6ca9398b545853a1709386cdcebf9f623f` |

## 已发布历史清理的下一阶段合同

[归档保留与清理设计](../docs/PLUGIN_ARCHIVE_RETENTION.md)已完成，明确标记为设计而非实现。首期整档材料退休需保留原Manifest／root／ReadObservation／签名／State和永久ID，新增独立清理操作、审计事件及墓碑，将材料状态与历史Ready分开。删除Part、更新账本与写清理事件需同事务和末次授权；CommitUnknown通过清理ID确认，封存失败仅表示清理事件待封存。

验证器和CLI应分别报告签名链、材料可用性与实际回放，不能把缺片或仅有墓碑称为完整证据。旧WAL cursor可能仍持有原材料，逻辑减量不承诺物理文件立即缩小；签名备份恢复也必须保留清理事实。下一阶段不得直接DELETE published后忽略完整性错误。

当前尚未实现历史删除、自动期限清理、GC或元数据压缩。首期材料退休仍保留8192档和4096状态占用，需后续明确长期身份／元数据政策；不能把本轮硬限额宣传为无限长期保存能力。历史修订UI、独立查询CLI、Web来源／存储、依赖图证据及全平台验收仍未完成。阶段验证不能作为完整插件系统完成。

日志为 `build/test47-{core,host,runtime,audit}-{full,clippy,list}.log`、`build/test47-core-full-initial.log`、`build/test47-read-retention-{initial,clippy}.log`、`build/test47-host-query-initial.log`、`build/test47-query-ui.log`，以及本轮Flutter/Windows验证日志。契约见[分片归档](../docs/PLUGIN_READ_ARCHIVE.md)，下一阶段见[路线](../docs/FUTURE_ROADMAP.md)。

阶段结论PASS_SCOPED；全量重构目标继续，尚未完成已发布历史清理及全平台插件验收。
